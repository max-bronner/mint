use super::*;
use mint_sage_ini::{draw_animation_states, AnimationMode, BlockTable, Document};

const ARCHER: &str = "\
Object A
  Draw = W3DScriptedModelDraw Tag
    IdleAnimationState
      Animation = IdleA
        AnimationName = GUArcher_IDLA
        AnimationSpeedFactorRange = 0.7 1.3
      End
      Animation = IdleH
        AnimationName = GUArcher_IDLH
      End
    End
    AnimationState = THROWN_PROJECTILE
      Animation
        AnimationName = GUArcher_FLYA
        AnimationMode = LOOP
      End
    End
    AnimationState = DYING SPLATTED
      Animation
        AnimationName = GUArcher_DIEB
        AnimationMode = ONCE
      End
    End
    AnimationState = DYING
      Animation
        AnimationName = GUArcher_DIEA
        AnimationMode = ONCE
      End
    End
    AnimationState = PREATTACK_A
      Animation
        AnimationName = GUArcher_ATKF1
        AnimationMode = ONCE
      End
    End
    AnimationState = FIRING_OR_RELOADING_A
      Animation
        AnimationName = GUArcher_ATKF2
        AnimationMode = ONCE
      End
    End
    AnimationState = ATTACKING MOVING
      Animation = Move2
        AnimationName = GUArcher_RUNB
        Distance = 28
      End
    End
    AnimationState = MOVING
      Animation = Move1
        AnimationName = GUArcher_RUNA
        Distance = 28
        AnimationMode = LOOP
      End
      Flags = RANDOMSTART
    End
  End
End
";

fn archer_states() -> Vec<AnimationStateDef> {
    let table = BlockTable::object_model_path();
    let document = Document::parse("t.ini", ARCHER, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap();
    draw_animation_states(draw)
}

#[test]
fn a_moving_unit_selects_the_walk_and_a_dying_one_the_death() {
    let states = archer_states();
    let machine = StateMachine::compile(&states);
    let pick = |live: Conditions| {
        let index = machine.select(live).unwrap();
        states[index].clips[0].name.clone()
    };
    assert_eq!(pick(Conditions::NONE), "GUArcher_IDLA");
    assert_eq!(pick(Conditions::MOVING), "GUArcher_RUNA");
    assert_eq!(pick(Conditions::DYING), "GUArcher_DIEA");
    assert_eq!(pick(Conditions::PREATTACK_A), "GUArcher_ATKF1");
    assert_eq!(pick(Conditions::FIRING_OR_RELOADING_A), "GUArcher_ATKF2");
}

#[test]
fn a_more_specific_state_declared_first_beats_the_fallback_below_it() {
    let states = archer_states();
    let machine = StateMachine::compile(&states);
    let both = Conditions::MOVING.union(Conditions::ATTACKING);
    let index = machine.select(both).unwrap();
    assert_eq!(states[index].clips[0].name, "GUArcher_RUNB");
    assert_eq!(states[index].conditions, ["ATTACKING", "MOVING"]);
}

#[test]
fn a_state_naming_an_unmodelled_token_is_unreachable_rather_than_universal() {
    let states = archer_states();
    let machine = StateMachine::compile(&states);
    assert_eq!(machine.report().unreachable, 2);
    assert_eq!(machine.report().unmodelled_tokens, 2);
    for live in reachable_condition_sets() {
        let index = machine.select(live).unwrap();
        let clip = states[index].clips[0].name.as_str();
        assert!(
            clip != "GUArcher_FLYA" && clip != "GUArcher_DIEB",
            "{} selected an unreachable state ({clip})",
            live.label()
        );
    }
}

#[test]
fn a_moving_accelerate_header_compiles_reachable() {
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    IdleAnimationState\n      Animation\n        AnimationName = idle\n      End\n    End\n    AnimationState = MOVING ACCELERATE\n      Animation\n        AnimationName = runup\n      End\n    End\n  End\nEnd\n";
    let table = BlockTable::object_model_path();
    let document = Document::parse("t.ini", text, &table).unwrap();
    let states = draw_animation_states(document.roots()[0].blocks().next().unwrap());
    let machine = StateMachine::compile(&states);
    assert_eq!(machine.report().unreachable, 0);
    let live = Conditions::MOVING.union(Conditions::ACCELERATE);
    let index = machine.select(live).unwrap();
    assert_eq!(states[index].clips[0].name, "runup");
}

#[test]
fn every_reachable_flag_set_selects_something() {
    let states = archer_states();
    let machine = StateMachine::compile(&states);
    let probe = reachable_condition_sets();
    assert!(probe.len() > 20, "{}", probe.len());
    for live in &probe {
        assert!(machine.select(*live).is_some(), "{}", live.label());
    }
    assert!(machine.report().has_fallback);
}

#[test]
fn a_module_with_no_fallback_selects_nothing_for_an_unmatched_set() {
    let table = BlockTable::object_model_path();
    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
                    Animation\n        AnimationName = X\n      End\n    End\n  End\nEnd\n";
    let document = Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap();
    let states = draw_animation_states(draw);
    let machine = StateMachine::compile(&states);
    assert!(!machine.report().has_fallback);
    assert_eq!(machine.select(Conditions::NONE), None);
    assert_eq!(machine.select(Conditions::MOVING), Some(0));
}

#[test]
fn death_clears_movement_and_every_weapon_flag_in_the_same_operation() {
    let busy = Conditions::MOVING
        .union(Conditions::ATTACKING)
        .union(Conditions::PREATTACK_A)
        .union(Conditions::FIRING_OR_PREATTACK_A)
        .union(Conditions::FIRING_OR_RELOADING_A)
        .union(Conditions::SELECTED);
    let dead = busy.on_death();
    assert!(dead.contains(Conditions::DYING));
    assert_eq!(
        dead,
        Conditions::DYING.union(Conditions::SELECTED),
        "{}",
        dead.label()
    );
}

#[test]
fn a_clip_pick_is_stable_for_one_drawable_and_spread_across_many() {
    for id in 0..64u32 {
        assert_eq!(
            pick_clip(&[None; 5], id, 3, 2, None),
            pick_clip(&[None; 5], id, 3, 2, None)
        );
    }
    let picks: Vec<usize> = (0..64u32)
        .filter_map(|id| pick_clip(&[None; 5], id, 3, 0, None))
        .collect();
    let mut distinct = picks.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 5, "{picks:?}");
    assert_eq!(pick_clip(&[], 1, 1, 0, None), None);
}

