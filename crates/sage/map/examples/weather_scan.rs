use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, Value};

#[derive(Debug, Default, Clone)]
struct Seen {
    archive: String,
    weather: Option<i32>,
    weather_ini: BTreeMap<String, String>,
    has_map_ini: bool,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: weather_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    let mut maps: BTreeMap<String, Seen> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut files = 0u64;
    let mut inis: BTreeMap<String, String> = BTreeMap::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('/', "\\");
                if let Some(folder) = lower.strip_suffix("\\map.ini") {
                    if let Ok(bytes) = archive.read_entry(entry) {
                        inis.insert(
                            folder.to_owned(),
                            String::from_utf8_lossy(&bytes).into_owned(),
                        );
                    }
                }
            }
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('/', "\\");
                if !lower.ends_with(".map") {
                    continue;
                }
                files = files.saturating_add(1);
                let name = format!("{label}::{}", entry.name);
                let Ok(raw) = archive.read_entry(entry) else {
                    failures.push(format!("{name}: unreadable BIG entry"));
                    continue;
                };
                let Some(stream) = mint_sage_refpack::strip_ear_wrapper(&raw) else {
                    failures.push(format!("{name}: no EAR\\0 wrapper"));
                    continue;
                };
                let body = match mint_sage_refpack::decompress(stream) {
                    Ok(body) => body,
                    Err(e) => {
                        failures.push(format!("{name}: decompress: {e}"));
                        continue;
                    }
                };
                let map = match MapFile::parse_body(body) {
                    Ok(map) => map,
                    Err(e) => {
                        failures.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                let weather = match map.world_info.properties.get("weather") {
                    Some(Value::Int(value)) => Some(*value),
                    _ => None,
                };
                maps.insert(
                    lower.clone(),
                    Seen {
                        archive: label.clone(),
                        weather,
                        weather_ini: BTreeMap::new(),
                        has_map_ini: false,
                    },
                );
            }
        }
    }
    for (path, seen) in &mut maps {
        let folder = path
            .rsplit_once('\\')
            .map_or(String::new(), |(folder, _)| folder.to_owned());
        if let Some(text) = inis.get(&folder) {
            seen.has_map_ini = true;
            seen.weather_ini = weather_block(text);
        }
    }

    let mut by_value: BTreeMap<Option<i32>, usize> = BTreeMap::new();
    for seen in maps.values() {
        let slot = by_value.entry(seen.weather).or_insert(0);
        *slot = slot.saturating_add(1);
    }
    println!(
        "weather: {files} .map entries read, {} unique after shadowing, {} failed",
        maps.len(),
        failures.len()
    );
    for (value, count) in &by_value {
        println!("  weather = {value:?}: {count}");
    }
    let with_ini = maps.values().filter(|seen| seen.has_map_ini).count();
    let with_block = maps
        .values()
        .filter(|seen| !seen.weather_ini.is_empty())
        .count();
    println!("  map.ini beside the map: {with_ini}; with a Weather block: {with_block}");
    let mut keys: BTreeMap<String, usize> = BTreeMap::new();
    for seen in maps.values() {
        for key in seen.weather_ini.keys() {
            let slot = keys.entry(key.clone()).or_insert(0);
            *slot = slot.saturating_add(1);
        }
    }
    for (key, count) in &keys {
        println!("  map.ini Weather key {key}: {count}");
    }
    println!("snowy maps (weather = 1):");
    for (path, seen) in maps.iter().filter(|(_, seen)| seen.weather == Some(1)) {
        let snow: Vec<String> = seen
            .weather_ini
            .iter()
            .filter(|(key, _)| key.contains("snow"))
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        println!(
            "  {path} [{}] map.ini snow keys: {}",
            seen.archive,
            if snow.is_empty() {
                "none".to_owned()
            } else {
                snow.join(" ")
            }
        );
    }
    println!("maps whose map.ini names a snow key, any weather:");
    for (path, seen) in &maps {
        let snow: Vec<String> = seen
            .weather_ini
            .iter()
            .filter(|(key, _)| key.contains("snow"))
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        if !snow.is_empty() {
            println!("  {path} weather={:?}: {}", seen.weather, snow.join(" "));
        }
    }
    for failure in &failures {
        println!("FAIL {failure}");
    }
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn weather_block(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut inside = false;
    for raw in text.lines() {
        let line = raw.split(';').next().unwrap_or("");
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if !inside {
            if line.eq_ignore_ascii_case("Weather") {
                inside = true;
            }
            continue;
        }
        if line.eq_ignore_ascii_case("End") {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            out.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    out
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
