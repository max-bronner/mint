use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            roots.push(PathBuf::from(corpus));
        }
    }
    if roots.is_empty() {
        eprintln!("usage: skybox_scan <install dir> [...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }

    let mut files = 0u64;
    let mut with_chunk = 0u64;
    let mut with_object = 0u64;
    let mut with_neither = 0u64;
    let mut schemes: BTreeMap<String, u64> = BTreeMap::new();
    let mut positions: BTreeMap<String, u64> = BTreeMap::new();
    let mut scales: BTreeMap<String, u64> = BTreeMap::new();
    let mut rotations: BTreeMap<String, u64> = BTreeMap::new();
    let mut templates: BTreeMap<String, u64> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    let mut placing: Vec<String> = Vec::new();

    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                files = files.saturating_add(1);
                let map = match MapFile::parse(&raw) {
                    Ok(map) => map,
                    Err(error) => {
                        failures.push(format!("{}: {error}", entry.name));
                        continue;
                    }
                };

                let mut placed: Vec<String> = Vec::new();
                for object in &map.objects {
                    if object.template_name.to_ascii_lowercase().contains("skybox") {
                        *templates.entry(object.template_name.clone()).or_default() += 1;
                        placed.push(object.template_name.clone());
                    }
                }
                if !placed.is_empty() {
                    with_object = with_object.saturating_add(1);
                    placing.push(format!("{} -> {placed:?}", entry.name));
                }

                if let Some(sky) = &map.skybox {
                    with_chunk = with_chunk.saturating_add(1);
                    *schemes.entry(sky.texture_scheme.clone()).or_default() += 1;
                    *positions
                        .entry(format!("{:.1},{:.1},{:.1}", sky.x, sky.y, sky.z))
                        .or_default() += 1;
                    *scales.entry(format!("{:.3}", sky.scale)).or_default() += 1;
                    *rotations.entry(format!("{:.3}", sky.rotation)).or_default() += 1;
                } else if placed.is_empty() {
                    with_neither = with_neither.saturating_add(1);
                }

                for wanted in ["wor celduin", "wor rhun", "good rivendell", "wor ithilien"] {
                    if lower.contains(wanted) {
                        named.insert(
                            wanted.to_owned(),
                            format!(
                                "chunk {:?}, {} placed skybox object(s) {:?}",
                                map.skybox.as_ref().map(|s| format!(
                                    "{} at {:.0},{:.0},{:.0} scale {} rot {}",
                                    s.texture_scheme, s.x, s.y, s.z, s.scale, s.rotation
                                )),
                                placed.len(),
                                placed
                            ),
                        );
                    }
                }
            }
        }
    }

    println!("{files} map file(s) walked");
    println!("  {with_chunk} carry SkyboxSettings");
    println!("  {with_object} place at least one object whose template names a skybox");
    println!("  {with_neither} carry neither");
    println!("texture schemes named by the chunk:");
    for (value, count) in &schemes {
        println!("  {value:<24} {count}");
    }
    println!("distinct chunk positions: {}", positions.len());
    for (value, count) in positions.iter().take(8) {
        println!("  {value:<24} {count}");
    }
    println!("scales: {scales:?}");
    println!("rotations: {rotations:?}");
    placing.sort();
    placing.dedup();
    println!("{} file(s) placing a skybox object:", placing.len());
    for line in &placing {
        println!("  {line}");
    }
    println!("{} distinct skybox object templates:", templates.len());
    for (value, count) in &templates {
        println!("  {value:<40} {count}");
    }
    for (name, line) in &named {
        println!("  {name}: {line}");
    }

    if failures.is_empty() {
        println!("0 parse failures");
        ExitCode::SUCCESS
    } else {
        println!("{} parse failure(s):", failures.len());
        for f in &failures {
            println!("  {f}");
        }
        ExitCode::FAILURE
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
