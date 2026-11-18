use super::*;
use crate::rules::{RotwkCommand, RotwkRules};
use crate::unit::{Shape, Spawn};
use mint_movement::{Facing, MotionPhase};
use mint_sage_combat::WeaponState;
use mint_sage_map::HeightMapData;
use mint_sage_passability::Passability;
use mint_sage_scene::WeaponPhase;
use mint_sim::Simulation;

fn simulation() -> Simulation<RotwkRules> {
    let map = HeightMapData {
        width: 48,
        height: 48,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; 48 * 48],
    };
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        Spawn::inert(
            Vec2::new(55.0, 55.0),
            Facing::EAST,
            Shape {
                kind_of: KindOf::SELECTABLE,
                radius: 5.0,
                height: 18.0,
                geometry_is_small: false,
            },
            PlayerId::new(0),
        ),
        Spawn::inert(
            Vec2::new(85.0, 55.0),
            Facing::EAST,
            Shape {
                kind_of: KindOf::SHRUBBERY,
                radius: 6.0,
                height: 12.0,
                geometry_is_small: false,
            },
            PlayerId::new(0),
        ),
    ];
    Simulation::new(3, RotwkRules::new(passability, spawns)).unwrap()
}

#[test]
fn match_clock_reads_what_the_simulation_actually_holds() {
    let mut simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let state = RenderState::new(simulation.state(), 0.5);
    assert_eq!(state.match_clock(ids.clock), Some(2));
    assert_eq!(state.blend(), 0.5);
}

#[test]
fn a_fresh_simulations_clock_reads_zero_before_the_first_tick() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let state = RenderState::new(simulation.state(), 0.0);
    assert_eq!(state.match_clock(ids.clock), Some(0));
}

#[test]
fn the_unit_views_carry_the_simulations_own_positions_and_kinds() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let state = RenderState::new(simulation.state(), 0.0);
    let views = state.units(
        ids,
        PlayerId::new(0),
        PlayerId::new(0),
        |_, _, _, _, _| 0.0,
        |_| LogicFrames::ZERO,
    );
    assert_eq!(views.len(), 2);
    let first = views.first().unwrap();
    assert_eq!(first.position, Vec2::new(55.0, 55.0));
    assert_eq!(first.pick_type, PickType::Selectable);
    assert!(!first.selected);
    assert!(!first.moving);
    assert_eq!(views.get(1).unwrap().pick_type, PickType::Shrubbery);
    assert!(first.entity.index() < views.get(1).unwrap().entity.index());
}

#[test]
fn a_selection_command_shows_up_in_the_views_on_the_next_tick() {
    let mut simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation
        .submit(
            PlayerId::new(0),
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    let before = RenderState::new(simulation.state(), 0.0);
    assert_eq!(before.selected_count(ids, PlayerId::new(0)), 0);
    simulation.tick().unwrap();
    let after = RenderState::new(simulation.state(), 0.0);
    assert_eq!(after.selected_count(ids, PlayerId::new(0)), 1);
    assert!(
        after
            .units(
                ids,
                PlayerId::new(0),
                PlayerId::new(0),
                |_, _, _, _, _| 0.0,
                |_| LogicFrames::ZERO
            )
            .first()
            .unwrap()
            .selected
    );
}

fn walking() -> (Simulation<RotwkRules>, Ids, mint_sim::Entity) {
    let mut simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation
        .submit(
            PlayerId::new(0),
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PlayerId::new(0),
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids, unit)
}

fn drawn(state: &RenderState<'_>, ids: Ids, unit: mint_sim::Entity) -> UnitView {
    state
        .units(
            ids,
            PlayerId::new(0),
            PlayerId::new(0),
            |_, _, _, _, _| 0.0,
            |_| LogicFrames::ZERO,
        )
        .into_iter()
        .find(|view| view.entity == unit)
        .unwrap()
}

#[test]
fn interpolating_every_frame_does_not_move_the_simulation_or_its_hash() {
    let (mut plain, _, _) = walking();
    let mut plain_hashes = Vec::new();
    for _ in 0..60 {
        plain.tick().unwrap();
        plain_hashes.push(plain.state_hash());
    }

    let (mut drawn_run, ids, unit) = walking();
    let mut track = PoseTrack::new();
    let mut drawn_hashes = Vec::new();
    let mut seen_positions = Vec::new();
    for _ in 0..60 {
        track.capture(drawn_run.state(), ids);
        drawn_run.tick().unwrap();
        for blend in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let state = RenderState::interpolated(drawn_run.state(), &track, blend);
            seen_positions.push(drawn(&state, ids, unit).position);
        }
        drawn_hashes.push(drawn_run.state_hash());
    }

    assert_eq!(
        plain_hashes, drawn_hashes,
        "drawing changed the simulation - the seam ADR 0005 section 1 fixes has leaked"
    );
    let distinct = {
        let mut all = plain_hashes.clone();
        all.sort_unstable();
        all.dedup();
        all.len()
    };
    assert!(
        distinct > 40,
        "the world barely changed over 60 ticks ({distinct} distinct hashes); \
             this test would pass on a frozen simulation"
    );
    let first = *seen_positions.first().unwrap();
    let last = *seen_positions.last().unwrap();
    assert!(
        last.minus(first).length() > 50.0,
        "the unit did not travel, so nothing was interpolated"
    );
}

