use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_modules::{
    slaved_update_from_fields, BlockFields, TemplateIndex, TemplateRecords, UpgradeBits,
};
use mint_sim::Frame;

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: slaved_updates <install dir> [<install dir> ...]  (ROTWK last)");
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
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(entry.path.as_str(), bytes);
                }
            }
        }
    }
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents: Vec<_> = paths
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .collect();
    let _ = mint_sage_ini::expand_macros(&mut documents);
    let mut index = TemplateIndex::new();
    for document in &documents {
        index.insert_document(document);
    }
    let bits = UpgradeBits::from_index(index.upgrades());
    let (records, refused) = TemplateRecords::build_collecting(&index);
    for error in &refused {
        println!("refused: {error}");
    }
    let names: Vec<String> = index.names().map(str::to_owned).collect();

    let mut spawned_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in &names {
        let Some(record) = records.get(name) else {
            continue;
        };
        for module in &record.declared_modules {
            let field_name = match module.name.as_str() {
                "SpawnBehavior" => "SpawnTemplateName",
                "ObjectCreationUpgrade" => "ThingToSpawn",
                _ => continue,
            };
            for field in &module.fields {
                if field.keyword.eq_ignore_ascii_case(field_name) {
                    for spawn in &field.args {
                        spawned_by
                            .entry(spawn.to_ascii_lowercase())
                            .or_default()
                            .insert(format!("{name} ({})", module.name));
                    }
                }
            }
        }
    }

    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let (mut built, mut refused, mut inherited, mut enslaved) = (0usize, 0usize, 0usize, 0usize);
    for name in &names {
        for declaration in index.declared_modules(name) {
            if declaration.name != "SlavedUpdate" {
                continue;
            }
            let first = seen.insert(std::ptr::from_ref(declaration.block) as usize);
            let fields = BlockFields(declaration.block, index.macros(), &bits);
            match slaved_update_from_fields(&fields, Frame::ZERO) {
                Ok(module) => {
                    if first {
                        built += 1;
                    } else {
                        inherited += 1;
                    }
                    let masters = spawned_by
                        .get(&name.to_ascii_lowercase())
                        .map(|set| set.iter().cloned().collect::<Vec<_>>().join(" "));
                    if masters.is_some() {
                        enslaved += 1;
                    }
                    let parsed_only: Vec<String> = module
                        .parsed_only()
                        .into_iter()
                        .map(|(field, value)| format!("{field}={value}"))
                        .collect();
                    let (ox, oy, oz) = module.guard_offset();
                    println!(
                        "{name}{}: GuardMaxRange {} GuardWanderRange {} AttackRange {} LeashRange {} \
                         GuardPositionOffset ({ox}, {oy}, {oz}) DieOnMastersDeath {}; parsed only: {}; \
                         made by {}",
                        if first { "" } else { " (inherited)" },
                        module.guard_max_range(),
                        module.guard_wander_range(),
                        module.attack_range(),
                        module.leash_range(),
                        if module.dies_with_master() { "Yes" } else { "No" },
                        if parsed_only.is_empty() {
                            String::from("-")
                        } else {
                            parsed_only.join(" ")
                        },
                        masters.unwrap_or_else(|| String::from("nothing"))
                    );
                }
                Err(err) => {
                    refused += 1;
                    println!("{name}: REFUSED {err}");
                }
            }
        }
    }
    println!(
        "SlavedUpdate: {built} declaration(s) built, {refused} refused, {inherited} inherited \
         by a ChildObject; {enslaved} of {} name-occurrence(s) have a master \
         (SpawnBehavior or ObjectCreationUpgrade)",
        built + inherited
    );
    if refused == 0 {
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
