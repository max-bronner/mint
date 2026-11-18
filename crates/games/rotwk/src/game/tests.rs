use super::*;

#[test]
fn control_set_parses_all_and_a_comma_list_and_refuses_the_rest() {
    assert_eq!(ControlSet::parse("all").unwrap(), ControlSet::All);
    assert_eq!(ControlSet::parse("ALL").unwrap(), ControlSet::All);
    assert_eq!(
        ControlSet::parse("0,1").unwrap(),
        ControlSet::Listed(vec![PlayerId::new(0), PlayerId::new(1)])
    );
    assert_eq!(
        ControlSet::parse(" 2 , 0 ").unwrap(),
        ControlSet::Listed(vec![PlayerId::new(2), PlayerId::new(0)]),
        "whitespace around a comma must not be significant"
    );
    assert!(
        ControlSet::parse("").is_err(),
        "an empty list must be refused"
    );
    assert!(
        ControlSet::parse("one").is_err(),
        "a non-numeric index must be refused"
    );
    assert_eq!(
        ControlSet::default_local(),
        ControlSet::Listed(vec![LOCAL_PLAYER]),
        "the default must be today's whole behaviour - the human's own seat only"
    );
}

#[test]
fn a_box_drags_release_is_captured_but_nothing_else_is() {
    assert!(captures_the_release(
        mint_app::PointerButton::Primary,
        false,
        true
    ));
    assert!(
        !captures_the_release(mint_app::PointerButton::Primary, true, true),
        "a press, not a release"
    );
    assert!(
        !captures_the_release(mint_app::PointerButton::Primary, false, false),
        "no drag to capture"
    );
    assert!(
        !captures_the_release(mint_app::PointerButton::Secondary, false, true),
        "only the primary box drag"
    );
}

#[test]
fn control_set_cycles_in_order_and_wraps() {
    let listed = ControlSet::Listed(vec![PlayerId::new(0), PlayerId::new(2)]);
    assert_eq!(listed.next(PlayerId::new(0), 8), PlayerId::new(2));
    assert_eq!(listed.next(PlayerId::new(2), 8), PlayerId::new(0));
    assert_eq!(listed.next(PlayerId::new(5), 8), PlayerId::new(2));

    assert_eq!(ControlSet::All.next(PlayerId::new(0), 2), PlayerId::new(1));
    assert_eq!(ControlSet::All.next(PlayerId::new(1), 2), PlayerId::new(0));
    assert_eq!(
        ControlSet::All.next(PlayerId::new(0), 0),
        PlayerId::new(0),
        "a zero seat count must not panic or invent a player"
    );
}

#[test]
fn the_mode_flags_accept_every_mode_and_refuse_anything_else() {
    assert_eq!(
        parse_modes(Some("straight-line"), Some("off")).unwrap(),
        Modes {
            search: SearchMode::StraightLine,
            smoothing: SmoothingMode::Off,
            avoidance: AvoidanceMode::Off,
            formation: FormationMode::On,
        }
    );
    assert_eq!(parse_modes(None, None).unwrap(), Modes::default());
    assert!(parse_modes(Some("dijkstra"), None).is_err());
    assert!(parse_modes(None, Some("funnel")).is_err());
}

#[test]
fn the_drag_overlay_spans_exactly_the_rectangle_it_is_given() {
    let rect = ScreenRect::from_corners((100.0, 40.0), (260.0, 300.0));
    let (fill, outline) = drag_geometry(rect);
    assert_eq!(fill.len(), 6, "two triangles");
    assert_eq!(outline.len(), 8, "four segments");
    let xs: Vec<f32> = fill.iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = fill.iter().map(|p| p[1]).collect();
    assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), 100.0);
    assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 260.0);
    assert_eq!(ys.iter().cloned().fold(f32::MAX, f32::min), 40.0);
    assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 300.0);
}

#[test]
fn every_passability_rung_wraps_and_only_the_last_removes_a_source() {
    let mut rung = PassabilitySources::Cliffs;
    let mut seen = Vec::new();
    for _ in 0..PassabilitySources::ALL.len() {
        seen.push(rung);
        rung = rung.next();
    }
    assert_eq!(seen, PassabilitySources::ALL.to_vec());
    assert_eq!(rung, PassabilitySources::Cliffs, "the ladder wraps");
    let flags: Vec<(bool, bool, bool)> = PassabilitySources::ALL
        .iter()
        .map(|rung| (rung.water(), rung.objects(), rung.boundary()))
        .collect();
    assert_eq!(
        flags,
        vec![
            (false, false, false),
            (true, false, false),
            (true, true, true),
            (true, true, false),
        ]
    );
}

