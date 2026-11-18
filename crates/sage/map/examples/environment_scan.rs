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
        eprintln!("usage: environment_scan <install dir> [...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }

    let mut files = 0u64;
    let mut with_chunk = 0u64;
    let mut versions: BTreeMap<u16, u64> = BTreeMap::new();
    let mut depths: BTreeMap<String, u64> = BTreeMap::new();
    let mut alphas: BTreeMap<String, u64> = BTreeMap::new();
    let mut pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut stretched = 0u64;
    let mut macros: BTreeMap<String, u64> = BTreeMap::new();
    let mut clouds: BTreeMap<String, u64> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    const MODAL: (Option<f32>, Option<f32>) = (Some(3.0), Some(1.0));
    let mut away_from_modal = 0u64;
    let mut named: BTreeMap<String, String> = BTreeMap::new();

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
                let Some(env) = &map.environment else {
                    continue;
                };
                with_chunk = with_chunk.saturating_add(1);
                *versions.entry(env.version).or_default() += 1;
                let depth = format!("{:?}", env.water_max_alpha_depth);
                let alpha = format!("{:?}", env.deep_water_alpha);
                *pairs.entry(format!("{depth} / {alpha}")).or_default() += 1;
                *depths.entry(depth).or_default() += 1;
                *alphas.entry(alpha).or_default() += 1;
                if env.is_macro_texture_stretched {
                    stretched = stretched.saturating_add(1);
                }
                *macros.entry(env.macro_texture.clone()).or_default() += 1;
                *clouds.entry(env.cloud_texture.clone()).or_default() += 1;
                if (env.water_max_alpha_depth, env.deep_water_alpha) != MODAL {
                    away_from_modal = away_from_modal.saturating_add(1);
                }
                for wanted in [
                    "wor rhun",
                    "wor ithilien",
                    "wor mount doom",
                    "wor ettenmoors",
                ] {
                    if lower.contains(wanted) {
                        named.insert(
                            wanted.to_owned(),
                            format!(
                                "depth {:?} alpha {:?} macro {} cloud {}",
                                env.water_max_alpha_depth,
                                env.deep_water_alpha,
                                env.macro_texture,
                                env.cloud_texture
                            ),
                        );
                    }
                }
            }
        }
    }

    println!("{files} map file(s) walked, {with_chunk} carry EnvironmentData");
    println!("versions: {versions:?}");
    println!("waterMaxAlphaDepth / deepWaterAlpha, by file count:");
    for (value, count) in &pairs {
        println!("  {value:<24} {count}");
    }
    println!("distinct waterMaxAlphaDepth values: {}", depths.len());
    println!("distinct deepWaterAlpha values:     {}", alphas.len());
    println!("{stretched} of {with_chunk} stretch the macro texture");
    println!(
        "{} distinct macro textures, {} distinct cloud textures",
        macros.len(),
        clouds.len()
    );
    println!(
        "{away_from_modal} of {with_chunk} differ from the modal pair {MODAL:?}, over {} distinct pairs",
        pairs.len()
    );
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