#[test]
fn animating_every_frame_does_not_move_the_simulation_or_its_hash() {
    let (mut plain, _, _) = walking();
    let mut plain_hashes = Vec::new();
    for _ in 0..60 {
        plain.tick().unwrap();
        plain_hashes.push(plain.state_hash());
    }

    let states = test_states();
    let machine = mint_sage_scene::StateMachine::compile(&states);
    let (mut animated, ids, _) = walking();
    let mut animated_hashes = Vec::new();
    let mut selected = Vec::new();
    let mut frames = Vec::new();
    let mut track = PoseTrack::new();
    for tick in 0..60u32 {
        track.capture(animated.state(), ids);
        animated.tick().unwrap();
        for step in 0..4u32 {
            let blend = f32::from(u16::try_from(step).unwrap_or(0)) / 4.0;
            let view = RenderState::interpolated(animated.state(), &track, blend);
            let seconds = (f32::from(u16::try_from(tick).unwrap_or(0)) + blend) / 30.0;
            for unit in view.units(
                ids,
                PlayerId::new(0),
                PlayerId::new(0),
                |_, _, _, _, _| 0.0,
                |_| LogicFrames::ZERO,
            ) {
                let state = machine.select(unit.conditions);
                selected.push(state);
                let clip = state.and_then(|state| {
                    let priorities: Vec<Option<i32>> = states
                        .get(state)
                        .map(|state| state.clips.iter().map(|clip| clip.priority).collect())
                        .unwrap_or_default();
                    mint_sage_scene::pick_clip(&priorities, unit.entity.index(), state, 0, None)
                });
                let scale = state
                    .and_then(|state| states.get(state))
                    .and_then(|state| state.clips.get(clip.unwrap_or(0)))
                    .map_or(1.0, |clip| {
                        mint_sage_scene::rate_scale(
                            clip,
                            unit.speed,
                            1.0,
                            unit.entity.index(),
                            unit.weapon_cycle,
                        )
                    });
                frames.push(seconds * scale);
            }
        }
        animated_hashes.push(animated.state_hash());
    }

    assert_eq!(
        plain_hashes, animated_hashes,
        "advancing the animation changed the simulation - ADR 0006's boundary has leaked"
    );
    let distinct_hashes = {
        let mut all = plain_hashes.clone();
        all.sort_unstable();
        all.dedup();
        all.len()
    };
    assert!(
        distinct_hashes > 40,
        "the world barely changed over 60 ticks ({distinct_hashes} distinct hashes)"
    );
    let distinct_states = {
        let mut all = selected.clone();
        all.sort_unstable();
        all.dedup();
        all.len()
    };
    assert!(
        distinct_states >= 2,
        "the matcher answered the same state all run ({distinct_states} distinct); \
             this test would pass on a matcher that ignored its input"
    );
    let last = *frames.last().unwrap();
    assert!(last > 1.0, "the playback clock never advanced ({last})");
}

