use mint_sage_ini::{AnimationClip, AnimationMode};

use mint_render::{Aabb, Vec3};
use mint_sage_ini::AnimationFxEvent;

use super::fx::{queue_fx_events, MAX_FX_EVENTS_QUEUED};
use super::pose::{
    pose_drawable, record_sub_object_writes, run_chunked, sub_object_overrides,
    write_permanent_parts, Posed,
};
use super::*;
use crate::animation_state::{rate_scale, WeaponPhase};

#[test]
fn a_units_part_uv_mapper_is_resampled_over_time_and_a_bare_part_is_not() {
    let info = mint_sage_w3d::VertexMaterialInfo {
        attributes: 0x0004_0000,
        ..Default::default()
    };
    let state = mint_sage_model::convert::UvMapperState {
        info,
        args0: mint_sage_w3d::MapperArgs::parse("UPerSec=0.3\r\nVPerSec=0.0"),
        args1: mint_sage_w3d::MapperArgs::parse(""),
    };

    let at0 = uv_override_for_part(Some(&state), 0.0).expect("a mapper always overrides");
    let at1 = uv_override_for_part(Some(&state), 1.0).expect("a mapper always overrides");
    assert_ne!(
        at0, at1,
        "LinearOffset must move the stage 0 transform over a second"
    );

    assert_eq!(
        uv_override_for_part(None, 1.0),
        None,
        "a part with no mapper must not override the baked-in instance"
    );
}

#[test]
fn a_drawable_id_is_the_entity_index_unchanged() {
    for index in [0u32, 1, 7, 1_000, u32::MAX] {
        assert_eq!(drawable_id(index), index);
    }
}

#[test]
fn a_speed_change_never_moves_the_phase_backwards() {
    let natural = natural_seconds(34, 30);
    let clip = AnimationClip {
        label: Some("Move1".to_owned()),
        name: "GUArcher_RUNA".to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: Some(28.0),
        use_weapon_timing: false,
        priority: None,
    };
    let speeds = [
        8.0f32, 16.0, 24.0, 32.0, 40.0, 40.0, 40.0, 38.0, 30.0, 22.0, 14.0, 6.0, 2.0,
    ];
    let step = 1.0 / 30.0;
    let seed = 0.7 * natural;

    let mut phase: Option<f32> = None;
    let mut last = f32::NEG_INFINITY;
    let mut old_went_backwards = false;
    let mut old_last = f32::NEG_INFINITY;
    for (tick, speed) in speeds.iter().enumerate() {
        let scale = rate_scale(&clip, *speed, natural, 7, 0.0);
        let now = advance_phase(phase, step, scale, seed);
        assert!(
            now >= last,
            "phase went backwards at tick {tick}: {last} then {now}"
        );
        last = now;
        phase = Some(now);

        #[allow(clippy::cast_precision_loss)]
        let elapsed = (tick as f32) * step;
        let old = elapsed * scale + seed;
        if old < old_last {
            old_went_backwards = true;
        }
        old_last = old;
    }
    assert!(
        old_went_backwards,
        "the profile has to make the old formula regress, or this test proves nothing"
    );
}

#[test]
fn an_unseeded_phase_takes_the_offset_and_a_negative_step_holds() {
    assert!((advance_phase(None, 1.0, 2.0, 0.25) - 0.25).abs() < f32::EPSILON);
    assert!((advance_phase(Some(0.25), -1.0, 2.0, 0.0) - 0.25).abs() < f32::EPSILON);
    assert!((advance_phase(Some(0.25), 1.0, -2.0, 0.0) - 0.25).abs() < f32::EPSILON);
}

#[test]
fn a_maintained_frame_carries_the_outgoing_fraction_into_the_incoming_frame_count() {
    assert_eq!(carried_start_frame(16.5, 34, 61), Some(30.0));
    assert_eq!(carried_start_frame(33.0, 34, 61), Some(61.0));
    assert_eq!(carried_start_frame(-1.0, 34, 61), None);
    assert_eq!(carried_start_frame(f32::NAN, 34, 61), None);
    assert_eq!(
        carried_start_frame(0.0, 1, 61),
        None,
        "a one-frame outgoing clip"
    );
}

fn pose(id: u32, template: usize) -> UnitPose {
    UnitPose {
        opacity: 1.0,
        id,
        generation: 1,
        template,
        placement: Mat4::IDENTITY,
        conditions: Conditions::NONE,
        attack_cycle: 0,
        attack_slot: None,
        speed: 0.0,
        weapon_cycle: 0.0,
        house_color: [0.0, 0.0, 0.0, 1.0],
        hero_colours: None,
        shroud_masked: false,
        progress: None,
        ghost: None,
        casts_shadow: true,
        look: LookKey::PLAIN,
        ride: None,
    }
}

#[test]
fn a_missing_drawable_shifts_nothing_after_it() {
    let poses = [pose(3, 0), pose(4, 1), pose(9, 0), pose(11, 1)];
    let drawables = [(3u32, 0usize), (9, 0), (11, 1)];
    let mut cursor = 0usize;
    let mut paired = Vec::new();
    for entry in &poses {
        let Some((id, template)) = drawables.get(cursor).copied() else {
            break;
        };
        if !is_drawable_for(id, 1, template, entry) {
            continue;
        }
        cursor = cursor.saturating_add(1);
        paired.push((id, entry.id));
    }
    assert_eq!(paired, [(3, 3), (9, 9), (11, 11)]);
}

#[test]
fn a_recycled_entity_index_with_another_template_is_not_the_same_drawable() {
    let archer = pose(7, 0);
    assert!(is_drawable_for(7, 1, 0, &archer));
    assert!(!is_drawable_for(7, 1, 1, &archer));
    assert!(!is_drawable_for(8, 1, 0, &archer));
    assert!(!is_drawable_for(7, 2, 0, &archer));
}

#[test]
fn a_reused_entity_slot_of_the_same_template_gets_a_fresh_drawable() {
    let mut scene = scene_with_one_template();
    assert_eq!(scene.sync(&[pose(7, 0)], 1.0), 1);
    let successor = UnitPose {
        generation: 2,
        ..pose(7, 0)
    };
    assert_eq!(
        scene.sync(&[successor], 2.0),
        1,
        "a new object, not a survivor"
    );
    let drawables: Vec<(u32, u32, f32)> = scene
        .drawables()
        .iter()
        .map(|d| (d.id, d.generation, d.entered_at()))
        .collect();
    assert_eq!(drawables, [(7, 2, 2.0)]);
}

#[test]
fn a_degenerate_animation_has_no_duration_to_divide_by() {
    assert_eq!(natural_seconds(0, 30), 0.0);
    assert_eq!(natural_seconds(1, 30), 0.0);
    assert_eq!(natural_seconds(31, 0), 0.0);
    assert!((natural_seconds(31, 30) - 1.0).abs() < 1e-6);
}

fn camera_frustum() -> Frustum {
    let camera = mint_render::Camera {
        eye: Vec3::new(0.0, -10.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Z,
        fov_y: 1.047,
        near: 1.0,
        far: 100.0,
    };
    Frustum::from_view_proj(camera.view_projection(16.0 / 9.0))
}

#[test]
fn a_unit_is_skipped_only_when_its_whole_reach_is_out_of_view() {
    let views = [camera_frustum()];
    let at = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
    assert!(may_be_seen(at(0.0, 0.0, 0.0), 1.0, &views), "dead centre");
    assert!(
        !may_be_seen(at(0.0, -30.0, 5.0), 1.0, &views),
        "behind the eye"
    );
    assert!(
        !may_be_seen(at(200.0, 0.0, 0.0), 1.0, &views),
        "far off to the side"
    );
    assert!(!may_be_seen(at(60.0, 0.0, 0.0), 20.0, &views));
    let scaled = at(60.0, 0.0, 0.0).mul_mat4(Mat4::from_scale(Vec3::splat(4.0)));
    assert!(
        may_be_seen(scaled, 20.0, &views),
        "a placement that scales the model scales its reach"
    );
    assert!(may_be_seen(at(0.0, -30.0, 5.0), 1.0, &[]));
    assert!(may_be_seen(at(0.0, -30.0, 5.0), f32::INFINITY, &views));
    let other = mint_render::Camera {
        eye: Vec3::new(200.0, -10.0, 5.0),
        target: Vec3::new(200.0, 0.0, 0.0),
        up: Vec3::Z,
        fov_y: 1.047,
        near: 1.0,
        far: 100.0,
    };
    let second = Frustum::from_view_proj(other.view_projection(16.0 / 9.0));
    assert!(may_be_seen(at(200.0, 0.0, 0.0), 1.0, &[views[0], second]));
}

#[test]
fn the_stretch_bound_is_never_below_the_real_stretch() {
    let c = core::f32::consts::FRAC_1_SQRT_2;
    let matrices = [
        Mat4::IDENTITY,
        Mat4::from_scale(Vec3::new(3.0, 0.5, 1.0)),
        Mat4::from_cols([
            [c * 2.0, c * 2.0, 0.0, 0.0],
            [-c * 2.0, c * 2.0, 0.0, 0.0],
            [0.0, 0.0, 2.0, 0.0],
            [5.0, 6.0, 7.0, 1.0],
        ]),
        Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [1.5, 1.0, 0.0, 0.0],
            [0.0, -0.7, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]),
    ];
    for m in matrices {
        let bound = stretch_bound(m);
        for i in 0..200 {
            let spread = |k: f32| {
                let x = i as f32 * k;
                x - x.floor() - 0.5
            };
            let v = Vec3::new(spread(0.618_034), spread(0.414_214), spread(0.732_051));
            if v.length() < 1e-3 {
                continue;
            }
            let out = m.transform_vector(v);
            assert!(
                out.length() <= bound * v.length() * (1.0 + 1e-5),
                "{m:?} stretches {v:?} by {} past its bound {bound}",
                out.length() / v.length()
            );
        }
    }
    assert!((stretch_bound(matrices[2]) - 2.0).abs() < 1e-3);
}

#[test]
fn every_chunked_item_travels_with_its_own_pair() {
    for threads in [1usize, 2, 3, 8, 64] {
        let mut items: Vec<u32> = (0..1000).collect();
        let pairs: Vec<u32> = (0..1000).map(|i| i * 3).collect();
        let sums = run_chunked(&mut items, &pairs, threads, 1, |chunk, pairs| {
            assert_eq!(chunk.len(), pairs.len());
            for (item, pair) in chunk.iter_mut().zip(pairs) {
                assert_eq!(*pair, *item * 3, "an item met another's pair");
                *item += 1;
            }
            chunk.len()
        });
        assert_eq!(sums.iter().sum::<usize>(), 1000, "{threads} thread(s)");
        assert!(items
            .iter()
            .zip(1u32..)
            .all(|(value, expected)| *value == expected));
    }
}