#[test]
fn the_passability_flag_accepts_every_label_the_hotkey_prints() {
    for (index, rung) in PassabilitySources::ALL.iter().enumerate() {
        assert_eq!(PassabilitySources::parse(rung.label()), Ok(*rung));
        let ordinal = index.saturating_add(1).to_string();
        assert_eq!(PassabilitySources::parse(&ordinal), Ok(*rung));
    }
    assert_eq!(
        PassabilitySources::parse("CLIFFS"),
        Ok(PassabilitySources::Cliffs),
        "labels are matched without regard to case"
    );
    let error = PassabilitySources::parse("everything").unwrap_err();
    assert!(error.contains("cliffs"), "{error}");
    assert!(PassabilitySources::parse("0").is_err());
    assert!(PassabilitySources::parse("5").is_err());
}

#[test]
fn the_default_camera_is_read_from_game_data() {
    let data = data_from(
        "GameData\n  DefaultCameraPitchAngle = 40.0\n  DefaultCameraMaxHeight = 350.0\nEnd\n",
    );
    assert_eq!(data.camera.pitch_degrees, 40.0);
    assert_eq!(data.camera.max_height, 350.0);
    assert_eq!(data.camera.min_height, 120.0);
    let shipped = data_from("");
    assert_eq!(shipped.camera, crate::combat::CameraDefaults::SHIPPED);
}

#[test]
fn the_tactical_lens_is_fifty_degrees_across_the_window() {
    let across = |viewport: (u32, u32)| {
        let fov_y = super::tactical_fov_y(viewport).unwrap();
        #[allow(clippy::cast_precision_loss)]
        let aspect = viewport.0 as f32 / viewport.1 as f32;
        2.0 * mint_math::f32::atan(mint_math::f32::tan(fov_y * 0.5) * aspect)
    };
    for viewport in [(1600, 1200), (1920, 1080), (800, 600)] {
        assert!((across(viewport).to_degrees() - 50.0).abs() < 1e-3);
    }
    assert!(super::tactical_fov_y((0, 10)).is_none());
    let distance = super::opening_distance(&crate::combat::CameraDefaults::SHIPPED);
    let height = distance * mint_math::f32::sin(37.5f32.to_radians());
    assert!((height - 300.0).abs() < 1e-2, "{height}");
    let mut camera = RtsCamera::new(Vec3::Z);
    super::apply_tactical_lens(
        &mut camera,
        &crate::combat::CameraDefaults::SHIPPED,
        (1600, 1200),
    );
    assert!((camera.pitch().to_degrees() - 37.5).abs() < 1e-3);
}