fn test_states() -> Vec<mint_sage_ini::AnimationStateDef> {
    let table = mint_sage_ini::BlockTable::object_model_path();
    let text = "\
Object A
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
      Animation = Idle1
        AnimationName = stand
      End
      Animation = Idle2
        AnimationName = shuffle
      End
    End
    AnimationState = DYING
      Animation
        AnimationName = die
        AnimationMode = ONCE
      End
    End
    AnimationState = MOVING
      Animation = Move1
        AnimationName = walk
        Distance = 28
        AnimationMode = LOOP
      End
    End
  End
End
";
    let document = mint_sage_ini::Document::parse("t.ini", text, &table).unwrap();
    let draw = document
        .roots()
        .first()
        .and_then(|object| object.blocks().next())
        .unwrap();
    mint_sage_ini::draw_animation_states(draw)
}

#[test]
fn a_shot_cycle_walks_the_slots_tokens_in_eas_order() {
    let template = mint_sage_combat::WeaponTemplate {
        pre_attack_delay: LogicFrames::from_frames(19),
        firing_duration: LogicFrames::from_frames(5),
        delay_between_shots: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(26)),
        ..mint_sage_combat::WeaponTemplate::default()
    };
    let mut state = WeaponState::new(&template);
    state.begin_attack(
        &template,
        Frame::new(100),
        true,
        true,
        1.0,
        &mut mint_sim::Pcg32::new(1),
    );
    let winding = state;
    state.fire(&template, Frame::new(119), &mut mint_sim::Pcg32::new(1));
    let fired = state;
    let armed = |state: WeaponState| ArmedView {
        slot: 0,
        state,
        firing: template.firing_duration,
        attack_cycle: state.attack_cycle(),
    };
    let at = |weapon, tick: u64, attack| {
        unit_conditions(
            false,
            false,
            false,
            false,
            false,
            attack,
            Some(armed(weapon)),
            Frame::new(tick + 1),
            MotionPhase::NONE,
            Conditions::NONE,
        )
        .without(Conditions::ATTACKING)
    };
    let tokens = |names: &[&str]| {
        names
            .iter()
            .map(|name| Conditions::from_token(name).unwrap())
            .fold(Conditions::NONE, Conditions::union)
    };
    let engaged = Attack::Engaged;
    assert_eq!(
        at(winding, 110, engaged),
        tokens(&["PREATTACK_A", "USING_WEAPON_A", "FIRING_OR_PREATTACK_A"])
    );
    assert_eq!(
        at(fired, 123, engaged),
        tokens(&[
            "FIRING_A",
            "USING_WEAPON_A",
            "FIRING_OR_PREATTACK_A",
            "FIRING_OR_RELOADING_A"
        ])
    );
    assert_eq!(
        at(fired, 124, engaged),
        tokens(&["BETWEEN_FIRING_SHOTS_A", "USING_WEAPON_A"]),
        "past FiringDuration the merged tokens drop"
    );
    assert_eq!(
        at(fired, 145, engaged),
        tokens(&["BETWEEN_FIRING_SHOTS_A", "USING_WEAPON_A"]),
        "ready and still engaged holds between shots rather than popping to idle"
    );
    assert_eq!(at(fired, 145, Attack::Pursuing), Conditions::NONE);
}

#[test]
fn unit_conditions_does_not_vary_with_the_weapons_attack_cycle() {
    let template = mint_sage_combat::WeaponTemplate::default();
    let state = WeaponState::new(&template);
    let at = |attack_cycle: u32| {
        unit_conditions(
            false,
            false,
            false,
            false,
            false,
            Attack::Engaged,
            Some(ArmedView {
                slot: 0,
                state,
                firing: LogicFrames::from_frames(5),
                attack_cycle,
            }),
            Frame::new(1),
            MotionPhase::NONE,
            Conditions::NONE,
        )
    };
    let first = at(0);
    for cycle in 1..5 {
        assert_eq!(
            at(cycle),
            first,
            "cycle {cycle} must not change the conditions bits"
        );
    }
}