fn empty_body() -> Arc<BodyArt> {
    Arc::new(BodyArt {
        model: Arc::new(CachedModel {
            name: "synthetic".to_owned(),
            hierarchy: None,
            parts: Vec::new(),
            bounds: Aabb::EMPTY,
            vertices: 0,
            triangles: 0,
        }),
        gpu: GpuSkin::default(),
    })
}

fn scene_with_one_template() -> UnitScene {
    let mut scene = UnitScene::new();
    scene.templates.push(UnitTemplate {
        name: "Synthetic".to_owned(),
        light_set: mint_sage_model::OBJECT_LIGHT_SET,
        swirl_host: false,
        model: Some("synthetic".to_owned()),
        bodies: vec!["synthetic".to_owned()],
        pool: 1,
        state_bodies: Vec::new(),
        clip_bodies: Vec::new(),
        states: Vec::new(),
        scripts: Vec::new(),
        transition_names: BTreeMap::new(),
        machine: StateMachine::compile(&[]),
        model_states: Vec::new(),
        model_machine: ModelStateMachine::compile(&[]),
        hide_if: Conditions::NONE,
        ok_to_change_model_color: false,
        truck_tires: None,
        extras: ExtraTokens::default(),
        companions: Vec::new(),
        floors: Vec::new(),
        modules: Vec::new(),
        module_of: None,
        attach: None,
        look_conditions: Vec::new(),
        looks: BTreeMap::new(),
        skins: BTreeMap::new(),
        bound: BTreeMap::new(),
        failures: Vec::new(),
        art: vec![Some(empty_body())],
        radius: BTreeMap::new(),
        animation_sounds: Vec::new(),
        max_update_range_cap: None,
        sound_cues: BTreeMap::new(),
        frame_event_parts: BTreeMap::new(),
    });
    scene
}

fn scene_with_two_templates() -> UnitScene {
    let mut scene = scene_with_one_template();
    scene.templates.push(UnitTemplate {
        name: "Synthetic".to_owned(),
        light_set: mint_sage_model::OBJECT_LIGHT_SET,
        swirl_host: false,
        model: Some("synthetic".to_owned()),
        bodies: vec!["synthetic".to_owned()],
        pool: 1,
        state_bodies: Vec::new(),
        clip_bodies: Vec::new(),
        states: Vec::new(),
        scripts: Vec::new(),
        transition_names: BTreeMap::new(),
        machine: StateMachine::compile(&[]),
        model_states: Vec::new(),
        model_machine: ModelStateMachine::compile(&[]),
        hide_if: Conditions::NONE,
        ok_to_change_model_color: false,
        truck_tires: None,
        extras: ExtraTokens::default(),
        companions: Vec::new(),
        floors: Vec::new(),
        modules: Vec::new(),
        module_of: None,
        attach: None,
        look_conditions: Vec::new(),
        looks: BTreeMap::new(),
        skins: BTreeMap::new(),
        bound: BTreeMap::new(),
        failures: Vec::new(),
        art: vec![Some(empty_body())],
        radius: BTreeMap::new(),
        animation_sounds: Vec::new(),
        max_update_range_cap: None,
        sound_cues: BTreeMap::new(),
        frame_event_parts: BTreeMap::new(),
    });
    scene
}

fn state_def(conditions: &[&str], model: Option<&str>) -> ModelStateDef {
    ModelStateDef {
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        model: model.map(str::to_owned),
        textures: Vec::new(),
        particles: Vec::new(),
        decal: None,
        launch_bones: Default::default(),
        retain_sub_objects: false,
        model_animation_prefix: None,
        portrait_image: None,
        button_image: None,
        file: "t.ini".into(),
        line: 1,
    }
}

#[test]
fn each_model_state_resolves_to_the_pool_a_swap_body_or_nothing() {
    let states = [
        state_def(&[], Some("Barracks")),
        state_def(&["AWAITING_CONSTRUCTION"], Some("Barracks_A")),
        state_def(&["ACTIVELY_BEING_CONSTRUCTED"], Some("barracks_a")),
        state_def(&["DAMAGED"], Some("Barracks_D1")),
        state_def(&["RUBBLE"], Some("Barracks_D3")),
        state_def(&["SNOW"], Some("BARRACKS")),
        state_def(&["REALLYDAMAGED"], None),
    ];
    let machine = ModelStateMachine::compile(&states);
    let mut bodies = vec!["Barracks".to_owned(), "Barracks_Alt".to_owned()];
    let resolved = resolve_state_bodies(&states, &machine, &mut bodies);
    assert_eq!(
        resolved,
        [
            StateBody::Pool,
            StateBody::Body(2),
            StateBody::Body(2),
            StateBody::Body(3),
            StateBody::Body(4),
            StateBody::Pool,
            StateBody::Nothing,
        ]
    );
    assert_eq!(
        bodies,
        [
            "Barracks",
            "Barracks_Alt",
            "Barracks_A",
            "Barracks_D1",
            "Barracks_D3"
        ]
    );
}

#[test]
fn a_state_behind_a_template_own_token_still_gets_a_body() {
    let states = [
        state_def(&["AWAITING_CONSTRUCTION"], Some("Pad_A")),
        state_def(&["BUILD_VARIATION_ONE"], Some("Pad_B")),
    ];
    let mut extras = ExtraTokens::default();
    let machine = ModelStateMachine::compile_with(&states, Some(&mut extras));
    let mut bodies = Vec::new();
    let resolved = resolve_state_bodies(&states, &machine, &mut bodies);
    assert_eq!(
        resolved,
        [StateBody::Body(0), StateBody::Body(1)],
        "a template-own token is selectable once interned, not unreachable"
    );
}

#[test]
fn only_an_unbound_state_with_an_explicit_hierarchy_is_named_for_rescue() {
    let anim = |name: &str| AnimationClip {
        label: None,
        name: name.to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    let state = |conditions: &[&str], clips: Vec<AnimationClip>| AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips,
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    let states = vec![
        state(
            &["BUILD_VARIATION_ONE", "ACTIVELY_BEING_CONSTRUCTED"],
            vec![anim("GBFARTOWA_ASKL.GBFARTOWA_ABLD")],
        ),
        state(
            &["ACTIVELY_BEING_CONSTRUCTED"],
            vec![anim("GBFARTOWA_ASKL.GBFARTOWA_ABLD")],
        ),
        state(&["DAMAGED"], vec![anim("Barracks_D1")]),
    ];
    let machine = StateMachine::compile(&states);
    let needed = states_needing_a_rescue_body(&states, machine.states(), |_| false);
    assert_eq!(needed, [(1, "GBFARTOWA_ASKL".to_owned())]);
}

#[test]
fn a_state_already_bound_is_never_named_for_rescue() {
    let clip = AnimationClip {
        label: None,
        name: "GBFARTOWA_ASKL.GBFARTOWA_ABLD".to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    let states = vec![AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: vec!["ACTIVELY_BEING_CONSTRUCTED".to_owned()],
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: vec![clip],
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    }];
    let machine = StateMachine::compile(&states);
    let needed = states_needing_a_rescue_body(&states, machine.states(), |index| index == 0);
    assert!(needed.is_empty());
}

#[test]
fn a_model_swap_wears_the_state_body_and_returns_to_the_pool() {
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model_states = vec![
            state_def(&[], Some("synthetic")),
            state_def(&["AWAITING_CONSTRUCTION"], Some("scaffold")),
            state_def(&["DAMAGED"], None),
        ];
        template.model_machine = ModelStateMachine::compile(&template.model_states);
        template.bodies.push("scaffold".to_owned());
        template.art.push(Some(empty_body()));
        template.state_bodies = vec![StateBody::Pool, StateBody::Body(1), StateBody::Nothing];
    }
    let plain = pose(9, 0);
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let frame = |scene: &mut UnitScene, conditions: Conditions, scratch: &mut Scratch| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        let pose = UnitPose {
            conditions,
            ..plain
        };
        pose_drawable(
            drawable,
            template,
            &pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            scratch,
        );
        (drawable.body, drawable.pool_body, drawable.drawn)
    };
    assert_eq!(
        frame(&mut scene, Conditions::NONE, &mut scratch),
        (0, 0, true)
    );
    assert_eq!(
        frame(&mut scene, Conditions::AWAITING_CONSTRUCTION, &mut scratch),
        (1, 0, true),
        "the scaffold is worn"
    );
    assert_eq!(
        frame(&mut scene, Conditions::NONE, &mut scratch),
        (0, 0, true)
    );
    let (_, _, drawn) = frame(&mut scene, Conditions::DAMAGED, &mut scratch);
    assert!(!drawn, "Model = NONE draws nothing");
    assert_eq!(
        frame(&mut scene, Conditions::NONE, &mut scratch),
        (0, 0, true)
    );
}

#[test]
fn a_module_with_no_default_model_draws_only_while_a_state_names_one() {
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model = None;
        template.bodies = vec!["upgrade".to_owned()];
        template.pool = 0;
        template.model_states = vec![state_def(&["AWAITING_CONSTRUCTION"], Some("upgrade"))];
        template.model_machine = ModelStateMachine::compile(&template.model_states);
        template.state_bodies = vec![StateBody::Body(0)];
    }
    let plain = pose(9, 0);
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let mut frame = |conditions: Conditions| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        let pose = UnitPose {
            conditions,
            ..plain
        };
        pose_drawable(
            drawable,
            template,
            &pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        drawable.drawn
    };
    assert!(!frame(Conditions::NONE), "nothing before the flag");
    assert!(
        frame(Conditions::AWAITING_CONSTRUCTION),
        "the state's body after"
    );
    assert!(!frame(Conditions::NONE), "nothing once it clears");
}

#[test]
fn a_hide_if_flag_hides_the_whole_drawable_until_it_clears() {
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.hide_if = Conditions::AWAITING_CONSTRUCTION.union(Conditions::RUBBLE);
    }
    let plain = pose(9, 0);
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let frame = |scene: &mut UnitScene, conditions: Conditions, scratch: &mut Scratch| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        let pose = UnitPose {
            conditions,
            ..plain
        };
        pose_drawable(
            drawable,
            template,
            &pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            scratch,
        );
        drawable.drawn
    };
    assert!(frame(&mut scene, Conditions::NONE, &mut scratch));
    assert!(
        !frame(&mut scene, Conditions::AWAITING_CONSTRUCTION, &mut scratch),
        "a named flag hides the decal"
    );
    assert!(
        !frame(&mut scene, Conditions::RUBBLE, &mut scratch),
        "either named flag hides it"
    );
    assert!(
        frame(&mut scene, Conditions::MOVING, &mut scratch),
        "an unnamed flag draws it as usual"
    );
    assert!(frame(&mut scene, Conditions::NONE, &mut scratch));
}