#[test]
fn a_state_re_entered_varies_its_clip_and_never_repeats_the_one_showing() {
    let entries: Vec<usize> = (0..32u32)
        .filter_map(|entry| pick_clip(&[None; 3], 7, 4, entry, None))
        .collect();
    let mut distinct = entries.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 3, "{entries:?}");
    for entry in 0..64u32 {
        for showing in 0..3usize {
            let next = pick_clip(&[None; 3], 7, 4, entry, Some(showing));
            assert_ne!(next, Some(showing));
            assert!(next.is_some_and(|clip| clip < 3), "{next:?}");
        }
    }
    assert_eq!(pick_clip(&[None], 7, 4, 0, Some(0)), Some(0));
}

#[test]
fn a_clip_comes_up_by_its_priority_and_the_one_just_played_loses_one_point() {
    let weights = [Some(30), Some(30), Some(5), Some(5)];
    let draws = 7_000u32;
    let mut fresh = [0u32; 4];
    for id in 0..draws {
        fresh[pick_clip(&weights, id, 2, 0, None).unwrap()] += 1;
    }
    let light = f64::from(fresh[2] + fresh[3]) / f64::from(draws);
    assert!((light - 10.0 / 70.0).abs() < 0.02, "{fresh:?}");
    let repeats = (0..draws)
        .filter(|id| pick_clip(&weights, *id, 2, 1, Some(0)) == Some(0))
        .count();
    let share = f64::from(u32::try_from(repeats).unwrap()) / f64::from(draws);
    assert!((share - 29.0 / 69.0).abs() < 0.02, "{share}");
    let clamped = (0..draws)
        .filter(|id| pick_clip(&[Some(500), Some(100)], *id, 2, 0, None) == Some(0))
        .count();
    let half = f64::from(u32::try_from(clamped).unwrap()) / f64::from(draws);
    assert!((half - 0.5).abs() < 0.03, "{half}");
    assert!((0..64u32).all(|id| pick_clip(&[Some(-4), Some(1)], id, 2, 0, None) == Some(1)));
    assert!((0..64u32).all(|id| pick_clip(&[Some(0), Some(0)], id, 2, 0, None).is_some()));
}