#[test]
fn the_attack_move_cursor_is_the_arming_buttons_cursor_name() {
    use crate::cursors::Want;
    let document = mint_sage_ini::Document::parse(
        "t.ini",
        "CommandButton Command_Whatever
  Command = ATTACK_MOVE
  CursorName = SomeOtherCursor
  InvalidCursorName = Nope
End
",
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let mut buttons = mint_sage_ini::CommandButtonIndex::new();
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    assert_eq!(
        super::attack_move_cursor(&buttons).want(false, false, false),
        Want::Command(Some("SomeOtherCursor".to_owned()))
    );
    let empty = mint_sage_ini::CommandButtonIndex::new();
    assert_eq!(
        super::attack_move_cursor(&empty).want(false, true, false),
        Want::Command(None),
        "no button naming ATTACK_MOVE falls back as an unnamed cursor does"
    );
}

#[test]
fn an_armed_buttons_cursor_follows_its_fields_and_the_targets_validity() {
    use crate::cursors::Want;
    let look = super::CommandCursor {
        valid: Some("Good".to_owned()),
        invalid: Some("Bad".to_owned()),
    };
    assert_eq!(
        look.want(true, true, false),
        Want::Command(Some("Good".to_owned()))
    );
    assert_eq!(
        look.want(true, false, false),
        Want::Command(Some("Bad".to_owned()))
    );
    assert_eq!(look.want(true, true, true), Want::Hidden);
    assert_eq!(look.want(true, false, true), Want::Hidden);
}

pub(crate) fn data_from(text: &str) -> GameData {
    let document =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    crate::combat::game_data_from_documents(&[document], 0, std::time::Instant::now()).0
}

#[test]
fn a_kind_that_draws_nothing_is_dropped_and_a_failed_model_is_not() {
    let (kind_of, _) = KindOf::parse("SELECTABLE STRUCTURE");
    let unit = |index: u32, kind: Option<u16>| crate::render_state::UnitView {
        kind,
        art_kind: kind,
        ..crate::pick::tests::view(index, 0.0, 0.0, kind_of, 0)
    };
    let art = [KindArt::Nothing, KindArt::Prism, KindArt::Model(3)];
    let mut units = vec![
        unit(0, Some(0)),
        unit(1, Some(1)),
        unit(2, Some(2)),
        unit(3, None),
    ];
    drop_undrawn(&mut units, &art);
    let kept: Vec<Option<u16>> = units.iter().map(|unit| unit.kind).collect();
    assert_eq!(kept, [Some(1), Some(2), None]);
}

#[test]
fn an_unstarted_site_draws_its_awaiting_construction_state() {
    let (kind_of, _) = KindOf::parse("SELECTABLE STRUCTURE");
    let site = crate::render_state::UnitView {
        kind: Some(0),
        art_kind: Some(0),
        conditions: mint_sage_scene::Conditions::AWAITING_CONSTRUCTION,
        build_elapsed: Some(0),
        build_total: None,
        under_repair: false,
        ..crate::pick::tests::view(0, 0.0, 0.0, kind_of, 0)
    };
    let colours = PlayerColors::default();
    let poses = unit_poses(
        &[site],
        &[KindArt::Model(0)],
        &colours,
        &crate::templates::KindTable::default(),
        &mut |_| mint_sage_scene::LookKey::PLAIN,
        &|_| 1.0,
        &|_| true,
        &|_| 1.0,
        &|_| None,
    );
    let [pose] = poses.as_slice() else {
        panic!("one pose: {}", poses.len())
    };
    assert!(pose
        .conditions
        .contains(mint_sage_scene::Conditions::AWAITING_CONSTRUCTION));
    assert!(pose.ghost.is_none());
}

#[test]
fn the_faction_icon_is_the_sides_apt_icon() {
    assert_eq!(faction_icon_image("Men"), "AptIconMen");
    assert_eq!(faction_icon_image("Angmar"), "AptIconAngmar");
}

#[test]
fn q_and_the_digits_are_the_command_bar_stand_in() {
    assert_eq!(queue_button('q'), None);
    assert_eq!(queue_button('1'), Some(0));
    assert_eq!(queue_button('2'), Some(1));
    assert_eq!(queue_button('9'), Some(8));
    assert_eq!(queue_button('0'), None);
    assert_eq!(queue_button('x'), None);
}

#[test]
fn the_placement_claim_ring_is_gated_by_max_income_not_visible() {
    let claiming = |max_income: f32, visible: bool| mint_sage_modules::ObjectRecord {
        claim_radius: Some(80.0),
        claim_max_income: Some(max_income),
        claim_decal_visible: visible,
        ..Default::default()
    };
    assert_eq!(
        placement_claim_radius(&claiming(0.0, true)),
        None,
        "a MaxIncome = 0 claimant draws no placement disc whatever Visible says"
    );
    assert_eq!(
        placement_claim_radius(&claiming(0.0, false)),
        None,
        "a MaxIncome = 0 claimant draws no placement disc whatever Visible says"
    );
    assert_eq!(
        placement_claim_radius(&claiming(50.0, false)),
        Some(80.0),
        "a positive MaxIncome draws it even with Visible = No"
    );
    assert_eq!(
        placement_claim_radius(&mint_sage_modules::ObjectRecord {
            claim_radius: None,
            claim_max_income: Some(50.0),
            ..Default::default()
        }),
        None,
        "a template that claims no ground draws nothing"
    );
}

#[test]
fn the_rotwk_pan_inset_policy_zeroes_a_non_finite_inset_and_drops_one_past_a_quarter_extent() {
    assert_eq!(rotwk_pan_inset_policy(f32::NAN, 100.0), 0.0);
    assert_eq!(rotwk_pan_inset_policy(26.0, 100.0), 0.0);
    assert_eq!(rotwk_pan_inset_policy(24.0, 100.0), 24.0);
}

#[test]
fn frame_camera_drops_an_inset_past_the_quarter_extent_rather_than_shrinking_the_clamp() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pitch(0.5);
    let bounds =
        mint_render::Aabb::from_corners(Vec3::new(-50.0, -50.0, 0.0), Vec3::new(50.0, 50.0, 0.0));
    camera.pan_bounds = Some(bounds);
    camera.pan_inset_policy = Some(rotwk_pan_inset_policy);
    assert!(
        camera.ground_inset() > (bounds.max().x - bounds.min().x) * 0.25,
        "this test needs a raw inset past the cap to exercise it"
    );
    for _ in 0..10_000 {
        camera.pan(1.0, -1.0, -1.0);
    }
    let focus = camera.focus();
    assert!(
        (focus.x - bounds.min().x).abs() < 1e-3 && (focus.y - bounds.min().y).abs() < 1e-3,
        "focus {focus:?} is not on the raw corner {:?} - the dropped inset was applied anyway",
        bounds.min()
    );
}

