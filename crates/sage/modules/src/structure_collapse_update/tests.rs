use super::*;
use mint_sim::module::{DestroyRequests, Effects, ModuleColumn, ModuleRegistry};
use mint_sim::{Entity, World};

struct TestFields(Vec<(&'static str, &'static str)>);

impl ModuleFieldSource for TestFields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| *k == keyword).map(|(_, v)| *v)
    }

    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        self.0
            .iter()
            .filter(|(k, _)| *k == keyword)
            .map(|(_, v)| v.split_whitespace().collect())
            .collect()
    }
}

fn dying_structure(destroy: &'static str) -> (World, ModuleRegistry, Entity) {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<StructureCollapseUpdate>(&mut world, "StructureCollapseUpdate");
    let entity = world.spawn().unwrap();
    let fields = TestFields(vec![
        ("MinCollapseDelay", "333.4"),
        ("MaxCollapseDelay", "333.4"),
        ("CollapseDamping", "0.5"),
        ("CollapseHeight", "3"),
        ("DestroyObjectWhenDone", destroy),
        ("FXList", "INITIAL FX_Start"),
        ("FXList", "FINAL FX_End"),
    ]);
    let module = structure_collapse_update_from_fields(&fields, Frame::ZERO).unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, module)
        .unwrap();
    let id = world.register::<ModuleColumn<StructureCollapseUpdate>>();
    world.get_mut(id, entity).unwrap().0[0]
        .state
        .set_physics(-1.0, 1.0);
    (world, registry, entity)
}

fn state(world: &mut World, entity: Entity) -> CollapseState {
    let id = world.register::<ModuleColumn<StructureCollapseUpdate>>();
    world.get(id, entity).unwrap().0[0].state.state()
}

#[test]
fn it_waits_its_delay_then_sinks_its_depth_and_is_removed_only_when_the_data_says_so() {
    for (destroy, removed) in [("Yes", true), ("No", false)] {
        let (mut world, registry, entity) = dying_structure(destroy);
        let mut rng = Pcg32::new(1);
        let mut destroy_requests = DestroyRequests::new();
        registry.broadcast_die(
            &mut world,
            entity,
            Frame::new(5),
            &mut rng,
            &mut destroy_requests,
        );
        for frame in 5..18 {
            registry.update(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy_requests,
            );
            assert_ne!(
                state(&mut world, entity),
                CollapseState::Done,
                "frame {frame}"
            );
        }
        assert_eq!(state(&mut world, entity), CollapseState::Collapsing);
        registry.update(&mut world, Frame::new(18), &mut rng, &mut destroy_requests);
        assert_eq!(state(&mut world, entity), CollapseState::Done);
        let expected = if removed { vec![entity] } else { Vec::new() };
        assert_eq!(destroy_requests.drain_sorted(), expected, "{destroy}");
    }
}

