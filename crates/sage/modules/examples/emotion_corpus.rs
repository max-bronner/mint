use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{Block, BlockTable, EmotionIndex, MemorySources, Reader, ValueErrors};
use mint_sage_modules::TemplateIndex;

const INI_ROOT: &str = "data/ini/";
const TRACKER: &str = "EmotionTrackerUpdate";
const MAX_LISTED: usize = 20;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: emotion_corpus <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last, so its archives shadow BFME II's");
        return ExitCode::FAILURE;
    }

    let mut sources = MemorySources::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
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
    }
    if sources.is_empty() {
        eprintln!("no data/ini entries found under any given install - wrong path?");
        return ExitCode::FAILURE;
    }

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents = Vec::new();
    for path in &paths {
        if let Ok(doc) = reader.read(path, &table) {
            documents.push(doc);
        }
    }
    mint_sage_ini::expand_macros(&mut documents);

    let nugget_errors = ValueErrors::new();
    let mut emotions = EmotionIndex::new();
    for doc in &documents {
        emotions.insert_document(doc, &nugget_errors);
    }
    println!(
        "{} EmotionNugget definition(s), {} refused value(s)",
        emotions.len(),
        nugget_errors.len()
    );
    for error in nugget_errors.take().iter().take(MAX_LISTED) {
        println!("  refused: {error}");
    }

    let mut index = TemplateIndex::new();
    for doc in &documents {
        index.insert_document(doc);
    }
    let names: Vec<String> = index.names().map(str::to_owned).collect();

    let tracker_errors = ValueErrors::new();
    let mut trackers = 0usize;
    let mut nuggets = 0usize;
    let mut overriding: Vec<String> = Vec::new();
    for name in &names {
        let mut wrote_override = false;
        for declaration in index.declared_modules(name) {
            if !declaration.name.eq_ignore_ascii_case(TRACKER) {
                continue;
            }
            trackers += 1;
            wrote_override |= writes_override(declaration.block);
            nuggets += emotions
                .compose_tracker(declaration.block, &tracker_errors)
                .len();
        }
        if wrote_override {
            overriding.push(name.clone());
        }
    }
    println!(
        "{trackers} composed EmotionTrackerUpdate declaration(s) over {} template(s), \
         {nuggets} nugget(s) resolved, {} refused value(s)",
        names.len(),
        tracker_errors.len()
    );
    for error in tracker_errors.take().iter().take(MAX_LISTED) {
        println!("  refused: {error}");
    }

    println!("\n-- Q491: AddEmotion = OVERRIDE, composed lists --");
    println!("{} template(s) write it", overriding.len());
    let quiet = ValueErrors::new();
    for name in &overriding {
        let lineage = index.lineage(name);
        println!(
            "{name} (lineage {}):",
            lineage
                .iter()
                .filter_map(|block| block.name())
                .collect::<Vec<_>>()
                .join(" <- ")
        );
        for declaration in index.declared_modules(name) {
            if !declaration.name.eq_ignore_ascii_case(TRACKER) {
                continue;
            }
            let composed = emotions.compose_tracker(declaration.block, &quiet);
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut duplicated: Vec<String> = Vec::new();
            for def in &composed {
                let Some(kind) = def.emotion_type else {
                    continue;
                };
                if !seen.insert(kind.as_str().to_owned()) {
                    duplicated.push(kind.as_str().to_owned());
                }
            }
            println!(
                "  {} nugget(s), {} distinct Type(s){}",
                composed.len(),
                seen.len(),
                if duplicated.is_empty() {
                    String::new()
                } else {
                    format!(", repeated: {}", duplicated.join(" "))
                }
            );
        }
    }
    let _ = quiet.take();

    ExitCode::SUCCESS
}

fn writes_override(block: &Block) -> bool {
    block
        .blocks_named(mint_sage_ini::ADD_EMOTION_KEYWORD)
        .any(|child| {
            child
                .args
                .first()
                .is_some_and(|token| token.eq_ignore_ascii_case(mint_sage_ini::OVERRIDE_TOKEN))
        })
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
