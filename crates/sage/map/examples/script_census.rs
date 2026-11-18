use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{initial_int_values, MapFile, ScriptNode};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: script_census <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut overlay: BTreeMap<String, (PathBuf, mint_sage_big::BigEntry)> = BTreeMap::new();
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('\\', "/");
                if lower.contains("libraries/") && lower.ends_with(".map") {
                    overlay.insert(lower, (archive_path.clone(), entry.clone()));
                }
            }
        }
    }

    let mut files_ok = 0u64;
    let mut files_failed: Vec<String> = Vec::new();
    let mut scripts_seen = 0u64;
    let mut conditions_seen = 0u64;
    let mut actions_seen = 0u64;
    let mut condition_names: BTreeMap<String, u64> = BTreeMap::new();
    let mut action_names: BTreeMap<String, u64> = BTreeMap::new();
    let mut initial_values: BTreeMap<String, i32> = BTreeMap::new();

    for (name, (archive_path, entry)) in &overlay {
        let Ok(archive) = BigArchive::open(archive_path) else {
            continue;
        };
        let Ok(bytes) = archive.read_entry(entry) else {
            continue;
        };
        let map = match MapFile::parse(&bytes) {
            Ok(map) => map,
            Err(error) => {
                files_failed.push(format!("{name}: container parse failed: {error}"));
                continue;
            }
        };
        let Some(tree) = &map.scripts else {
            files_ok = files_ok.saturating_add(1);
            continue;
        };
        files_ok = files_ok.saturating_add(1);
        for slot in &tree.player_slots {
            census(
                slot,
                &mut scripts_seen,
                &mut conditions_seen,
                &mut actions_seen,
                &mut condition_names,
                &mut action_names,
            );
        }
        if name.contains("ai_initialize") {
            for (key, value) in initial_int_values(tree, &["SET_COUNTER", "SET_FLAG"]) {
                initial_values.insert(key, value);
            }
        }
        if name.contains("dl3_attack") || name.contains("dl4_attack") {
            let values = initial_int_values(tree, &["SET_COUNTER", "SET_FLAG"]);
            println!(
                "{name}: {} SET_COUNTER/SET_FLAG value(s): {values:?}",
                values.len()
            );
        }
    }

    println!(
        "{} library .map file(s) found across {} root(s)",
        overlay.len(),
        roots.len()
    );
    println!(
        "{files_ok} of {} decoded to their exact declared end, {} failed",
        overlay.len(),
        files_failed.len()
    );
    for failure in &files_failed {
        println!("  FAILED: {failure}");
    }
    println!("{scripts_seen} script(s), {conditions_seen} condition(s), {actions_seen} action(s)");
    println!(
        "{} distinct condition name(s), {} distinct action name(s)",
        condition_names.len(),
        action_names.len()
    );
    println!("most common conditions:");
    for (name, count) in top(&condition_names, 15) {
        println!("  {count:>6}  {name}");
    }
    println!("most common actions:");
    for (name, count) in top(&action_names, 15) {
        println!("  {count:>6}  {name}");
    }
    println!(
        "ai_initialize.map SET_COUNTER/SET_FLAG named values ({}):",
        initial_values.len()
    );
    for (name, value) in &initial_values {
        println!("  {name} = {value}");
    }

    if files_failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn census(
    nodes: &[ScriptNode],
    scripts: &mut u64,
    conditions: &mut u64,
    actions: &mut u64,
    condition_names: &mut BTreeMap<String, u64>,
    action_names: &mut BTreeMap<String, u64>,
) {
    for node in nodes {
        match node {
            ScriptNode::Group(group) => census(
                &group.children,
                scripts,
                conditions,
                actions,
                condition_names,
                action_names,
            ),
            ScriptNode::Script(script) => {
                *scripts += 1;
                for or_condition in &script.or_conditions {
                    for condition in &or_condition.conditions {
                        *conditions += 1;
                        *condition_names.entry(condition.name.clone()).or_default() += 1;
                    }
                }
                for action in script.actions.iter().chain(&script.actions_false) {
                    *actions += 1;
                    *action_names.entry(action.name.clone()).or_default() += 1;
                }
            }
        }
    }
}

fn top(counts: &BTreeMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut sorted: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted.truncate(n);
    sorted
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