#[test]
fn a_clip_reload_shows_from_the_frame_after_the_shot() {
    let template = mint_sage_combat::WeaponTemplate {
        firing_duration: LogicFrames::from_frames(60),
        clip_size: 1,
        clip_reload_time: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(63)),
        ..mint_sage_combat::WeaponTemplate::default()
    };
    let mut state = WeaponState::new(&template);
    state.fire(&template, Frame::new(100), &mut mint_sim::Pcg32::new(1));
    let view = ArmedView {
        slot: 0,
        state,
        firing: template.firing_duration,
        attack_cycle: state.attack_cycle(),
    };
    let phase = |tick: u64| weapon_phase(view, Attack::Engaged, Frame::new(tick));
    assert_eq!(phase(100), Some(WeaponPhase::Firing));
    for tick in [101, 130, 159, 162] {
        assert_eq!(phase(tick), Some(WeaponPhase::Reloading), "frame {tick}");
    }
}

#[test]
fn a_dying_unit_carries_no_movement_or_weapon_flag_whatever_it_was_doing() {
    let template = mint_sage_combat::WeaponTemplate {
        pre_attack_delay: LogicFrames::from_frames(200),
        ..mint_sage_combat::WeaponTemplate::default()
    };
    let mut winding = WeaponState::new(&template);
    winding.begin_attack(
        &template,
        Frame::new(0),
        true,
        true,
        1.0,
        &mut mint_sim::Pcg32::new(1),
    );
    let dying = unit_conditions(
        true,
        true,
        true,
        false,
        false,
        Attack::Pursuing,
        Some(ArmedView {
            slot: 0,
            state: winding,
            firing: LogicFrames::ZERO,
            attack_cycle: winding.attack_cycle(),
        }),
        Frame::new(100),
        MotionPhase::NONE,
        Conditions::NONE,
    );
    assert_eq!(dying, Conditions::DYING.union(Conditions::SELECTED));

    let states = test_states();
    let machine = mint_sage_scene::StateMachine::compile(&states);
    let chosen = machine.select(dying).unwrap();
    assert_eq!(
        states
            .get(chosen)
            .and_then(|state| state.clips.first())
            .map(|clip| clip.name.as_str()),
        Some("die")
    );
}

#[test]
fn a_dying_structure_gets_rubble_not_dying() {
    let rubble = unit_conditions(
        false,
        true,
        true,
        true,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::NONE,
        Conditions::NONE,
    );
    assert_eq!(rubble, Conditions::RUBBLE.union(Conditions::SELECTED));
    assert!(!rubble.contains(Conditions::DYING));

    assert!(!Conditions::REALLYDAMAGED
        .on_structure_death()
        .contains(Conditions::REALLYDAMAGED));
}

#[test]
fn a_finished_collapse_stays_down_and_keeps_rubble_without_a_post_collapse_state() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let state = RenderState::new(simulation.state(), 0.0);
    let template = state.units(
        ids,
        PlayerId::new(0),
        PlayerId::new(0),
        |_, _, _, _, _| 0.0,
        |_| LogicFrames::ZERO,
    )[0];
    let done = |declared: bool| {
        let mut units = vec![UnitView {
            dying: true,
            conditions: Conditions::RUBBLE,
            ..template
        }];
        apply_collapse(
            &mut units,
            mint_sim::Frame::ZERO,
            &|_| Some((mint_sage_modules::CollapseState::Done, 12.0, 0.0)),
            &|_, conditions| declared && conditions.contains(Conditions::POST_COLLAPSE),
        );
        units[0]
    };
    let kept = done(false);
    assert_eq!(kept.conditions, Conditions::RUBBLE);
    assert!((kept.ground - (template.ground - 12.0)).abs() < 1e-4);
    let post = done(true);
    assert_eq!(post.conditions, Conditions::POST_COLLAPSE);
    assert!((post.ground - (template.ground - 12.0)).abs() < 1e-4);
}

