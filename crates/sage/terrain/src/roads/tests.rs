use super::*;
use mint_sage_map::Dictionary;

fn flat_map(width: u32, height: u32, sample: u16) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![sample; (width * height) as usize],
    }
}

fn point(name: &str, x: f32, y: f32, flags: i32) -> Object {
    Object {
        x,
        y,
        z: 0.0,
        angle: 0.0,
        flags,
        template_name: name.to_owned(),
        properties: Dictionary::default(),
    }
}

fn style(_: &str) -> Option<RoadStyle> {
    Some(RoadStyle {
        width: 40.0,
        width_in_texture: 0.9,
    })
}

#[test]
fn a_pair_is_the_next_object_and_nothing_else() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 10.0, 0.0, ROAD_END_FLAG),
        point("r", 10.0, 0.0, ROAD_START_FLAG),
        point("r", 20.0, 0.0, ROAD_END_FLAG),
        point("tree", 5.0, 5.0, 0),
    ];
    let paired = road_segments(&objects);
    assert_eq!(paired.segments.len(), 2);
    assert_eq!(paired.segments[0].first, 0);
    assert_eq!(paired.segments[0].second, 1);
    assert_eq!(paired.segments[1].first, 2);
    assert_eq!(paired.unpaired_starts, 0);
    assert_eq!(paired.orphan_ends, 0);
    assert_eq!(paired.both_bits, 0);
}

#[test]
fn a_start_with_no_end_is_counted_and_not_invented() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("tree", 500.0, 500.0, 0),
        point("r", 1.0, 1.0, ROAD_END_FLAG),
    ];
    let paired = road_segments(&objects);
    assert!(paired.segments.is_empty());
    assert_eq!(paired.unpaired_starts, 1);
    assert_eq!(paired.orphan_ends, 1);
}

#[test]
fn endpoints_join_only_on_exact_equality() {
    let shared = 100.0f32;
    let joined = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", shared, 0.0, ROAD_END_FLAG),
        point("r", shared, 0.0, ROAD_START_FLAG),
        point("r", shared, 100.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 100);
    let built = build_roads(&hm, 0.04, &joined, &RoadOptions::default(), style);
    assert_eq!(built.stats.mitred, 1);
    assert_eq!(built.stats.endpoint_classes[1], 1, "one two-way join");
    assert_eq!(built.stats.endpoint_classes[0], 2, "two free ends");

    let mut apart = joined;
    apart[2].x = shared + 0.01;
    let built = build_roads(&hm, 0.04, &apart, &RoadOptions::default(), style);
    assert_eq!(built.stats.mitred, 0);
    assert_eq!(built.stats.endpoint_classes[0], 4, "four free ends");
}

