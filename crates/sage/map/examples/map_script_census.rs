use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::{BigArchive, BigEntry};
use mint_sage_map::{MapFile, ScriptNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    CampaignMission,
    Tutorial,
    Cinematic,
    SkirmishWor,
    SkirmishMp,
    Library,
    Base,
    Other,
}

impl Class {
    fn label(self) -> &'static str {
        match self {
            Class::CampaignMission => "campaign mission",
            Class::Tutorial => "tutorial",
            Class::Cinematic => "cinematic",
            Class::SkirmishWor => "skirmish (map wor)",
            Class::SkirmishMp => "skirmish (map mp)",
            Class::Library => "script library",
            Class::Base => "AI base",
            Class::Other => "other",
        }
    }

    fn is_campaign(self) -> bool {
        matches!(
            self,
            Class::CampaignMission | Class::Tutorial | Class::Cinematic
        )
    }

    fn is_skirmish(self) -> bool {
        matches!(self, Class::SkirmishWor | Class::SkirmishMp)
    }
}

fn classify(archive_name_lower: &str, entry_path_lower: &str) -> Class {
    if archive_name_lower == "libraries.big" {
        return Class::Library;
    }
    if archive_name_lower == "bases.big" {
        return Class::Base;
    }
    if entry_path_lower.contains("tutorial") {
        return Class::Tutorial;
    }
    if entry_path_lower.contains("cin ") {
        return Class::Cinematic;
    }
    if entry_path_lower.contains("map good")
        || entry_path_lower.contains("map evil")
        || entry_path_lower.contains("map ang")
    {
        return Class::CampaignMission;
    }
    if entry_path_lower.contains("map wor") {
        return Class::SkirmishWor;
    }
    if entry_path_lower.contains("map mp") {
        return Class::SkirmishMp;
    }
    Class::Other
}

