use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::living_world::{load_living_world, resolve, ExternalNames};
use mint_sage_ini::MemorySources;

const INI_ROOT: &str = "data/ini/";
const MAX_LISTED: usize = 40;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: living_world <install dir> [<later install dir> ...]");
        return ExitCode::FAILURE;
    }
    let archives: Vec<PathBuf> = roots.iter().flat_map(|root| big_archives(root)).collect();
    let (sources, maps) = read_archives(&archives);
    let load = load_living_world(&sources);
    println!(
        "{} documents parsed, {} hold Living World blocks, {} unreadable",
        load.documents,
        load.living_world_documents,
        load.unreadable.len()
    );
    for (path, error) in &load.unreadable {
        println!("  unreadable {path}: {error}");
    }
    println!("{:<52} {:>6} {:>6}", "block", "read", "held");
    for count in load.data.counts() {
        println!("{:<52} {:>6} {:>6}", count.keyword, count.read, count.held);
    }
    let regions: usize = load
        .data
        .region_campaigns
        .iter()
        .map(|c| c.regions.len())
        .sum();
    let bonuses: usize = load
        .data
        .region_campaigns
        .iter()
        .map(|c| c.concurrent_bonuses.len())
        .sum();
    println!("{:<52} {:>6}", "Region (all region campaigns)", regions);
    println!("{:<52} {:>6}", "ConcurrentRegionBonus", bonuses);
    for campaign in load.data.region_campaigns.iter() {
        let graph = campaign.graph();
        let mapped = campaign
            .regions
            .iter()
            .filter(|r| {
                r.map_name
                    .as_deref()
                    .is_some_and(|m| maps.contains(&m.to_ascii_lowercase()))
            })
            .count();
        println!(
            "region campaign {}: {} regions, {} directed edges, {} asymmetric, {} of {} MapName resolve",
            campaign.name, graph.regions, graph.edges, graph.asymmetric, mapped, graph.regions
        );
    }
    let victory_types: Vec<usize> = load
        .data
        .campaigns
        .iter()
        .map(|c| c.victory_types.len())
        .collect();
    println!("victory types per campaign: {victory_types:?}");

    let external = ExternalNames {
        objects: &load.object_names,
        player_templates: &load.player_template_names,
        maps: Some(&maps),
    };
    let unresolved = resolve(&load.data, &external);
    println!("errors: {}", load.errors.len());
    let distinct: BTreeSet<String> = load.errors.iter().map(ToString::to_string).collect();
    for error in distinct.iter().take(MAX_LISTED) {
        println!("  {error}");
    }
    println!("unresolved: {}", unresolved.len());
    for item in unresolved.iter().take(MAX_LISTED) {
        println!(
            "  {} {}: {} = {} is not a {}",
            item.block, item.name, item.field, item.missing, item.target
        );
    }
    if load.errors.is_empty() && unresolved.is_empty() {
        ExitCode::SUCCESS
    } else {
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

fn read_archives(archives: &[PathBuf]) -> (MemorySources, BTreeSet<String>) {
    let mut sources = MemorySources::new();
    let mut maps = BTreeSet::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            let name = entry.path.as_str();
            if name.starts_with(INI_ROOT) {
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(name, bytes);
                }
            } else if let Some(rest) = name.strip_prefix("maps/") {
                let lower = rest.to_ascii_lowercase();
                if let Some((dir, file)) = lower.split_once('/') {
                    if file.strip_suffix(".map") == Some(dir) {
                        maps.insert(dir.to_owned());
                    }
                }
            }
        }
    }
    (sources, maps)
}