#[test]
fn blend_zero_draws_the_previous_step_and_blend_one_draws_the_current() {
    let (mut simulation, ids, unit) = walking();
    let mut track = PoseTrack::new();
    track.capture(simulation.state(), ids);
    let before = simulation.state().get(ids.body, unit).copied().unwrap();
    simulation.tick().unwrap();
    let after = simulation.state().get(ids.body, unit).copied().unwrap();
    assert_ne!(before.0.position, after.0.position, "the unit must move");

    let at_zero = RenderState::interpolated(simulation.state(), &track, 0.0);
    assert_eq!(drawn(&at_zero, ids, unit).position, before.0.position);
    let at_one = RenderState::interpolated(simulation.state(), &track, 1.0);
    assert_eq!(drawn(&at_one, ids, unit).position, after.0.position);
    let plain = RenderState::new(simulation.state(), 0.5);
    assert_eq!(drawn(&plain, ids, unit).position, after.0.position);
    assert!(!plain.interpolates());
    assert!(at_zero.interpolates());
}

#[test]
fn a_frame_between_two_ticks_is_drawn_between_the_two_positions() {
    let (mut simulation, ids, unit) = walking();
    let mut track = PoseTrack::new();
    track.capture(simulation.state(), ids);
    let before = simulation.state().get(ids.body, unit).copied().unwrap().0;
    simulation.tick().unwrap();
    let after = simulation.state().get(ids.body, unit).copied().unwrap().0;

    let mut previous = before.position;
    let mut frames = 0usize;
    for blend in [0.2f32, 0.4, 0.6, 0.8] {
        let view = drawn(
            &RenderState::interpolated(simulation.state(), &track, blend),
            ids,
            unit,
        );
        assert!(view.position.minus(before.position).length() > 0.0);
        assert!(view.position.minus(after.position).length() > 0.0);
        assert!(
            view.position.minus(previous).length() > 0.0,
            "the frame at blend {blend} drew the same position as the one before it"
        );
        let expected = Vec2::new(
            lerp(before.position.x, after.position.x, blend),
            lerp(before.position.y, after.position.y, blend),
        );
        assert!(
            view.position.minus(expected).length() < 1e-4,
            "blend {blend}: drew {:?}, expected {expected:?}",
            view.position
        );
        previous = view.position;
        frames = frames.saturating_add(1);
    }
    assert_eq!(frames, 4);
}

#[test]
fn facing_interpolates_the_short_way_across_the_pi_seam() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    let mut track = PoseTrack::new();
    let just_under = core::f32::consts::PI - 0.05;
    let just_over = -core::f32::consts::PI + 0.05;
    track.units.push((unit, Vec2::ZERO, 0.0, just_under));

    let state = RenderState::interpolated(simulation.state(), &track, 0.5);
    let (_, _, facing) = state.blended_pose(unit, (Vec2::ZERO, 0.0, just_over));
    assert!(
        facing.abs() > core::f32::consts::PI - 1e-3,
        "halfway between two headings either side of the seam is the seam, got {facing}"
    );
}

#[test]
fn a_unit_with_no_previous_pose_is_drawn_where_it_stands() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let empty = PoseTrack::new();
    let state = RenderState::interpolated(simulation.state(), &empty, 0.5);
    let views = state.units(
        ids,
        PlayerId::new(0),
        PlayerId::new(0),
        |_, _, _, _, _| 0.0,
        |_| LogicFrames::ZERO,
    );
    assert_eq!(views.first().unwrap().position, Vec2::new(55.0, 55.0));
}