#[test]
fn placement_drag_angle_holds_below_five_pixels_and_points_east_past_it() {
    let anchor_px = (100.0, 100.0);
    let anchor_w = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(
        placement_drag_angle(
            anchor_px,
            (103.0, 100.0),
            anchor_w,
            Vec3::new(3.0, 0.0, 0.0),
            1.23
        ),
        1.23,
        "3 px is below the threshold"
    );
    let angle = placement_drag_angle(
        anchor_px,
        (110.0, 100.0),
        anchor_w,
        Vec3::new(10.0, 0.0, 0.0),
        1.23,
    );
    assert!(
        angle.abs() < 1e-6,
        "east of the anchor is angle 0, got {angle}"
    );
}

#[test]
fn two_order_events_in_one_frame_both_join_the_vote() {
    let first_horde = mint_sim::Entity::from_parts(1, 0);
    let second_horde = mint_sim::Entity::from_parts(2, 0);
    let events = [
        crate::voice::VoiceEvent {
            entity: first_horde,
            kind: crate::voice::VoiceKind::Move,
            target: None,
            was_attacking: false,
            sound: None,
        },
        crate::voice::VoiceEvent {
            entity: second_horde,
            kind: crate::voice::VoiceKind::Move,
            target: None,
            was_attacking: false,
            sound: None,
        },
    ];
    let (templates, event) = merge_order_votes(events.iter(), |entity| {
        if entity == first_horde {
            vec![10, 10, 10]
        } else {
            vec![20, 20, 20]
        }
    });
    assert_eq!(templates, vec![10, 10, 10, 20, 20, 20]);
    assert_eq!(
        event.map(|event| event.entity),
        Some(first_horde),
        "the first order event plays the sound"
    );
}

#[test]
fn a_non_order_event_is_not_gathered_into_the_vote() {
    let horde = mint_sim::Entity::from_parts(1, 0);
    let spawned = mint_sim::Entity::from_parts(2, 0);
    let events = [
        crate::voice::VoiceEvent {
            entity: horde,
            kind: crate::voice::VoiceKind::Move,
            target: None,
            was_attacking: false,
            sound: None,
        },
        crate::voice::VoiceEvent {
            entity: spawned,
            kind: crate::voice::VoiceKind::Created,
            target: None,
            was_attacking: false,
            sound: None,
        },
    ];
    let (templates, _) = merge_order_votes(events.iter(), |_| vec![30]);
    assert_eq!(templates, vec![30], "only the order event's group counts");
}

#[test]
fn move_hint_cell_fogged_fires_only_on_explored() {
    assert!(move_hint_cell_fogged(
        mint_sage_shroud::CellVisibility::Explored
    ));
    assert!(!move_hint_cell_fogged(
        mint_sage_shroud::CellVisibility::Unexplored
    ));
    assert!(!move_hint_cell_fogged(
        mint_sage_shroud::CellVisibility::Visible
    ));
}

#[test]
fn hover_target_changed_fires_once_per_change_not_once_per_move() {
    let mut previous = None;
    assert_eq!(
        hover_target_changed(&mut previous, "terrain=(1.0,2.0)".to_owned()),
        Some("terrain=(1.0,2.0)".to_owned())
    );
    assert_eq!(
        hover_target_changed(&mut previous, "terrain=(1.0,2.0)".to_owned()),
        None,
        "the same target reported twice must log once"
    );
    assert_eq!(
        hover_target_changed(&mut previous, "entity=4 template=GondorAragorn".to_owned()),
        Some("entity=4 template=GondorAragorn".to_owned())
    );
}

