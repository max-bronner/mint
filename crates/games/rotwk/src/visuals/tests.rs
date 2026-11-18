use super::*;
use mint_render::Vec3;

#[test]
fn a_radius_cursor_needs_a_template_a_texture_and_a_positive_radius() {
    let table = mint_sage_ini::BlockTable::corpus_path();
    let doc = |name: &str, text: &str| {
        mint_sage_ini::Document::parse(name, text, &table).expect("parses")
    };
    let mut buttons = mint_sage_ini::CommandButtonIndex::new();
    buttons.insert_document(
        &doc(
            "commandbutton.ini",
            "CommandButton Command_Aimed\n  Command = SPECIAL_POWER\n  \
               Options = NEED_TARGET_POS\n  SpecialPower = PowerAimed\n  \
               RadiusCursorType = AimedCursor\nEnd\n\
             CommandButton Command_Untextured\n  SpecialPower = PowerAimed\n  \
               RadiusCursorType = BareCursor\nEnd\n\
             CommandButton Command_Leader\n  SpecialPower = PowerNoRadius\n  \
               RadiusCursorType = AimedCursor\nEnd\n",
        ),
        &mint_sage_ini::MacroTable::new(),
    );
    let mut powers = mint_sage_ini::SpecialPowerIndex::new();
    powers.insert_document(
        &doc(
            "specialpower.ini",
            "SpecialPower PowerAimed\n  RadiusCursorRadius = 120.0\nEnd\n\
                 SpecialPower PowerNoRadius\n  ReloadTime = 0\nEnd\n",
        ),
        &mint_sage_ini::MacroTable::new(),
    );
    let mut templates = mint_sage_ini::RadiusCursorIndex::new();
    templates.insert_document(&doc(
        "ingameui.ini",
        "InGameUI\n  RadiusCursorTemplate = AimedCursor\n    \
               Texture = SCCArrowVolley\n    OpacityMin = 20%\n    \
               OpacityMax = 60%\n    OpacityThrobTime = 1000\n    \
               Color = R:255 G:128 B:0 A:255\n  End\n  \
               RadiusCursorTemplate = BareCursor\n  End\nEnd\n",
    ));
    let of = |name: &str| RadiusCursor::of(name, &buttons, &powers, &templates);
    let aimed = of("Command_Aimed").expect("the whole chain resolves");
    assert_eq!(aimed.texture, "SCCArrowVolley");
    assert!((aimed.radius - 120.0).abs() < 1e-6);
    assert_eq!(of("Command_Untextured"), None, "no Texture, nothing drawn");
    assert_eq!(of("Command_Leader"), None, "no radius, nothing drawn");
    assert_eq!(of("Command_Zero"), None, "a radius of 0 draws nothing");
    assert!((aimed.opacity(0, 30) - 0.4).abs() < 1e-3);
    assert!((aimed.opacity(7, 30) - aimed.opacity_max).abs() < 0.02);
}

#[test]
fn a_decal_ring_is_the_outer_mask_less_the_inner() {
    let px = |a: u8| [255u8, 255, 255, a];
    let outer: Vec<u8> = [px(255), px(255), px(255), px(128)].concat();
    let inner: Vec<u8> = px(255).to_vec();
    let whole = decal_ring_alpha((&outer, 2, 2), None);
    assert_eq!(
        whole.chunks(4).map(|p| p[3]).collect::<Vec<_>>(),
        [255, 255, 255, 128]
    );
    let ring = decal_ring_alpha((&outer, 2, 2), Some((&inner, 1, 1)));
    assert!(
        ring.chunks(4).all(|p| p[3] == 0),
        "an opaque inner cuts all"
    );
    let half_inner: Vec<u8> = [px(0), px(255), px(0), px(0)].concat();
    let cut = decal_ring_alpha((&outer, 2, 2), Some((&half_inner, 2, 2)));
    assert_eq!(
        cut.chunks(4).map(|p| p[3]).collect::<Vec<_>>(),
        [255, 0, 255, 128]
    );
    assert!(
        cut.chunks(4).all(|p| p[..3] == [255, 255, 255]),
        "white, tinted later"
    );
    let short = decal_ring_alpha((&outer[..6], 2, 2), None);
    assert_eq!(
        short.chunks(4).map(|p| p[3]).collect::<Vec<_>>(),
        [255, 0, 0, 0]
    );
}