#[test]
fn a_hostile_blend_is_clamped_rather_than_drawn() {
    let (mut simulation, ids, unit) = walking();
    let mut track = PoseTrack::new();
    track.capture(simulation.state(), ids);
    let before = simulation.state().get(ids.body, unit).copied().unwrap().0;
    simulation.tick().unwrap();
    let after = simulation.state().get(ids.body, unit).copied().unwrap().0;

    for (blend, expected) in [
        (f32::NAN, before.position),
        (f32::INFINITY, before.position),
        (f32::NEG_INFINITY, before.position),
        (-1.0, before.position),
        (2.0, after.position),
    ] {
        let state = RenderState::interpolated(simulation.state(), &track, blend);
        let view = drawn(&state, ids, unit);
        assert!(view.position.x.is_finite() && view.position.y.is_finite());
        assert_eq!(view.position, expected, "blend {blend}");
        assert!((0.0..=1.0).contains(&state.blend()));
    }
}

#[test]
fn a_recycled_entity_slot_does_not_inherit_the_dead_objects_pose() {
    let mut world = World::new();
    let dead = world.spawn().unwrap();
    assert!(world.despawn(dead));
    let reborn = world.spawn().unwrap();
    assert_eq!(
        dead.index(),
        reborn.index(),
        "the allocator must reuse the slot, or this test proves nothing"
    );
    assert_ne!(dead, reborn, "and hand back a different generation");

    let mut track = PoseTrack::new();
    track.units.push((dead, Vec2::new(10.0, 20.0), 5.0, 0.0));
    track.projectiles.push((dead, 10.0, 20.0, 30.0));
    assert_eq!(track.unit(dead), Some((Vec2::new(10.0, 20.0), 5.0, 0.0)));
    assert_eq!(track.unit(reborn), None);
    assert_eq!(track.projectile(dead), Some((10.0, 20.0, 30.0)));
    assert_eq!(track.projectile(reborn), None);
}

#[test]
fn stepping_remembers_the_state_before_the_step_and_only_the_last_one() {
    let (mut simulation, ids, unit) = walking();
    let mut track = PoseTrack::new();
    let mut seen = Vec::new();
    for _ in 0..10 {
        step_remembering(&mut simulation, &mut track, ids).unwrap();
        let was = track.unit(unit).expect("the unit was captured").0;
        let now = simulation.state().get(ids.body, unit).unwrap().0.position;
        assert!(
            now.minus(was).length() > 0.0,
            "the remembered pose is the post-tick one, so nothing can be interpolated"
        );
        assert_eq!(track.units.len(), 2, "the capture accumulated");
        seen.push(was);
    }
    for pair in seen.windows(2) {
        assert!(pair[1].minus(pair[0]).length() > 0.0);
    }
}

#[test]
fn a_unit_with_no_route_but_a_speed_is_still_moving() {
    assert!(is_moving(false, 12.0), "a horde member walks with no route");
    assert!(is_moving(true, 0.0), "a unit that has just been ordered");
    assert!(!is_moving(false, 0.0), "standing still");
    assert!(
        !is_moving(false, STANDING_STILL),
        "the arrival residual is not motion"
    );
    assert!(is_moving(false, -12.0));
}

#[test]
fn damage_states_follow_the_bodys_own_thresholds() {
    let barracks = mint_sage_modules::ObjectRecord {
        max_health: Some(3000.0),
        max_health_damaged: Some(2000.0),
        max_health_really_damaged: Some(1000.0),
        ..mint_sage_modules::ObjectRecord::default()
    };
    let global = DamageThresholds {
        damaged: 0.65,
        really: 0.4,
    };
    let own = DamageThresholds::of(Some(&barracks), global);
    assert!((own.damaged - 2.0 / 3.0).abs() < 1e-6);
    assert!((own.really - 1.0 / 3.0).abs() < 1e-6);
    assert_eq!(DamageThresholds::of(None, global), global);
    let hostile = mint_sage_modules::ObjectRecord {
        max_health: Some(0.0),
        max_health_damaged: Some(f32::NAN),
        ..mint_sage_modules::ObjectRecord::default()
    };
    assert_eq!(DamageThresholds::of(Some(&hostile), global), global);

    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let state = RenderState::new(simulation.state(), 0.0);
    let template = state.units(
        ids,
        PlayerId::new(0),
        PlayerId::new(0),
        |_, _, _, _, _| 0.0,
        |_| LogicFrames::ZERO,
    )[0];
    let at = |health: f32, build: Option<u32>| {
        let mut units = vec![UnitView {
            health: Some(health),
            build_elapsed: build,
            build_total: None,
            under_repair: false,
            ..template
        }];
        apply_damage(&mut units, &|_| own);
        units[0].conditions
    };
    assert_eq!(at(0.9, None), template.conditions);
    assert!(at(0.6, None).contains(Conditions::DAMAGED));
    assert!(!at(0.6, None).contains(Conditions::REALLYDAMAGED));
    assert!(at(0.2, None).contains(Conditions::REALLYDAMAGED));
    assert!(!at(0.2, None).contains(Conditions::DAMAGED));
    assert_eq!(
        at(0.2, Some(10)),
        template.conditions,
        "a site is not damaged"
    );
}

