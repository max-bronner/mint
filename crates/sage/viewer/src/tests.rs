use super::*;
use mint_sage_model::ConvertedModel;
use mint_vfs::VfsPath;

fn parse(args: &[&str]) -> Options {
    parse_from(args.iter().map(|s| (*s).to_owned())).expect("should parse")
}

#[test]
fn list_animations_with_no_value_at_all_lists_everything() {
    let options = parse(&["--install", "x", "--list-animations"]);
    assert_eq!(options.list_animations.as_deref(), Some(""));
}

#[test]
fn list_animations_followed_by_another_flag_does_not_swallow_it_as_a_filter() {
    let options = parse(&["--install", "x", "--list-animations", "--paused"]);
    assert_eq!(options.list_animations.as_deref(), Some(""));
    assert!(options.paused, "the flag after it must still be parsed");
}

#[test]
fn list_animations_with_a_value_keeps_it_as_the_filter() {
    let options = parse(&["--install", "x", "--list-animations", "gbfarm"]);
    assert_eq!(options.list_animations.as_deref(), Some("gbfarm"));
}

#[test]
fn list_animations_at_the_end_of_the_command_line_lists_everything() {
    let options = parse(&["--install", "x", "--list-animations"]);
    assert_eq!(options.list_animations.as_deref(), Some(""));
    assert!(options.model.is_none());
}

fn part_with(blend: Blend, material_opacity: f32) -> Part {
    Part {
        blend,
        material_opacity,
        ..Part::default()
    }
}

#[test]
fn alpha_blend_uses_the_materials_own_opacity() {
    assert_eq!(material_alpha(&part_with(Blend::AlphaBlend, 0.3)), 0.3);
    assert_eq!(material_alpha(&part_with(Blend::AlphaBlend, 0.0)), 0.0);
}

#[test]
fn additive_ignores_a_low_opacity_regardless_of_the_files_own_value() {
    assert_eq!(material_alpha(&part_with(Blend::Additive, 0.0)), 1.0);
}

#[test]
fn opaque_ignores_the_materials_opacity_too() {
    assert_eq!(material_alpha(&part_with(Blend::Opaque, 0.0)), 1.0);
}

fn empty_loaded_model() -> LoadedModel {
    LoadedModel {
        path: VfsPath::new("art/w3d/bo/box.w3d"),
        skeleton: None,
        hierarchy: None,
        embedded_animation_name: None,
        model: ConvertedModel::default(),
    }
}

fn with_embedded(name: &str) -> LoadedModel {
    LoadedModel {
        embedded_animation_name: Some(name.to_owned()),
        ..empty_loaded_model()
    }
}

#[test]
fn embedded_animation_name_is_carried_for_the_report_model_hint_only() {
    let loaded = with_embedded("SFE_MENUSPELLS");
    assert_eq!(
        loaded.embedded_animation_name.as_deref(),
        Some("SFE_MENUSPELLS")
    );
}

fn playback_naming(hierarchy: &str) -> AnimationPlayback {
    AnimationPlayback {
        requested: "some_anim".to_owned(),
        animation: Animation::default(),
        hierarchy: Hierarchy {
            header: mint_sage_w3d::HierarchyHeader {
                name: hierarchy.to_owned(),
                ..mint_sage_w3d::HierarchyHeader::default()
            },
            pivots: Vec::new(),
        },
        frame: 0.0,
        playing: true,
        last_tick: Instant::now(),
    }
}

#[test]
fn an_animation_that_moves_nothing_at_all_is_refused() {
    let playback = playback_naming("BOXSKEL");
    let uploaded = Uploaded {
        instances: Vec::new(),
        animated: Vec::new(),
        rigid: Vec::new(),
        uv_animated: Vec::new(),
    };
    let error = require_animation_moves_something(&playback, &uploaded, &empty_loaded_model())
        .expect_err("nothing here binds to the animation");
    match error {
        Error::AnimationHasNoEffect {
            animation,
            hierarchy,
            ..
        } => {
            assert_eq!(animation, "some_anim");
            assert_eq!(hierarchy, "BOXSKEL");
        }
        other => panic!("expected AnimationHasNoEffect, got {other:?}"),
    }
}

