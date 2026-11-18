#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, missing_docs)]

use mint_vp6::{Decoder, Picture};
use std::path::{Path, PathBuf};

struct Movie {
    width: usize,
    height: usize,
    count: u32,
    colour: Vec<Vec<u8>>,
    alpha: Vec<Vec<u8>>,
}

fn walk(data: &[u8]) -> Result<Movie, String> {
    let mut movie = Movie {
        width: 0,
        height: 0,
        count: 0,
        colour: Vec::new(),
        alpha: Vec::new(),
    };
    let mut pos = 0usize;
    while pos < data.len() {
        let head = data.get(pos..pos + 8).ok_or("chunk header past EOF")?;
        let size = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
        let payload = data.get(pos + 8..pos + size).ok_or("chunk past EOF")?;
        match &head[..4] {
            b"MVhd" => {
                movie.width = u16::from_le_bytes(payload[4..6].try_into().unwrap()) as usize;
                movie.height = u16::from_le_bytes(payload[6..8].try_into().unwrap()) as usize;
                movie.count = u32::from_le_bytes(payload[8..12].try_into().unwrap());
            }
            b"MV0K" | b"MV0F" => movie.colour.push(payload.to_vec()),
            b"AV0K" | b"AV0F" => movie.alpha.push(payload.to_vec()),
            _ => {}
        }
        if size < 8 {
            return Err("chunk size below 8".into());
        }
        pos += size;
    }
    Ok(movie)
}

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x100_0000_01b3);
    }
}

fn hash_picture(hash: &mut u64, p: &Picture) {
    fnv(hash, p.y().data());
    fnv(hash, p.u().data());
    fnv(hash, p.v().data());
}

fn movies(dirs: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in dirs {
        let mut files: Vec<_> = std::fs::read_dir(d)
            .expect("movies directory")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vp6")))
            .collect();
        files.sort();
        out.extend(files);
    }
    out
}

fn corpus(dirs: &[String]) {
    let (mut files, mut empty, mut frames, mut errors) = (0, 0, 0u64, 0u64);
    for path in movies(dirs) {
        let data = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if data.is_empty() {
            empty += 1;
            println!("{name}: empty file");
            continue;
        }
        let movie = match walk(&data) {
            Ok(m) => m,
            Err(e) => {
                errors += 1;
                println!("{name}: container: {e}");
                continue;
            }
        };
        files += 1;
        let mut file_errors = 0u64;
        let mut hashes = [0xcbf2_9ce4_8422_2325u64; 2];
        for (stream, (chunks, hash)) in [&movie.colour, &movie.alpha]
            .into_iter()
            .zip(hashes.iter_mut())
            .enumerate()
        {
            let mut dec = Decoder::new();
            for (i, chunk) in chunks.iter().enumerate() {
                match dec.decode(chunk) {
                    Ok(p) => {
                        frames += 1;
                        hash_picture(hash, p);
                    }
                    Err(e) => {
                        file_errors += 1;
                        if file_errors <= 3 {
                            println!("{name}: stream {stream} frame {i}: {e}");
                        }
                    }
                }
            }
        }
        errors += file_errors;
        println!(
            "{name}: {}x{} header count {} colour {} alpha {} errors {file_errors} hash {:016x} {}",
            movie.width,
            movie.height,
            movie.count,
            movie.colour.len(),
            movie.alpha.len(),
            hashes[0],
            if movie.alpha.is_empty() {
                String::new()
            } else {
                format!("{:016x}", hashes[1])
            }
        );
    }
    println!("total: files {files} empty {empty} frames {frames} errors {errors}");
}

fn rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let (u, v) = (i32::from(u) - 128, i32::from(v) - 128);
    let clamp =
        |c: i32| -> u8 { ((1.2 * f64::from(i32::from(y) + c) - 15.0) as i32).clamp(0, 255) as u8 };
    [
        clamp(351 * v / 256),
        clamp(-86 * u / 256 + -179 * v / 256),
        clamp(443 * u / 256),
    ]
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn write_png(path: &Path, width: usize, height: usize, rgb: &[u8]) {
    let mut raw = Vec::new();
    for row in rgb.chunks(width * 3) {
        raw.push(0u8);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65535).enumerate() {
        z.push(u8::from((i + 1) * 65535 >= raw.len()));
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |tag: &[u8], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut t = tag.to_vec();
        t.extend_from_slice(body);
        out.extend_from_slice(&t);
        out.extend_from_slice(&crc32(&t).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}

fn dump(file: &str, frame: usize, out: &str) {
    let movie = walk(&std::fs::read(file).unwrap()).unwrap();
    let mut dec = Decoder::new();
    for chunk in &movie.colour[..frame] {
        dec.decode(chunk).unwrap();
    }
    let p = dec.decode(&movie.colour[frame]).unwrap();
    let (w, h) = (movie.width.min(p.width()), movie.height.min(p.height()));
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let (luma, cb, cr) = (
                p.y().row(y).unwrap()[x],
                p.u().row(y / 2).unwrap()[x / 2],
                p.v().row(y / 2).unwrap()[x / 2],
            );
            px.extend_from_slice(&rgb(luma, cb, cr));
        }
    }
    write_png(Path::new(out), w, h, &px);
    println!("wrote {out}: {w}x{h}, coded {}x{}", p.width(), p.height());
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--png") {
        dump(&args[1], args[2].parse().unwrap(), &args[3]);
    } else {
        args.retain(|a| !a.is_empty());
        corpus(&args);
    }
}