#[test]
fn a_zero_length_segment_is_nudged_rather_than_dividing_by_zero() {
    let objects = vec![
        point("r", 50.0, 50.0, ROAD_START_FLAG),
        point("r", 50.0, 50.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(32, 32, 200);
    let built = build_roads(&hm, 0.04, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.degenerate, 1);
    assert_eq!(built.stats.drawn, 1);
    let mesh = &built.meshes[0].mesh;
    assert_eq!(mesh.validate(), None);
    for vertex in &mesh.vertices {
        assert!(
            vertex.position.into_iter().all(f32::is_finite),
            "{:?}",
            vertex.position
        );
    }
}

#[test]
fn the_ribbon_covers_the_atlas_straight_band_and_repeats_isotropically() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 160.0, 0.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(
        &hm,
        0.0,
        &objects,
        &RoadOptions {
            tessellation: RoadTessellation::Ribbon,
            ..RoadOptions::default()
        },
        style,
    );
    let mesh = &built.meshes[0].mesh;
    let left = mesh.vertices[0].uv;
    let right = mesh.vertices[1].uv;
    assert!((left[1] - 0.053_515_6).abs() < 1e-5, "{left:?}");
    assert!((right[1] - 0.278_515_6).abs() < 1e-5, "{right:?}");
    assert!((left[0]).abs() < 1e-6, "u restarts at the segment start");
    let end = mesh.vertices[mesh.vertices.len() - 2].uv;
    assert!((end[0] - 1.0).abs() < 1e-5, "{end:?}");
}

#[test]
fn a_mitred_corner_cuts_the_texture_rather_than_deforming_it() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 200.0, 0.0, ROAD_END_FLAG),
        point("r", 200.0, 0.0, ROAD_START_FLAG),
        point("r", 200.0, 200.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(
        &hm,
        0.0,
        &objects,
        &RoadOptions {
            tessellation: RoadTessellation::Ribbon,
            ..RoadOptions::default()
        },
        style,
    );
    assert_eq!(built.stats.mitred, 1, "a right angle mitres");
    let mesh = &built.meshes[0].mesh;
    let repeat = 4.0 * 40.0;

    let mut widest = 0.0f32;
    for station in mesh.vertices.chunks_exact(2) {
        let [left, right] = [&station[0], &station[1]];
        let length = |dx: f32, dy: f32| (dx * dx + dy * dy).sqrt();
        let ground = length(
            left.position[0] - right.position[0],
            left.position[1] - right.position[1],
        );
        let texture = length(left.uv[0] - right.uv[0], left.uv[1] - right.uv[1]) * repeat;
        assert!(
            (ground - texture).abs() < 1e-3,
            "{ground} world units across map to {texture}"
        );
        widest = widest.max(ground);
    }
    let nominal = 40.0 * 0.9;
    assert!(
        widest > nominal * 1.3,
        "the widest station is {widest}, nominal {nominal}"
    );
}

#[test]
fn two_segments_filed_nose_to_nose_paint_the_seam_the_same_way() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 0.0, ROAD_END_FLAG),
        point("r", 200.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 0.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let build = |order| {
        build_roads(
            &hm,
            0.0,
            &objects,
            &RoadOptions {
                tessellation: RoadTessellation::Ribbon,
                chain_order: order,
                ..RoadOptions::default()
            },
            style,
        )
    };
    let (half_width, repeat) = (40.0 * 0.9 / 2.0, 4.0 * 40.0);
    let seam = |built: &RoadNetwork| {
        let mut found: Vec<f32> = built.meshes[0]
            .mesh
            .vertices
            .iter()
            .filter(|vertex| {
                (vertex.position[0] - 100.0).abs() < 1e-3
                    && (vertex.position[1] - half_width).abs() < 1e-3
            })
            .map(|vertex| vertex.uv[1])
            .collect();
        found.sort_by(f32::total_cmp);
        found
    };

    let chained = build(RoadChainOrder::Chained);
    assert_eq!(
        chained.stats.opposed_joins, 1,
        "the file files them opposed"
    );
    assert_eq!(
        chained.stats.reoriented, 1,
        "one of the two is turned round"
    );
    assert_eq!(chained.stats.orientation_conflicts, 0);
    let after = seam(&chained);
    assert_eq!(after.len(), 2, "both segments reach the shared corner");
    assert!(
        (after[1] - after[0]).abs() < 1e-6,
        "the two disagree by {} after chaining",
        after[1] - after[0]
    );

    let filed = build(RoadChainOrder::AsFile);
    let before = seam(&filed);
    assert_eq!(before.len(), 2);
    assert!(
        (before[1] - before[0] - 2.0 * half_width / repeat).abs() < 1e-5,
        "filed as they are the two are {} apart, and the band is {} wide",
        before[1] - before[0],
        2.0 * half_width / repeat
    );
    assert_eq!(filed.stats.vertices, chained.stats.vertices);
    assert_eq!(filed.stats.triangles, chained.stats.triangles);
}