#[test]
fn a_hash_fraction_stays_inside_the_unit_interval() {
    for id in 0..512u32 {
        let f = variant_fraction(id, id % 7, 3);
        assert!((0.0..1.0).contains(&f), "{id} gave {f}");
    }
}

#[test]
fn distance_fits_the_cycle_to_the_speed_the_simulation_reports() {
    let clip = AnimationClip {
        label: None,
        name: "run".into(),
        mode: Some(AnimationMode::Loop),
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: Some(28.0),
        use_weapon_timing: false,
        priority: None,
    };
    let scale = rate_scale(&clip, 28.0, 1.0, 7, 0.0);
    assert!((scale - 1.0).abs() < 1e-4, "{scale}");
    let scale = rate_scale(&clip, 56.0, 1.0, 7, 0.0);
    assert!((scale - 2.0).abs() < 1e-4, "{scale}");
    let scale = rate_scale(&clip, 0.0, 1.0, 7, 0.0);
    assert!((scale - 1.0).abs() < 1e-4, "{scale}");
    let ranged = AnimationClip {
        speed_range: Some((0.4, 0.4)),
        ..clip.clone()
    };
    let scale = rate_scale(&ranged, 28.0, 1.0, 7, 0.0);
    assert!((scale - 1.0).abs() < 1e-4, "{scale}");
    let scale = rate_scale(&ranged, 0.0, 1.0, 7, 0.0);
    assert!((scale - 0.4).abs() < 1e-4, "{scale}");
}