#[test]
fn a_force_to_back_floor_sorts_after_the_others() {
    let mut scene = scene_with_one_template();
    let model = Arc::new(CachedModel {
        name: "floor".to_owned(),
        hierarchy: None,
        parts: Vec::new(),
        bounds: Aabb::EMPTY,
        vertices: 0,
        triangles: 0,
    });
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.floors = vec![
            FloorModule {
                model: model.clone(),
                hide_if: Conditions::NONE,
                weather: Vec::new(),
                force_to_back: true,
            },
            FloorModule {
                model,
                hide_if: Conditions::NONE,
                weather: Vec::new(),
                force_to_back: false,
            },
        ];
    }
    let plain = pose(9, 0);
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &plain,
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
    }
    let wanted = floors_wanted(&scene.drawables, &scene.templates);
    assert_eq!(wanted.len(), 2, "both floors are present and unhidden");
    assert!(
        !wanted[0].0,
        "the floor without ForceToBack draws first: {wanted:?}"
    );
    assert!(wanted[1].0, "ForceToBack draws last: {wanted:?}");
}

#[test]
fn a_blend_lasts_its_declared_frames_of_the_incoming_clip() {
    let rows: [(Option<f32>, u32, f32, f32); 8] = [
        (Some(10.0), 61, 1.0, 10.0 / 30.0),
        (None, 61, 1.0, 5.0 / 30.0),
        (Some(-3.0), 61, 1.0, 5.0 / 30.0),
        (Some(0.0), 61, 1.0, 0.0),
        (Some(1666.0), 61, 1.0, 60.0 / 30.0),
        (Some(0.25), 61, 1.0, 1.0 / 30.0),
        (Some(10.0), 61, 2.0, 10.0 / 60.0),
        (Some(10.0), 4, 1.0, 3.0 / 30.0),
    ];
    for (declared, frames, speed, seconds) in rows {
        let got = blend_seconds(declared, frames, 30, speed);
        assert!(
            (got - seconds).abs() < 1e-6,
            "{declared:?} over {frames} frames at speed {speed}: {got}"
        );
    }
    assert_eq!(blend_seconds(Some(10.0), 61, 0, 1.0), 0.0, "no frame rate");
}

#[test]
fn a_ghost_is_see_through_tinted_and_still() {
    let ghost = GhostLook {
        opacity: 0.45,
        tint: [1.0, 0.2, 0.2],
        light_add: [0.0; 3],
        state_particles: false,
    };
    assert_eq!(
        ghost_tint([0.5, 0.5, 1.0, 1.0], ghost),
        [0.5, 0.1, 0.2, 0.45]
    );
    let hostile = GhostLook {
        opacity: f32::NAN,
        tint: [2.0, -1.0, f32::INFINITY],
        light_add: [0.0; 3],
        state_particles: false,
    };
    assert_eq!(ghost_tint([1.0; 4], hostile), [1.0, 0.0, 1.0, 1.0]);

    let mut scene = scene_with_one_template();
    let table = mint_sage_ini::BlockTable::object_model_path();
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = stand\n      End\n    End\n  End\nEnd\n";
    let document = mint_sage_ini::Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap().clone();
    let states = mint_sage_ini::draw_animation_states(&draw);
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.machine = StateMachine::compile(&states);
        template.states = states;
    }
    let shown = UnitPose {
        ghost: Some(ghost),
        ..pose(5, 0)
    };
    scene.sync(&[shown], 0.0);
    let template = scene.templates.first().unwrap();
    let drawable = scene.drawables.first_mut().unwrap();
    let mut scratch = Scratch::default();
    let posed = pose_drawable(
        drawable,
        template,
        &shown,
        1.0,
        &[],
        Blending::Retail,
        true,
        &mut scratch,
    );
    assert_eq!(posed, Posed::Held);
    assert!(drawable.drawn);
    assert_eq!(drawable.ghost, Some(ghost));
    assert_eq!(drawable.state(), None, "a ghost does not animate");
    let solid = pose(5, 0);
    pose_drawable(
        drawable,
        template,
        &solid,
        2.0,
        &[],
        Blending::Retail,
        true,
        &mut scratch,
    );
    assert!(drawable.state().is_some());
    assert_eq!(drawable.ghost, None);
}

#[test]
fn entering_state_fx_fires_once_on_the_edge_only() {
    let mut scene = scene_with_one_template();
    let anim = |name: &str| AnimationClip {
        label: None,
        name: name.to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    let state = |conditions: &[&str], fx: Option<&str>| AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        state_name: None,
        entering_state_fx: fx.map(str::to_owned),
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: vec![anim("Clip")],
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    let states = vec![
        state(&[], Some("FX_Idle")),
        state(&["MOVING"], Some("FX_Move")),
    ];
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.machine = StateMachine::compile(&states);
        template.states = states;
    }
    let idle = pose(6, 0);
    scene.sync(&[idle], 0.0);
    let mut scratch = Scratch::default();
    let mut frame = |scene: &mut UnitScene, pose: &UnitPose, now: f32| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            pose,
            now,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        drawable.fx_pending.drain(..).collect::<Vec<_>>()
    };
    assert!(
        frame(&mut scene, &idle, 0.0).is_empty(),
        "a spawn lands on its state directly, no edge - though idle declares FX_Idle"
    );
    let moving = UnitPose {
        conditions: Conditions::MOVING,
        ..idle
    };
    assert_eq!(
        frame(&mut scene, &moving, 1.0),
        ["FX_Move"],
        "entering MOVING fires its EnteringStateFX"
    );
    assert!(
        frame(&mut scene, &moving, 2.0).is_empty(),
        "still MOVING: re-selecting the same state does not repeat it"
    );
    assert_eq!(
        frame(&mut scene, &idle, 3.0),
        ["FX_Idle"],
        "a genuine edge back to idle fires its own EnteringStateFX"
    );
}

#[test]
fn a_merged_attack_state_re_enters_every_cycle_even_when_firing_never_drops() {
    let mut scene = scene_with_one_template();
    let state = |conditions: &[&str]| AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: Vec::new(),
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    let states = vec![state(&[]), state(&["FIRING_OR_PREATTACK_A"])];
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.machine = StateMachine::compile(&states);
        template.states = states;
    }
    let idle = pose(6, 0);
    scene.sync(&[idle], 0.0);
    let mut scratch = Scratch::default();
    let conditions = Conditions::weapon(0, WeaponPhase::Firing);
    for (at, attack_cycle) in [0u32, 1, 3].into_iter().enumerate() {
        let swinging = UnitPose {
            conditions,
            attack_cycle,
            attack_slot: Some(0),
            ..idle
        };
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &swinging,
            f32::from(u8::try_from(at).unwrap()),
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
    }
    assert_eq!(
        scratch.entries, 3,
        "every cycle re-enters, not only the first"
    );
}

#[test]
fn an_attack_cycle_change_alone_does_not_reselect_the_model_condition_state() {
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model_states = vec![
            state_def(&[], Some("synthetic")),
            state_def(&["UPGRADE_WEAPON"], Some("upgraded")),
        ];
        template.model_machine =
            ModelStateMachine::compile_with(&template.model_states, Some(&mut template.extras));
        template.bodies.push("upgraded".to_owned());
        template.art.push(Some(empty_body()));
        template.state_bodies = vec![StateBody::Pool, StateBody::Body(1)];
    }
    let idle = pose(9, 0);
    scene.sync(&[idle], 0.0);
    let mut scratch = Scratch::default();
    let conditions = Conditions::weapon(0, WeaponPhase::Firing);
    for cycle in 0..4u32 {
        let swinging = UnitPose {
            conditions,
            attack_cycle: cycle,
            attack_slot: Some(0),
            ..idle
        };
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &swinging,
            f32::from(u8::try_from(cycle).unwrap()),
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        assert_eq!(
            drawable.body, 0,
            "cycle {cycle} must still wear the plain body, not UPGRADE_WEAPON's"
        );
    }
}

#[test]
fn a_manual_clip_frame_is_progress_times_the_clip_length() {
    assert_eq!(manual_frame(1001, 0.0), 0.0);
    assert_eq!(manual_frame(1001, 0.5), 500.0);
    assert_eq!(manual_frame(1001, 1.0), 1000.0);
    assert_eq!(manual_frame(301, 1.0), 300.0);
    assert_eq!(manual_frame(1001, 7.0), 1000.0);
    assert_eq!(manual_frame(1001, f32::NAN), 0.0);
    assert_eq!(manual_frame(0, 0.5), 0.0);
}

#[test]
fn a_drawable_wears_the_skin_its_conditions_select_and_only_then() {
    let mut scene = scene_with_one_template();
    let state = |conditions: &[&str]| ModelStateDef {
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        model: Some("synthetic".to_owned()),
        textures: vec![("a.tga".to_owned(), "a_snow.tga".to_owned())],
        particles: Vec::new(),
        decal: None,
        launch_bones: Default::default(),
        retain_sub_objects: false,
        model_animation_prefix: None,
        portrait_image: None,
        button_image: None,
        file: "t.ini".into(),
        line: 1,
    };
    let skin: Skin = vec![None].into();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model_states = vec![state(&[]), state(&["SNOW"])];
        template.model_machine = ModelStateMachine::compile(&template.model_states);
        template.skins.insert((0, 1), Arc::clone(&skin));
    }
    let plain = pose(4, 0);
    let snowy = UnitPose {
        conditions: Conditions::SNOW,
        ..plain
    };
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let wear = |scene: &mut UnitScene, pose: &UnitPose, scratch: &mut Scratch| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            scratch,
        );
        (drawable.model_state, drawable.skin.clone())
    };
    let (state_now, worn) = wear(&mut scene, &plain, &mut scratch);
    assert_eq!(state_now, Some(0));
    assert!(worn.is_none(), "the default state substitutes nothing");
    let (state_now, worn) = wear(&mut scene, &snowy, &mut scratch);
    assert_eq!(state_now, Some(1));
    assert!(worn.is_some_and(|worn| Arc::ptr_eq(&worn, &skin)));
    let (_, worn) = wear(&mut scene, &plain, &mut scratch);
    assert!(worn.is_none(), "the thaw takes the skin off again");
}

#[test]
fn the_selected_model_states_decal_is_drawn_under_the_drawable() {
    let mut scene = scene_with_one_template();
    let ring = mint_sage_ini::StateDecal {
        blend: mint_sage_ini::DecalBlend::Alpha,
        texture: "ring".to_owned(),
        size_x: 440.0,
        size_y: 440.0,
    };
    let state = |conditions: &[&str], decal: Option<mint_sage_ini::StateDecal>| ModelStateDef {
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        model: Some("synthetic".to_owned()),
        textures: Vec::new(),
        particles: Vec::new(),
        decal,
        launch_bones: Default::default(),
        retain_sub_objects: false,
        model_animation_prefix: None,
        portrait_image: None,
        button_image: None,
        file: "t.ini".into(),
        line: 1,
    };
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model_states = vec![state(&[], None), state(&["SNOW"], Some(ring.clone()))];
        template.model_machine = ModelStateMachine::compile(&template.model_states);
    }
    let plain = pose(4, 0);
    let placing = UnitPose {
        conditions: Conditions::SNOW,
        ghost: Some(GhostLook {
            opacity: 0.45,
            tint: [1.0; 3],
            light_add: [0.0; 3],
            state_particles: false,
        }),
        placement: Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [7.0, 9.0, 0.0, 1.0],
        ]),
        ..plain
    };
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let mut apply = |scene: &mut UnitScene, pose: &UnitPose| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        drawable.placement = pose.placement;
        pose_drawable(
            drawable,
            template,
            pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
    };
    apply(&mut scene, &plain);
    assert_eq!(scene.state_decals().count(), 0);
    apply(&mut scene, &placing);
    let drawn: Vec<_> = scene.state_decals().collect();
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].2, &ring);
    assert_eq!(drawn[0].1, placing.placement);
}

