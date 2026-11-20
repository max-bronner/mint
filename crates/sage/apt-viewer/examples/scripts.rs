use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use mint_sage_apt::{
    decode_block, parse_frames, AptMovie, Block, Character, ConstItem, DecodeLimits, Frame,
    FrameItem, Instruction, Op, Rect,
};
use mint_sage_apt_viewer::{resolve_character, MovieLibrary};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(install) = args.first() else {
        eprintln!(
            "usage: scripts <install> --grep <needle>\n\
             \x20      scripts <install> --movie <name> [--filter <needle>]"
        );
        return ExitCode::FAILURE;
    };
    let mut roots = vec![PathBuf::from(install)];
    roots.extend(args.windows(2).filter_map(|w| match w {
        [flag, root] if flag == "--also" => Some(PathBuf::from(root)),
        _ => None,
    }));
    let mut library = MovieLibrary::new_with_roots(roots);
    let names = library.list_available();

    let mode = args.get(1).map(String::as_str).unwrap_or("");
    match mode {
        "--grep" => {
            let Some(needle) = args.get(2) else {
                eprintln!("--grep needs a needle");
                return ExitCode::FAILURE;
            };
            for name in &names {
                scan_movie(&mut library, name, Some(needle.as_str()), false);
            }
        }
        "--movie" => {
            let Some(movie) = args.get(2) else {
                eprintln!("--movie needs a name");
                return ExitCode::FAILURE;
            };
            let filter = args
                .iter()
                .position(|a| a == "--filter")
                .and_then(|i| args.get(i.saturating_add(1)))
                .map(String::as_str);
            scan_movie(&mut library, &movie.to_ascii_lowercase(), filter, true);
        }
        "--pushdata" => pushdata_census(&mut library, &names),
        "--attach" => attach_census(&mut library, &names),
        "--props" => property_census(&mut library, &names),
        "--hit" => {
            let only = args.get(2).map(|m| m.to_ascii_lowercase());
            hit_census(&mut library, &names, only.as_deref());
        }
        "--clipdepth" => {
            let only = args.get(2).map(|m| m.to_ascii_lowercase());
            for name in &names {
                if only.as_deref().is_some_and(|m| m != name.as_str()) {
                    continue;
                }
                clip_depth_report(&mut library, name);
            }
        }
        other => {
            eprintln!("unknown mode {other}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn clip_depth_report(library: &mut MovieLibrary, name: &str) {
    if library.ensure_loaded(name).is_err() {
        return;
    }
    let Some(movie) = library.movie(name).cloned() else {
        return;
    };
    let Some(assets) = library.assets(name) else {
        return;
    };
    let apt = assets.apt.clone();
    let mut timelines: Vec<(String, u32, u32)> = vec![(
        "movie".to_owned(),
        movie.record.frame_count,
        movie.record.frames_offset,
    )];
    for (id, character) in movie.characters.iter().enumerate() {
        if let Character::Sprite(sprite) = character {
            timelines.push((
                format!("sprite #{id}"),
                sprite.frame_count,
                sprite.frames_offset,
            ));
        }
    }
    for (owner, count, offset) in timelines {
        let Ok(frames) = parse_frames(&apt, count, offset) else {
            continue;
        };
        for (index, frame) in frames.iter().enumerate() {
            for item in &frame.items {
                let FrameItem::PlaceObject(po) = item else {
                    continue;
                };
                let Some(clip) = po.clip_depth else {
                    continue;
                };
                let occupants: Vec<String> = frames
                    .iter()
                    .flat_map(|f| f.items.iter())
                    .filter_map(|i| match i {
                        FrameItem::PlaceObject(other)
                            if other.depth > po.depth && other.depth <= clip =>
                        {
                            Some(format!(
                                "{}{}",
                                other.depth,
                                other
                                    .character
                                    .map(|c| format!("(char {c})"))
                                    .unwrap_or_default()
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                println!(
                    "{name} {owner} frame {index}: depth {} clip_depth {clip} character {:?} name {:?}",
                    po.depth, po.character, po.name
                );
                println!(
                    "    masks depths ({}): {}",
                    occupants.len(),
                    occupants.join(" ")
                );
            }
        }
    }
}

fn hit_census(library: &mut MovieLibrary, names: &[String], only: Option<&str>) {
    let mut compared = 0u64;
    let mut agree = 0u64;
    let mut a_smaller_area = 0u64;
    let mut no_mesh = 0u64;
    let mut no_hit_records = 0u64;
    let mut mesh_equals_raw = 0u64;
    let mut mesh_differs_from_raw = 0u64;
    let mut mesh_equals_bounds = 0u64;
    let mut worst: Vec<(f32, String)> = Vec::new();
    for name in names {
        if only.is_some_and(|m| m != name.as_str()) {
            continue;
        }
        if library.ensure_loaded(name).is_err() {
            continue;
        }
        let Some(movie) = library.movie(name).cloned() else {
            continue;
        };
        let movie_set = library.movie_set();
        let Some(assets) = library.assets(name) else {
            continue;
        };
        let apt = assets.apt.clone();
        for (id, character) in movie.characters.iter().enumerate() {
            let Character::Button(button) = character else {
                continue;
            };
            let mesh = mint_sage_apt::hit_bounds(&apt, button).ok().flatten();
            let mut union: Option<Rect> = None;
            for record in &button.records {
                if record.flags & 0x8 == 0 {
                    continue;
                }
                let Some((_owner, _rid, Character::Shape { bounds, .. })) =
                    resolve_character(&movie_set, name, record.character, 0)
                else {
                    continue;
                };
                let corners = [
                    [bounds.left, bounds.top],
                    [bounds.right, bounds.top],
                    [bounds.right, bounds.bottom],
                    [bounds.left, bounds.bottom],
                ];
                for corner in corners {
                    let x = record.transform.m00 * corner[0]
                        + record.transform.m10 * corner[1]
                        + record.translate.x;
                    let y = record.transform.m01 * corner[0]
                        + record.transform.m11 * corner[1]
                        + record.translate.y;
                    union = Some(match union {
                        None => Rect {
                            left: x,
                            top: y,
                            right: x,
                            bottom: y,
                        },
                        Some(r) => Rect {
                            left: r.left.min(x),
                            top: r.top.min(y),
                            right: r.right.max(x),
                            bottom: r.bottom.max(y),
                        },
                    });
                }
            }
            if let Some(want) = only {
                let _ = want;
                println!(
                    "{name}#{id}: bounds {:?} mesh {:?} records:",
                    button.bounds, mesh
                );
                for (i, record) in button.records.iter().enumerate() {
                    let kind = resolve_character(&movie_set, name, record.character, 0)
                        .map(|(o, rid, c)| {
                            format!("{}#{rid} {:?}", o.name, std::mem::discriminant(c))
                        })
                        .unwrap_or_else(|| "<unresolved>".to_owned());
                    let shape_bounds =
                        match resolve_character(&movie_set, name, record.character, 0) {
                            Some((_o, _rid, Character::Shape { bounds, .. })) => Some(*bounds),
                            _ => None,
                        };
                    println!(
                        "    [{i}] flags={:#04x} char={} depth={} m=({:.3},{:.3},{:.3},{:.3}) t=({:.1},{:.1}) shape_bounds={:?} {kind}",
                        record.flags,
                        record.character,
                        record.depth,
                        record.transform.m00,
                        record.transform.m01,
                        record.transform.m10,
                        record.transform.m11,
                        record.translate.x,
                        record.translate.y,
                        shape_bounds
                    );
                }
            }
            let mut raw: Option<Rect> = None;
            for record in &button.records {
                if record.flags & 0x8 == 0 {
                    continue;
                }
                let Some((_o, _rid, Character::Shape { bounds, .. })) =
                    resolve_character(&movie_set, name, record.character, 0)
                else {
                    continue;
                };
                raw = Some(match raw {
                    None => *bounds,
                    Some(r) => Rect {
                        left: r.left.min(bounds.left),
                        top: r.top.min(bounds.top),
                        right: r.right.max(bounds.right),
                        bottom: r.bottom.max(bounds.bottom),
                    },
                });
            }
            if let (Some(m), Some(r)) = (mesh, raw) {
                let close = |x: f32, y: f32| (x - y).abs() <= 0.5;
                if close(m.left, r.left)
                    && close(m.top, r.top)
                    && close(m.right, r.right)
                    && close(m.bottom, r.bottom)
                {
                    mesh_equals_raw += 1;
                } else {
                    mesh_differs_from_raw += 1;
                }
            }
            {
                let close = |x: f32, y: f32| (x - y).abs() <= 0.5;
                let bb = button.bounds;
                if let Some(m) = mesh {
                    if close(m.left, bb.left)
                        && close(m.top, bb.top)
                        && close(m.right, bb.right)
                        && close(m.bottom, bb.bottom)
                    {
                        mesh_equals_bounds += 1;
                    }
                }
            }
            let (Some(a), Some(b)) = (mesh, union) else {
                if mesh.is_none() {
                    no_mesh += 1;
                }
                if union.is_none() {
                    no_hit_records += 1;
                }
                continue;
            };
            compared += 1;
            let close = |x: f32, y: f32| (x - y).abs() <= 0.5;
            if close(a.left, b.left)
                && close(a.top, b.top)
                && close(a.right, b.right)
                && close(a.bottom, b.bottom)
            {
                agree += 1;
                continue;
            }
            let area = |r: Rect| ((r.right - r.left) * (r.bottom - r.top)).abs();
            if area(a) < area(b) {
                a_smaller_area += 1;
            }
            let ratio = if area(a) > 0.0 {
                area(b) / area(a)
            } else {
                f32::INFINITY
            };
            worst.push((
                ratio,
                format!(
                    "{name}#{id}: mesh {:.1}x{:.1} at ({:.1},{:.1}) vs hit records {:.1}x{:.1} at ({:.1},{:.1}), bounds {:.1}x{:.1}",
                    a.right - a.left,
                    a.bottom - a.top,
                    a.left,
                    a.top,
                    b.right - b.left,
                    b.bottom - b.top,
                    b.left,
                    b.top,
                    button.bounds.right - button.bounds.left,
                    button.bounds.bottom - button.bounds.top,
                ),
            ));
        }
    }
    println!("Buttons with both a hit mesh and HitTest records: {compared}");
    println!("  boxes agree to within 0.5 units: {agree}");
    println!("  disagree: {}", compared.saturating_sub(agree));
    println!("    of those, the mesh box is the SMALLER: {a_smaller_area}");
    println!("  buttons with no hit mesh at all: {no_mesh}");
    println!("  buttons with no HitTest-flagged record: {no_hit_records}");
    println!(
        "  mesh box == HitTest shape bounds with NO record matrix applied: {mesh_equals_raw}          (differs in {mesh_differs_from_raw})"
    );
    println!("  mesh box == Button::bounds: {mesh_equals_bounds}");
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    println!("  largest disagreements (hit-record area / mesh area first):");
    for (ratio, line) in worst.iter().take(25) {
        println!("    x{ratio:.1}  {line}");
    }
}

fn property_census(library: &mut MovieLibrary, names: &[String]) {
    const SWF: [&str; 22] = [
        "_x",
        "_y",
        "_xscale",
        "_yscale",
        "_currentframe",
        "_totalframes",
        "_alpha",
        "_visible",
        "_width",
        "_height",
        "_rotation",
        "_target",
        "_framesloaded",
        "_name",
        "_droptarget",
        "_url",
        "_highquality",
        "_focusrect",
        "_soundbuftime",
        "_quality",
        "_xmouse",
        "_ymouse",
    ];
    let mut get: BTreeMap<i64, u64> = BTreeMap::new();
    let mut set: BTreeMap<i64, u64> = BTreeMap::new();
    let mut unknown_operand = 0u64;
    for name in names {
        if library.ensure_loaded(name).is_err() {
            continue;
        }
        let Some(movie) = library.movie(name).cloned() else {
            continue;
        };
        let Some(assets) = library.assets(name) else {
            continue;
        };
        let apt = assets.apt.clone();
        for site in sites(&apt, &movie) {
            let Ok(block) = decode_block(&apt, site.offset, &DecodeLimits::default()) else {
                continue;
            };
            let mut regions: Vec<&[Instruction]> = vec![&block.code];
            for function in &block.functions {
                regions.push(&function.body);
            }
            for region in regions {
                for (i, instruction) in region.iter().enumerate() {
                    let target = match instruction.op {
                        Op::GetProperty => &mut get,
                        Op::SetProperty => &mut set,
                        _ => continue,
                    };
                    let back = if matches!(instruction.op, Op::SetProperty) {
                        2
                    } else {
                        1
                    };
                    let previous = i.checked_sub(back).and_then(|j| region.get(j));
                    let index = match previous.map(|p| &p.op) {
                        Some(Op::EaPushByte(v)) => i64::from(*v),
                        Some(Op::EaPushShort(v)) => i64::from(*v),
                        Some(Op::EaPushZero) => 0,
                        Some(Op::EaPushOne) => 1,
                        _ => {
                            unknown_operand += 1;
                            continue;
                        }
                    };
                    *target.entry(index).or_default() += 1;
                }
            }
        }
    }
    let show = |what: &str, table: &BTreeMap<i64, u64>| {
        println!("{what}: {} instructions", table.values().sum::<u64>());
        let mut ranked: Vec<(&i64, &u64)> = table.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        for (index, count) in ranked {
            let named = usize::try_from(*index)
                .ok()
                .and_then(|i| SWF.get(i))
                .copied()
                .unwrap_or("<outside SWF's table>");
            println!("  {count:6}  index {index:<4} {named}");
        }
    };
    show("GetProperty", &get);
    show("SetProperty", &set);
    println!("{unknown_operand} occurrence(s) whose index is not an immediately preceding literal");
}

fn pushdata_census(library: &mut MovieLibrary, names: &[String]) {
    let mut values: BTreeMap<u32, u64> = BTreeMap::new();
    let mut total = 0u64;
    let mut preceded_by_setregister = 0u64;
    let mut set_registers: BTreeMap<u32, u64> = BTreeMap::new();
    let mut used_as_object = 0u64;
    for name in names {
        if library.ensure_loaded(name).is_err() {
            continue;
        }
        let Some(movie) = library.movie(name).cloned() else {
            continue;
        };
        let Some(assets) = library.assets(name) else {
            continue;
        };
        let apt = assets.apt.clone();
        let consts = assets.consts.clone();
        for site in sites(&apt, &movie) {
            let Ok(block) = decode_block(&apt, site.offset, &DecodeLimits::default()) else {
                continue;
            };
            let mut regions: Vec<&[Instruction]> = vec![&block.code];
            for function in &block.functions {
                regions.push(&function.body);
            }
            let mut written: Vec<u32> = Vec::new();
            for region in &regions {
                for instruction in region.iter() {
                    if let Op::SetRegister(index) = instruction.op {
                        written.push(index);
                        *set_registers.entry(index).or_default() += 1;
                    }
                }
            }
            for region in &regions {
                for (i, instruction) in region.iter().enumerate() {
                    let Op::PushData(list) = &instruction.op else {
                        continue;
                    };
                    for item in list {
                        total += 1;
                        let value = match consts.get(*item as usize) {
                            Some(ConstItem::Number(n)) => *n,
                            _ => continue,
                        };
                        *values.entry(value).or_default() += 1;
                        if written.contains(&value) {
                            preceded_by_setregister += 1;
                        }
                        if let Some(next) = region.get(i.saturating_add(1)) {
                            if matches!(
                                next.op,
                                Op::EaGetNamedMember(_) | Op::GetMember | Op::EaGetStringMember(_)
                            ) {
                                used_as_object += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("PushData operands: {total} across the install");
    println!("  distinct values: {}", values.len());
    let mut ranked: Vec<(&u32, &u64)> = values.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    for (value, count) in ranked.iter().take(20) {
        println!("    value {value:<6} x{count}");
    }
    println!(
        "  {preceded_by_setregister} of {total} name a number some SetRegister in the same block          also writes ({:.1}%)",
        if total == 0 {
            0.0
        } else {
            100.0 * (preceded_by_setregister as f64) / (total as f64)
        }
    );
    println!(
        "  {used_as_object} of {total} are immediately followed by a member read - i.e. the          pushed value is used as an object"
    );
    println!("  SetRegister targets:");
    let mut regs: Vec<(&u32, &u64)> = set_registers.iter().collect();
    regs.sort_by(|a, b| b.1.cmp(a.1));
    for (index, count) in regs.iter().take(12) {
        println!("    register {index:<4} written x{count}");
    }
}

enum Pushed {
    Text(String),
    Number,
    Other,
}

fn attach_census(library: &mut MovieLibrary, names: &[String]) {
    for name in names {
        let _ = library.ensure_loaded(name);
    }
    let mut exporters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in names {
        if let Some(movie) = library.movie(name) {
            for e in &movie.exports {
                exporters
                    .entry(e.name.to_ascii_lowercase())
                    .or_default()
                    .push(name.clone());
            }
        }
    }
    let (mut total, mut literal, mut own_export, mut own_import, mut elsewhere, mut nowhere) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    for name in names {
        let Some(movie) = library.movie(name).cloned() else {
            continue;
        };
        let Some(assets) = library.assets(name) else {
            continue;
        };
        let apt = assets.apt.clone();
        let consts = assets.consts.clone();
        for site in sites(&apt, &movie) {
            let Ok(block) = decode_block(&apt, site.offset, &DecodeLimits::default()) else {
                continue;
            };
            let mut regions: Vec<&[Instruction]> = vec![&block.code];
            for function in &block.functions {
                regions.push(&function.body);
            }
            let mut pool: Vec<String> = Vec::new();
            for region in regions {
                let mut log: Vec<Pushed> = Vec::new();
                for instruction in region {
                    let method_is_attach = match &instruction.op {
                        Op::ConstantPool(list) => {
                            pool = list
                                .iter()
                                .map(|i| match consts.get(*i as usize) {
                                    Some(ConstItem::Text(t)) => t.clone(),
                                    _ => String::new(),
                                })
                                .collect();
                            false
                        }
                        Op::EaPushString(s) => {
                            log.push(Pushed::Text(s.clone()));
                            false
                        }
                        Op::EaPushConstant(i) => {
                            log.push(
                                pool.get(usize::from(*i))
                                    .cloned()
                                    .map_or(Pushed::Other, Pushed::Text),
                            );
                            false
                        }
                        Op::EaPushWordConstant(i) => {
                            log.push(
                                pool.get(usize::from(*i))
                                    .cloned()
                                    .map_or(Pushed::Other, Pushed::Text),
                            );
                            false
                        }
                        Op::PushData(list) => {
                            for i in list {
                                log.push(match consts.get(*i as usize) {
                                    Some(ConstItem::Text(t)) => Pushed::Text(t.clone()),
                                    Some(ConstItem::Number(_)) => Pushed::Number,
                                    None => Pushed::Other,
                                });
                            }
                            false
                        }
                        Op::EaPushByte(_)
                        | Op::EaPushShort(_)
                        | Op::EaPushFloat(_)
                        | Op::EaPushLong(_)
                        | Op::EaPushZero
                        | Op::EaPushOne => {
                            log.push(Pushed::Number);
                            false
                        }
                        Op::CallMethod | Op::EaCallMethod | Op::EaCallMethodPop => {
                            matches!(log.iter().rev().find_map(|p| match p {
                                Pushed::Text(t) => Some(t.as_str()),
                                _ => None,
                            }), Some(t) if t.eq_ignore_ascii_case("attachMovie"))
                        }
                        Op::EaCallNamedMethod(i)
                        | Op::EaCallNamedMethodPop(i)
                        | Op::EaCallNamedFunction(i)
                        | Op::EaCallNamedFunctionPop(i) => pool
                            .get(usize::from(*i))
                            .is_some_and(|t| t.eq_ignore_ascii_case("attachMovie")),
                        _ => false,
                    };
                    if !method_is_attach {
                        continue;
                    }
                    total += 1;
                    let export = log.iter().enumerate().rev().find_map(|(j, p)| {
                        let (Pushed::Number, Some(Pushed::Text(export))) =
                            (p, j.checked_sub(1).and_then(|k| log.get(k)))
                        else {
                            return None;
                        };
                        Some(export.clone())
                    });
                    let Some(export) = export else {
                        println!("{name}: {} @ {:#x}: non-literal", site.label, site.offset);
                        continue;
                    };
                    literal += 1;
                    let lower = export.to_ascii_lowercase();
                    let verdict = if movie
                        .exports
                        .iter()
                        .any(|e| e.name.eq_ignore_ascii_case(&lower))
                    {
                        own_export += 1;
                        "own export".to_owned()
                    } else if movie
                        .imports
                        .iter()
                        .any(|i| i.name.eq_ignore_ascii_case(&lower))
                    {
                        own_import += 1;
                        "OWN IMPORT (the root fallback never saw it)".to_owned()
                    } else if let Some(list) = exporters.get(&lower) {
                        elsewhere += 1;
                        format!("exported only by {list:?}")
                    } else {
                        nowhere += 1;
                        "exported by no movie".to_owned()
                    };
                    println!(
                        "{name}: {} @ {:#x}: {export:?}: {verdict}",
                        site.label, site.offset
                    );
                }
            }
        }
    }
    println!(
        "attachMovie sites {total}: literal {literal} (own export {own_export}, own import \
         {own_import}, ancestor export {elsewhere}, no exporter {nowhere}), non-literal {}",
        total.saturating_sub(literal)
    );
}

struct Site {
    label: String,
    offset: u32,
}

fn sites(apt: &[u8], movie: &AptMovie) -> Vec<Site> {
    let mut out = Vec::new();
    for (id, character) in movie.characters.iter().enumerate() {
        if let Character::Button(button) = character {
            for i in 0..button.action_count as usize {
                let Some(at) = (button.actions_offset as usize).checked_add(i.wrapping_mul(8))
                else {
                    continue;
                };
                let (Some(condition), Some(ptr)) =
                    (u32_at(apt, at), u32_at(apt, at.wrapping_add(4)))
                else {
                    continue;
                };
                out.push(Site {
                    label: format!("button #{id} action[{i}] condition={condition:#06x}"),
                    offset: ptr,
                });
            }
        }
    }
    let mut timelines: Vec<(String, u32, u32)> = vec![(
        "movie timeline".to_owned(),
        movie.record.frame_count,
        movie.record.frames_offset,
    )];
    for (id, character) in movie.characters.iter().enumerate() {
        if let Character::Sprite(sprite) = character {
            timelines.push((
                format!("sprite #{id}"),
                sprite.frame_count,
                sprite.frames_offset,
            ));
        }
    }
    for (owner, count, offset) in timelines {
        let Ok(frames) = parse_frames(apt, count, offset) else {
            continue;
        };
        frame_sites(apt, &frames, &owner, &mut out);
    }
    out
}

fn frame_sites(apt: &[u8], frames: &[Frame], owner: &str, out: &mut Vec<Site>) {
    for (index, frame) in frames.iter().enumerate() {
        let label = frame.items.iter().find_map(|item| match item {
            FrameItem::FrameLabel(l) => Some(l.label.clone()),
            _ => None,
        });
        let where_ = match &label {
            Some(l) => format!("{owner} frame {index} \"{l}\""),
            None => format!("{owner} frame {index}"),
        };
        for item in &frame.items {
            match item {
                FrameItem::Action(a) => out.push(Site {
                    label: format!("{where_} action"),
                    offset: a.block_offset,
                }),
                FrameItem::InitAction(i) => out.push(Site {
                    label: format!("{where_} init action"),
                    offset: i.block_offset,
                }),
                FrameItem::PlaceObject(po) => {
                    let Some(poa) = po.clip_actions_offset else {
                        continue;
                    };
                    let named = po.name.clone().unwrap_or_default();
                    let base = poa as usize;
                    let (Some(count), Some(array)) =
                        (u32_at(apt, base), u32_at(apt, base.wrapping_add(4)))
                    else {
                        continue;
                    };
                    for i in 0..count as usize {
                        let Some(at) = (array as usize).checked_add(i.wrapping_mul(12)) else {
                            continue;
                        };
                        let (Some(flags), Some(ptr)) =
                            (u32_at(apt, at), u32_at(apt, at.wrapping_add(8)))
                        else {
                            continue;
                        };
                        out.push(Site {
                            label: format!(
                                "{where_} clip action on depth {} \"{named}\" flags={flags:#x}",
                                po.depth
                            ),
                            offset: ptr,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(at..end)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Some(u32::from_le_bytes(buf))
}

fn strings_of(block: &Block, consts: &[ConstItem]) -> Vec<String> {
    let mut out = Vec::new();
    let push_code = |code: &[Instruction], out: &mut Vec<String>| {
        for instruction in code {
            match &instruction.op {
                Op::EaPushString(s)
                | Op::EaGetStringVar(s)
                | Op::EaSetStringVar(s)
                | Op::EaGetStringMember(s)
                | Op::EaSetStringMember(s)
                | Op::SetTarget(s)
                | Op::GotoLabel(s) => out.push(s.clone()),
                Op::GetUrl { url, target } => {
                    out.push(url.clone());
                    out.push(target.clone());
                }
                Op::ConstantPool(list) => {
                    for index in list {
                        if let Some(ConstItem::Text(t)) = consts.get(*index as usize) {
                            out.push(t.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    };
    push_code(&block.code, &mut out);
    for function in &block.functions {
        out.push(function.name.clone());
        push_code(&function.body, &mut out);
    }
    out
}

fn scan_movie(library: &mut MovieLibrary, name: &str, needle: Option<&str>, dump_all: bool) {
    if library.ensure_loaded(name).is_err() {
        return;
    }
    let Some(movie) = library.movie(name).cloned() else {
        return;
    };
    let Some(assets) = library.assets(name) else {
        return;
    };
    let apt = assets.apt.clone();
    let consts = assets.consts.clone();
    for site in sites(&apt, &movie) {
        let Ok(block) = decode_block(&apt, site.offset, &DecodeLimits::default()) else {
            continue;
        };
        let hit = match needle {
            None => true,
            Some(n) => {
                let lower = n.to_ascii_lowercase();
                strings_of(&block, &consts)
                    .iter()
                    .any(|s| s.to_ascii_lowercase().contains(&lower))
                    || site.label.to_ascii_lowercase().contains(&lower)
            }
        };
        if !(hit || (dump_all && needle.is_none())) {
            continue;
        }
        println!("\n=== {name}: {} @ {:#x} ===", site.label, site.offset);
        let pool = print_code(&block.code, &consts, "  ", &[]);
        for function in &block.functions {
            println!(
                "  fn `{}` flags {:#x} size {} params {:?}",
                function.name, function.flags, function.body_size, function.params
            );
            print_code(&function.body, &consts, "      ", &pool);
        }
    }
}

fn print_code(
    code: &[Instruction],
    consts: &[ConstItem],
    indent: &str,
    start_pool: &[String],
) -> Vec<String> {
    let mut pool: Vec<String> = start_pool.to_vec();
    for instruction in code {
        let mut note = String::new();
        match &instruction.op {
            Op::ConstantPool(list) => {
                pool = list
                    .iter()
                    .map(|index| match consts.get(*index as usize) {
                        Some(ConstItem::Text(t)) => t.clone(),
                        Some(ConstItem::Number(n)) => n.to_string(),
                        None => "<out of range>".to_owned(),
                    })
                    .collect();
                note = format!("  pool = {pool:?}");
            }
            Op::EaPushConstant(index) => {
                if let Some(text) = pool.get(usize::from(*index)) {
                    note = format!("  -> {text:?}");
                }
            }
            Op::EaPushValueOfVar(index)
            | Op::EaGetNamedMember(index)
            | Op::EaCallNamedFunction(index)
            | Op::EaCallNamedFunctionPop(index)
            | Op::EaCallNamedMethod(index)
            | Op::EaCallNamedMethodPop(index) => {
                if let Some(text) = pool.get(usize::from(*index)) {
                    note = format!("  -> {text:?}");
                }
            }
            Op::PushData(list) => {
                let values: Vec<String> = list
                    .iter()
                    .map(|index| match consts.get(*index as usize) {
                        Some(ConstItem::Text(t)) => format!("{t:?}"),
                        Some(ConstItem::Number(n)) => n.to_string(),
                        None => "<out of range>".to_owned(),
                    })
                    .collect();
                note = format!("  -> [{}]", values.join(", "));
            }
            _ => {}
        }
        println!(
            "{indent}{:#08x}  {:?}{note}",
            instruction.offset, instruction.op
        );
    }
    pool
}