#[test]
fn a_speed_factor_range_spreads_rates_across_drawables_and_stays_inside_it() {
    let clip = AnimationClip {
        label: None,
        name: "idle".into(),
        mode: Some(AnimationMode::Loop),
        blend_time: None,
        must_complete_blend: false,
        speed_range: Some((0.7, 1.3)),
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    let rates: Vec<f32> = (0..32u32)
        .map(|id| rate_scale(&clip, 0.0, 1.0, id, 0.0))
        .collect();
    for rate in &rates {
        assert!((0.7..=1.3).contains(rate), "{rate}");
    }
    let spread = rates.iter().copied().fold(f32::MIN, f32::max).max(0.0)
        - rates.iter().copied().fold(f32::MAX, f32::min);
    assert!(spread > 0.4, "every drawable got the same rate: {spread}");
}

#[test]
fn use_weapon_timing_fits_one_cycle_of_the_clip_into_the_weapons_own_cycle() {
    let mut clip = AnimationClip {
        label: None,
        name: "swing".into(),
        mode: Some(AnimationMode::Once),
        blend_time: None,
        must_complete_blend: false,
        speed_range: Some((0.7, 1.3)),
        distance: None,
        use_weapon_timing: true,
        priority: None,
    };
    let natural = 2.0;
    let cycle = 1.533;
    let played = natural / rate_scale(&clip, 0.0, natural, 7, cycle);
    assert!(
        (played - cycle).abs() < 1e-3,
        "the clip took {played} s, not the weapon's {cycle} s"
    );
    clip.use_weapon_timing = false;
    let unflagged = rate_scale(&clip, 0.0, natural, 7, cycle);
    clip.use_weapon_timing = true;
    let unarmed = rate_scale(&clip, 0.0, natural, 7, 0.0);
    assert!(
        (unflagged - unarmed).abs() < 1e-6,
        "{unflagged} against {unarmed}"
    );
    assert!(
        (natural / unarmed - cycle).abs() > 1e-2,
        "the control must not already play at the weapon's cycle"
    );
}

#[test]
fn a_rate_scale_is_bounded_whatever_the_data_says() {
    let clip = AnimationClip {
        label: None,
        name: "run".into(),
        mode: Some(AnimationMode::Loop),
        blend_time: None,
        must_complete_blend: false,
        speed_range: Some((f32::NAN, 4.0)),
        distance: Some(0.001),
        use_weapon_timing: false,
        priority: None,
    };
    let scale = rate_scale(&clip, 1.0e9, 1.0, 3, 0.0);
    let (low, high) = RATE_SCALE_LIMITS;
    assert!((low..=high).contains(&scale), "{scale}");
}

#[test]
fn a_looping_clip_is_offset_per_drawable_and_a_once_clip_never_is() {
    let offsets: Vec<f32> = (0..32u32)
        .map(|id| phase_offset(true, 2.0, id, 4))
        .collect();
    for offset in &offsets {
        assert!((0.0..2.0).contains(offset), "{offset}");
    }
    let mut distinct = offsets.clone();
    distinct.sort_by(f32::total_cmp);
    distinct.dedup();
    assert!(distinct.len() > 20, "{} distinct offsets", distinct.len());
    for id in 0..32u32 {
        assert_eq!(phase_offset(false, 2.0, id, 4), 0.0);
    }
}

fn model_state(conditions: &[&str], model: &str) -> mint_sage_ini::ModelStateDef {
    mint_sage_ini::ModelStateDef {
        conditions: conditions.iter().map(|c| (*c).to_owned()).collect(),
        model: Some(model.to_owned()),
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
fn the_model_matcher_is_ordered_and_with_the_first_default_as_fallback() {
    let states = [
        model_state(&[], "Base"),
        model_state(&["AWAITING_CONSTRUCTION", "SNOW"], "Plot_Snow"),
        model_state(&["AWAITING_CONSTRUCTION"], "Plot"),
        model_state(&["SNOW"], "Base_Snow"),
        model_state(&["UPGRADE_NUMENOR_STONEWORK", "SNOW"], "Stone_Snow"),
    ];
    let machine = ModelStateMachine::compile(&states);
    assert_eq!(machine.fallback(), Some(0));
    assert_eq!(machine.select(Conditions::NONE), Some(0));
    assert_eq!(machine.select(Conditions::SNOW), Some(3));
    assert_eq!(
        machine.select(Conditions::SNOW.union(Conditions::AWAITING_CONSTRUCTION)),
        Some(1)
    );
    assert_eq!(machine.select(Conditions::AWAITING_CONSTRUCTION), Some(2));
    assert_eq!(
        machine.select(Conditions::SNOW.union(Conditions::MOVING)),
        Some(3)
    );
    assert_eq!(machine.report().unreachable, 1);
    assert_eq!(machine.report().unmodelled_tokens, 1);
    assert!(machine.report().has_fallback);
}

#[test]
fn a_module_with_no_default_selects_nothing_for_an_unmatched_set() {
    let machine = ModelStateMachine::compile(&[model_state(&["SNOW"], "Snowy")]);
    assert_eq!(machine.select(Conditions::NONE), None);
    assert_eq!(machine.select(Conditions::SNOW), Some(0));
    assert!(!machine.report().has_fallback);
}

#[test]
fn a_template_own_token_selects_its_state_only_while_its_bit_is_live() {
    let states = [
        model_state(&[], "Base"),
        model_state(&["UPGRADE_NUMENOR_STONEWORK", "SNOW"], "Stone_Snow"),
        model_state(&["SNOW"], "Base_Snow"),
        model_state(&["UPGRADE_NUMENOR_STONEWORK"], "Stone"),
    ];
    let mut extras = ExtraTokens::default();
    let machine = ModelStateMachine::compile_with(&states, Some(&mut extras));
    assert_eq!(extras.len(), 1, "one token, interned once");
    let stone = extras.lookup("upgrade_numenor_stonework").unwrap();
    assert_eq!(
        stone.bits(),
        [1 << Conditions::NAMES.len(), 0],
        "the first bit past the modelled ones"
    );
    assert_eq!(machine.select(Conditions::NONE), Some(0));
    assert_eq!(machine.select(Conditions::SNOW), Some(2));
    assert_eq!(machine.select(stone), Some(3));
    assert_eq!(machine.select(stone.union(Conditions::SNOW)), Some(1));
    assert_eq!(machine.report().unreachable, 2);
    assert!(machine.states().iter().all(|state| state.selectable));

    let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = UPGRADE_NUMENOR_STONEWORK\n      Animation\n        AnimationName = up\n      End\n    End\n    AnimationState = WEAPONSET_PLAYER_UPGRADE\n      Animation\n        AnimationName = blade\n      End\n    End\n    IdleAnimationState\n      Animation\n        AnimationName = idle\n      End\n    End\n  End\nEnd\n";
    let table = BlockTable::object_model_path();
    let document = Document::parse("t.ini", text, &table).unwrap();
    let animations = draw_animation_states(document.roots()[0].blocks().next().unwrap());
    let animation = StateMachine::compile_with(&animations, Some(&mut extras));
    assert_eq!(extras.len(), 2);
    assert_eq!(animation.select(stone), Some(0));
    let blade = extras.bits_of(["WEAPONSET_PLAYER_UPGRADE", "NEVER_NAMED"]);
    assert_eq!(
        blade.bits(),
        [1 << (Conditions::NAMES.len() + 1), 0],
        "a name no state wants sets nothing"
    );
    assert_eq!(animation.select(blade), Some(1));
    assert_eq!(animation.select(Conditions::NONE), Some(2));
    let plain = StateMachine::compile(&animations);
    assert_eq!(plain.select(blade), Some(2));
}

#[test]
fn tokens_past_the_capacity_are_refused_and_their_states_stay_unselectable() {
    let names: Vec<String> = (0..=EXTRA_CAPACITY).map(|i| format!("USER_{i}")).collect();
    let states: Vec<_> = names
        .iter()
        .map(|name| model_state(&[name.as_str()], "M"))
        .collect();
    let mut extras = ExtraTokens::default();
    let machine = ModelStateMachine::compile_with(&states, Some(&mut extras));
    assert_eq!(extras.len(), EXTRA_CAPACITY);
    assert_eq!(extras.refused(), 1);
    let last = machine.states().last().unwrap();
    assert!(!last.selectable);
    let highest = extras.lookup(&names[EXTRA_CAPACITY - 1]).unwrap();
    assert_eq!(highest.bits(), [0, 1 << 127]);
    assert_eq!(machine.select(highest), Some(EXTRA_CAPACITY - 1));
}

#[test]
fn a_swing_is_in_its_follow_through_only_while_its_own_slot_fires_under_attack() {
    let showing = Conditions::FIRING_OR_PREATTACK_A;
    let walking_on = Conditions::MOVING
        .union(Conditions::ATTACKING)
        .union(Conditions::weapon_token(0, WeaponToken::Firing))
        .union(Conditions::FIRING_OR_PREATTACK_A);
    assert!(walking_on.in_follow_through(showing));
    assert!(
        !Conditions::MOVING.in_follow_through(showing),
        "a new order cuts it"
    );
    assert!(
        !Conditions::ATTACKING
            .union(Conditions::weapon_token(1, WeaponToken::Firing))
            .in_follow_through(showing),
        "another slot's shot is not this swing's follow-through"
    );
    assert!(
        !walking_on.in_follow_through(Conditions::MOVING),
        "a state naming no shot token carries nothing"
    );
}

#[test]
fn death_n_tokens_are_distinct_from_dying() {
    for name in ["DEATH_1", "DEATH_2", "DEATH_3", "DEATH_4", "DEATH_5"] {
        let bit = Conditions::from_token(name).unwrap();
        assert_ne!(bit, Conditions::DYING, "{name}");
        assert!(bit.contains(bit), "{name} is its own bit");
    }
    assert_ne!(
        Conditions::from_token("DEATH_2").unwrap(),
        Conditions::from_token("DEATH_3").unwrap()
    );
}

#[test]
fn a_played_clip_at_priority_zero_does_not_skew_the_others() {
    let weights = [Some(0), Some(1), Some(1)];
    let mut picked = [0_u32; 3];
    for id in 0..400 {
        picked[pick_clip(&weights, id, 2, 1, Some(0)).unwrap()] += 1;
    }
    assert_eq!(picked[0], 0, "{picked:?}");
    assert!(picked[1] > 100 && picked[2] > 100, "{picked:?}");
}