#[test]
fn the_throb_period_rounds_up_rather_than_down() {
    let opacity = throb_opacity((0.0, 1.0), 2500, 1, 1);
    assert!((opacity - 0.933_012_7).abs() < 0.01, "{opacity}");
}

#[test]
fn a_selection_decal_is_min_radius_wide_under_one_object() {
    let decal = |min: Option<f32>, max: Option<f32>| mint_sage_modules::SelectionDecal {
        texture: "decal".into(),
        texture2: None,
        style: None,
        opacity: (1.0, 1.0),
        throb_ms: 1000,
        min_radius: min,
        max_radius: max,
        max_selected: Some(40.0),
    };
    let half = |min, max| selection_decal_width(&decal(min, max), 1);
    assert_eq!(half(Some(50.0), Some(200.0)), Some(25.0));
    assert_eq!(half(Some(140.0), Some(100.0)), Some(50.0));
    assert_eq!(half(Some(0.0), Some(200.0)), None);
    assert_eq!(half(Some(50.0), None), None);
    let untextured = mint_sage_modules::SelectionDecal {
        texture: String::new(),
        ..decal(Some(50.0), Some(200.0))
    };
    assert_eq!(selection_decal_width(&untextured, 1), None);
}

#[test]
fn a_selection_decal_grows_with_count_up_to_max_selected_units() {
    let decal = mint_sage_modules::SelectionDecal {
        texture: "decal".into(),
        texture2: None,
        style: None,
        opacity: (1.0, 1.0),
        throb_ms: 1000,
        min_radius: Some(50.0),
        max_radius: Some(200.0),
        max_selected: Some(40.0),
    };
    assert_eq!(selection_decal_width(&decal, 1), Some(25.0));
    assert_eq!(selection_decal_width(&decal, 21), Some(62.5));
    assert_eq!(selection_decal_width(&decal, 41), Some(100.0));
    assert_eq!(selection_decal_width(&decal, 100), Some(100.0));
    assert_eq!(selection_decal_width(&decal, 0), None);
}

#[test]
fn the_generated_circle_closes_and_stays_on_the_unit_circle() {
    let circle = unit_circle();
    assert_eq!(circle.len(), SEGMENTS);
    for (c, s) in &circle {
        assert!(
            (c * c + s * s - 1.0).abs() < 1e-3,
            "({c}, {s}) drifted off the unit circle"
        );
    }
    let (c, s) = *circle.first().unwrap();
    assert!((c - 1.0).abs() < 1e-6 && s.abs() < 1e-6);
    let quarter = circle.get(SEGMENTS / 4).unwrap();
    assert!(quarter.0.abs() < 1e-3 && (quarter.1 - 1.0).abs() < 1e-3);
}

#[test]
fn both_meshes_index_only_vertices_they_have() {
    for mesh in [body_mesh(), ring_mesh()] {
        assert_eq!(mesh.validate(), None, "an index points past the vertices");
        let count = index_of(mesh.vertices.len());
        assert!(mesh.indices.iter().all(|index| *index < count));
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh.triangle_count() > 0);
    }
}

#[test]
fn the_body_reaches_further_forward_than_backward() {
    let mesh = body_mesh();
    let max_x = mesh
        .vertices
        .iter()
        .map(|v| v.position[0])
        .fold(f32::MIN, f32::max);
    let min_x = mesh
        .vertices
        .iter()
        .map(|v| v.position[0])
        .fold(f32::MAX, f32::min);
    assert!(max_x > 1.1, "{max_x}");
    assert!(min_x > -0.5, "{min_x}");
}

#[test]
fn a_placement_translates_to_the_position_and_rotates_to_the_facing() {
    let north = placement(100.0, 50.0, 7.0, std::f32::consts::FRAC_PI_2, 5.0, 20.0);
    let nose = north.transform_point(Vec3::new(1.0, 0.0, 0.0));
    assert!((nose.x - 100.0).abs() < 1e-3, "{nose:?}");
    assert!((nose.y - 55.0).abs() < 1e-3, "{nose:?}");
    assert!((nose.z - 7.0).abs() < 1e-3, "{nose:?}");
    let feet = north.transform_point(Vec3::ZERO);
    assert!((feet.x - 100.0).abs() < 1e-4 && (feet.y - 50.0).abs() < 1e-4);
    let head = north.transform_point(Vec3::new(0.0, 0.0, 1.0));
    assert!((head.z - 27.0).abs() < 1e-3, "{head:?}");
}