#[test]
fn a_companion_module_selects_its_own_state_on_the_drawables_conditions() {
    let mut scene = scene_with_one_template();
    let dust_state = |conditions: &[&str]| AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: Vec::new(),
        script: None,
        fx_events: Vec::new(),
        particles: vec![mint_sage_ini::ParticleSysBone {
            bone: None,
            system: "GenericSiegeTrailDust".to_owned(),
            follow_bone: false,
            module_flag: false,
            ..Default::default()
        }],
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    let states = vec![dust_state(&["MOVING", "ACCELERATE"])];
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.companions.push(CompanionModule {
            machine: StateMachine::compile(&states),
            states,
            model_states: Vec::new(),
            model_machine: ModelStateMachine::compile(&[]),
            has_model: false,
        });
    }
    let standing = pose(9, 0);
    let accelerating = UnitPose {
        conditions: Conditions::MOVING.union(Conditions::ACCELERATE),
        ..standing
    };
    scene.sync(&[standing], 0.0);
    let mut scratch = Scratch::default();
    let select = |scene: &mut UnitScene, pose: &UnitPose, scratch: &mut Scratch| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            scratch,
        );
        drawable.companion_states()[0].0
    };
    assert_eq!(select(&mut scene, &standing, &mut scratch), None);
    assert_eq!(select(&mut scene, &accelerating, &mut scratch), Some(0));
    assert_eq!(select(&mut scene, &standing, &mut scratch), None);
}

#[test]
fn a_sub_object_name_matches_whole_after_the_dot_or_as_a_prefix() {
    assert!(names_part("FireArowTip", "FIREAROWTIP"));
    assert!(names_part("FireArowTip", "GUARCHER_SKN.FIREAROWTIP"));
    assert!(names_part("FireArowTip", "FIREAROWTIP\0\0"), "file padding");
    assert!(names_part("V1_PIECE*", "V1_PIECE03"));
    assert!(names_part("V1_PIECE*", "GBBARRACKS.V1_PIECE03"));
    assert!(!names_part("V1", "V1FLAG"));
    assert!(!names_part("V1", "XV1"));
    assert!(!names_part("V1_PIECE*", "V2_PIECE01"));
    assert!(!names_part("V1_PIECE*", "V1_"), "shorter than the prefix");
}

#[test]
fn a_plain_sub_object_write_stands_and_a_permanent_one_decides_a_part_both_name() {
    let hidden = |plain: &[(String, bool)], permanent: &[(String, bool)], part: &str| {
        part_override(&sub_object_overrides(plain, permanent), part) == Some(false)
    };
    let (mut plain, mut permanent) = (Vec::new(), Vec::new());
    record_sub_object_writes(
        &mut plain,
        &mut permanent,
        vec![
            ("TIP".to_owned(), false, true),
            ("LIGHT".to_owned(), false, false),
        ],
    );
    record_sub_object_writes(&mut plain, &mut permanent, Vec::new());
    assert!(
        hidden(&plain, &permanent, "TIP") && hidden(&plain, &permanent, "LIGHT"),
        "both writes stand into a state that writes nothing"
    );
    record_sub_object_writes(
        &mut plain,
        &mut permanent,
        vec![("light".to_owned(), true, false)],
    );
    assert_eq!(
        plain,
        [("LIGHT".to_owned(), true)],
        "one entry per name, updated in place"
    );
    record_sub_object_writes(
        &mut plain,
        &mut permanent,
        vec![("tip".to_owned(), true, false)],
    );
    assert!(
        hidden(&plain, &permanent, "TIP"),
        "the permanent write decides a part a plain one also names"
    );
}

#[test]
fn a_plain_script_hide_stands_until_a_model_rebuild_that_does_not_retain_it() {
    let table = mint_sage_ini::BlockTable::object_model_path();
    let text = "Object A
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
      Model = Body
    End
    ModelConditionState = SNOW
      Model = Body
    End
    ModelConditionState = DAMAGED
      Model = BodyDamaged
    End
    ModelConditionState = REALLYDAMAGED
      Model = BodyWrecked
      RetainSubObjects = Yes
    End
    IdleAnimationState
      Animation
        AnimationName = idle
      End
      BeginScript
        CurDrawableHideSubObject(\"TIP\")
      EndScript
    End
    AnimationState = MOVING
      Animation
        AnimationName = run
      End
    End
  End
End
";
    let document = mint_sage_ini::Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap().clone();
    let states = mint_sage_ini::draw_animation_states(&draw);
    let model_states =
        mint_sage_ini::draw_model_states(&draw, &mint_sage_ini::MacroTable::default());
    let (scripts, report) = crate::anim_script::compile_states(&states);
    assert_eq!(report.failed, 0, "the idle script must parse");
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.machine = StateMachine::compile(&states);
        template.scripts = scripts;
        template.states = states;
        template.model_machine = ModelStateMachine::compile(&model_states);
        template.model_states = model_states;
    }
    let unit = pose(9, 0);
    scene.sync(&[unit], 0.0);
    let mut scratch = Scratch::default();
    let mut frame = |scene: &mut UnitScene, conditions: Conditions| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &UnitPose { conditions, ..unit },
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        drawable.plain_parts.clone()
    };
    let tip_hidden = vec![("TIP".to_owned(), false)];
    assert_eq!(
        frame(&mut scene, Conditions::NONE),
        tip_hidden,
        "idle hides it"
    );
    assert_eq!(
        frame(&mut scene, Conditions::SNOW),
        tip_hidden,
        "a model state on the same model keeps it"
    );
    assert_eq!(
        frame(&mut scene, Conditions::REALLYDAMAGED),
        tip_hidden,
        "RetainSubObjects keeps it across another model"
    );
    assert_eq!(
        frame(
            &mut scene,
            Conditions::MOVING.union(Conditions::REALLYDAMAGED)
        ),
        tip_hidden,
        "it stands while it walks"
    );
    assert!(
        frame(&mut scene, Conditions::MOVING.union(Conditions::DAMAGED)).is_empty(),
        "another model drops it"
    );
}

#[test]
fn the_last_override_naming_a_part_decides_and_an_unnamed_part_is_left_alone() {
    let overrides: Vec<(Arc<str>, bool)> = vec![
        ("V1*".into(), false),
        ("V1FLAG".into(), true),
        ("V2".into(), false),
    ];
    assert!(part_hidden(&overrides, "V1"));
    assert!(part_hidden(&overrides, "V1_PIECE01"));
    assert!(
        !part_hidden(&overrides, "V1FLAG"),
        "written after the wildcard"
    );
    assert!(part_hidden(&overrides, "V2"));
    assert!(!part_hidden(&overrides, "ROOF"));
    assert!(!part_hidden(&[], "V1"));
}

#[test]
fn an_override_carries_to_the_parts_on_bones_below_the_named_part() {
    let pivot = |name: &str, parent: Option<u32>| mint_sage_w3d::Pivot {
        name: name.to_owned(),
        parent,
        ..Default::default()
    };
    let pivots = [
        pivot("ROOT", None),
        pivot("MOUNT", Some(0)),
        pivot("FLAME", Some(1)),
        pivot("OTHER", Some(0)),
    ];
    let parts = [
        ("M.MOUNT", Some(1)),
        ("M.FLAME", Some(2)),
        ("M.OTHER", Some(3)),
        ("M.BODY", Some(0)),
        ("M.SKIN", None),
    ];
    let cases: [(&[(&str, bool)], [Option<bool>; 5], &str); 4] = [
        (
            &[("Mount", false)],
            [Some(false), Some(false), None, None, None],
            "the flame on the mount's bone goes with it",
        ),
        (
            &[("Mount", false), ("Flame", true)],
            [Some(false), Some(true), None, None, None],
            "a later write naming the child wins",
        ),
        (
            &[("Body", false)],
            [None, None, None, Some(false), None],
            "the root bone carries nothing",
        ),
        (
            &[("Nothing", false)],
            [None; 5],
            "a name that matches no part changes nothing",
        ),
    ];
    for (overrides, expected, why) in cases {
        assert_eq!(
            bone_part_overrides(overrides, &parts, &pivots),
            expected,
            "{why}"
        );
    }
}

#[test]
fn a_look_is_interned_once_and_rebuilt_only_when_its_stamp_moves() {
    let mut scene = UnitScene::new();
    assert_eq!(scene.intern_look(UnitLook::default()), LookKey::PLAIN);
    let tip = UnitLook {
        sub_objects: vec![("FireArowTip".into(), true)],
        ..UnitLook::default()
    };
    let key = scene.intern_look(tip.clone());
    assert_ne!(key, LookKey::PLAIN);
    assert_eq!(
        scene.intern_look(tip.clone()),
        key,
        "the same look, the same key"
    );
    assert_eq!(scene.look(key), tip);
    assert_eq!(scene.look(LookKey(99)), UnitLook::default());

    let built = std::cell::Cell::new(0);
    let ask = |scene: &mut UnitScene, id: u32, stamp: u64| {
        scene.look_for(id, stamp, || {
            built.set(built.get() + 1);
            tip.clone()
        })
    };
    assert_eq!(ask(&mut scene, 7, 1), key);
    assert_eq!(ask(&mut scene, 7, 1), key);
    assert_eq!(
        ask(&mut scene, 8, 1),
        key,
        "another drawable builds its own"
    );
    assert_eq!(ask(&mut scene, 7, 2), key, "a new stamp rebuilds");
    assert_eq!(built.get(), 3);
    assert_eq!(ask(&mut scene, u32::MAX, 1), key);
    assert_eq!(ask(&mut scene, u32::MAX, 1), key);
    assert_eq!(built.get(), 5);
    assert!(scene.look_stamps.len() <= 9);
}

#[test]
fn an_upgrades_override_wins_over_the_states_and_either_alone_decides() {
    assert!(!overridden_hidden(None, None));
    assert!(overridden_hidden(None, Some(false)));
    assert!(!overridden_hidden(None, Some(true)));
    assert!(overridden_hidden(Some(false), None));
    assert!(
        !overridden_hidden(Some(true), Some(false)),
        "the upgrade shows it"
    );
    assert!(
        overridden_hidden(Some(false), Some(true)),
        "the upgrade hides it"
    );
}

