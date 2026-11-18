use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, Value};

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            roots.push(PathBuf::from(corpus));
        }
    }
    if roots.is_empty() {
        eprintln!("usage: sides_scan <install dir> [...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }
    let mut files = 0u64;
    let mut failures: Vec<String> = Vec::new();
    let mut keys: BTreeMap<String, u64> = BTreeMap::new();
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
                let Ok(map) = MapFile::parse(&raw) else {
                    continue;
                };
                files = files.saturating_add(1);
                let sides = match map.sides() {
                    Ok(sides) => sides,
                    Err(error) => {
                        failures.push(format!("{}: {error}", entry.name));
                        continue;
                    }
                };
                for side in &sides {
                    for (key, value) in side.properties.iter() {
                        let kind = match value {
                            Value::Bool(_) => "bool",
                            Value::Int(_) => "int",
                            Value::Float(_) => "float",
                            Value::Str(_) => "str",
                            Value::WideStr(_) => "wide",
                        };
                        *keys.entry(format!("{key}:{kind}")).or_default() += 1;
                    }
                }
                if lower.starts_with("maps\\") || lower.starts_with("maps/") {
                    println!("{}", entry.name);
                    let mut owners: BTreeMap<String, u64> = BTreeMap::new();
                    for object in &map.objects {
                        let owner = match object.properties.get("originalOwner") {
                            Some(Value::Str(s)) => s.clone(),
                            Some(other) => format!("{other:?}"),
                            None => String::from("<none>"),
                        };
                        *owners.entry(owner).or_default() += 1;
                    }
                    println!("  owners {owners:?}");
                    for side in &sides {
                        println!(
                            "  {:?} faction {:?} human {:?} allies {:?} enemies {:?}",
                            side.name(),
                            side.text("playerFaction"),
                            side.properties.get("playerIsHuman"),
                            side.text("playerAllies"),
                            side.text("playerEnemies"),
                        );
                    }
                }
            }
        }
    }
    println!("{files} file(s); {} SidesList failure(s)", failures.len());
    for failure in &failures {
        println!("  FAIL {failure}");
    }
    for (key, count) in &keys {
        println!("  key {key} x{count}");
    }
    ExitCode::SUCCESS
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