#[derive(Default)]
struct ClassTotals {
    files: u64,
    files_with_scripts: u64,
    scripts: u64,
    condition_uses: u64,
    disabled_conditions: u64,
    files_with_disabled: u64,
    action_uses: u64,
    condition_names: BTreeSet<String>,
    action_names: BTreeSet<String>,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: map_script_census <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut overlay: BTreeMap<String, (PathBuf, BigEntry)> = BTreeMap::new();
    let mut raw_count = 0u64;
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('\\', "/");
                if lower.ends_with(".map") {
                    raw_count += 1;
                    overlay.insert(lower, (archive_path.clone(), entry.clone()));
                }
            }
        }
    }
    println!(
        "{raw_count} raw `.map` entry(-ies) found across both roots' archives \
         before the overlay dedup (a path shipped by both installs counts twice here)"
    );

    let mut totals: BTreeMap<Class, ClassTotals> = BTreeMap::new();
    let mut files_failed: Vec<String> = Vec::new();
    let mut disabled_files: Vec<String> = Vec::new();

    for (entry_path, (archive_path, entry)) in &overlay {
        let archive_name_lower = archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let class = classify(&archive_name_lower, entry_path);

        let Ok(archive) = BigArchive::open(archive_path) else {
            continue;
        };
        let Ok(bytes) = archive.read_entry(entry) else {
            continue;
        };
        let map = match MapFile::parse(&bytes) {
            Ok(map) => map,
            Err(error) => {
                files_failed.push(format!("{entry_path}: {error}"));
                continue;
            }
        };

        let class_totals = totals.entry(class).or_default();
        class_totals.files += 1;

        let mut scripts = 0u64;
        let mut condition_uses = 0u64;
        let mut disabled = 0u64;
        let mut action_uses = 0u64;
        if let Some(tree) = &map.scripts {
            for slot in &tree.player_slots {
                census(
                    slot,
                    &mut scripts,
                    &mut condition_uses,
                    &mut disabled,
                    &mut action_uses,
                    &mut class_totals.condition_names,
                    &mut class_totals.action_names,
                );
            }
        }
        if scripts > 0 {
            class_totals.files_with_scripts += 1;
        }
        class_totals.scripts += scripts;
        class_totals.condition_uses += condition_uses;
        class_totals.disabled_conditions += disabled;
        if disabled > 0 {
            class_totals.files_with_disabled += 1;
            if class != Class::Library && class != Class::Base {
                disabled_files.push(format!("{entry_path}: {disabled}"));
            }
        }
        class_totals.action_uses += action_uses;
    }

    println!(
        "{} .map file(s) found across {} root(s)",
        overlay.len(),
        roots.len()
    );
    println!(
        "{} of {} decoded to their exact declared end, {} failed",
        overlay.len() - files_failed.len(),
        overlay.len(),
        files_failed.len()
    );
    for failure in &files_failed {
        println!("  FAILED: {failure}");
    }

    println!();
    println!(
        "class\tfiles\tscripts\tconditions (distinct)\tactions (distinct)\tfiles with scripts"
    );
    for class in [
        Class::CampaignMission,
        Class::Tutorial,
        Class::Cinematic,
        Class::SkirmishWor,
        Class::SkirmishMp,
        Class::Library,
        Class::Base,
        Class::Other,
    ] {
        let Some(t) = totals.get(&class) else {
            continue;
        };
        println!(
            "{}\t{}\t{}\t{} ({})\t{} ({})\t{}",
            class.label(),
            t.files,
            t.scripts,
            t.condition_uses,
            t.condition_names.len(),
            t.action_uses,
            t.action_names.len(),
            t.files_with_scripts,
        );
    }

    println!();
    println!("class	disabled conditions	files with a disabled condition");
    for (class, t) in &totals {
        println!(
            "{}	{}	{}",
            class.label(),
            t.disabled_conditions,
            t.files_with_disabled
        );
    }
    for line in &disabled_files {
        println!("  disabled in {line}");
    }

    let campaign_names: BTreeMap<Class, &ClassTotals> = totals
        .iter()
        .filter(|(c, _)| c.is_campaign())
        .map(|(c, t)| (*c, t))
        .collect();
    let skirmish_or_library_conditions: BTreeSet<String> = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish() || **c == Class::Library)
        .flat_map(|(_, t)| t.condition_names.iter().cloned())
        .collect();
    let skirmish_or_library_actions: BTreeSet<String> = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish() || **c == Class::Library)
        .flat_map(|(_, t)| t.action_names.iter().cloned())
        .collect();
    let skirmish_conditions: BTreeSet<String> = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish())
        .flat_map(|(_, t)| t.condition_names.iter().cloned())
        .collect();
    let skirmish_actions: BTreeSet<String> = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish())
        .flat_map(|(_, t)| t.action_names.iter().cloned())
        .collect();

    let campaign_files: u64 = campaign_names.values().map(|t| t.files).sum();
    let campaign_scripts: u64 = campaign_names.values().map(|t| t.scripts).sum();
    let campaign_condition_uses: u64 = campaign_names.values().map(|t| t.condition_uses).sum();
    let campaign_action_uses: u64 = campaign_names.values().map(|t| t.action_uses).sum();
    let campaign_condition_names: BTreeSet<String> = campaign_names
        .values()
        .flat_map(|t| t.condition_names.iter().cloned())
        .collect();
    let campaign_action_names: BTreeSet<String> = campaign_names
        .values()
        .flat_map(|t| t.action_names.iter().cloned())
        .collect();

    let condition_only_count = campaign_condition_names
        .difference(&skirmish_or_library_conditions)
        .count();
    let action_only_count = campaign_action_names
        .difference(&skirmish_or_library_actions)
        .count();

    println!();
    println!(
        "campaign class: {campaign_files} files, {campaign_scripts} scripts, \
         {campaign_condition_uses} condition uses ({} distinct), \
         {campaign_action_uses} action uses ({} distinct)",
        campaign_condition_names.len(),
        campaign_action_names.len(),
    );
    println!(
        "campaign-only conditions: {condition_only_count} of {} \
         (also needed by skirmish maps + libraries: {})",
        campaign_condition_names.len(),
        skirmish_or_library_conditions
            .intersection(&campaign_condition_names)
            .count(),
    );
    println!(
        "campaign-only actions: {action_only_count} of {} \
         (also needed by skirmish maps + libraries: {})",
        campaign_action_names.len(),
        skirmish_or_library_actions
            .intersection(&campaign_action_names)
            .count(),
    );
    println!(
        "needed by skirmish maps alone (any campaign overlap or not): \
         {} distinct condition name(s), {} distinct action name(s)",
        skirmish_conditions.len(),
        skirmish_actions.len(),
    );

    let skirmish_files: u64 = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish())
        .map(|(_, t)| t.files)
        .sum();
    let skirmish_scripted_files: u64 = totals
        .iter()
        .filter(|(c, _)| c.is_skirmish())
        .map(|(_, t)| t.files_with_scripts)
        .sum();
    println!("{skirmish_scripted_files} of {skirmish_files} skirmish maps carry scripts");

    if files_failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn census(
    nodes: &[ScriptNode],
    scripts: &mut u64,
    condition_uses: &mut u64,
    disabled: &mut u64,
    action_uses: &mut u64,
    condition_names: &mut BTreeSet<String>,
    action_names: &mut BTreeSet<String>,
) {
    for node in nodes {
        match node {
            ScriptNode::Group(group) => census(
                &group.children,
                scripts,
                condition_uses,
                disabled,
                action_uses,
                condition_names,
                action_names,
            ),
            ScriptNode::Script(script) => {
                *scripts += 1;
                for or_condition in &script.or_conditions {
                    for condition in &or_condition.conditions {
                        *condition_uses += 1;
                        *disabled += u64::from(!condition.enabled);
                        condition_names.insert(condition.name.clone());
                    }
                }
                for action in script.actions.iter().chain(&script.actions_false) {
                    *action_uses += 1;
                    action_names.insert(action.name.clone());
                }
            }
        }
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
                    let is_big = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"));
                    let is_scratch = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("___"));
                    if is_big && !is_scratch {
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
