use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, Object};
use mint_sage_model::{load_object_index, ObjectIndex};
use mint_sage_terrain::{
    build_roads, RoadChainOrder, RoadMesh, RoadOptions, RoadSegmentBuild, RoadStyle,
    RoadTessellation, StepsPerCell, ROAD_END_CAP_FLAG, ROAD_END_FLAG, ROAD_START_FLAG,
    TEXTURE_REPEAT_WIDTHS,
};
use mint_vfs::Vfs;

const VERTICAL_SCALE: f32 = 0.039_062_5;

const DEFAULT_RADIUS: f32 = 40.0;

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut options = RoadOptions::default();
    let mut wanted: Option<String> = None;
    let mut at: Option<[f32; 2]> = None;
    let mut radius = DEFAULT_RADIUS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--map" => wanted = args.next().map(|value| value.to_ascii_lowercase()),
            "--at" => {
                let (Some(x), Some(y)) = (
                    args.next().and_then(|v| v.parse::<f32>().ok()),
                    args.next().and_then(|v| v.parse::<f32>().ok()),
                ) else {
                    eprintln!("--at wants two numbers");
                    return ExitCode::FAILURE;
                };
                at = Some([x, y]);
            }
            "--radius" => {
                let Some(value) = args.next().and_then(|v| v.parse::<f32>().ok()) else {
                    eprintln!("--radius wants a number");
                    return ExitCode::FAILURE;
                };
                radius = value;
            }
            "--tessellation" => {
                options.tessellation = match args.next().as_deref() {
                    Some("ribbon") => RoadTessellation::Ribbon,
                    Some("1") => RoadTessellation::Grid(StepsPerCell::One),
                    Some("2") => RoadTessellation::Grid(StepsPerCell::Two),
                    Some("4") => RoadTessellation::Grid(StepsPerCell::Four),
                    other => {
                        eprintln!("--tessellation wants ribbon, 1, 2 or 4, not {other:?}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            "--chain-order" => {
                options.chain_order = match args.next().as_deref() {
                    Some("file") => RoadChainOrder::AsFile,
                    Some("chained") => RoadChainOrder::Chained,
                    Some("reversed") => RoadChainOrder::Reversed,
                    other => {
                        eprintln!("--chain-order wants file, chained or reversed, not {other:?}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            other => roots.push(PathBuf::from(other)),
        }
    }
    let (Some(wanted), Some(at)) = (wanted, at) else {
        eprintln!("usage: road_probe --map <substring> --at <x> <y> [--radius <r>] <install dir>");
        return ExitCode::FAILURE;
    };
    if roots.is_empty() {
        eprintln!("no install directory given");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    let (index, _) = load_object_index(&vfs);

    println!(
        "probe ({}, {}) radius {radius}; tessellation = {}, chain order = {}",
        at[0],
        at[1],
        options.tessellation.label(),
        options.chain_order.label()
    );

    let mut found = 0usize;
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !lower.ends_with(".map") || !lower.contains(&wanted) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&raw) else {
                    continue;
                };
                found = found.saturating_add(1);
                println!("\n=== {} ===", entry.name);
                probe(&map, &index, &options, at, radius);
            }
        }
    }
    if found == 0 {
        eprintln!("no map path contained {wanted:?}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn probe(map: &MapFile, index: &ObjectIndex, options: &RoadOptions, at: [f32; 2], radius: f32) {
    for (slot, object) in map.objects.iter().enumerate() {
        if object.flags & (ROAD_START_FLAG | ROAD_END_FLAG) == 0 {
            continue;
        }
        if near(at, [object.x, object.y], radius) {
            println!(
                "  placement #{slot} ({:.4}, {:.4}) {} flags 0x{:x}{}",
                object.x,
                object.y,
                object.template_name,
                object.flags,
                if object.flags & ROAD_END_CAP_FLAG != 0 {
                    " END-CAP"
                } else {
                    ""
                }
            );
        }
    }

    let built = build_roads(
        &map.height_map,
        VERTICAL_SCALE,
        &map.objects,
        options,
        |road| style_for(index, road),
    );

    for mesh in &built.meshes {
        let repeat = TEXTURE_REPEAT_WIDTHS * mesh.style.width;
        let mut ends: Vec<(usize, usize, [f32; 2])> = Vec::new();
        for (slot, build) in mesh.builds.iter().enumerate() {
            for (which, point) in [(0usize, build.start), (1usize, build.end)] {
                if near(at, point, radius) {
                    ends.push((slot, which, point));
                }
            }
        }
        if ends.is_empty() {
            continue;
        }
        println!(
            "\n-- road type {:?}: width {} width_in_texture {} (repeat {repeat} world units)",
            mesh.name, mesh.style.width, mesh.style.width_in_texture
        );
        for &(slot, which, point) in &ends {
            let Some(build) = mesh.builds.get(slot) else {
                continue;
            };
            report_end(mesh, build, slot, which, point, &map.objects);
        }

        for first in 0..ends.len() {
            for second in first.saturating_add(1)..ends.len() {
                let (Some(&(a_slot, a_which, a_point)), Some(&(b_slot, b_which, b_point))) =
                    (ends.get(first), ends.get(second))
                else {
                    continue;
                };
                if a_slot == b_slot {
                    continue;
                }
                let (Some(a), Some(b)) = (mesh.builds.get(a_slot), mesh.builds.get(b_slot)) else {
                    continue;
                };
                compare(
                    mesh,
                    (a, a_slot, a_which, a_point),
                    (b, b_slot, b_which, b_point),
                );
            }
        }
    }
}

fn report_end(
    mesh: &RoadMesh,
    build: &RoadSegmentBuild,
    slot: usize,
    which: usize,
    point: [f32; 2],
    objects: &[Object],
) {
    let name = objects
        .get(build.first)
        .map_or("?", |object| object.template_name.as_str());
    let filed = |slot: usize| {
        objects
            .get(slot)
            .map_or([f32::NAN, f32::NAN], |object| [object.x, object.y])
    };
    println!(
        "  segment #{slot} ({name}) placements {}/{} filed {:?} -> {:?}",
        build.first,
        build.second,
        filed(build.first),
        filed(build.second)
    );
    println!(
        "    reversed {} chain seed #{} half_width {:.6} dir ({:.6}, {:.6}) origin ({:.4}, {:.4})",
        build.reversed,
        if build.chain_seed == usize::MAX {
            -1i64
        } else {
            i64::try_from(build.chain_seed).unwrap_or(-1)
        },
        build.half_width,
        build.dir[0],
        build.dir[1],
        build.origin[0],
        build.origin[1]
    );
    let end = if which == 0 { "start" } else { "end" };
    println!("    {end} centre ({:.6}, {:.6})", point[0], point[1]);
    for (label, corner) in corners_of(build, which) {
        match nearest_vertex(mesh, build, corner) {
            Some((position, uv, gap)) => println!(
                "      {label:<12} corner ({:.6}, {:.6})  vertex ({:.6}, {:.6}, {:.6}) uv ({:.6}, {:.6})  gap {gap:.9}",
                corner[0], corner[1], position[0], position[1], position[2], uv[0], uv[1]
            ),
            None => println!(
                "      {label:<12} corner ({:.6}, {:.6})  no vertex",
                corner[0], corner[1]
            ),
        }
    }
    println!("    ribbon width there {:.6}", width_at(build, which));
}

fn corners_of(build: &RoadSegmentBuild, which: usize) -> [(&'static str, [f32; 2]); 2] {
    let [start_left, start_right, end_left, end_right] = build.corners;
    if which == 0 {
        [("start left", start_left), ("start right", start_right)]
    } else {
        [("end left", end_left), ("end right", end_right)]
    }
}

fn width_at(build: &RoadSegmentBuild, which: usize) -> f32 {
    let [(_, a), (_, b)] = corners_of(build, which);
    ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1])).sqrt()
}

fn compare(
    mesh: &RoadMesh,
    (a, a_slot, a_which, a_point): (&RoadSegmentBuild, usize, usize, [f32; 2]),
    (b, b_slot, b_which, b_point): (&RoadSegmentBuild, usize, usize, [f32; 2]),
) {
    let centre_gap = distance(a_point, b_point);
    let shared = a_point[0].to_bits() == b_point[0].to_bits()
        && a_point[1].to_bits() == b_point[1].to_bits();
    println!(
        "\n  * #{a_slot}.{} vs #{b_slot}.{}: centres {:.9} apart, bit-identical {}",
        end_name(a_which),
        end_name(b_which),
        centre_gap,
        shared
    );
    println!(
        "    directions dot {:.6}; half widths {:.6} vs {:.6} (delta {:.9})",
        a.dir[0] * b.dir[0] + a.dir[1] * b.dir[1],
        a.half_width,
        b.half_width,
        a.half_width - b.half_width
    );
    for (a_label, a_corner) in corners_of(a, a_which) {
        let mut best: Option<(&'static str, [f32; 2], f32)> = None;
        for (b_label, b_corner) in corners_of(b, b_which) {
            let gap = distance(a_corner, b_corner);
            if best.is_none_or(|(_, _, held)| gap < held) {
                best = Some((b_label, b_corner, gap));
            }
        }
        let Some((b_label, b_corner, gap)) = best else {
            continue;
        };
        let a_vertex = nearest_vertex(mesh, a, a_corner);
        let b_vertex = nearest_vertex(mesh, b, b_corner);
        print!("    {a_label:<12} vs {b_label:<12} position {gap:.9}");
        match (a_vertex, b_vertex) {
            (Some((_, a_uv, _)), Some((_, b_uv, _))) => println!(
                "  du {:+.6} dv {:+.6}   (u {:.6} vs {:.6}, v {:.6} vs {:.6})",
                a_uv[0] - b_uv[0],
                a_uv[1] - b_uv[1],
                a_uv[0],
                a_uv[1],
                b_uv[0],
                b_uv[1]
            ),
            _ => println!("  no vertex on one side"),
        }
        let _ = b_corner;
    }
}

fn end_name(which: usize) -> &'static str {
    if which == 0 {
        "start"
    } else {
        "end"
    }
}

fn nearest_vertex(
    mesh: &RoadMesh,
    build: &RoadSegmentBuild,
    point: [f32; 2],
) -> Option<([f32; 3], [f32; 2], f32)> {
    let (from, to) = build.vertices;
    let block = mesh.mesh.vertices.get(from..to)?;
    let mut best: Option<([f32; 3], [f32; 2], f32)> = None;
    for vertex in block {
        let gap = distance([vertex.position[0], vertex.position[1]], point);
        if !gap.is_finite() {
            continue;
        }
        if best.is_none_or(|(_, _, held)| gap < held) {
            best = Some((vertex.position, vertex.uv, gap));
        }
    }
    best
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

fn near(at: [f32; 2], point: [f32; 2], radius: f32) -> bool {
    distance(at, point) <= radius
}

fn style_for(index: &ObjectIndex, name: &str) -> Option<RoadStyle> {
    let road = index.road(name)?;
    Some(RoadStyle {
        width: road.width?,
        width_in_texture: road.width_in_texture?,
    })
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
        {
            out.push(path);
        }
    }
    out.sort();
    out
}
