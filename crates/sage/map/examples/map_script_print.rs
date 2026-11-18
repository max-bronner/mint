use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::Value;
use mint_sage_map::{MapFile, Parameter, Script, ScriptAction, ScriptNode};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(map_name) = args.next().and_then(|a| a.into_string().ok()) else {
        eprintln!("usage: map_script_print <map dir name> <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    };
    let roots: Vec<PathBuf> = args.map(PathBuf::from).collect();
    let wanted = format!(
        "maps/{0}/{0}.map",
        map_name.to_ascii_lowercase().replace('\\', "/")
    );
    let mut found: Option<(PathBuf, mint_sage_big::BigEntry)> = None;
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                if entry.name.to_ascii_lowercase().replace('\\', "/") == wanted {
                    found = Some((archive_path.clone(), entry.clone()));
                }
            }
        }
    }
    let Some((archive_path, entry)) = found else {
        eprintln!("{wanted}: not found in any archive");
        return ExitCode::FAILURE;
    };
    let bytes = match BigArchive::open(&archive_path).and_then(|a| a.read_entry(&entry)) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{}: {error}", archive_path.display());
            return ExitCode::FAILURE;
        }
    };
    let map = match MapFile::parse(&bytes) {
        Ok(map) => map,
        Err(error) => {
            eprintln!("{wanted}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("# {wanted} from {}", archive_path.display());
    let sides = map.sides().unwrap_or_default();
    for (slot, side) in sides.iter().enumerate() {
        let props: Vec<String> = side
            .properties
            .iter()
            .map(|(key, value)| format!("{key}={}", value_text(value)))
            .collect();
        println!("side {slot}: {}", props.join(" "));
    }
    for team in map.teams().unwrap_or_default() {
        println!("team {} owner={}", team.name, team.owner);
    }
    for object in &map.objects {
        let name = object.properties.get("objectName");
        let team = object.properties.get("originalOwner");
        let named_team = team.map(value_text).is_some_and(|team| {
            team.split_once('/').is_some_and(|(side, rest)| {
                rest.trim_matches('"') != format!("team{}", side.trim_matches('"'))
            })
        });
        if name.is_some() || named_team {
            println!(
                "object {} {} team={} at {:.0},{:.0}",
                name.map(value_text).unwrap_or_default(),
                object.template_name,
                team.map(value_text).unwrap_or_default(),
                object.x,
                object.y
            );
        }
    }
    let mut waypoints = BTreeMap::new();
    for waypoint in map.waypoints().unwrap_or_default() {
        let labels: Vec<&str> = waypoint
            .labels
            .iter()
            .flatten()
            .map(String::as_str)
            .collect();
        waypoints.insert(
            (waypoint.name.clone(), waypoint.id),
            (waypoint.position, labels.join("|")),
        );
    }
    for ((name, id), ([x, y, _], labels)) in &waypoints {
        println!("waypoint {name} #{id} {x:.0},{y:.0} paths [{labels}]");
    }
    for link in map.waypoint_links().unwrap_or_default() {
        println!("link {link:?}");
    }
    for area in map.trigger_areas().unwrap_or_default() {
        let n = area.points.len().max(1) as f32;
        let (sx, sy) = area
            .points
            .iter()
            .fold((0.0f32, 0.0f32), |(ax, ay), (x, y)| (ax + x, ay + y));
        let points: Vec<String> = area
            .points
            .iter()
            .map(|(x, y)| format!("{x:.0},{y:.0}"))
            .collect();
        println!(
            "area {} centre {:.0},{:.0} points {}",
            area.name,
            sx / n,
            sy / n,
            points.join(" ")
        );
    }
    if let Some(tree) = &map.scripts {
        for (slot, nodes) in tree.player_slots.iter().enumerate() {
            if nodes.is_empty() {
                continue;
            }
            println!("\n## slot {slot}");
            for node in nodes {
                print_node(node, 0);
            }
        }
    }
    ExitCode::SUCCESS
}

fn print_node(node: &ScriptNode, depth: usize) {
    let pad = "  ".repeat(depth);
    match node {
        ScriptNode::Group(group) => {
            let flags = format!(
                "{}{}",
                if group.active { "" } else { " INACTIVE" },
                if group.is_subroutine { " SUB" } else { "" }
            );
            println!("{pad}[group {}]{flags}", group.name);
            for child in &group.children {
                print_node(child, depth + 1);
            }
        }
        ScriptNode::Script(script) => print_script(script, &pad),
    }
}

fn print_script(script: &Script, pad: &str) {
    let mut flags = Vec::new();
    if !script.active {
        flags.push("INACTIVE".to_owned());
    }
    if script.one_shot {
        flags.push("once".to_owned());
    }
    if script.subroutine {
        flags.push("SUB".to_owned());
    }
    if !(script.easy && script.normal && script.hard) {
        flags.push(format!(
            "diff={}{}{}",
            if script.easy { "E" } else { "" },
            if script.normal { "N" } else { "" },
            if script.hard { "H" } else { "" }
        ));
    }
    if script.delay_evaluation_seconds != 0 {
        flags.push(format!("every {}s", script.delay_evaluation_seconds));
    }
    println!("{pad}* {} {}", script.name, flags.join(" "));
    for (i, or) in script.or_conditions.iter().enumerate() {
        let ands: Vec<String> = or
            .conditions
            .iter()
            .map(|c| {
                format!(
                    "{}{}({})",
                    if c.enabled { "" } else { "DISABLED " },
                    c.name,
                    params_text(&c.params)
                )
            })
            .collect();
        println!(
            "{pad}    {} {}",
            if i == 0 { "if" } else { "or" },
            ands.join(" AND ")
        );
    }
    print_actions(&script.actions, pad, "then");
    print_actions(&script.actions_false, pad, "else");
}

fn print_actions(actions: &[ScriptAction], pad: &str, word: &str) {
    for action in actions {
        println!(
            "{pad}    {word} {}({})",
            action.name,
            params_text(&action.params)
        );
    }
}

fn params_text(params: &[Parameter]) -> String {
    params
        .iter()
        .map(|p| match p {
            Parameter::Coord3D { x, y, z } => format!("({x:.0},{y:.0},{z:.0})"),
            Parameter::Value {
                int_value,
                real_value,
                string_value,
                ..
            } => {
                if !string_value.is_empty() {
                    format!("{string_value:?}")
                } else if *real_value != 0.0 {
                    format!("{real_value}")
                } else {
                    format!("{int_value}")
                }
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Str(s) | Value::WideStr(s) => format!("{s:?}"),
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
                Ok(kind)
                    if kind.is_file()
                        && path
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("big")) =>
                {
                    found.push(path);
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}