fn bar_view(world: &mut mint_sim::World, x: f32, y: f32) -> UnitView {
    UnitView {
        entity: world.spawn().unwrap(),
        position: mint_movement::Vec2::new(x, y),
        ground: 0.0,
        facing: 0.0,
        radius: 5.0,
        height: 18.0,
        kind_of: mint_sage_asset::KindOf::SELECTABLE.union(mint_sage_asset::KindOf::HERO),
        targetable: false,
        pick_type: PickType::Selectable,
        owner: mint_sim::PlayerId::new(0),
        selected: true,
        moving: false,
        health: Some(1.0),
        dying: false,
        visibility: mint_sage_shroud::ObjectVisibility::Clear,
        speed: 0.0,
        weapon_cycle: 0.0,
        attack_cycle: 0,
        attack_slot: None,
        kind: None,
        art_kind: None,
        told_kind: None,
        hover_kind: None,
        conditions: mint_sage_scene::Conditions::NONE,
        build_elapsed: None,
        build_total: None,
        under_repair: false,
        held: false,
    }
}

fn view_proj(distance: f32) -> Mat4 {
    let camera = mint_render::Camera {
        eye: Vec3::new(0.0, -distance, distance),
        target: Vec3::ZERO,
        up: Vec3::Z,
        fov_y: mint_render::DEFAULT_FOV_Y,
        near: 1.0,
        far: 20_000.0,
    };
    camera.view_projection(1280.0 / 720.0)
}

fn bars(units: &[UnitView], distance: f32, style: HealthBarStyle) -> Vec<UiQuad> {
    bars_with(units, distance, None, false, style)
}

fn bars_with(
    units: &[UnitView],
    distance: f32,
    hovered: Option<mint_sim::Entity>,
    all_health_bars: bool,
    style: HealthBarStyle,
) -> Vec<UiQuad> {
    let mut out = Vec::new();
    push_health_bars(
        units,
        &|_| HealthBox {
            lift: 30.0,
            scale: 1.0,
        },
        view_proj(distance),
        (1280, 720),
        hovered,
        all_health_bars,
        style,
        &|_| [0.2, 0.4, 0.9, 1.0],
        &mut out,
    );
    out
}

#[test]
fn a_bar_is_drawn_only_over_a_selected_seen_unit_with_health() {
    let mut world = mint_sim::World::new();
    let shown = bar_view(&mut world, 0.0, 0.0);
    assert_eq!(bars(&[shown], 400.0, HealthBarStyle::Health).len(), 6);
    let refused = [
        UnitView {
            selected: false,
            ..shown
        },
        UnitView {
            visibility: mint_sage_shroud::ObjectVisibility::Fogged,
            ..shown
        },
        UnitView {
            kind_of: mint_sage_asset::KindOf::UNATTACKABLE,
            ..shown
        },
        UnitView {
            health: None,
            ..shown
        },
        UnitView {
            health: Some(0.0),
            ..shown
        },
        UnitView {
            health: Some(f32::NAN),
            ..shown
        },
    ];
    for unit in refused {
        assert!(
            bars(&[unit], 400.0, HealthBarStyle::Health).is_empty(),
            "{unit:?}"
        );
    }
}

#[test]
fn hover_alone_shows_the_bar_for_a_hero() {
    let mut world = mint_sim::World::new();
    let unit = UnitView {
        selected: false,
        ..bar_view(&mut world, 0.0, 0.0)
    };
    assert!(bars(&[unit], 400.0, HealthBarStyle::Health).is_empty());
    assert!(!bars_with(
        &[unit],
        400.0,
        Some(unit.entity),
        false,
        HealthBarStyle::Health
    )
    .is_empty());
}

