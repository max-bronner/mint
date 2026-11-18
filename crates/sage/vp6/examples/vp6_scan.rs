use mint_sage_vp6::{ChunkKind, Container};

fn main() {
    let (mut files, mut empty, mut exact, mut count_ok, mut max_ok) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut alpha_files, mut alpha_ok, mut key_first) = (0u32, 0u32, 0u32);
    for dir in std::env::args().skip(1) {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            eprintln!("cannot read {dir}");
            continue;
        };
        let mut paths: Vec<_> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vp6")))
            .collect();
        paths.sort();
        for p in paths {
            let Ok(data) = std::fs::read(&p) else {
                continue;
            };
            if data.is_empty() {
                empty += 1;
                continue;
            }
            files += 1;
            let name = p.display();
            let c = match Container::parse(&data) {
                Ok(c) => c,
                Err(e) => {
                    println!("FAIL header {name}: {e}");
                    continue;
                }
            };
            let (mut colour, mut alpha, mut biggest) = (0u32, 0u32, 0usize);
            let mut first = None;
            let mut ok = true;
            for ch in c.chunks() {
                match ch {
                    Ok(ch) => {
                        if first.is_none() && !ch.kind.is_alpha() {
                            first = Some(ch.kind);
                        }
                        biggest = biggest.max(ch.data.len());
                        if ch.kind.is_alpha() {
                            alpha += 1;
                        } else {
                            colour += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAIL walk {name}: {e}");
                        ok = false;
                    }
                }
            }
            exact += u32::from(ok);
            count_ok += u32::from(colour == c.header.frame_count);
            max_ok += u32::from(biggest == c.header.max_chunk as usize);
            key_first += u32::from(first == Some(ChunkKind::ColourKey));
            if c.alpha_header.is_some() {
                alpha_files += 1;
                alpha_ok += u32::from(alpha == c.header.frame_count);
            } else if alpha != 0 {
                println!("FAIL alpha chunks without AVP6 {name}");
            }
        }
    }
    println!(
        "walked {files} (+{empty} empty): EOF exact {exact}, colour count == header {count_ok}, \
         largest == header max {max_ok}, first colour chunk key {key_first}, \
         alpha files {alpha_files}: alpha count == header {alpha_ok}"
    );
}