#[test]
fn a_tee_junction_takes_its_own_atlas_cell_and_cuts_the_arms_back() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 0.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 200.0, 0.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 100.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(&hm, 0.0, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.tees, 1);
    assert_eq!(built.stats.junction_y, 0);
    assert_eq!(built.stats.junction_h, 0);
    assert_eq!(built.stats.endpoint_classes[2], 1, "one three-way");

    let mesh = &built.meshes[0].mesh;
    let piece: Vec<&Vertex> = mesh
        .vertices
        .iter()
        .filter(|v| (v.uv[1] - TEE_CELL_V).abs() < 0.2)
        .collect();
    assert!(!piece.is_empty(), "no vertex sampled the tee cell");
    let bound = |f: fn(&Vertex) -> f32| {
        piece.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(f(v)), hi.max(f(v)))
        })
    };
    let (x0, x1) = bound(|v| v.position[0]);
    let (y0, y1) = bound(|v| v.position[1]);
    for (got, want) in [(x0, 79.4), (x1, 120.6), (y0, -18.0), (y1, 20.6)] {
        assert!((got - want).abs() < 1e-2, "corner {got}, expected {want}");
    }
    let (u0, u1) = bound(|v| v.uv[0]);
    let (v0, v1) = bound(|v| v.uv[1]);
    for (got, want) in [
        (u0, JUNCTION_CELL_U - 18.0 / 160.0),
        (u1, JUNCTION_CELL_U + 20.6 / 160.0),
        (v0, TEE_CELL_V - 20.6 / 160.0),
        (v1, TEE_CELL_V + 20.6 / 160.0),
    ] {
        assert!((got - want).abs() < 1e-4, "uv {got}, expected {want}");
    }

    let nearest = mesh
        .vertices
        .iter()
        .filter(|v| (v.uv[1] - STRAIGHT_BAND_V).abs() < 0.2)
        .map(|v| {
            let (dx, dy) = (v.position[0] - 100.0, v.position[1]);
            (dx * dx + dy * dy).sqrt()
        })
        .fold(f32::MAX, f32::min);
    assert!(nearest > 17.9, "an arm still reaches to {nearest}");
}

#[test]
fn a_crossroads_is_square_and_takes_the_cell_below_the_tee() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 0.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 200.0, 0.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 100.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, -100.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(&hm, 0.0, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.crossroads, 1);
    assert_eq!(built.stats.tees, 0);
    let mesh = &built.meshes[0].mesh;
    let piece: Vec<&Vertex> = mesh
        .vertices
        .iter()
        .filter(|v| (v.uv[1] - CROSSROADS_CELL_V).abs() < 0.2)
        .collect();
    assert!(!piece.is_empty(), "no vertex sampled the crossroads cell");
    let span = |f: fn(&Vertex) -> f32| {
        let (lo, hi) = piece.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(f(v)), hi.max(f(v)))
        });
        hi - lo
    };
    let side = JUNCTION_WIDTH_ADJUSTMENT * 40.0;
    assert!((span(|v| v.position[0]) - side).abs() < 1e-2);
    assert!((span(|v| v.position[1]) - side).abs() < 1e-2);
}

#[test]
fn three_arms_at_equal_angles_are_declined_rather_than_teed() {
    let mut objects = Vec::new();
    for (dx, dy) in [(0.0, 100.0), (-86.602_54, -50.0), (86.602_54, -50.0)] {
        objects.push(point("r", 200.0, 200.0, ROAD_START_FLAG));
        objects.push(point("r", 200.0 + dx, 200.0 + dy, ROAD_END_FLAG));
    }
    let hm = flat_map(64, 64, 0);
    let built = build_roads(&hm, 0.0, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.endpoint_classes[2], 1, "one three-way");
    assert_eq!(built.stats.junction_y, 1);
    assert_eq!(built.stats.tees, 0);
}

