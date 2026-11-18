use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_vfs::{DirMount, Vfs, VfsPath};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: mount <install dir> [<install dir> ...]");
        eprintln!("  later directories shadow earlier ones");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    let mut totals = Totals::default();
    let started = Instant::now();

    for root in &roots {
        println!("== {} ==", root.display());
        mount_install(&mut vfs, root, &mut totals);
        println!();
    }

    let elapsed = started.elapsed();
    println!("== totals ==");
    println!(
        "  {} of {} archives parsed, {} entries, {} bytes of archive",
        totals.archives_ok, totals.archives_seen, totals.entries, totals.bytes
    );
    println!(
        "  {} visible paths after shadowing ({} shadowed)",
        vfs.len(),
        totals.shadowed
    );
    println!(
        "  directory parse: {:.3}s, {:.0} entries/s",
        elapsed.as_secs_f64(),
        f64::from(u32::try_from(totals.entries).unwrap_or(u32::MAX))
            / elapsed.as_secs_f64().max(1e-9)
    );

    println!("\n  cross-checks (both would fail if the format were misread):");
    println!(
        "    declared size == real file length:   {} of {}",
        totals.size_agreed, totals.archives_ok
    );
    println!(
        "    entry table ends at or before data:  {} of {}",
        totals.table_fits, totals.archives_ok
    );
    println!(
        "    entry read length == directory size: {} of {} probes",
        totals.reads_agreed, totals.reads
    );

    if !totals.failures.is_empty() {
        println!("\n  {} failures:", totals.failures.len());
        for failure in &totals.failures {
            println!("    {failure}");
        }
    }

    println!("\n  by extension (top ten of the visible namespace):");
    let mut by_extension: BTreeMap<&str, usize> = BTreeMap::new();
    for path in vfs.paths() {
        let extension = path.extension().unwrap_or("(none)");
        *by_extension.entry(extension).or_default() += 1;
    }
    let mut ranked: Vec<_> = by_extension.into_iter().collect();
    ranked.sort_by_key(|(extension, count)| (std::cmp::Reverse(*count), *extension));
    for (extension, count) in ranked.into_iter().take(10) {
        println!("    {extension:<10} {count:>7}");
    }

    if totals.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct Totals {
    archives_seen: usize,
    archives_ok: usize,
    entries: usize,
    bytes: u64,
    shadowed: usize,
    size_agreed: usize,
    table_fits: usize,
    reads: usize,
    reads_agreed: usize,
    failures: Vec<String>,
}

fn mount_install(vfs: &mut Vfs, root: &Path, totals: &mut Totals) {
    for path in archives(root) {
        totals.archives_seen += 1;
        let archive = match BigArchive::open(&path) {
            Ok(archive) => archive,
            Err(error) => {
                totals.failures.push(format!("{}: {error}", path.display()));
                println!("  {:<40} FAILED: {error}", name_of(&path));
                continue;
            }
        };

        totals.archives_ok += 1;
        totals.entries += archive.entries().len();
        totals.bytes += archive.file_len();
        if u64::from(archive.declared_size()) == archive.file_len() {
            totals.size_agreed += 1;
        } else {
            totals.failures.push(format!(
                "{}: header declares {} bytes, file is {}",
                path.display(),
                archive.declared_size(),
                archive.file_len()
            ));
        }
        if archive.entry_table_end() <= u64::from(archive.data_offset()) {
            totals.table_fits += 1;
        } else {
            totals.failures.push(format!(
                "{}: entry table ends at {}, data starts at {}",
                path.display(),
                archive.entry_table_end(),
                archive.data_offset()
            ));
        }

        let listed = archive.entries();
        let probes = [listed.first(), listed.get(listed.len() / 2), listed.last()];
        for entry in probes.into_iter().flatten() {
            totals.reads += 1;
            match archive.read_entry(entry) {
                Ok(bytes) if bytes.len() as u64 == u64::from(entry.size) => {
                    totals.reads_agreed += 1;
                }
                Ok(bytes) => totals.failures.push(format!(
                    "{} :: {}: read {} bytes, directory says {}",
                    path.display(),
                    entry.name,
                    bytes.len(),
                    entry.size
                )),
                Err(error) => {
                    totals
                        .failures
                        .push(format!("{} :: {}: {error}", path.display(), entry.name))
                }
            }
        }

        let count = archive.entries().len();
        let format = archive.format();
        let report = vfs.mount(Arc::new(archive));
        totals.shadowed += report.shadowed;
        println!(
            "  {:<40} {format} {count:>7} entries, {:>5} shadowed",
            name_of(&path),
            report.shadowed
        );
    }

    let data = root.join("data");
    if data.is_dir() {
        match DirMount::with_prefix(&data, "data") {
            Ok(mount) => {
                let report = vfs.mount(Arc::new(mount));
                totals.shadowed += report.shadowed;
                println!(
                    "  {:<40} {:>12} loose files, {:>5} shadowed",
                    "data/", report.listed, report.shadowed
                );
            }
            Err(error) => {
                totals.failures.push(format!("{}: {error}", data.display()));
            }
        }
    }

    for probe in [
        r"data\ini\gamedata.ini",
        r"data\ini\object\goodfaction\units\men\meninfantry.ini",
        r"art\w3d\ai\aicamp_skn.w3d",
    ] {
        let path = VfsPath::new(probe);
        match vfs.origin(probe) {
            Some(mount) => println!("    {path:<62} <- {}", name_of(Path::new(mount.label()))),
            None => println!("    {path:<62} <- not found"),
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

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
