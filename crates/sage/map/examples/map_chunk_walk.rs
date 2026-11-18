use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{
    build_links, split_team_ref, CameraTrack, Interpolation, MapFile, Parameter, Script,
    ScriptNode, Waypoint,
};

const TEAM_PARAM: i32 = 3;
const WAYPOINT_PATH_PARAM: i32 = 24;
const THIS_TEAM: &str = "<This Team>";

#[derive(Default)]
struct Stats {
    files: u64,
    parse_failed: u64,
    chunk_failures: Vec<String>,
    sides_ok: u64,
    teams_chunks: u64,
    team_dicts: u64,
    duplicate_pairs: u64,
    owner_not_a_side: u64,
    team_params_qualified: u64,
    team_params_bare: u64,
    team_params_sentinel: u64,
    team_params_dangling: u64,
    waypoint_chunks: u64,
    links: u64,
    unresolved_endpoints: u64,
    path_params: u64,
    path_params_resolved: u64,
    camera_chunks: u64,
    animations: u64,
    stopped_early: u64,
    free_tracks: u64,
    look_tracks: u64,
    step_keys: u64,
    linear_keys: u64,
    catmull_keys: u64,
    quaternions: u64,
    unit_quaternions: u64,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: map_chunk_walk <install dir> [...]");
        return ExitCode::FAILURE;
    }
    let mut all = Stats::default();
    let mut campaign = Stats::default();
    for root in &roots {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('\\', "/");
                if !lower.ends_with(".map") {
                    continue;
                }
                if !seen.insert(lower.clone()) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let is_campaign = lower.strip_prefix("maps/").is_some_and(|rest| {
                    let file = rest.rsplit('/').next().unwrap_or(rest);
                    ["map good", "map evil", "map ang"]
                        .iter()
                        .any(|p| file.starts_with(p))
                });
                walk(&entry.name, &raw, &mut all);
                if is_campaign {
                    walk(&entry.name, &raw, &mut campaign);
                }
            }
        }
    }
    report("all maps", &all);
    report("campaign maps", &campaign);
    if all.chunk_failures.is_empty() && all.parse_failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn bump(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}

fn walk(name: &str, raw: &[u8], s: &mut Stats) {
    bump(&mut s.files);
    let map = match MapFile::parse(raw) {
        Ok(map) => map,
        Err(error) => {
            bump(&mut s.parse_failed);
            s.chunk_failures.push(format!("{name}: parse: {error}"));
            return;
        }
    };
    let sides = match map.sides() {
        Ok(sides) => {
            bump(&mut s.sides_ok);
            sides
        }
        Err(error) => {
            s.chunk_failures.push(format!("{name}: SidesList: {error}"));
            Vec::new()
        }
    };
    let side_names: Vec<&str> = sides.iter().map(|side| side.name().unwrap_or("")).collect();
    let has = |chunk: &str| map.chunks.iter().any(|c| c.name == chunk);

    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    if has("Teams") {
        bump(&mut s.teams_chunks);
        match map.teams() {
            Ok(teams) => {
                for team in &teams {
                    bump(&mut s.team_dicts);
                    if !side_names.contains(&team.owner.as_str()) {
                        bump(&mut s.owner_not_a_side);
                    }
                    if !pairs.insert((team.owner.clone(), team.name.clone())) {
                        bump(&mut s.duplicate_pairs);
                    }
                    if let Err(error) = team.template() {
                        s.chunk_failures
                            .push(format!("{name}: team template: {error}"));
                    }
                }
            }
            Err(error) => s.chunk_failures.push(format!("{name}: Teams: {error}")),
        }
    }

    let waypoints = match map.waypoints() {
        Ok(w) => w,
        Err(error) => {
            s.chunk_failures.push(format!("{name}: waypoints: {error}"));
            Vec::new()
        }
    };
    if has("WaypointsList") {
        bump(&mut s.waypoint_chunks);
        match map.waypoint_links() {
            Ok(links) => {
                for link in &links {
                    bump(&mut s.links);
                    let known = |id: i32| waypoints.iter().any(|w| w.id == id);
                    if !known(link.from) || !known(link.to) {
                        bump(&mut s.unresolved_endpoints);
                    }
                }
                let _ = build_links(&waypoints, &links);
            }
            Err(error) => s
                .chunk_failures
                .push(format!("{name}: WaypointsList: {error}")),
        }
    }

    let mut animation_names: BTreeSet<String> = BTreeSet::new();
    if has("CameraAnimationList") {
        bump(&mut s.camera_chunks);
        match map.camera_animations() {
            Ok(list) => {
                if list.stopped_at_unknown_kind {
                    bump(&mut s.stopped_early);
                }
                for animation in &list.animations {
                    bump(&mut s.animations);
                    animation_names.insert(animation.name.clone());
                    camera_stats(&animation.track, s);
                }
            }
            Err(error) => s
                .chunk_failures
                .push(format!("{name}: CameraAnimationList: {error}")),
        }
    }

    if let Some(tree) = &map.scripts {
        for (slot, nodes) in tree.player_slots.iter().enumerate() {
            let owner = side_names.get(slot).copied().unwrap_or("");
            for node in nodes {
                script_params(node, owner, &pairs, &waypoints, s);
            }
        }
    }
}

