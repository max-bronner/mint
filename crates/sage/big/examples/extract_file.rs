use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use mint_sage_big::BigArchive;
use mint_vfs::{DirMount, Vfs};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out_file, vfs_path, install_dirs @ ..] = args.as_slice() else {
        eprintln!("usage: extract_file <out file> <vfs path> <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    };
    if install_dirs.is_empty() {
        eprintln!("need at least one install dir");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in install_dirs {
        mount_install(&mut vfs, Path::new(root));
    }

    match vfs.read(vfs_path) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(out_file, &bytes) {
                eprintln!("{out_file}: {e}");
                return ExitCode::FAILURE;
            }
            println!("wrote {} ({} bytes) from {vfs_path}", out_file, bytes.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{vfs_path}: {e}");
            let needle = Path::new(vfs_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase());
            if let Some(needle) = needle {
                eprintln!("  nearest matches by file name:");
                for path in vfs.paths() {
                    let path_str = path.as_str();
                    if path_str.to_lowercase().contains(&needle) {
                        eprintln!("    {path_str}");
                    }
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn mount_install(vfs: &mut Vfs, root: &Path) {
    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        vfs.mount(Arc::new(archive));
    }
    let data = root.join("data");
    if data.is_dir() {
        if let Ok(mount) = DirMount::with_prefix(&data, "data") {
            vfs.mount(Arc::new(mount));
        }
    }
}

fn archives(root: &Path) -> Vec<PathBuf> {
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
                    let is_big = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("big"));
                    if is_big {
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