#[test]
fn a_part_reads_the_shroud_at_its_objects_placement_not_its_pivot() {
    let placement = mint_render::Mat4::from_translation(Vec3::new(40.0, 60.0, 0.0));
    let proxy = mint_sage_terrain::ObjectProxy {
        index: 0,
        position: Vec3::new(40.0, 60.0, 0.0),
        ground: Some(0.0),
        angle: 0.0,
        align_to_terrain: false,
        waypoint: false,
        placement,
    };
    let part = mint_render::Mat4::from_translation(Vec3::new(9000.0, -700.0, 30.0));
    assert_eq!(shroud_point(part, Some(0), &[proxy]), Vec2::new(40.0, 60.0));
    assert_eq!(
        shroud_point(part, None, &[proxy]),
        Vec2::new(9000.0, -700.0)
    );
}

#[test]
fn a_placed_object_on_unexplored_ground_or_past_the_grid_is_not_drawn() {
    use mint_sage_shroud::CellVisibility::{Explored, Unexplored, Visible};
    assert_eq!(object_shroud(Visible, true, false), Some(false));
    assert_eq!(object_shroud(Explored, true, false), Some(true));
    assert_eq!(object_shroud(Unexplored, true, false), None);
    assert_eq!(object_shroud(Visible, false, false), None);
    assert_eq!(object_shroud(Unexplored, true, true), Some(false));
    assert_eq!(object_shroud(Visible, false, true), None);
}

#[test]
fn a_tree_on_the_grid_is_cut_by_the_shroud_per_pixel_not_hidden_whole() {
    assert_eq!(tree_shroud(None, true, true, false), (true, true));
    assert_eq!(tree_shroud(None, true, false, false), (false, false));
    assert_eq!(tree_shroud(Some(false), true, true, true), (true, false));
    assert_eq!(tree_shroud(None, false, true, false), (false, false));
    assert_eq!(tree_shroud(Some(true), false, true, false), (true, true));
}

#[test]
fn an_fx_list_the_viewer_cannot_see_is_skipped_unless_it_plays_even_if_shrouded() {
    let viewer = PlayerId::new(0);
    let mut grid = crate::shroud::grid_for((0.0, 0.0), (400.0, 400.0), 2).expect("grid");
    let (x, y) = grid.world_to_cell(100.0, 100.0);
    grid.add_looker(viewer, x, y);
    let (x, y) = grid.world_to_cell(300.0, 300.0);
    grid.add_looker(viewer, x, y);
    grid.remove_looker(viewer, x, y);
    let clear = Vec3::new(100.0, 100.0, 0.0);
    let fogged = Vec3::new(300.0, 300.0, 0.0);
    let unexplored = Vec3::new(200.0, 50.0, 0.0);
    let past_the_grid = Vec3::new(-150.0, 100.0, 0.0);

    assert!(!fx_play_shrouded(&grid, viewer, false, None, None, clear));
    assert!(fx_play_shrouded(&grid, viewer, false, None, None, fogged));
    assert!(fx_play_shrouded(
        &grid, viewer, false, None, None, unexplored
    ));
    assert!(fx_play_shrouded(
        &grid,
        viewer,
        false,
        Some(false),
        None,
        past_the_grid
    ));
    assert!(!fx_play_shrouded(&grid, viewer, true, None, None, fogged));
    assert!(fx_play_shrouded(
        &grid,
        viewer,
        true,
        None,
        None,
        past_the_grid
    ));
    assert!(!fx_play_shrouded(
        &grid,
        viewer,
        false,
        Some(true),
        None,
        fogged
    ));
    assert!(!fx_play_shrouded(
        &grid,
        viewer,
        false,
        Some(true),
        None,
        past_the_grid
    ));
    use mint_sage_shroud::ObjectVisibility::{Fogged, PartialClear, Shrouded};
    assert!(!fx_play_shrouded(
        &grid,
        viewer,
        false,
        None,
        Some(PartialClear),
        fogged
    ));
    assert!(fx_play_shrouded(
        &grid,
        viewer,
        false,
        None,
        Some(Fogged),
        clear
    ));
    assert!(fx_play_shrouded(
        &grid,
        viewer,
        false,
        None,
        Some(Shrouded),
        clear
    ));
    assert!(!fx_play_shrouded(
        &grid,
        viewer,
        true,
        None,
        Some(Fogged),
        clear
    ));
}

#[test]
fn the_script_camera_spends_game_time_not_wall_time() {
    let tick = 1.0 / 30.0;
    let dt = 1.0 / 60.0;
    assert!((script_camera_spend(tick, dt, tick) - dt).abs() < 1e-6);
    assert!((script_camera_spend(100.0, dt, tick) - (100.0 - tick)).abs() < 1e-3);
    assert!(script_camera_spend(0.0, dt, tick).abs() < 1e-9);
}