#[test]
fn a_junction_arms_band_follows_the_ribbon_it_is_drawn_on() {
    let objects = vec![
        point("r", 300.0, 300.0, ROAD_START_FLAG),
        point("r", 104.38, 258.42, ROAD_END_FLAG),
        point("r", 300.0, 300.0, ROAD_START_FLAG),
        point("r", 495.62, 258.42, ROAD_END_FLAG),
        point("r", 300.0, 300.0, ROAD_START_FLAG),
        point("r", 300.0, 500.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(&hm, 0.0, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.tees, 1, "the fixture must build a tee");
    let mesh = &built.meshes[0];
    let half_band = mesh.style.width_in_texture / 8.0;
    let worst = mesh.mesh.vertices[..mesh.segment_vertices]
        .iter()
        .fold(0.0f32, |held, vertex| {
            let over = ((vertex.uv[1] - STRAIGHT_BAND_V).abs() - half_band) / half_band;
            held.max(over)
        });
    assert!(
        worst < 0.1,
        "the band should stay on the ribbon it is drawn on, worst {worst}"
    );
}

#[test]
fn a_capped_free_end_gets_a_piece_that_reaches_past_the_waypoint() {
    let objects = vec![
        point("r", 100.0, 200.0, ROAD_START_FLAG),
        point("r", 300.0, 200.0, ROAD_END_FLAG | ROAD_END_CAP_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let capped = build_roads(&hm, 0.0, &objects, &RoadOptions::default(), style);
    assert_eq!(capped.stats.end_caps, 1);
    assert_eq!(capped.stats.end_caps_drawn, 1);

    let mesh = &capped.meshes[0];
    let piece = &mesh.mesh.vertices[mesh.segment_vertices..];
    assert!(!piece.is_empty(), "the cap should have vertices of its own");
    let width = 40.0f32;
    let furthest = piece
        .iter()
        .fold(f32::NEG_INFINITY, |held, v| held.max(v.position[0]));
    let overhang = furthest - 300.0;
    let expected = width * END_CAP_LENGTH * END_CAP_OVERHANG;
    assert!(
        (overhang - expected).abs() < 1e-3,
        "the cap should reach {expected} past the end, reached {overhang}"
    );
    let across = piece.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
        (lo.min(v.position[1]), hi.max(v.position[1]))
    });
    assert!(
        (across.1 - across.0 - width * END_CAP_WIDTH).abs() < 1e-3,
        "cap width {:?}",
        across.1 - across.0
    );
    for vertex in piece {
        assert!(
            (vertex.uv[1] - END_CAP_CELL_V).abs() < 0.2,
            "cap v {:?} is not on its own cell",
            vertex.uv[1]
        );
        assert!(
            (vertex.uv[0] - END_CAP_CELL_U).abs() < 0.1,
            "cap u {:?} is not on its own cell",
            vertex.uv[0]
        );
    }
}

#[test]
fn an_undeclared_road_type_is_skipped_and_named() {
    let objects = vec![
        point("nosuchroad", 0.0, 0.0, ROAD_START_FLAG),
        point("nosuchroad", 50.0, 0.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(32, 32, 0);
    let built = build_roads(&hm, 0.04, &objects, &RoadOptions::default(), |_| None);
    assert!(built.meshes.is_empty());
    assert_eq!(built.stats.drawn, 0);
    assert_eq!(
        built.stats.skipped.get(&RoadSkip::UndeclaredType).copied(),
        Some(1)
    );
    assert_eq!(built.undeclared, vec!["nosuchroad".to_owned()]);
}

#[test]
fn a_hairpin_corner_is_left_square_rather_than_spiked() {
    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", 100.0, 0.0, ROAD_END_FLAG),
        point("r", 100.0, 0.0, ROAD_START_FLAG),
        point("r", 1.0, 1.0, ROAD_END_FLAG),
    ];
    let hm = flat_map(64, 64, 0);
    let built = build_roads(&hm, 0.04, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.mitred, 0);
    assert_eq!(built.stats.mitre_capped, 1);
    let mesh = &built.meshes[0].mesh;
    let far = mesh
        .vertices
        .iter()
        .map(|v| v.position[0])
        .fold(0.0f32, f32::max);
    assert!(far < 120.0, "a spike reached x = {far}");
}

#[test]
fn the_lift_raises_every_vertex_and_the_two_height_rules_differ_on_a_slope() {
    let mut hm = flat_map(16, 16, 0);
    for y in 0..16u32 {
        for x in 0..16u32 {
            hm.samples[(y * 16 + x) as usize] = u16::try_from(x * 100).unwrap_or(0);
        }
    }
    let objects = vec![
        point("r", 50.0, 50.0, ROAD_START_FLAG),
        point("r", 50.0, 100.0, ROAD_END_FLAG),
    ];
    let flat = build_roads(
        &hm,
        0.04,
        &objects,
        &RoadOptions {
            lift: 0.0,
            height: RoadHeight::Surface,
            tessellation: RoadTessellation::Ribbon,
            chain_order: RoadChainOrder::Chained,
        },
        style,
    );
    let lifted = build_roads(
        &hm,
        0.04,
        &objects,
        &RoadOptions {
            lift: ROAD_LIFT_XY_128,
            height: RoadHeight::Surface,
            tessellation: RoadTessellation::Ribbon,
            chain_order: RoadChainOrder::Chained,
        },
        style,
    );
    for (a, b) in flat.meshes[0]
        .mesh
        .vertices
        .iter()
        .zip(&lifted.meshes[0].mesh.vertices)
    {
        assert!((b.position[2] - a.position[2] - ROAD_LIFT_XY_128).abs() < 1e-4);
    }

    let maxed = build_roads(
        &hm,
        0.04,
        &objects,
        &RoadOptions {
            lift: 0.0,
            height: RoadHeight::StationMax,
            tessellation: RoadTessellation::Ribbon,
            chain_order: RoadChainOrder::Chained,
        },
        style,
    );
    let (l, r) = (
        flat.meshes[0].mesh.vertices[0].position[2],
        flat.meshes[0].mesh.vertices[1].position[2],
    );
    assert!((l - r).abs() > 1.0, "the slope should tilt the ribbon");
    let (ml, mr) = (
        maxed.meshes[0].mesh.vertices[0].position[2],
        maxed.meshes[0].mesh.vertices[1].position[2],
    );
    assert!((ml - mr).abs() < 1e-6, "a station shares one z");
    assert!(ml >= l.max(r) - 1e-4, "and it is the higher of the two");
}

#[test]
fn a_rebuild_keeps_the_topology_a_vertex_rewrite_needs() {
    let mut hm = flat_map(16, 16, 0);
    for y in 0..16u32 {
        for x in 0..16u32 {
            hm.samples[(y * 16 + x) as usize] = u16::try_from(x * 37 + y * 11).unwrap_or(0);
        }
    }
    let objects = vec![
        point("r", 20.0, 20.0, ROAD_START_FLAG),
        point("r", 120.0, 65.0, ROAD_END_FLAG),
        point("r", 120.0, 65.0, ROAD_START_FLAG),
        point("r", 60.0, 130.0, ROAD_END_FLAG),
    ];
    let shape = |scale: f32, options: &RoadOptions| {
        build_roads(&hm, scale, &objects, options, style)
            .meshes
            .iter()
            .map(|road| (road.mesh.vertices.len(), road.mesh.indices.len()))
            .collect::<Vec<_>>()
    };
    let base = shape(0.04, &RoadOptions::default());
    assert!(!base.is_empty());
    for options in [
        RoadOptions {
            lift: 0.0,
            height: RoadHeight::Surface,
            ..RoadOptions::default()
        },
        RoadOptions {
            lift: ROAD_LIFT_HEIGHT_STEP_EIGHTH,
            height: RoadHeight::StationMax,
            ..RoadOptions::default()
        },
    ] {
        for scale in [0.0, 0.04, 0.5] {
            assert_eq!(base, shape(scale, &options), "{options:?} at scale {scale}");
        }
    }
}

#[test]
fn a_wild_coordinate_cannot_size_the_allocation() {
    assert_eq!(station_count(f32::NAN, 1.0, false), 2);
    assert_eq!(station_count(f32::INFINITY, 1.0, false), 2);
    assert_eq!(station_count(1e30, 1.0, false), MAX_STATIONS);
    assert_eq!(station_count(-5.0, 1.0, false), 2);
    assert_eq!(station_count(0.0, 1.0, false), 2);
    assert_eq!(station_count(105.0, 1.0, false), 11);

    let objects = vec![
        point("r", 0.0, 0.0, ROAD_START_FLAG),
        point("r", f32::NAN, 1e30, ROAD_END_FLAG),
    ];
    let hm = flat_map(16, 16, 0);
    let built = build_roads(&hm, 0.04, &objects, &RoadOptions::default(), style);
    assert_eq!(built.stats.drawn, 0);
    assert_eq!(
        built.stats.skipped.get(&RoadSkip::NonFinite).copied(),
        Some(1)
    );
}