fn camera_stats(track: &CameraTrack, s: &mut Stats) {
    let mut tally = |interpolation: Interpolation| match interpolation {
        Interpolation::Step => bump(&mut s.step_keys),
        Interpolation::Linear => bump(&mut s.linear_keys),
        Interpolation::CatmullRom => bump(&mut s.catmull_keys),
    };
    match track {
        CameraTrack::Free(keys) => {
            for key in keys {
                tally(key.interpolation);
            }
            bump(&mut s.free_tracks);
            for key in keys {
                bump(&mut s.quaternions);
                let norm: f32 = key.value.rotation.iter().map(|v| v * v).sum::<f32>();
                if (norm - 1.0).abs() < 0.01 {
                    bump(&mut s.unit_quaternions);
                }
            }
        }
        CameraTrack::Look { camera, target } => {
            bump(&mut s.look_tracks);
            for key in camera {
                tally(key.interpolation);
            }
            for key in target {
                tally(key.interpolation);
            }
        }
    }
}

fn script_params(
    node: &ScriptNode,
    owner: &str,
    pairs: &BTreeSet<(String, String)>,
    waypoints: &[Waypoint],
    s: &mut Stats,
) {
    match node {
        ScriptNode::Group(group) => {
            for child in &group.children {
                script_params(child, owner, pairs, waypoints, s);
            }
        }
        ScriptNode::Script(script) => script_stats(script, owner, pairs, waypoints, s),
    }
}

fn script_stats(
    script: &Script,
    owner: &str,
    pairs: &BTreeSet<(String, String)>,
    waypoints: &[Waypoint],
    s: &mut Stats,
) {
    let conditions = script.or_conditions.iter().flat_map(|or| &or.conditions);
    let condition_params = conditions.flat_map(|c| &c.params);
    let action_params = script
        .actions
        .iter()
        .chain(&script.actions_false)
        .flat_map(|a| &a.params);
    for param in condition_params.chain(action_params) {
        let Parameter::Value {
            param_type,
            string_value,
            ..
        } = param
        else {
            continue;
        };
        match *param_type {
            TEAM_PARAM => {
                if string_value == THIS_TEAM {
                    bump(&mut s.team_params_sentinel);
                    continue;
                }
                let (team_owner, team) = split_team_ref(string_value, owner);
                let found = pairs.contains(&(team_owner.to_string(), team.to_string()));
                if string_value.contains('/') {
                    bump(&mut s.team_params_qualified);
                } else {
                    bump(&mut s.team_params_bare);
                }
                if !found {
                    bump(&mut s.team_params_dangling);
                }
            }
            WAYPOINT_PATH_PARAM => {
                bump(&mut s.path_params);
                let wanted = string_value.to_ascii_lowercase();
                let named = waypoints.iter().any(|w| {
                    w.labels
                        .iter()
                        .flatten()
                        .any(|l| l.to_ascii_lowercase() == wanted)
                });
                if named {
                    bump(&mut s.path_params_resolved);
                }
            }
            _ => {}
        }
    }
}

fn report(label: &str, s: &Stats) {
    println!(
        "== {label}: {} file(s), {} unparsed",
        s.files, s.parse_failed
    );
    println!("SidesList exact: {}", s.sides_ok);
    println!(
        "Teams: {} chunk(s), {} dict(s), {} duplicate (owner, name), {} owner(s) naming no side",
        s.teams_chunks, s.team_dicts, s.duplicate_pairs, s.owner_not_a_side
    );
    println!(
        "TEAM params: {} qualified, {} bare, {} sentinel, {} dangling (qualified + bare)",
        s.team_params_qualified, s.team_params_bare, s.team_params_sentinel, s.team_params_dangling
    );
    println!(
        "WaypointsList: {} chunk(s), {} link(s), {} with an unresolved endpoint; path params {}/{} resolve",
        s.waypoint_chunks, s.links, s.unresolved_endpoints, s.path_params_resolved, s.path_params
    );
    println!(
        "CameraAnimationList: {} chunk(s), {} animation(s) ({} free, {} look), {} stopped early; keys step {} linear {} catmull {}; unit quaternions {}/{}",
        s.camera_chunks,
        s.animations,
        s.free_tracks,
        s.look_tracks,
        s.stopped_early,
        s.step_keys,
        s.linear_keys,
        s.catmull_keys,
        s.unit_quaternions,
        s.quaternions
    );
    println!("{} chunk failure(s)", s.chunk_failures.len());
    for failure in &s.chunk_failures {
        println!("  FAIL {failure}");
    }
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
                && !path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('_'))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