#[test]
fn an_infantry_unit_needs_the_all_health_bars_option() {
    let mut world = mint_sim::World::new();
    let selected = UnitView {
        kind_of: mint_sage_asset::KindOf::SELECTABLE.union(mint_sage_asset::KindOf::INFANTRY),
        ..bar_view(&mut world, 0.0, 0.0)
    };
    assert!(bars(&[selected], 400.0, HealthBarStyle::Health).is_empty());
    assert!(!bars_with(&[selected], 400.0, None, true, HealthBarStyle::Health).is_empty());
    let unselected = UnitView {
        selected: false,
        ..selected
    };
    assert!(bars_with(&[unselected], 400.0, None, true, HealthBarStyle::Health).is_empty());
}

#[test]
fn hovering_one_soldier_draws_that_soldiers_bar_only() {
    let mut world = mint_sim::World::new();
    let soldier = |world: &mut mint_sim::World, x: f32| UnitView {
        selected: false,
        kind_of: mint_sage_asset::KindOf::SELECTABLE.union(mint_sage_asset::KindOf::INFANTRY),
        ..bar_view(world, x, 0.0)
    };
    let group = [
        soldier(&mut world, -20.0),
        soldier(&mut world, 0.0),
        soldier(&mut world, 20.0),
    ];
    let hovered = group[1].entity;
    let drawn = bars_with(&group, 400.0, Some(hovered), true, HealthBarStyle::Health);
    assert_eq!(drawn.len(), 6, "one bar is six quads");
    let selected = group.map(|unit| UnitView {
        selected: true,
        ..unit
    });
    assert_eq!(
        bars_with(&selected, 400.0, None, true, HealthBarStyle::Health).len(),
        18,
        "a selected group draws every member's bar"
    );
}

#[test]
fn unattackable_never_shows_a_bar() {
    use mint_sage_asset::KindOf;
    let kind_of = KindOf::HERO.union(KindOf::UNATTACKABLE);
    assert!(!shows_health_bar(kind_of, true, false, false));
    assert!(!shows_health_bar(kind_of, false, true, false));
}

#[test]
fn the_bar_is_the_same_size_at_any_camera_distance() {
    let mut world = mint_sim::World::new();
    let unit = bar_view(&mut world, 0.0, 0.0);
    let near = bars(&[unit], 200.0, HealthBarStyle::Health);
    let far = bars(&[unit], 800.0, HealthBarStyle::Health);
    let near_frame = near.first().unwrap();
    let far_frame = far.first().unwrap();
    assert_eq!(near_frame.affine[0][1], 0.0, "upright, no shear");
    assert_eq!(near_frame.affine[1][0], 0.0, "upright, no shear");
    assert!(
        (far_frame.affine[0][0] - near_frame.affine[0][0]).abs() < 1e-3,
        "near {near_frame:?}, far {far_frame:?}"
    );
}

#[test]
fn the_width_clamps_at_20_and_150() {
    assert_eq!(health_bar_width(1.0, 1.0), 2.0 * 20.0);
    assert_eq!(health_bar_width(1000.0, 1.0), 2.0 * 150.0);
    assert_eq!(health_bar_width(40.0, 1.0), 2.0 * 40.0);
}

#[test]
fn the_fill_is_as_wide_as_the_health_left() {
    let mut world = mint_sim::World::new();
    let full = bars(
        &[bar_view(&mut world, 0.0, 0.0)],
        400.0,
        HealthBarStyle::Health,
    );
    let half = bars(
        &[UnitView {
            health: Some(0.5),
            ..bar_view(&mut world, 0.0, 0.0)
        }],
        400.0,
        HealthBarStyle::Health,
    );
    for row in 3..6 {
        assert_eq!(full[row].affine[0][0], 40.0);
        assert_eq!(half[row].affine[0][0], 20.0, "40 x 0.5, rounded");
        assert_eq!(half[row].translation[0], half[0].translation[0] + 3.0);
    }
}

#[test]
fn a_point_behind_the_eye_or_off_screen_draws_no_bar() {
    let mut world = mint_sim::World::new();
    let behind = UnitView {
        ground: 770.0,
        ..bar_view(&mut world, 0.0, -800.0)
    };
    assert!(bars(&[behind], 400.0, HealthBarStyle::Health).is_empty());
    let aside = bar_view(&mut world, 20_000.0, 0.0);
    assert!(bars(&[aside], 400.0, HealthBarStyle::Health).is_empty());
    let nan = Mat4::from_cols([[f32::NAN; 4]; 4]);
    assert_eq!(project_to_pixel(nan, Vec3::ZERO, (1280, 720)), None);
}

