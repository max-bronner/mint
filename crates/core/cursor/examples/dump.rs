use std::path::{Path, PathBuf};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: dump <data/cursors dir> <out dir>");
        std::process::exit(2);
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&input)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    let mut total = 0usize;
    let mut ok_static = 0usize;
    let mut ok_animated = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();

    for path in entries {
        total = total.saturating_add(1);
        let bytes = std::fs::read(&path)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cursor")
            .to_owned();
        match mint_cursor::decode(&bytes) {
            Ok(mint_cursor::Cursor::Static(image)) => {
                ok_static = ok_static.saturating_add(1);
                write_bmp(&out.join(format!("{stem}.bmp")), &image)?;
            }
            Ok(mint_cursor::Cursor::Animated(anim)) => {
                ok_animated = ok_animated.saturating_add(1);
                for (i, image) in anim.frames().iter().take(4).enumerate() {
                    write_bmp(&out.join(format!("{stem}_frame{i}.bmp")), image)?;
                }
            }
            Err(e) => failures.push((path.display().to_string(), e.to_string())),
        }
    }

    println!("mint-cursor dump: {total} files under {input}");
    println!(
        "  {ok_static} static, {ok_animated} animated, {} failed",
        failures.len()
    );
    for (name, reason) in &failures {
        println!("  FAILED {name}: {reason}");
    }
    println!("frames written to {}", out.display());
    Ok(())
}

fn write_bmp(path: &Path, image: &mint_cursor::CursorImage) -> std::io::Result<()> {
    let width = image.width();
    let height = image.height();
    let stride = width.saturating_mul(3).saturating_add(3) / 4 * 4;
    let pixel_bytes = stride.saturating_mul(height);
    let file_size = 14u32.saturating_add(40).saturating_add(pixel_bytes);

    let mut out = Vec::with_capacity(file_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&i32_le(width));
    out.extend_from_slice(&i32_le(height));
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&pixel_bytes.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    let pixels = image.pixels();
    for row in (0..height).rev() {
        let mut written = 0u32;
        for col in 0..width {
            let src = row
                .saturating_mul(width)
                .saturating_add(col)
                .saturating_mul(4) as usize;
            let Some(px) = pixels.get(src..src.saturating_add(4)) else {
                continue;
            };
            if let [r, g, b, _a] = *px {
                out.extend_from_slice(&[b, g, r]);
                written = written.saturating_add(3);
            }
        }
        out.resize(
            out.len()
                .saturating_add(stride.saturating_sub(written) as usize),
            0,
        );
    }

    std::fs::write(path, out)
}

fn i32_le(value: u32) -> [u8; 4] {
    i32::try_from(value).unwrap_or(i32::MAX).to_le_bytes()
}