#[test]
fn a_look_selects_its_states_and_follows_the_body_worn() {
    let mut scene = scene_with_one_template();
    let upgraded = UnitLook {
        conditions: vec!["WEAPONSET_PLAYER_UPGRADE".into()],
        sub_objects: vec![("Forged_Blade".into(), true)],
        ..UnitLook::default()
    };
    let key = scene.intern_look(upgraded);
    let art = |hidden: bool| {
        Arc::new(LookArt {
            parts: vec![Some(!hidden)],
            skin: None,
            conditions: Conditions::NONE,
        })
    };
    let (pool_art, scaffold_art) = (art(false), art(true));
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.model_states = vec![
            state_def(&[], Some("synthetic")),
            state_def(&["AWAITING_CONSTRUCTION"], Some("scaffold")),
            state_def(&["WEAPONSET_PLAYER_UPGRADE"], Some("synthetic")),
        ];
        template.model_machine =
            ModelStateMachine::compile_with(&template.model_states, Some(&mut template.extras));
        template.bodies.push("scaffold".to_owned());
        template.art.push(Some(empty_body()));
        template.state_bodies = vec![StateBody::Pool, StateBody::Body(1), StateBody::Pool];
        let bit = template.extras.bits_of(["WEAPONSET_PLAYER_UPGRADE"]);
        assert!(!bit.is_empty());
        template.set_look_conditions(key, bit);
        template.looks.insert((0, key), Arc::clone(&pool_art));
        template.looks.insert((1, key), Arc::clone(&scaffold_art));
    }
    let plain = pose(9, 0);
    scene.sync(&[plain], 0.0);
    let mut scratch = Scratch::default();
    let mut frame = |scene: &mut UnitScene, pose: UnitPose| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        (drawable.model_state, drawable.body, drawable.look.clone())
    };
    let (state, _, look) = frame(&mut scene, plain);
    assert_eq!(state, Some(0));
    assert!(look.is_none());
    let worn = UnitPose { look: key, ..plain };
    let (state, body, look) = frame(&mut scene, worn);
    assert_eq!(state, Some(2), "the flag's own state");
    assert_eq!(body, 0);
    assert!(look.is_some_and(|look| Arc::ptr_eq(&look, &pool_art)));
    let site = UnitPose {
        conditions: Conditions::AWAITING_CONSTRUCTION,
        ..worn
    };
    let (state, body, look) = frame(&mut scene, site);
    assert_eq!(
        (state, body),
        (Some(1), 1),
        "declared first, the scaffold wins"
    );
    assert!(look.is_some_and(|look| Arc::ptr_eq(&look, &scaffold_art)));
    let (state, _, look) = frame(&mut scene, plain);
    assert_eq!(state, Some(0));
    assert!(look.is_none(), "the plain look wears nothing");
}

#[test]
fn a_look_resolved_after_first_sight_is_still_worn() {
    let mut scene = scene_with_one_template();
    let key = scene.intern_look(UnitLook {
        sub_objects: vec![("Tip".into(), true)],
        ..UnitLook::default()
    });
    let worn = UnitPose {
        look: key,
        ..pose(3, 0)
    };
    scene.sync(&[worn], 0.0);
    let mut scratch = Scratch::default();
    let mut frame = |scene: &mut UnitScene| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        pose_drawable(
            drawable,
            template,
            &worn,
            1.0,
            &[],
            Blending::Retail,
            true,
            &mut scratch,
        );
        drawable.look.is_some()
    };
    assert!(!frame(&mut scene), "not resolved yet");
    let art = Arc::new(LookArt::default());
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.set_look_conditions(key, Conditions::NONE);
        template.looks.insert((0, key), art);
    }
    assert!(frame(&mut scene), "worn once resolved");
}

#[test]
fn sync_keeps_survivors_adds_newcomers_and_drops_the_gone() {
    let mut scene = scene_with_one_template();
    let poses: Vec<UnitPose> = [9u32, 1, 5, 7, 3].iter().map(|id| pose(*id, 0)).collect();
    assert_eq!(scene.sync(&poses, 1.0), 5);
    let next: Vec<UnitPose> = [3u32, 12, 9, 5].iter().map(|id| pose(*id, 0)).collect();
    assert_eq!(scene.sync(&next, 2.0), 1, "only 12 is new");
    let kept: Vec<(u32, f32)> = scene
        .drawables()
        .iter()
        .map(|d| (d.id, d.entered_at()))
        .collect();
    assert_eq!(kept, [(3, 1.0), (12, 2.0), (9, 1.0), (5, 1.0)]);
    assert_eq!(scene.sync(&[pose(4, 7)], 3.0), 0);
    assert!(scene.drawables().is_empty());
}

fn fx_event(frame: u32, fire_when_skipped: bool) -> AnimationFxEvent {
    AnimationFxEvent {
        frame,
        fire_when_skipped,
        fx_list: "FX_Test".to_owned(),
        frame_stop: None,
        bone: None,
    }
}

fn fx_event_with_stop(frame: u32, frame_stop: u32) -> AnimationFxEvent {
    AnimationFxEvent {
        frame,
        fire_when_skipped: false,
        fx_list: "FX_Test".to_owned(),
        frame_stop: Some(frame_stop),
        bone: None,
    }
}

#[test]
fn an_fx_event_at_the_start_frame_fires_once_on_entry() {
    let events = [fx_event(0, false)];
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&events, None, 0.0, 30.0, &mut out, &mut bone_out);
    assert_eq!(out, ["FX_Test"]);
    out.clear();
    queue_fx_events(&events, Some(0.0), 0.0, 30.0, &mut out, &mut bone_out);
    assert!(out.is_empty(), "paused at the same frame refired");
}

#[test]
fn crossing_the_target_frame_fires_exactly_once() {
    let events = [fx_event(11, false)];
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&events, Some(10.5), 11.0, 30.0, &mut out, &mut bone_out);
    assert_eq!(out, ["FX_Test"]);
    out.clear();
    queue_fx_events(&events, Some(9.0), 10.0, 30.0, &mut out, &mut bone_out);
    assert!(out.is_empty(), "fired before the target");
    out.clear();
    queue_fx_events(&events, Some(11.0), 12.0, 30.0, &mut out, &mut bone_out);
    assert!(out.is_empty(), "fired again past the target");
}

#[test]
fn a_loop_wrap_still_fires_a_target_near_the_end() {
    let events = [fx_event(3, false)];
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&events, Some(2.5), 0.5, 3.0, &mut out, &mut bone_out);
    assert_eq!(out, ["FX_Test"]);
}

#[test]
fn fire_when_skipped_is_what_a_large_step_needs_to_still_fire() {
    let quiet = [fx_event(11, false)];
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&quiet, Some(9.0), 15.0, 30.0, &mut out, &mut bone_out);
    assert!(out.is_empty(), "an unflagged event fired across a skip");

    let flagged = [fx_event(11, true)];
    queue_fx_events(&flagged, Some(9.0), 15.0, 30.0, &mut out, &mut bone_out);
    assert_eq!(out, ["FX_Test"]);
}

#[test]
fn queued_fx_events_are_capped_per_frame() {
    let events: Vec<AnimationFxEvent> = (0..u32::try_from(MAX_FX_EVENTS_QUEUED + 10).unwrap())
        .map(|frame| fx_event(frame, false))
        .collect();
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&events, None, 100.0, 100.0, &mut out, &mut bone_out);
    assert_eq!(out.len(), MAX_FX_EVENTS_QUEUED);
}

#[test]
fn an_fx_event_with_frame_stop_also_queues_a_bone_fx_cue() {
    let events = [fx_event_with_stop(14, 70)];
    let mut out = Vec::new();
    let mut bone_out = Vec::new();
    queue_fx_events(&events, None, 14.0, 100.0, &mut out, &mut bone_out);
    assert_eq!(out, ["FX_Test"]);
    assert_eq!(bone_out.len(), 1);
    assert_eq!(bone_out[0].frame_stop, 70);
    assert_eq!(bone_out[0].slot, 0);
}

#[test]
fn a_rigid_body_with_no_clip_resolves_a_bone_at_its_bind_pose() {
    let pivot = |name: &str, x: f32, y: f32, z: f32| mint_sage_w3d::Pivot {
        name: name.to_owned(),
        translation: mint_sage_w3d::Vec3 { x, y, z },
        ..Default::default()
    };
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.art = vec![Some(Arc::new(BodyArt {
            model: Arc::new(CachedModel {
                name: "rigid".to_owned(),
                hierarchy: Some(mint_sage_w3d::Hierarchy {
                    header: mint_sage_w3d::HierarchyHeader::default(),
                    pivots: vec![
                        pivot("ROOTTRANSFORM", 0.0, 0.0, 0.0),
                        pivot("FIRE01", 10.0, 0.0, 5.0),
                    ],
                }),
                parts: Vec::new(),
                bounds: Aabb::EMPTY,
                vertices: 0,
                triangles: 0,
            }),
            gpu: GpuSkin::default(),
        }))];
    }
    scene.sync(&[pose(9, 0)], 0.0);
    let drawable = scene.drawables.first().unwrap();
    assert!(drawable.bones().is_empty(), "the body must be rigid");

    let local = crate::particles_present::unit_bone_local(drawable, Some("FIRE01"));
    let frame = drawable.placement().mul_mat4(local);
    assert_eq!(frame.transform_point(Vec3::ZERO), Vec3::new(10.0, 0.0, 5.0));
}

#[test]
fn a_transition_in_flight_is_abandoned_when_conditions_pick_a_new_destination() {
    let table = mint_sage_ini::BlockTable::object_model_path();
    let text = "\
Object A
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
      StateName = IDLE
      Animation
        AnimationName = idle
      End
    End
    AnimationState = SELECTED
      Animation
        AnimationName = sel
      End
      BeginScript
        Prev = CurDrawablePrevAnimationState()
        if Prev == \"IDLE\" then
          CurDrawableSetTransitionAnimState(\"TRANS_Idle_to_Selected\")
        end
      EndScript
    End
    AnimationState = MOVING
      Animation
        AnimationName = run
      End
    End
    TransitionState = TRANS_Idle_to_Selected
      Animation
        AnimationName = trans
      End
    End
  End
End
";
    let document = mint_sage_ini::Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap().clone();
    let states = mint_sage_ini::draw_animation_states(&draw);
    let (scripts, report) = crate::anim_script::compile_states(&states);
    assert_eq!(report.failed, 0, "the SELECTED script must parse");
    let transition_names: BTreeMap<String, usize> = states
        .iter()
        .enumerate()
        .filter_map(|(index, state)| {
            state
                .transition_name()
                .map(|name| (name.to_ascii_uppercase(), index))
        })
        .collect();
    let moving_index = states
        .iter()
        .position(|state| state.conditions.iter().any(|c| c == "MOVING"))
        .expect("the fixture declares MOVING");

    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.machine = StateMachine::compile(&states);
        template.scripts = scripts;
        template.transition_names = transition_names;
        template.states = states;
    }

    let idle = pose(9, 0);
    scene.sync(&[idle], 0.0);
    let mut scratch = Scratch::default();
    let frame = |scene: &mut UnitScene, conditions: Conditions, scratch: &mut Scratch| {
        let template = scene.templates.first().unwrap();
        let drawable = scene.drawables.first_mut().unwrap();
        let pose = UnitPose { conditions, ..idle };
        pose_drawable(
            drawable,
            template,
            &pose,
            1.0,
            &[],
            Blending::Retail,
            true,
            scratch,
        );
    };

    frame(&mut scene, Conditions::NONE, &mut scratch);
    frame(&mut scene, Conditions::SELECTED, &mut scratch);
    assert!(
        scene.drawables.first().unwrap().mid_transition,
        "selecting from idle must queue the transition"
    );

    frame(&mut scene, Conditions::MOVING, &mut scratch);
    let drawable = scene.drawables.first().unwrap();
    assert!(
        !drawable.mid_transition,
        "a move order must abandon the selection transition, not play it out"
    );
    assert_eq!(
        drawable.state(),
        Some(moving_index),
        "the drawable must land on MOVING the same frame the order arrives"
    );
}