#[test]
fn the_colour_at_0_7_health_is_the_mid_high_blend() {
    assert_eq!(
        health_colour_rows(1.0)[0],
        [HEALTH_HIGH[0][0], HEALTH_HIGH[0][1], HEALTH_HIGH[0][2], 1.0]
    );
    let rows = health_colour_rows(0.7);
    let blend = |lo: f32, hi: f32| lo + (hi - lo) * 0.5;
    for row in 0..3 {
        for channel in 0..3 {
            let expected = blend(HEALTH_MID[row][channel], HEALTH_HIGH[row][channel]);
            assert!(
                (rows[row][channel] - expected).abs() < 1e-4,
                "row {row} channel {channel}: {} vs {expected}",
                rows[row][channel]
            );
        }
    }
}

#[test]
fn the_construction_bar_rows_blend_with_progress() {
    let byte = |rows: [[f32; 4]; 4], row: usize| {
        let [r, g, b, _] = rows[row].map(|channel| (channel * 255.0).round());
        [r, g, b]
    };
    assert_eq!(byte(construction_colour_rows(0.5), 0), [24.0, 146.0, 212.0]);
    assert_eq!(byte(construction_colour_rows(0.0), 3), [8.0, 39.0, 117.0]);
    assert_eq!(byte(construction_colour_rows(1.0), 3), [3.0, 165.0, 186.0]);
}

#[test]
fn the_player_style_bevels_the_owners_colour() {
    let mut world = mint_sim::World::new();
    let player = bars(
        &[bar_view(&mut world, 0.0, 0.0)],
        400.0,
        HealthBarStyle::Player,
    );
    assert_eq!(player[3].color_multiply, bevel([0.2, 0.4, 0.9, 1.0]).0);
    assert_eq!(HealthBarStyle::Health.next().next(), HealthBarStyle::Health);
}

#[test]
fn the_construction_label_gate_wants_actual_construction_not_just_awaiting() {
    use mint_sage_scene::Conditions;
    assert!(is_actively_being_constructed(
        Conditions::ACTIVELY_BEING_CONSTRUCTED
    ));
    assert!(!is_actively_being_constructed(
        Conditions::AWAITING_CONSTRUCTION
    ));
    assert!(!is_actively_being_constructed(
        Conditions::PARTIALLY_CONSTRUCTED
    ));
    assert!(!is_actively_being_constructed(Conditions::NONE));
}

#[test]
fn the_health_box_reads_its_templates_fields() {
    let record = mint_sage_modules::ObjectRecord {
        geometry: mint_sage_modules::Geometry {
            health_box_height: Some(225.0),
            ..mint_sage_modules::Geometry::default()
        },
        health_box_offset: Some(-200.0),
        health_box_scale: Some(3.0),
        ..mint_sage_modules::ObjectRecord::default()
    };
    let found = HealthBox::of(Some(&record), 18.0);
    assert_eq!(found.lift, 225.0 - 200.0);
    assert_eq!(found.scale, 3.0);
    assert_eq!(
        HealthBox::of(None, 18.0).lift,
        18.0 + HEALTH_BOX_OFFSET_DEFAULT
    );
    let hostile = mint_sage_modules::ObjectRecord {
        health_box_scale: Some(1.0e9),
        ..mint_sage_modules::ObjectRecord::default()
    };
    assert_eq!(
        HealthBox::of(Some(&hostile), 18.0).scale,
        HEALTH_BOX_SCALE_MAX
    );
}

