use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, LinearCampaignIndex, MemorySources, Reader, Sources, VideoIndex};

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: linear_campaigns <install dir> [<later install dir> ...]");
        return ExitCode::FAILURE;
    }
    let archives: Vec<PathBuf> = roots.iter().flat_map(|root| big_archives(root)).collect();
    let sources = ini_sources(&archives);
    let tops = top_level_paths(&sources);
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let documents: Vec<_> = tops
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .collect();
    println!(
        "{} of {} top-level documents read",
        documents.len(),
        tops.len()
    );
    let mut campaigns = LinearCampaignIndex::new();
    let mut videos = VideoIndex::new();
    for document in &documents {
        campaigns.insert_document(document);
        videos.insert_document(document);
    }
    let missions: usize = campaigns.iter().map(|c| c.missions.len()).sum();
    println!(
        "{} campaign(s), {missions} mission(s), {} value error(s); {} Video(s), {} value error(s)",
        campaigns.len(),
        campaigns.errors().len(),
        videos.len(),
        videos.errors().len()
    );
    let (mut movies, mut resolved) = (0usize, 0usize);
    for campaign in campaigns.iter() {
        println!(
            "{}: {} mission(s), {} carryover unit(s)",
            campaign.name,
            campaign.missions.len(),
            campaign.carryover_units.len()
        );
        for mission in &campaign.missions {
            println!(
                "  {} map={} fade_up={} (set: {})",
                mission.name,
                mission.map.as_deref().unwrap_or("-"),
                mission.fade_up(),
                mission.fade_up_frames.is_some()
            );
            if let Some(movie) = &mission.intro_movie {
                movies = movies.saturating_add(1);
                if videos.get(movie).is_some() {
                    resolved = resolved.saturating_add(1);
                } else {
                    println!("    unresolved IntroMovie {movie}");
                }
            }
        }
    }
    println!("IntroMovie: {resolved} of {movies} resolve to a Video");
    ExitCode::SUCCESS
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

fn ini_sources(archives: &[PathBuf]) -> MemorySources {
    let mut sources = MemorySources::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            if !entry.path.as_str().starts_with(INI_ROOT) {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                continue;
            };
            sources.insert(entry.path.as_str(), bytes);
        }
    }
    sources
}

fn top_level_paths(sources: &MemorySources) -> Vec<String> {
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut included: BTreeSet<String> = BTreeSet::new();
    for path in &paths {
        let Some(bytes) = Sources::read(sources, path) else {
            continue;
        };
        for item in mint_sage_ini::lex::lex(&mint_sage_ini::lex::decode_latin1(&bytes)) {
            if let mint_sage_ini::lex::Lexed::Directive {
                directive: mint_sage_ini::lex::Directive::Include(target),
                ..
            } = item
            {
                included.insert(mint_sage_ini::resolve_include(path, target));
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !included.contains(path))
        .collect()
}