fn bound_test_clip(model: &CachedModel, frames: u32) -> Arc<ModelAnimation> {
    let declared = DefaultAnimation {
        name: "TEST.CLIP".to_owned(),
        mode: Some(AnimationMode::Loop),
        blend_frames: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        source: mint_sage_ini::AnimationSource::IdleAnimationState,
    };
    let loaded = LoadedAnimation {
        path: mint_vfs::VfsPath::new("clip.w3d"),
        animation: mint_sage_w3d::Animation {
            name: "CLIP".to_owned(),
            hierarchy_name: "TEST".to_owned(),
            num_frames: frames,
            frame_rate: 30,
            compressed: false,
            flavor: None,
            channels: Vec::new(),
            channel_chunks: 0,
        },
        hierarchy_path: mint_vfs::VfsPath::new("test_skl.w3d"),
        hierarchy: model.hierarchy.clone().unwrap(),
    };
    Arc::new(ModelAnimation::bind(model, &declared, loaded).unwrap())
}

fn scene_with_states(text: &str) -> UnitScene {
    let table = mint_sage_ini::BlockTable::object_model_path();
    let document = mint_sage_ini::Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap().clone();
    let states = mint_sage_ini::draw_animation_states(&draw);
    let model = Arc::new(CachedModel {
        name: "test".to_owned(),
        hierarchy: Some(mint_sage_w3d::Hierarchy {
            header: mint_sage_w3d::HierarchyHeader {
                name: "TEST".to_owned(),
                ..Default::default()
            },
            pivots: vec![mint_sage_w3d::Pivot {
                name: "ROOT".to_owned(),
                ..Default::default()
            }],
        }),
        parts: Vec::new(),
        bounds: Aabb::EMPTY,
        vertices: 0,
        triangles: 0,
    });
    let mut scene = scene_with_one_template();
    let template = scene.templates.get_mut(0).unwrap();
    for (index, state) in states.iter().enumerate() {
        if state
            .clips
            .iter()
            .any(|clip| !clip.name.starts_with("MISSING"))
        {
            template
                .bound
                .insert((0, index, 0), bound_test_clip(&model, 61));
        }
    }
    template.art = vec![Some(Arc::new(BodyArt {
        model,
        gpu: GpuSkin::default(),
    }))];
    template.machine = StateMachine::compile(&states);
    template.states = states;
    scene
}

fn pose_at(scene: &mut UnitScene, conditions: Conditions, now: f32) -> Posed {
    let unit = UnitPose {
        conditions,
        ..pose(3, 0)
    };
    if scene.drawables.is_empty() {
        scene.sync(&[unit], 0.0);
    }
    let template = scene.templates.first().unwrap();
    let drawable = scene.drawables.first_mut().unwrap();
    pose_drawable(
        drawable,
        template,
        &unit,
        now,
        &[],
        Blending::Retail,
        true,
        &mut Scratch::default(),
    )
}

#[test]
fn a_state_whose_clip_is_missing_blends_to_the_pose_the_rest_select_and_plays_it() {
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n      End\n    End\n    AnimationState = SELECTED\n      Animation = Attention\n        AnimationName = MISSING.ATTENTION\n        AnimationBlendTime = 300\n      End\n    End\n    AnimationState = MOVING\n      Animation = Run\n        AnimationName = TEST.RUN\n      End\n    End\n  End\nEnd\n";
    let (idle, selected, moving) = (0, 1, 2);
    let mut scene = scene_with_states(text);
    assert_eq!(pose_at(&mut scene, Conditions::MOVING, 1.0), Posed::Sampled);
    assert_eq!(scene.drawables[0].state(), Some(moving));

    assert_eq!(
        pose_at(&mut scene, Conditions::SELECTED, 2.0),
        Posed::Sampled
    );
    let drawable = &scene.drawables[0];
    assert_eq!(drawable.state(), Some(selected), "the state itself is kept");
    let blend = drawable.blend.unwrap();
    assert_eq!(blend.from.state, moving, "out of the run");
    assert!(
        (blend.duration - 5.0 / 30.0).abs() < 1e-6,
        "over the idle clip's own blend time"
    );

    assert_eq!(
        pose_at(&mut scene, Conditions::SELECTED, 2.5),
        Posed::Sampled
    );
    let (state, _, first) = scene.drawables[0].sampled.unwrap();
    assert_eq!(state, idle, "into the idle, once the blend is done");
    pose_at(&mut scene, Conditions::SELECTED, 2.6);
    let (_, _, later) = scene.drawables[0].sampled.unwrap();
    assert_ne!(first, later, "and the idle plays on");
}

#[test]
fn a_missing_clip_scans_the_state_machine_once_per_entry_not_per_frame() {
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n      End\n    End\n    AnimationState = SELECTED\n      Animation = Attention\n        AnimationName = MISSING.ATTENTION\n        AnimationBlendTime = 300\n      End\n    End\n  End\nEnd\n";
    let mut scene = scene_with_states(text);

    let before_entry = super::pose::STAND_IN_RECOMPUTES.with(std::cell::Cell::get);
    pose_at(&mut scene, Conditions::SELECTED, 1.0);
    let after_entry = super::pose::STAND_IN_RECOMPUTES.with(std::cell::Cell::get);
    assert_eq!(after_entry - before_entry, 1, "one scan to enter the state");

    pose_at(&mut scene, Conditions::SELECTED, 1.1);
    pose_at(&mut scene, Conditions::SELECTED, 1.2);
    let after_steady = super::pose::STAND_IN_RECOMPUTES.with(std::cell::Cell::get);
    assert_eq!(
        after_steady, after_entry,
        "two more frames with the same conditions must not rescan"
    );
}

#[test]
fn a_builder_and_the_plan_it_carries_hold_separate_tint_slots() {
    let mut scene = scene_with_two_templates();
    let body_pose = pose(7, 0);
    let plan_pose = UnitPose {
        ghost: Some(GhostLook {
            opacity: 0.45,
            tint: [1.0, 1.0, 1.0],
            light_add: [0.0; 3],
            state_particles: false,
        }),
        ..pose(7, 1)
    };
    scene.sync(&[body_pose, plan_pose], 0.0);
    for unit in [body_pose, plan_pose] {
        let (drawables, templates) = (&mut scene.drawables, &scene.templates);
        let drawable = drawables
            .iter_mut()
            .find(|drawable| drawable.id == unit.id && drawable.template == unit.template)
            .unwrap();
        pose_drawable(
            drawable,
            &templates[unit.template],
            &unit,
            0.0,
            &[],
            Blending::Retail,
            true,
            &mut Scratch::default(),
        );
    }

    let envelope = TintEnvelope {
        color: [1.0, 0.0, 0.0],
        attack: 0.0,
        sustain: 1.0,
        decay: 0.0,
    };
    let reached = scene.play_tint(7, TintSlot::Selection, envelope, 0.0);
    assert_eq!(reached, 1, "only the selected body, not the plan ghost");
    assert_eq!(
        scene.tint_at(7, 0.0),
        Some([1.0, 0.0, 0.0]),
        "read back off the body, not whichever drawable sorts first"
    );
}

#[test]
fn a_state_change_mid_fade_blends_out_of_the_fades_dominant_side() {
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n        AnimationBlendTime = 1000\n      End\n    End\n    AnimationState = SELECTED\n      Animation = Attention\n        AnimationName = TEST.ATTENTION\n        AnimationBlendTime = 1000\n      End\n    End\n    AnimationState = MOVING\n      Animation = Run\n        AnimationName = TEST.RUN\n        AnimationBlendTime = 1000\n      End\n    End\n  End\nEnd\n";
    let (selected, moving) = (1, 2);
    let mut scene = scene_with_states(text);
    let fading_from = |scene: &UnitScene| scene.drawables[0].blend.map(|blend| blend.from.state);
    pose_at(&mut scene, Conditions::MOVING, 1.0);
    pose_at(&mut scene, Conditions::NONE, 2.0);
    assert_eq!(fading_from(&scene), Some(moving));
    pose_at(&mut scene, Conditions::NONE, 2.2);
    pose_at(&mut scene, Conditions::SELECTED, 2.3);
    assert_eq!(
        fading_from(&scene),
        Some(moving),
        "a fifth of the way in, the run still dominates"
    );
    pose_at(&mut scene, Conditions::SELECTED, 3.4);
    pose_at(&mut scene, Conditions::NONE, 3.5);
    assert_eq!(
        fading_from(&scene),
        Some(selected),
        "past half way, the attention pose dominates"
    );
}

#[test]
fn a_model_modules_own_hide_flag_hides_only_that_module() {
    let mut scene = scene_with_two_templates();
    scene.templates[0].modules = vec![1];
    scene.templates[1].module_of = Some(0);
    scene.templates[1].hide_if = Conditions::SNOW;
    let snowing = UnitPose {
        conditions: Conditions::SNOW,
        ..pose(7, 0)
    };
    scene.sync(&[snowing], 0.0);
    scene.draw(&[snowing], 0.0, &mut Vec::new(), &[]);
    let hidden = |template: usize| {
        scene
            .drawables
            .iter()
            .find(|drawable| drawable.id == 7 && drawable.template == template)
            .map(|drawable| drawable.hidden)
    };
    assert_eq!(hidden(0), Some(false), "the body");
    assert_eq!(hidden(1), Some(true), "the module");
}

