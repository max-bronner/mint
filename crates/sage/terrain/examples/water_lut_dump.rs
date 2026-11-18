use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_vfs::{Vfs, VfsPath};

const RAMPS: &[&str] = &[
    "lutdepthtint.tga",
    "lutdepthtint02.tga",
    "lutdepthtint03.tga",
    "lutdepthtint_old.tga",
    "lutmirkwood.tga",
    "wtrdepth_harlindon.tga",
    "wdepth_test.tga",
];

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let mut vfs = Vfs::new();
    for root in roots.iter().skip(1).rev().chain(roots.first()) {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(std::sync::Arc::new(archive));
            }
        }
    }
    println!("{} entries", vfs.len());

    for name in RAMPS {
        let path = VfsPath::new(&format!("art/terrain/{name}"));
        let Ok(bytes) = vfs.read_path(&path) else {
            println!("{name}: not present");
            continue;
        };
        let image = match mint_image::tga::decode(&bytes) {
            Ok(image) => image,
            Err(error) => {
                println!("{name}: {error}");
                continue;
            }
        };
        println!(
            "{name}: {}x{}, {} bytes",
            image.width(),
            image.height(),
            bytes.len()
        );
        let mut alpha_min = 255u8;
        let mut alpha_max = 0u8;
        for x in 0..image.width() {
            if let Some(p) = image.pixel(x, 0) {
                alpha_min = alpha_min.min(p[3]);
                alpha_max = alpha_max.max(p[3]);
            }
        }
        println!("  alpha {alpha_min}..{alpha_max}");
        for x in 0..image.width() {
            if let Some(p) = image.pixel(x, 0) {
                println!(
                    "  {x:>4}  {:>3} {:>3} {:>3}  a {:>3}",
                    p[0], p[1], p[2], p[3]
                );
            }
        }
    }
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}