#[test]
fn the_initial_list_plays_at_death_and_the_final_one_once_sunk() {
    let (mut world, registry, entity) = dying_structure("Yes");
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut log = EffectLog::new();
    registry.broadcast_die_into(
        &mut world,
        entity,
        Frame::new(5),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        mint_sim::module::Death::NONE,
    );
    let fx = |log: &mut EffectLog| -> Vec<String> {
        log.drain()
            .into_iter()
            .filter_map(|(_, effect)| match effect {
                Effect::PlayFx { list, .. } => Some(list.to_string()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(fx(&mut log), vec!["FX_Start"]);
    for frame in 5..=18 {
        registry.update_into(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
        );
    }
    assert_eq!(fx(&mut log), vec!["FX_End"]);
}

#[test]
fn every_phase_plays_at_the_structures_own_position() {
    let (mut world, registry, entity) = dying_structure("No");
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut log = EffectLog::new();
    let places = |log: &mut EffectLog| -> Vec<PowerPlace> {
        log.drain()
            .into_iter()
            .filter_map(|(_, effect)| match effect {
                Effect::PlayFx { at, .. } => Some(at),
                _ => None,
            })
            .collect()
    };
    registry.broadcast_die_into(
        &mut world,
        entity,
        Frame::new(5),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        mint_sim::module::Death::NONE,
    );
    assert_eq!(places(&mut log), vec![PowerPlace::Source], "INITIAL");
    for frame in 5..=18 {
        registry.update_into(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
        );
    }
    assert_eq!(
        places(&mut log),
        vec![PowerPlace::Source],
        "FINAL, still at the object's own place"
    );
}

#[test]
fn a_phase_the_engine_does_not_know_is_refused() {
    let fields = TestFields(vec![("FXList", "HALFWAY FX_Start")]);
    assert!(structure_collapse_update_from_fields(&fields, Frame::ZERO).is_err());
}

#[test]
fn update_does_nothing_before_death_is_broadcast() {
    let (mut world, registry, entity) = dying_structure("Yes");
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    for frame in 0..50 {
        registry.update(&mut world, Frame::new(frame), &mut rng, &mut destroy);
    }
    assert!(destroy.drain_sorted().is_empty());
    assert_eq!(state(&mut world, entity), CollapseState::Standing);
}

#[test]
fn a_second_death_broadcast_does_not_restart_the_collapse() {
    let (mut world, registry, entity) = dying_structure("Yes");
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.broadcast_die(&mut world, entity, Frame::new(5), &mut rng, &mut destroy);
    registry.broadcast_die(&mut world, entity, Frame::new(14), &mut rng, &mut destroy);
    for frame in 5..=18 {
        registry.update(&mut world, Frame::new(frame), &mut rng, &mut destroy);
    }
    assert_eq!(state(&mut world, entity), CollapseState::Done);
}

#[test]
fn a_sunk_structure_stays_at_its_depth_until_it_is_removed() {
    let (mut world, registry, entity) = dying_structure("No");
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.broadcast_die(&mut world, entity, Frame::new(5), &mut rng, &mut destroy);
    let id = world.register::<ModuleColumn<StructureCollapseUpdate>>();
    let sink = |world: &World| world.get(id, entity).unwrap().0[0].state.sink();
    for frame in 5..=18 {
        registry.update(&mut world, Frame::new(frame), &mut rng, &mut destroy);
    }
    assert_eq!(state(&mut world, entity), CollapseState::Done);
    let sunk = sink(&world);
    assert!(sunk >= 3.0, "sunk its depth: {sunk}");
    for frame in 19..60 {
        registry.update(&mut world, Frame::new(frame), &mut rng, &mut destroy);
        assert_eq!(sink(&world).to_bits(), sunk.to_bits(), "frame {frame}");
    }
}

#[test]
fn almost_final_plays_every_frame_before_final_not_with_it() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<StructureCollapseUpdate>(&mut world, "StructureCollapseUpdate");
    let entity = world.spawn().unwrap();
    let fields = TestFields(vec![
        ("MinCollapseDelay", "333.4"),
        ("MaxCollapseDelay", "333.4"),
        ("CollapseDamping", "0.9"),
        ("CollapseHeight", "10"),
        ("DestroyObjectWhenDone", "Yes"),
        ("FXList", "INITIAL FX_Start"),
        ("FXList", "ALMOST_FINAL FX_Almost"),
        ("FXList", "FINAL FX_End"),
    ]);
    let module = structure_collapse_update_from_fields(&fields, Frame::ZERO).unwrap();
    registry
        .attach(&mut world, entity, Frame::ZERO, module)
        .unwrap();
    let id = world.register::<ModuleColumn<StructureCollapseUpdate>>();
    world.get_mut(id, entity).unwrap().0[0]
        .state
        .set_physics(-1.0, 1.0);

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut log = EffectLog::new();
    registry.broadcast_die_into(
        &mut world,
        entity,
        Frame::new(5),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        mint_sim::module::Death::NONE,
    );
    log.drain();

    let mut almost_frame = None;
    let mut final_frame = None;
    let mut almost_count = 0;
    for frame in 5..=40 {
        registry.update_into(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
        );
        let fired: Vec<String> = log
            .drain()
            .into_iter()
            .filter_map(|(_, effect)| match effect {
                Effect::PlayFx { list, .. } => Some(list.to_string()),
                _ => None,
            })
            .collect();
        if fired.contains(&"FX_Almost".to_string()) {
            almost_count += 1;
            almost_frame = Some(frame);
        }
        if fired.contains(&"FX_End".to_string()) {
            final_frame = Some(frame);
            assert!(
                !fired.contains(&"FX_Almost".to_string()),
                "ALMOST_FINAL and FINAL fired on the same frame {frame}"
            );
        }
    }
    assert!(
        almost_count > 1,
        "ALMOST_FINAL fired only once: {almost_count}"
    );
    let (almost_frame, final_frame) = (
        almost_frame.expect("ALMOST_FINAL never fired"),
        final_frame.expect("FINAL never fired"),
    );
    assert!(
        almost_frame < final_frame,
        "ALMOST_FINAL ({almost_frame}) did not fire before FINAL ({final_frame})"
    );
}

#[test]
fn draw_delay_stays_within_the_inclusive_bounds_over_many_rolls() {
    let mut rng = Pcg32::new(7);
    for _ in 0..500 {
        let d = draw_delay(
            &mut rng,
            LogicFrames::from_frames(3),
            LogicFrames::from_frames(9),
        );
        assert!((3..=9).contains(&d.get()), "{d:?} out of range");
    }
}