#[test]
fn an_object_draws_nothing_only_when_every_module_is_hidden() {
    let mut scene = scene_with_two_templates();
    scene.templates[0].modules = vec![1];
    scene.templates[1].module_of = Some(0);
    scene.templates[1].hide_if = Conditions::SNOW;
    let snowing = UnitPose {
        conditions: Conditions::SNOW,
        ..pose(7, 0)
    };
    scene.sync(&[snowing], 0.0);
    scene.draw(&[snowing], 0.0, &mut Vec::new(), &[]);
    assert!(scene.ids_drawing_nothing().is_empty(), "the body shows");
    scene.templates[0].hide_if = Conditions::SNOW;
    scene.draw(&[snowing], 0.0, &mut Vec::new(), &[]);
    assert_eq!(scene.ids_drawing_nothing(), [7]);
}

#[test]
fn resolve_sound_cues_matches_a_clips_own_name_case_insensitively() {
    let mut scene = scene_with_one_template();
    let anim = |name: &str| AnimationClip {
        label: None,
        name: name.to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    let state = AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: Vec::new(),
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: vec![anim("Unit_SKL.Unit_DIEB"), anim("Unit_SKL.Unit_WALK")],
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    let entry = mint_sage_ini::AnimationSoundEntry {
        sound: "BodyFallGeneric1".to_owned(),
        animations: vec!["unit_skl.unit_dieb".to_owned()],
        frames: vec![19],
    };
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.states = vec![state];
        template.animation_sounds = vec![entry];
    }
    scene.resolve_sound_cues(0);
    let template = scene.templates.first().unwrap();
    assert_eq!(
        template.sound_cues.get(&(0, 0)),
        Some(&vec![("BodyFallGeneric1".to_owned(), vec![19])]),
        "the DIEB clip matched despite the case difference"
    );
    assert!(
        !template.sound_cues.contains_key(&(0, 1)),
        "the WALK clip named nothing and matched no entry"
    );
    assert_eq!(scene.report().animation_sound_cues, 1);
}

#[test]
fn a_rider_is_drawn_on_its_containers_bone_as_posed_now() {
    let pivot = |name: &str, x: f32, y: f32, z: f32| mint_sage_w3d::Pivot {
        name: name.to_owned(),
        translation: mint_sage_w3d::Vec3 { x, y, z },
        ..Default::default()
    };
    let mut scene = scene_with_one_template();
    {
        let template = scene.templates.get_mut(0).unwrap();
        template.art = vec![Some(Arc::new(BodyArt {
            model: Arc::new(CachedModel {
                name: "bird".to_owned(),
                hierarchy: Some(mint_sage_w3d::Hierarchy {
                    header: mint_sage_w3d::HierarchyHeader::default(),
                    pivots: vec![
                        pivot("ROOTTRANSFORM", 0.0, 0.0, 0.0),
                        pivot("B_PASSENGER", 10.0, 0.0, 5.0),
                    ],
                }),
                parts: Vec::new(),
                bounds: Aabb::EMPTY,
                vertices: 0,
                triangles: 0,
            }),
            gpu: GpuSkin::default(),
        }))];
    }
    let rider = UnitPose {
        ride: Some(Ride {
            container: 1,
            pivot: 1,
        }),
        ..pose(2, 0)
    };
    let poses = [pose(1, 0), rider];
    scene.sync(&poses, 0.0);
    assert_eq!(scene.passenger_pivot(1, "B_Passenger", 0), Some(1));
    assert_eq!(scene.passenger_pivot(1, "B_Passenger", 1), None);
    {
        let container = scene.drawables.get_mut(0).unwrap();
        container.placement = Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0));
        container.bones = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(10.0, 0.0, 15.0)),
        ];
    }
    scene.drawables.get_mut(1).unwrap().placement =
        Mat4::from_translation(Vec3::new(110.0, 0.0, 5.0));
    super::ride::carry_riders(&mut scene.drawables, &[Some(0), Some(1)], &poses);
    let drawn = scene.drawables[1].placement().transform_point(Vec3::ZERO);
    assert!(
        (drawn.x - 110.0).abs() < 1e-4 && drawn.y.abs() < 1e-4 && (drawn.z - 15.0).abs() < 1e-4,
        "{drawn:?}"
    );
    assert_eq!(
        scene.drawables[0].placement().transform_point(Vec3::ZERO),
        Vec3::new(100.0, 0.0, 0.0),
        "the container itself does not move"
    );
}

#[test]
fn frame_lua_events_list_their_data_and_write_permanent_sub_objects() {
    let mut scene = scene_with_one_template();
    let event = |frame, data: &str| mint_sage_ini::AnimationLuaEvent {
        frame,
        data: Some(data.to_owned()),
    };
    let state = |events| AnimationStateDef {
        kind: mint_sage_ini::AnimationStateKind::Conditioned,
        conditions: Vec::new(),
        state_name: None,
        entering_state_fx: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        clips: Vec::new(),
        script: None,
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: events,
        allow_repeat_in_random_pick: false,
        file: "t.ini".into(),
        line: 1,
    };
    scene.templates.get_mut(0).unwrap().states = vec![
        state(vec![event(23, "hide_rock")]),
        state(vec![event(35, "show_rock"), event(40, "hide_rock")]),
    ];
    assert_eq!(scene.frame_event_data(0), ["hide_rock", "show_rock"]);
    scene.set_frame_event_parts(0, "hide_rock", vec![("ROCK".to_owned(), false)]);
    assert_eq!(
        scene.templates[0].frame_event_parts.get("hide_rock"),
        Some(&vec![("ROCK".to_owned(), false)])
    );

    scene.sync(&[pose(6, 0)], 0.0);
    let drawable = scene.drawables.first_mut().unwrap();
    write_permanent_parts(drawable, vec![("ROCK".to_owned(), false)]);
    write_permanent_parts(drawable, vec![("rock".to_owned(), true)]);
    assert_eq!(drawable.permanent_parts, [("ROCK".to_owned(), true)]);
}

#[test]
fn a_restarting_state_picks_another_clip_each_time_one_ends() {
    let clips = |flags: &str| {
        format!(
            "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n      End\n    End\n    AnimationState = ACTIVELY_CONSTRUCTING\n      Animation = Swing\n        AnimationName = TEST.SWING\n        AnimationMode = ONCE\n      End\n      Animation = Other\n        AnimationName = TEST.OTHER\n        AnimationMode = ONCE\n      End\n{flags}    End\n  End\nEnd\n"
        )
    };
    let shown_clips = |text: &str| {
        let mut scene = scene_with_states(text);
        {
            let template = scene.templates.get_mut(0).unwrap();
            let model = template.art[0].as_ref().unwrap().model.clone();
            for clip in 0..2 {
                template
                    .bound
                    .insert((0, 1, clip), bound_once_clip(&model, 31));
            }
        }
        let (mut shown, mut blended) = (Vec::new(), 0);
        for tenth in 0..=100u8 {
            pose_at(
                &mut scene,
                Conditions::ACTIVELY_CONSTRUCTING,
                f32::from(tenth) * 0.1,
            );
            let clip = scene.drawables[0].clip;
            if shown.last() != Some(&clip) {
                shown.push(clip);
            }
            if scene.drawables[0].blend.is_some() {
                blended += 1;
            }
        }
        (shown, blended)
    };
    let (restarting, blended) = shown_clips(&clips("      Flags = RESTART_ANIM_WHEN_COMPLETE\n"));
    assert!(
        restarting.len() >= 8,
        "about one pick a second over ten seconds: {restarting:?}"
    );
    assert!(
        restarting.windows(2).all(|pair| pair[0] != pair[1]),
        "never the same clip twice running: {restarting:?}"
    );
    assert!(
        blended >= restarting.len() - 1,
        "each pick blends in out of the clip that ended: {blended} frames"
    );
    let (holding, _) = shown_clips(&clips(""));
    assert_eq!(
        holding.len(),
        1,
        "unflagged, one clip plays and holds: {holding:?}"
    );
}

fn swing_scene(flags: &str) -> UnitScene {
    let text = format!(
        "Object A
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
      Animation = Idle
        AnimationName = TEST.IDLE
      End
    End
    AnimationState = FIRING_OR_PREATTACK_A
      Animation = Swing
        AnimationName = TEST.SWING
        AnimationMode = ONCE
        UseWeaponTiming = Yes
      End
      Animation = Other
        AnimationName = TEST.OTHER
        AnimationMode = ONCE
        UseWeaponTiming = Yes
      End
{flags}    End
  End
End
"
    );
    let mut scene = scene_with_states(&text);
    let template = scene.templates.get_mut(0).unwrap();
    let model = template.art[0].as_ref().unwrap().model.clone();
    for clip in 0..2 {
        template
            .bound
            .insert((0, 1, clip), bound_once_clip(&model, 31));
    }
    scene
}

fn swing_at(scene: &mut UnitScene, id: u32, cycle: u32, now: f32) -> Option<f32> {
    let unit = UnitPose {
        conditions: Conditions::weapon(0, WeaponPhase::Firing),
        attack_cycle: cycle,
        attack_slot: Some(0),
        ..pose(id, 0)
    };
    if scene.drawables.is_empty() {
        scene.sync(&[unit], 0.0);
    }
    let template = scene.templates.first().unwrap();
    let drawable = scene.drawables.first_mut().unwrap();
    pose_drawable(
        drawable,
        template,
        &unit,
        now,
        &[],
        Blending::Retail,
        true,
        &mut Scratch::default(),
    );
    drawable.phase
}

#[test]
fn a_restarting_swing_starts_at_its_first_frame() {
    let id = 38;
    assert!(
        crate::animation_state::phase_offset(true, 1.0, id, 1) > 0.05,
        "the id must be one a random start would visibly move"
    );
    let mut scene = swing_scene(
        "      Flags = RESTART_ANIM_WHEN_COMPLETE
",
    );
    pose_at(&mut scene, Conditions::NONE, 0.0);
    assert_eq!(swing_at(&mut scene, id, 1, 0.1), Some(0.0));
}

#[test]
fn an_idle_once_clip_starts_at_its_first_frame_unless_randomstart() {
    let first_phase = |flags: &str| {
        let text = format!(
            "Object A
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
      Animation = Spawn
        AnimationName = TEST.SPAWN
        AnimationMode = ONCE
      End
{flags}    End
  End
End
"
        );
        let mut scene = scene_with_states(&text);
        let template = scene.templates.get_mut(0).unwrap();
        let model = template.art[0].as_ref().unwrap().model.clone();
        template
            .bound
            .insert((0, 0, 0), bound_once_clip(&model, 31));
        pose_at(&mut scene, Conditions::NONE, 0.0);
        scene.drawables.first().unwrap().phase.unwrap()
    };
    assert_eq!(first_phase(""), 0.0);
    assert!(
        first_phase("      Flags = RANDOMSTART\n") > 0.05,
        "RANDOMSTART must move this id's start visibly"
    );
}

