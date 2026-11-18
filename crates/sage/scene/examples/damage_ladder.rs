use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use mint_sage_asset::Resolver;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_model::{load_animation, load_model, load_object_index};
use mint_sage_scene::{Conditions, ExtraTokens, ModelStateMachine, StateMachine};
use mint_vfs::Vfs;

const RUNGS: [&str; 5] = [
    "DAMAGED",
    "REALLYDAMAGED",
    "RUBBLE",
    "POST_RUBBLE",
    "POST_COLLAPSE",
];

#[derive(Default)]
struct Row {
    model_declared: usize,
    model_loads: usize,
    model_none: usize,
    animation_declared: usize,
    with_clip: usize,
    clip_loads: usize,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: damage_ladder <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    let mut vfs = Vfs::new();
    for root in &roots {
        for path in mint_sage_big::archives_under(root) {
            if let Ok(archive) = mint_sage_big::BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    let (index, _) = load_object_index(&vfs);

    let mut declared: BTreeMap<String, (Option<String>, Option<Vec<String>>)> = BTreeMap::new();
    let mut sources = MemorySources::new();
    let mut paths = Vec::new();
    for path in vfs.paths() {
        if !path.as_str().starts_with("data/ini/") {
            continue;
        }
        if let Ok(bytes) = vfs.read_path(path) {
            sources.insert(path.as_str(), bytes);
            paths.push(path.as_str().to_owned());
        }
    }
    let reader = Reader::new(&sources);
    let table = BlockTable::corpus_path();
    for path in &paths {
        let Ok(document) = reader.read(path, &table) else {
            continue;
        };
        for block in document.roots() {
            let parent = match block.keyword.as_str() {
                "Object" => None,
                "ChildObject" => block.args.get(1).cloned(),
                _ => continue,
            };
            let Some(name) = block.args.first() else {
                continue;
            };
            let kind_of = block.field("KindOf").map(|field| field.args.clone());
            declared.insert(name.to_ascii_lowercase(), (parent, kind_of));
        }
    }
    let is_structure = |name: &str| {
        let mut at = name.to_ascii_lowercase();
        for _ in 0..16 {
            let Some((parent, kind_of)) = declared.get(&at) else {
                return false;
            };
            if let Some(tokens) = kind_of {
                return tokens.iter().any(|t| t.eq_ignore_ascii_case("STRUCTURE"));
            }
            let Some(parent) = parent else {
                return false;
            };
            at = parent.to_ascii_lowercase();
        }
        false
    };

    let resolver = Resolver::new();
    let mut models: BTreeMap<String, bool> = BTreeMap::new();
    let mut clips: BTreeMap<String, Result<(), String>> = BTreeMap::new();
    let mut clip_errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows: BTreeMap<&str, Row> = BTreeMap::new();
    let mut structures = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for name in declared.keys() {
        if !is_structure(name) {
            continue;
        }
        let model_states = index.model_states(name);
        let states = index.animation_states(name);
        if model_states.is_empty() && states.is_empty() {
            continue;
        }
        structures += 1;
        let mut extras = ExtraTokens::default();
        let model_machine = ModelStateMachine::compile_with(model_states, Some(&mut extras));
        let machine = StateMachine::compile_with(states, Some(&mut extras));
        for rung in RUNGS {
            let Some(live) = Conditions::from_token(rung) else {
                continue;
            };
            let row = rows.entry(rung).or_default();
            let names =
                |conditions: &[String]| conditions.iter().any(|c| c.eq_ignore_ascii_case(rung));
            if let Some(state) = model_machine
                .select(live)
                .and_then(|at| model_states.get(at))
                .filter(|state| names(&state.conditions))
            {
                row.model_declared += 1;
                match state.model.as_deref() {
                    None => row.model_none += 1,
                    Some(model) if model.eq_ignore_ascii_case("none") => row.model_none += 1,
                    Some(model) => {
                        let loads = *models
                            .entry(model.to_ascii_lowercase())
                            .or_insert_with(|| load_model(&vfs, &resolver, model).is_ok());
                        if loads {
                            row.model_loads += 1;
                        } else if failures.len() < 12 {
                            failures.push(format!("{name} {rung}: model {model}"));
                        }
                    }
                }
            }
            if let Some(state) = machine
                .select(live)
                .and_then(|at| states.get(at))
                .filter(|state| names(&state.conditions))
            {
                row.animation_declared += 1;
                if let Some(clip) = state.clips.first() {
                    row.with_clip += 1;
                    let file = clip
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&clip.name)
                        .to_owned();
                    let loaded = clips.entry(file.to_ascii_lowercase()).or_insert_with(|| {
                        load_animation(&vfs, &resolver, &file)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    });
                    match loaded {
                        Ok(()) => row.clip_loads += 1,
                        Err(error) => {
                            let kind: String = error.chars().take(40).collect();
                            *clip_errors.entry(kind).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }
    println!("{structures} structure template(s) with a draw module");
    println!("rung            model-state  loads  none  anim-state  with-clip  clip-loads");
    for rung in RUNGS {
        let row = rows.get(rung).map_or_else(Row::default, |r| Row { ..*r });
        println!(
            "{rung:<15} {:>11} {:>6} {:>5} {:>11} {:>10} {:>11}",
            row.model_declared,
            row.model_loads,
            row.model_none,
            row.animation_declared,
            row.with_clip,
            row.clip_loads
        );
    }
    for (kind, count) in &clip_errors {
        println!("  clip not loaded x{count}: {kind}...");
    }
    for line in &failures {
        println!("  unresolved: {line}");
    }
    ExitCode::SUCCESS
}