#[test]
#[ignore = "a timing, not a check - run by hand"]
fn ten_thousand_selected_bars_cost() {
    let mut world = mint_sim::World::new();
    let units: Vec<UnitView> = (0..10_000u16)
        .map(|i| {
            bar_view(
                &mut world,
                f32::from(i % 100) * 8.0 - 400.0,
                f32::from(i / 100) * 8.0 - 400.0,
            )
        })
        .collect();
    let mut out = Vec::new();
    let started = std::time::Instant::now();
    for _ in 0..20 {
        out.clear();
        push_health_bars(
            &units,
            &|_| HealthBox {
                lift: 30.0,
                scale: 1.0,
            },
            view_proj(900.0),
            (1280, 720),
            None,
            false,
            HealthBarStyle::Health,
            &|_| [1.0; 4],
            &mut out,
        );
    }
    println!(
        "{} quads, {:.3} ms a frame",
        out.len(),
        started.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
}

#[test]
fn a_unit_the_predicate_refuses_is_drawn_in_a_different_colour() {
    use mint_sage_asset::KindOf;
    let mut unit = UnitView {
        entity: {
            let mut world = mint_sim::World::new();
            world.spawn().unwrap()
        },
        position: mint_movement::Vec2::ZERO,
        ground: 0.0,
        facing: 0.0,
        radius: 5.0,
        height: 18.0,
        kind_of: KindOf::SELECTABLE,
        targetable: false,
        pick_type: PickType::Selectable,
        owner: mint_sim::PlayerId::new(0),
        selected: false,
        moving: false,
        health: None,
        dying: false,
        visibility: mint_sage_shroud::ObjectVisibility::Clear,
        speed: 0.0,
        weapon_cycle: 0.0,
        attack_cycle: 0,
        attack_slot: None,
        kind: None,
        art_kind: None,
        told_kind: None,
        hover_kind: None,
        conditions: mint_sage_scene::Conditions::NONE,
        build_elapsed: None,
        build_total: None,
        under_repair: false,
        held: false,
    };
    let selectable = tint_for(&unit);
    unit.pick_type = PickType::Shrubbery;
    assert_ne!(selectable, tint_for(&unit));
}

#[test]
fn a_move_hint_needs_a_mobile_selection_or_a_fogged_destination() {
    use mint_sage_asset::KindOf;
    let unit = |selected: bool, kind_of: KindOf| UnitView {
        selected,
        ..crate::pick::tests::view(0, 0.0, 0.0, kind_of, 0)
    };
    let soldier = unit(true, KindOf::SELECTABLE);
    let building = unit(true, KindOf::STRUCTURE.union(KindOf::IMMOBILE));
    let idle = unit(false, KindOf::SELECTABLE);
    let held = UnitView {
        held: true,
        ..unit(true, KindOf::SELECTABLE)
    };

    assert!(shows_move_hint(&[soldier, idle], false));
    assert!(!shows_move_hint(&[building, idle], false));
    assert!(shows_move_hint(&[building, soldier], false));
    assert!(!shows_move_hint(&[idle], false));
    assert!(!shows_move_hint(&[held], false));
    assert!(!shows_move_hint(&[building, held], false));
    assert!(shows_move_hint(&[building, held], true));
}

#[test]
fn a_building_ghost_blinks_only_code_nine() {
    assert_eq!(ghost_tint(0, 4), None);
    assert_eq!(ghost_tint(7, 0), Some([1.0, 0.0, 0.0]));
    assert_eq!(ghost_tint(7, 4), Some([1.0, 0.0, 0.0]));
    assert_eq!(ghost_tint(9, 0), Some([0.0, 0.0, 0.5]));
    assert_eq!(ghost_tint(9, 4), Some([0.0, 0.0, 1.7]));
}

#[test]
fn a_span_holds_red_and_blinks_only_its_refused_pieces() {
    assert_eq!(span_piece_tint(0, 0, 4), None);
    assert_eq!(span_piece_tint(8, 0, 4), Some([1.0, 0.0, 0.0]));
    assert_eq!(span_piece_tint(8, 8, 0), Some([1.0, 0.0, 0.0]));
    assert_eq!(span_piece_tint(8, 8, 4), Some([1.0, 0.8, 0.0]));
    assert_eq!(span_piece_tint(8, 7, 4), Some([1.0, 0.8, 0.0]));
    assert_eq!(span_piece_tint(7, 7, 0), Some([0.0, 0.0, 0.0]));
    assert_eq!(span_piece_tint(7, 7, 4), Some([0.0, 1.0, 0.0]));
    assert_eq!(span_piece_tint(10, 10, 4), Some([0.5, 1.3, 0.5]));
    assert_eq!(span_piece_tint(10, 0, 4), Some([1.0, 0.0, 0.0]));
}

#[test]
fn the_caption_size_is_the_floored_width_scaled_point_size() {
    assert_eq!(caption_text_px(12.0, 1280), 15);
    assert_eq!(caption_text_px(12.0, 1536), 18);
    assert_eq!(caption_text_px(12.0, 1920), 22);
    assert_eq!(caption_text_px(12.0, 800), 9);
}