#[test]
fn a_new_cycle_cuts_a_clip_the_swing_state_only_re_picked() {
    let mut scene = swing_scene(
        "      Flags = RESTART_ANIM_WHEN_COMPLETE
",
    );
    let mut phase = None;
    for tick in 0..=30u8 {
        let now = f32::from(tick) * 0.05;
        let cycle = if now < 1.5 { 1 } else { 2 };
        phase = swing_at(&mut scene, 3, cycle, now);
    }
    let phase = phase.unwrap();
    assert!(
        phase < 0.01,
        "the second cycle restarts its swing at once: phase {phase}"
    );
}

#[test]
fn a_queued_swing_starts_where_its_cycle_would_have_put_it() {
    let mut scene = swing_scene("");
    let mut phase = None;
    for (now, cycle) in [
        (0.0, 1),
        (0.3, 1),
        (0.6, 1),
        (0.9, 1),
        (0.95, 2),
        (1.05, 2),
        (1.15, 2),
    ] {
        phase = swing_at(&mut scene, 3, cycle, now);
    }
    let phase = phase.unwrap();
    assert!(
        (phase - 0.2).abs() < 1e-4,
        "0.2 s after its cycle began: phase {phase}"
    );
}

#[test]
fn the_weapon_timing_fit_latches_at_clip_start_and_repicks_on_completion() {
    let mut scene = swing_scene(
        "      Flags = RESTART_ANIM_WHEN_COMPLETE
",
    );
    let unit_at = |weapon_cycle: f32| UnitPose {
        conditions: Conditions::weapon(0, WeaponPhase::Firing),
        attack_cycle: 1,
        attack_slot: Some(0),
        weapon_cycle,
        ..pose(50, 0)
    };
    scene.sync(&[unit_at(2.0)], 0.0);
    let template = scene.templates.first().unwrap();
    let drawable = scene.drawables.first_mut().unwrap();
    for call in 1..=20u32 {
        let now = f32::from(u8::try_from(call).unwrap_or(u8::MAX)) * 0.1;
        let weapon_cycle = if call <= 2 { 2.0 } else { 0.5 };
        pose_drawable(
            drawable,
            template,
            &unit_at(weapon_cycle),
            now,
            &[],
            Blending::Retail,
            true,
            &mut Scratch::default(),
        );
        let expected = f32::from(u8::try_from(call - 1).unwrap_or(u8::MAX)) * 0.05;
        let phase = drawable.phase.unwrap_or(f32::NAN);
        assert!(
            (phase - expected).abs() < 1e-3,
            "call {call}: held-rate phase {phase}, expected {expected}"
        );
    }
    pose_drawable(
        drawable,
        template,
        &unit_at(0.5),
        3.0,
        &[],
        Blending::Retail,
        true,
        &mut Scratch::default(),
    );
    assert!(drawable.swing_repicked, "the clip completed and re-picked");
    assert_eq!(
        drawable.phase,
        Some(0.0),
        "the re-pick starts at its first frame"
    );
    pose_drawable(
        drawable,
        template,
        &unit_at(0.5),
        3.1,
        &[],
        Blending::Retail,
        true,
        &mut Scratch::default(),
    );
    let phase = drawable.phase.unwrap();
    assert!(
        (phase - 0.2).abs() < 1e-3,
        "re-fitted rate: phase {phase}, expected 0.2"
    );
}

fn bound_once_clip(model: &CachedModel, frames: u32) -> Arc<ModelAnimation> {
    let declared = DefaultAnimation {
        name: "TEST.CLIP".to_owned(),
        mode: Some(AnimationMode::Once),
        blend_frames: None,
        flags: mint_sage_ini::AnimationFlags::default(),
        source: mint_sage_ini::AnimationSource::IdleAnimationState,
    };
    let loaded = LoadedAnimation {
        path: mint_vfs::VfsPath::new("clip.w3d"),
        animation: mint_sage_w3d::Animation {
            name: "CLIP".to_owned(),
            hierarchy_name: "TEST".to_owned(),
            num_frames: frames,
            frame_rate: 30,
            compressed: false,
            flavor: None,
            channels: Vec::new(),
            channel_chunks: 0,
        },
        hierarchy_path: mint_vfs::VfsPath::new("test_skl.w3d"),
        hierarchy: model.hierarchy.clone().unwrap(),
    };
    Arc::new(ModelAnimation::bind(model, &declared, loaded).unwrap())
}

#[test]
fn a_clip_that_must_complete_its_blend_queues_the_next_request() {
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n      End\n    End\n    AnimationState = SELECTED\n      Animation = Attention\n        AnimationName = TEST.ATTENTION\n        AnimationBlendTime = 30\n        AnimationMustCompleteBlend = Yes\n      End\n    End\n    AnimationState = MOVING\n      Animation = Run\n        AnimationName = TEST.RUN\n        AnimationBlendTime = 15\n      End\n    End\n  End\nEnd\n";
    let (idle, selected, moving) = (0, 1, 2);
    let mut scene = scene_with_states(text);
    pose_at(&mut scene, Conditions::NONE, 1.0);
    pose_at(&mut scene, Conditions::SELECTED, 2.0);
    pose_at(&mut scene, Conditions::MOVING, 2.3);
    let drawable = &scene.drawables[0];
    assert_eq!(drawable.state(), Some(moving));
    let blend = drawable.blend.unwrap();
    assert_eq!(
        (
            blend.from.state,
            blend.into.map(|slot| slot.state),
            blend.started
        ),
        (idle, Some(selected), 2.0),
        "the attention clip goes on blending in"
    );
    pose_at(&mut scene, Conditions::MOVING, 3.05);
    let blend = scene.drawables[0].blend.unwrap();
    assert_eq!((blend.from.state, blend.into), (selected, None));
    assert!(
        (blend.duration - 0.5).abs() < 1e-6,
        "then the run blends in over its own 15 frames"
    );
}

#[test]
fn only_states_sharing_a_maintain_frame_group_carry_the_frame() {
    let carried = |selected_flags: &str| {
        let text = format!(
            "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation = Idle\n        AnimationName = TEST.IDLE\n      End\n    End\n    AnimationState = SELECTED\n      Animation = Attention\n        AnimationName = TEST.ATTENTION\n      End\n      Flags = {selected_flags}\n    End\n    AnimationState = MOVING\n      Animation = Run\n        AnimationName = TEST.RUN\n      End\n      Flags = MAINTAIN_FRAME_ACROSS_STATES2\n    End\n  End\nEnd\n"
        );
        let mut scene = scene_with_states(&text);
        pose_at(&mut scene, Conditions::MOVING, 1.0);
        pose_at(&mut scene, Conditions::MOVING, 1.4);
        let (_, _, bits) = scene.drawables[0].sampled.unwrap();
        let start = carried_start_frame(f32::from_bits(bits), 61, 61).unwrap();
        pose_at(&mut scene, Conditions::SELECTED, 1.4);
        let phase = scene.drawables[0].phase.unwrap();
        (phase - start / 30.0).abs() < 1e-5
    };
    assert!(carried(
        "MAINTAIN_FRAME_ACROSS_STATES2 MAINTAIN_FRAME_ACROSS_STATES3"
    ));
    assert!(!carried("MAINTAIN_FRAME_ACROSS_STATES3"));
}

#[test]
fn a_draw_module_naming_no_model_still_attaches_its_states_systems() {
    use mint_sage_ini::{BlockTable, Document, FxListIndex, ObjectIndex, ParticleIndex};
    let mut renderer = match Renderer::offscreen(4, 4) {
        Ok(renderer) => renderer,
        Err(mint_render::Error::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter");
            return;
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    };
    let (vfs, resolver) = (Vfs::new(), Resolver::new());
    let objects = Document::parse(
        "objects.ini",
        "Object GroundMarkTest\n\
         \x20 Draw = W3DScriptedModelDraw Tag_Draw\n\
         \x20   DefaultModelConditionState\n\
         \x20     Model = NONE\n\
         \x20     ParticleSysBone = None MarkTestGlow\n\
         \x20   End\n\
         \x20 End\n\
         End\n",
        &BlockTable::object_model_path(),
    )
    .unwrap();
    let mut index = ObjectIndex::new();
    index.insert_document(&objects);
    let systems = Document::parse(
        "particles.ini",
        "FXParticleSystem MarkTestGlow\n  System\n    Lifetime = 10 10\n    BurstCount = 1 1\n  End\nEnd\n",
        &BlockTable::particle_path(),
    )
    .unwrap();
    let mut particle_index = ParticleIndex::new();
    particle_index.insert_document(&systems);
    let mut particles = crate::ParticleScene::new(particle_index, FxListIndex::new());

    let mut units = UnitScene::new();
    let slot = units
        .template_for(&mut renderer, &vfs, &resolver, &index, "GroundMarkTest")
        .expect("a model-less module with an attachment is a template");
    units.sync(&[pose(7, slot)], 0.0);
    units.draw(&[pose(7, slot)], 0.0, &mut Vec::new(), &[]);
    let mut held = crate::ConditionParticles::new();
    held.sync(&mut renderer, &vfs, &resolver, &mut particles, &units);

    assert_eq!(held.attached(), 1, "the default state's system runs");
    assert_eq!(units.ids_drawing_nothing(), vec![7], "and nothing is drawn");
}

#[test]
fn only_a_marked_template_is_a_swirl_host() {
    let mut scene = scene_with_two_templates();
    scene.set_swirl_host(1, true);
    assert!(!scene.is_swirl_host(0));
    assert!(scene.is_swirl_host(1));
    assert!(!scene.is_swirl_host(7));
}

#[test]
fn a_body_takes_the_first_non_empty_prefix_of_the_states_wearing_its_model() {
    let mut scene = scene_with_one_template();
    let template = &mut scene.templates[0];
    template.bodies = vec!["m_a".to_owned(), "m_b".to_owned()];
    let with = |model: &str, prefix: Option<&str>| {
        let mut state = state_def(&[], Some(model));
        state.model_animation_prefix = prefix.map(str::to_owned);
        state
    };
    template.model_states = vec![
        with("M_A", None),
        with("M_A", Some("")),
        with("M_A", Some("PA")),
        with("M_B", Some("PB")),
    ];
    assert_eq!(template.animation_prefix(0), "PA");
    assert_eq!(template.animation_prefix(1), "PB");
    assert_eq!(template.animation_prefix(2), "");
}

#[test]
fn a_house_colour_list_is_one_colour_or_three_and_black_alone_is_nothing() {
    let black = [0.0, 0.0, 0.0, 1.0];
    let red = [1.0, 0.0, 0.0, 1.0];
    let green = [0.0, 1.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 1.0];
    assert_eq!(house_colour_layer(&[red]), Some((red, [black; 2])));
    assert_eq!(house_colour_layer(&[black]), None);
    assert_eq!(house_colour_layer(&[]), None);
    assert_eq!(
        house_colour_layer(&[red, green, blue]),
        Some((red, [green, blue]))
    );
    assert_eq!(
        house_colour_layer(&[black, black, black]),
        Some((black, [black; 2])),
        "a three-colour list is always drawn, even all black"
    );
}
