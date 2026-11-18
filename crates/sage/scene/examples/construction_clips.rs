use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use mint_sage_asset::Resolver;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_model::{load_animation, load_model, load_object_index};
use mint_sage_scene::{Conditions, ExtraTokens, ModelStateMachine, StateMachine};
use mint_vfs::Vfs;

const TOKENS: [&str; 3] = [
    "AWAITING_CONSTRUCTION",
    "PARTIALLY_CONSTRUCTED",
    "ACTIVELY_BEING_CONSTRUCTED",
];

#[derive(Default)]
struct Row {
    states: usize,
    unreachable: usize,
    no_clip: usize,
    clip_missing: usize,
    no_model_state: usize,
    model_missing: usize,
    binds: usize,
    refused: usize,
    rescued: usize,
    stands_in: usize,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: construction_clips <install dir> [<install dir> ...]");
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
    let mut row = Row::default();
    let mut templates = 0usize;
    let mut refusals: Vec<String> = Vec::new();
    let mut stand_ins: Vec<String> = Vec::new();
    for name in declared.keys() {
        if !is_structure(name) {
            continue;
        }
        let model_states = index.model_states(name);
        let states = index.animation_states(name);
        if states.is_empty() {
            continue;
        }
        let mut extras = ExtraTokens::default();
        let model_machine = ModelStateMachine::compile_with(model_states, Some(&mut extras));
        let machine = StateMachine::compile_with(states, Some(&mut extras));
        let stand_in = |live: Conditions| {
            let have = model_machine
                .select(live)
                .and_then(|at| model_states.get(at))
                .and_then(|state| state.model.clone())
                .and_then(|model| load_model(&vfs, &resolver, &model).ok())
                .and_then(|model| model.hierarchy.map(|h| h.header.name))?;
            let at = machine.select_playable(live, |index| {
                states
                    .get(index)
                    .and_then(|state| state.clips.first())
                    .and_then(|clip| load_animation(&vfs, &resolver, clip.asset()).ok())
                    .is_some_and(|l| l.animation.hierarchy_name.eq_ignore_ascii_case(&have))
            })?;
            let state = states.get(at)?;
            Some(format!(
                "[{}] {}",
                state.conditions.join(" "),
                state.clips.first().map_or("", |clip| clip.name.as_str())
            ))
        };
        let mut counted = false;
        for state in states {
            if !state
                .conditions
                .iter()
                .any(|c| TOKENS.iter().any(|t| c.eq_ignore_ascii_case(t)))
            {
                continue;
            }
            row.states += 1;
            if !counted {
                counted = true;
                templates += 1;
            }
            let mut live = Conditions::default();
            let mut reachable = true;
            for token in &state.conditions {
                match Conditions::from_token(token).or_else(|| extras.lookup(token)) {
                    Some(bit) => live = live.union(bit),
                    None => reachable = false,
                }
            }
            if !reachable {
                row.unreachable += 1;
                continue;
            }
            let Some(clip) = state.clips.first() else {
                row.no_clip += 1;
                continue;
            };
            let Ok(loaded) = load_animation(&vfs, &resolver, clip.asset()) else {
                row.clip_missing += 1;
                let found = stand_in(live);
                row.stands_in += usize::from(found.is_some());
                stand_ins.push(format!(
                    "{name} [{}] {}: {}",
                    state.conditions.join(" "),
                    clip.name,
                    found.as_deref().unwrap_or("nothing binds, the pose holds")
                ));
                continue;
            };
            let Some(model_name) = model_machine
                .select(live)
                .and_then(|at| model_states.get(at))
                .and_then(|state| state.model.clone())
            else {
                row.no_model_state += 1;
                continue;
            };
            let Ok(model) = load_model(&vfs, &resolver, &model_name) else {
                row.model_missing += 1;
                continue;
            };
            let want = &loaded.animation.hierarchy_name;
            match model.hierarchy.as_ref().map(|h| h.header.name.clone()) {
                Some(have) if have.eq_ignore_ascii_case(want) => row.binds += 1,
                have => {
                    let rescued = clip
                        .hierarchy()
                        .and_then(|h| load_model(&vfs, &resolver, h).ok())
                        .and_then(|m| m.hierarchy.map(|h| h.header.name))
                        .is_some_and(|name| name.eq_ignore_ascii_case(want));
                    if rescued {
                        row.rescued += 1;
                    } else {
                        row.refused += 1;
                    }
                    if refusals.len() < 12 {
                        refusals.push(format!(
                            "{name}: {} wants {want}, {model_name} has {have:?}{}",
                            clip.name,
                            if rescued { " (rescued)" } else { "" }
                        ));
                    }
                }
            }
        }
    }
    println!("{templates} structure template(s) with a construction animation state");
    println!("construction animation states      {}", row.states);
    println!("  unreachable (unmodelled token)   {}", row.unreachable);
    println!("  no clip in the state             {}", row.no_clip);
    println!("  clip file not found              {}", row.clip_missing);
    println!("    of which a stand-in plays      {}", row.stands_in);
    for line in &stand_ins {
        println!("    {line}");
    }
    println!("  no model state for the same set  {}", row.no_model_state);
    println!("  model file not found             {}", row.model_missing);
    println!("  binds to the state's own model   {}", row.binds);
    println!("  binds only to the clip hierarchy {}", row.rescued);
    println!("  binds to neither                 {}", row.refused);
    for line in &refusals {
        println!("  {line}");
    }
    ExitCode::SUCCESS
}