#[test]
fn an_animation_with_at_least_one_rigid_part_bound_is_allowed() {
    let playback = playback_naming("BOXSKEL");
    let uploaded = Uploaded {
        instances: Vec::new(),
        animated: Vec::new(),
        rigid: vec![RigidPart {
            instance: 0,
            pivot: 1,
            base: (Blend::Opaque, [1.0; 4]),
        }],
        uv_animated: Vec::new(),
    };
    require_animation_moves_something(&playback, &uploaded, &empty_loaded_model())
        .expect("a rigid binding is enough to allow playback");
}

#[test]
fn view_defaults_to_unset() {
    let options = parse(&["--install", "x", "model"]);
    assert_eq!(options.view, None);
}

#[test]
fn view_parses_every_documented_name() {
    for (name, expected) in [
        ("front", FixedView::Front),
        ("back", FixedView::Back),
        ("left", FixedView::Left),
        ("right", FixedView::Right),
        ("top", FixedView::Top),
        ("bottom", FixedView::Bottom),
    ] {
        let options = parse(&["--install", "x", "--view", name, "model"]);
        assert_eq!(options.view, Some(expected), "--view {name}");
    }
}

#[test]
fn an_unknown_view_name_is_a_parse_error_not_a_silent_default() {
    let error = parse_from(
        ["--install", "x", "--view", "diagonal", "model"]
            .into_iter()
            .map(str::to_owned),
    )
    .expect_err("an unrecognised --view name must be refused");
    assert!(
        error.contains("--view"),
        "error should name the flag: {error}"
    );
}

fn eye_direction(camera: &OrbitCamera) -> Vec3 {
    let mut camera = *camera;
    camera.target = Vec3::ZERO;
    camera.distance = 1.0;
    camera.eye()
}

fn approx_vec3(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4
}

#[test]
fn each_fixed_view_looks_from_the_axis_task_2_specifies() {
    for (view, expected) in [
        (FixedView::Front, -Vec3::Y),
        (FixedView::Back, Vec3::Y),
        (FixedView::Left, -Vec3::X),
        (FixedView::Right, Vec3::X),
        (FixedView::Top, Vec3::Z),
        (FixedView::Bottom, -Vec3::Z),
    ] {
        let mut camera = OrbitCamera::new(Vec3::Z);
        view.apply(&mut camera);
        let direction = eye_direction(&camera);
        assert!(
            approx_vec3(direction, expected),
            "{view:?}: expected direction {expected:?}, got {direction:?}"
        );
    }
}

#[test]
fn top_and_bottom_report_y_as_up_front_and_back_report_z() {
    let mut top = OrbitCamera::new(Vec3::Z);
    FixedView::Top.apply(&mut top);
    assert_eq!(top.up, Vec3::Y);

    let mut bottom = OrbitCamera::new(Vec3::Z);
    FixedView::Bottom.apply(&mut bottom);
    assert_eq!(bottom.up, Vec3::Y);

    let mut front = OrbitCamera::new(Vec3::Z);
    FixedView::Front.apply(&mut front);
    assert_eq!(front.up, Vec3::Z);
}

#[test]
fn applying_a_fixed_view_leaves_the_auto_fit_target_and_distance_alone() {
    let bounds = mint_render::Aabb::from_corners(Vec3::new(-2.0, -3.0, -1.0), Vec3::ONE);
    let mut camera = OrbitCamera::new(Vec3::Z);
    camera.frame(&bounds, 1.0);
    let (target, distance) = (camera.target, camera.distance);

    FixedView::Top.apply(&mut camera);

    assert_eq!(camera.target, target, "--view must not move the target");
    assert_eq!(
        camera.distance, distance,
        "--view must keep the existing auto-fit distance"
    );
}

#[test]
fn every_fixed_view_produces_a_finite_view_projection() {
    for view in [
        FixedView::Front,
        FixedView::Back,
        FixedView::Left,
        FixedView::Right,
        FixedView::Top,
        FixedView::Bottom,
    ] {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.distance = 10.0;
        view.apply(&mut camera);
        let finite = camera
            .camera()
            .view_projection(1.0)
            .to_cols_array()
            .iter()
            .all(|v| v.is_finite());
        assert!(finite, "{view:?} produced a non-finite view-projection");
    }
}
