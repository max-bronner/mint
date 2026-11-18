use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use std::sync::Arc;

use mint_sage_apt::Character;
use mint_sage_apt_player::{Library, Limits, Player, RenderCommand, RenderList, Value};
use mint_sage_apt_viewer::{load_string_table, localises, MovieLibrary};
use mint_sage_big::BigArchive;
use mint_sage_str::StringTable;

const MAX_IMPORT_HOPS: u32 = 4;
const SETTLE_TICKS: u32 = 60;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: labels <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    for root in &roots {
        println!("== {} ==", root.display());
        sweep(root);
        println!();
    }
    ExitCode::SUCCESS
}

#[derive(Default)]
struct Totals {
    movies: usize,
    characters: usize,
    with_literal: usize,
    with_variable: usize,
    with_variable_present: usize,
    without_literal: usize,
    with_neither: usize,
    dollar_literals: usize,
    dollar_literals_resolving: usize,
    placements: usize,
    bound: usize,
    bound_unresolved: usize,
    no_content: usize,
    key_resolved: usize,
    key_missing: usize,
    plain_text: usize,
}

fn sweep(root: &Path) {
    let started = Instant::now();
    let (table, files, failures) = load_string_table(root);
    println!(
        "  localisation: {} labels from {files} .str file(s), {} duplicate(s) dropped, \
         {failures} file(s) failed",
        table.len(),
        table.duplicates()
    );
    if table.is_empty() {
        println!("  no string table found - every $key will report as missing");
    }

    let mut library = MovieLibrary::new(root);
    let names = library.list_available();
    let player_library = load_player_library(root, &names);
    let mut totals = Totals::default();
    let mut missing_keys: BTreeSet<String> = BTreeSet::new();
    let mut unresolved_paths: BTreeMap<String, usize> = BTreeMap::new();
    let mut worst: Vec<(usize, String)> = Vec::new();

    for name in &names {
        if library
            .load_imports_transitively(name, MAX_IMPORT_HOPS)
            .is_err()
        {
            println!("  {name}: failed to load");
            continue;
        }
        totals.movies = totals.movies.saturating_add(1);
        let movie_set = library.movie_set();

        if let Some(movie) = movie_set.get(name) {
            for character in &movie.characters {
                let Character::EditText { text, variable, .. } = character else {
                    continue;
                };
                totals.characters = totals.characters.saturating_add(1);
                let literal = text.as_deref().filter(|t| !t.is_empty());
                if literal.is_some() {
                    totals.with_literal = totals.with_literal.saturating_add(1);
                }
                if literal.is_none() {
                    totals.without_literal = totals.without_literal.saturating_add(1);
                }
                if variable.is_some() {
                    totals.with_variable_present = totals.with_variable_present.saturating_add(1);
                }
                if variable.as_deref().is_some_and(|v| !v.is_empty()) {
                    totals.with_variable = totals.with_variable.saturating_add(1);
                }
                if literal.is_none() && variable.as_deref().is_none_or(str::is_empty) {
                    totals.with_neither = totals.with_neither.saturating_add(1);
                }
                if let Some(key) = literal.filter(|t| t.starts_with('$')) {
                    totals.dollar_literals = totals.dollar_literals.saturating_add(1);
                    if localises(key, &table) {
                        totals.dollar_literals_resolving =
                            totals.dollar_literals_resolving.saturating_add(1);
                    } else {
                        missing_keys.insert(key.to_owned());
                    }
                }
            }
        }

        let before = totals.placements;
        let unresolved_before = totals.bound_unresolved.saturating_add(totals.no_content);
        walk_movie(
            &player_library,
            name,
            &table,
            &mut totals,
            &mut missing_keys,
            &mut unresolved_paths,
        );
        let bad = totals
            .bound_unresolved
            .saturating_add(totals.no_content)
            .saturating_sub(unresolved_before);
        if bad > 0 {
            worst.push((
                bad,
                format!(
                    "{name} ({} placements)",
                    totals.placements.saturating_sub(before)
                ),
            ));
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!("  {} movies walked in {elapsed:.1}s", totals.movies);
    println!(
        "  per character: {} EditText, {} with literal text ({} without), {} carry a `variable` \
         field at all, {} of those are non-empty, {} have neither literal text nor a non-empty \
         binding",
        totals.characters,
        totals.with_literal,
        totals.without_literal,
        totals.with_variable_present,
        totals.with_variable,
        totals.with_neither
    );
    println!(
        "  per character: {} literals are a $key, {} of them resolve against the string table",
        totals.dollar_literals, totals.dollar_literals_resolving
    );
    println!(
        "  per placement: {} EditText drawn after {SETTLE_TICKS} ticks - {} bindings resolved, \
         {} unresolved; {} with no content at all",
        totals.placements, totals.bound, totals.bound_unresolved, totals.no_content
    );
    println!(
        "  per placement: {} drew a localised string, {} drew MISSING (key absent), \
         {} drew plain text (no $ prefix)",
        totals.key_resolved, totals.key_missing, totals.plain_text
    );

    worst.sort_by(|a, b| b.0.cmp(&a.0));
    if !worst.is_empty() {
        println!("  movies with the most unresolved placements:");
        for (count, name) in worst.iter().take(10) {
            println!("    {count:4}  {name}");
        }
    }
    if !unresolved_paths.is_empty() {
        let mut paths: Vec<(&String, &usize)> = unresolved_paths.iter().collect();
        paths.sort_by(|a, b| b.1.cmp(a.1));
        println!("  most common unresolved binding paths:");
        for (path, count) in paths.iter().take(10) {
            println!("    {count:4}  {path}");
        }
    }
    if !missing_keys.is_empty() {
        println!(
            "  {} distinct $keys have no entry in the string table; the first 20:",
            missing_keys.len()
        );
        for key in missing_keys.iter().take(20) {
            println!("    {key}");
        }
    }
}

fn load_player_library(root: &Path, names: &[String]) -> Arc<Library> {
    let mut library = Library::new();
    for name in names {
        let Ok(archive) = BigArchive::open(root.join("apt").join(format!("{name}.big"))) else {
            continue;
        };
        let mut apt = None;
        let mut constant = None;
        for entry in archive.entries() {
            let lower = entry.path.as_str().to_ascii_lowercase();
            if lower.ends_with(".apt") {
                apt = archive.read_entry(entry).ok();
            } else if lower.ends_with(".const") {
                constant = archive.read_entry(entry).ok();
            }
        }
        if let (Some(apt), Some(constant)) = (apt, constant) {
            let _ = library.add(name, &constant, apt);
        }
    }
    library.link();
    Arc::new(library)
}

fn walk_movie(
    library: &Arc<Library>,
    name: &str,
    table: &StringTable,
    totals: &mut Totals,
    missing_keys: &mut BTreeSet<String>,
    unresolved_paths: &mut BTreeMap<String, usize>,
) {
    let Some(id) = library.movie_id(name) else {
        return;
    };
    let Ok(mut player) = Player::new(Arc::clone(library), id, Limits::default()) else {
        return;
    };
    for _ in 0..SETTLE_TICKS {
        let _ = player.tick();
    }
    let mut list = RenderList::default();
    player.render(&mut list);
    for command in &list.commands {
        let RenderCommand::EditText {
            instance,
            character,
            ..
        } = *command
        else {
            continue;
        };
        let Some(Character::EditText { variable, .. }) = library.character(character) else {
            continue;
        };
        totals.placements = totals.placements.saturating_add(1);
        if let Some(path) = variable.as_deref().filter(|p| !p.is_empty()) {
            let parent = player.instance(instance).and_then(|i| i.parent());
            let value = parent.map_or(Value::Undefined, |p| player.variable(p, path));
            if matches!(value, Value::Undefined) {
                totals.bound_unresolved = totals.bound_unresolved.saturating_add(1);
                *unresolved_paths.entry(path.to_owned()).or_default() += 1;
            } else {
                totals.bound = totals.bound.saturating_add(1);
            }
        }
        let raw = player.text(instance).unwrap_or_default();
        if raw.is_empty() {
            totals.no_content = totals.no_content.saturating_add(1);
            continue;
        }
        if raw.starts_with('$') {
            if localises(&raw, table) {
                totals.key_resolved = totals.key_resolved.saturating_add(1);
            } else {
                totals.key_missing = totals.key_missing.saturating_add(1);
                missing_keys.insert(raw);
            }
        } else {
            totals.plain_text = totals.plain_text.saturating_add(1);
        }
    }
}
