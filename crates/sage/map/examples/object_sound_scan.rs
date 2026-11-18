use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, Value};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: object_sound_scan <install dir> [...]");
        return ExitCode::FAILURE;
    }
    let mut files = 0u64;
    let mut objects = 0u64;
    let mut keys: BTreeMap<String, u64> = BTreeMap::new();
    let mut values: BTreeMap<String, u64> = BTreeMap::new();
    let mut templates: BTreeMap<String, u64> = BTreeMap::new();
    let mut template_maps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !lower.ends_with(".map") {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&raw) else {
                    continue;
                };
                files = files.saturating_add(1);
                for object in &map.objects {
                    let mut any = false;
                    for (key, value) in object.properties.iter() {
                        if !key.to_ascii_lowercase().starts_with("objectsound") {
                            continue;
                        }
                        any = true;
                        *keys.entry(key.to_string()).or_default() += 1;
                        let shown = match value {
                            Value::Bool(b) => format!("{b}"),
                            Value::Int(i) => format!("{i}"),
                            Value::Float(f) => format!("{f}"),
                            Value::Str(s) => format!("{s:?}"),
                            Value::WideStr(s) => format!("{s:?}"),
                        };
                        *values.entry(format!("{key} = {shown}")).or_default() += 1;
                    }
                    if any {
                        objects += 1;
                        *templates.entry(object.template_name.clone()).or_default() += 1;
                        template_maps
                            .entry(object.template_name.clone())
                            .or_default()
                            .insert(entry.name.clone());
                    }
                }
            }
        }
    }
    println!("{files} maps, {objects} objects with objectSound* keys");
    for (key, count) in &keys {
        println!("key {key}: {count}");
    }
    for (value, count) in &values {
        println!("value {value}: {count}");
    }
    for (template, count) in &templates {
        let maps: Vec<&str> = template_maps
            .get(template)
            .into_iter()
            .flatten()
            .take(3)
            .map(String::as_str)
            .collect();
        println!("template {template}: {count} ({})", maps.join(", "));
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