#[test]
fn build_progress_is_elapsed_over_the_templates_build_frames() {
    let record = mint_sage_modules::ObjectRecord {
        build_cost: Some(400.0),
        build_time: Some(30.0),
        ..mint_sage_modules::ObjectRecord::default()
    };
    assert_eq!(build_progress(0, None, &record), Some(0.0));
    assert_eq!(build_progress(450, None, &record), Some(0.5));
    assert_eq!(build_progress(5000, None, &record), Some(1.0));
    let unpriced = mint_sage_modules::ObjectRecord::default();
    assert_eq!(build_progress(10, None, &unpriced), None);
}

#[test]
fn a_snowy_map_gives_every_object_snow_and_nothing_else() {
    let simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let state = RenderState::new(simulation.state(), 0.0);
    let plain = state.units(
        ids,
        PlayerId::new(0),
        PlayerId::new(0),
        |_, _, _, _, _| 0.0,
        |_| LogicFrames::ZERO,
    );
    let mut snowy = plain.clone();
    apply_weather(&mut snowy, true);
    for (before, after) in plain.iter().zip(&snowy) {
        assert!(after.conditions.contains(Conditions::SNOW));
        assert_eq!(
            after.conditions.without(Conditions::SNOW),
            before.conditions
        );
    }
    let mut clear = plain.clone();
    apply_weather(&mut clear, false);
    assert_eq!(clear, plain);
}

#[test]
fn a_dying_units_death_flag_joins_dying() {
    let plain = unit_conditions(
        false,
        false,
        true,
        false,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::NONE,
        Conditions::NONE,
    );
    assert_eq!(plain, Conditions::DYING);

    let with_flag = unit_conditions(
        false,
        false,
        true,
        false,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::NONE,
        Conditions::DEATH_2,
    );
    assert_eq!(with_flag, Conditions::DYING.union(Conditions::DEATH_2));
}

#[test]
fn walking_without_a_route_sets_the_moving_condition() {
    let live = unit_conditions(
        is_moving(false, 12.0),
        true,
        false,
        false,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::NONE,
        Conditions::NONE,
    );
    assert!(live.contains(Conditions::MOVING));
    assert!(live.contains(Conditions::SELECTED));
}

#[test]
fn an_accelerating_phase_sets_accelerate_and_keeps_moving() {
    let accelerating = unit_conditions(
        true,
        false,
        false,
        false,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::ACCELERATING,
        Conditions::NONE,
    );
    assert!(accelerating.contains(Conditions::MOVING));
    assert!(accelerating.contains(Conditions::ACCELERATE));
    assert!(!accelerating.contains(Conditions::DECELERATE));

    let turning = unit_conditions(
        true,
        false,
        false,
        false,
        false,
        Attack::None,
        None,
        Frame::new(0),
        MotionPhase::TURNING_LEFT,
        Conditions::NONE,
    );
    assert!(turning.contains(Conditions::TURN_LEFT));
    assert!(!turning.contains(Conditions::TURN_RIGHT));
}
