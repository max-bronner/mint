use super::*;
use mint_sage_combat::WeaponStatus;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Rock\n  AttackRange = 300\n  DelayBetweenShots = 6000\n  FiringDuration = 1000\n  \
  AutoReloadWhenIdle = 2000\n  ClipReloadTime = 3000\n  HoldDuringReload = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Thrower\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Rock\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Weapon Fist\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Toggler\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  CommandSet = TogglerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  \
  WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Rock\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet TogglerSet\n  1 = Command_ToggleRocks\nEnd\n\
CommandButton Command_ToggleRocks\n  Command = TOGGLE_WEAPONSET\n  \
  FlagsUsedForToggle = WEAPONSET_TOGGLE_1\nEnd\n\
Object Target\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let (simulation, ids, units) = scene_with_toggler();
    (simulation, ids, units[0], units[1])
}

fn scene_with_toggler() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Thrower".to_owned(),
        "Target".to_owned(),
        "Toggler".to_owned(),
    ];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("Thrower", Vec2::new(300.0, 480.0), PLAYER),
        ("Target", Vec2::new(450.0, 480.0), OTHER),
        ("Toggler", Vec2::new(300.0, 300.0), PLAYER),
    ]
    .iter()
    .map(|(name, at, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                *at,
                Facing::EAST,
                *owner,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn order(simulation: &mut Simulation<RotwkRules>, unit: Entity, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
}

fn weapon(simulation: &Simulation<RotwkRules>, ids: Ids, unit: Entity) -> (WeaponStatus, u64) {
    let mut state = simulation.state().get(ids.armed, unit).unwrap().state;
    let now = Frame::new(simulation.frame().get().saturating_sub(1));
    (
        state.status(now),
        state.last_shot().map_or(u64::MAX, Frame::get),
    )
}

fn first_throw(simulation: &mut Simulation<RotwkRules>, ids: Ids, thrower: Entity) -> u64 {
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let (_, shot) = weapon(simulation, ids, thrower);
        if shot != u64::MAX {
            return shot;
        }
    }
    panic!("never threw");
}

#[test]
fn a_clipless_thrower_reloads_when_idle_for_its_clip_reload_time() {
    let (mut simulation, ids, thrower, target) = scene();
    simulation.tick().unwrap();
    order(
        &mut simulation,
        thrower,
        RotwkCommand::AttackObject { target },
    );
    let shot = first_throw(&mut simulation, ids, thrower);
    let mut reload: Option<(u64, u64)> = None;
    for _ in 0..(6 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let now = simulation.frame().get().saturating_sub(1);
        let (status, _) = weapon(&simulation, ids, thrower);
        if status == WeaponStatus::ReloadingClip {
            reload = Some(reload.map_or((now, now), |(start, _)| (start, now)));
        }
    }
    let (start, end) = reload.expect("never reloaded");
    let hz = u64::from(SIMULATION_HZ);
    assert_eq!(
        start - shot,
        2 * hz,
        "reload began {} after the throw",
        start - shot
    );
    assert_eq!(end + 1 - start, 3 * hz, "reload lasted {}", end + 1 - start);
}

#[test]
fn a_move_order_waits_for_a_hold_during_reload() {
    let (mut simulation, ids, thrower, target) = scene();
    simulation.tick().unwrap();
    order(
        &mut simulation,
        thrower,
        RotwkCommand::AttackObject { target },
    );
    first_throw(&mut simulation, ids, thrower);
    let mut waited = 0;
    while weapon(&simulation, ids, thrower).0 != WeaponStatus::ReloadingClip {
        simulation.tick().unwrap();
        waited += 1;
        assert!(waited < 10 * SIMULATION_HZ, "never reloaded");
    }
    let held_at = position(&simulation, ids, thrower);
    order(
        &mut simulation,
        thrower,
        RotwkCommand::MoveTo {
            x: 300.0,
            y: 300.0,
            queue: false,
        },
    );
    let mut reloading_ticks = 0;
    while weapon(&simulation, ids, thrower).0 == WeaponStatus::ReloadingClip {
        simulation.tick().unwrap();
        reloading_ticks += 1;
        assert!(reloading_ticks < 200, "the reload never ended");
        if weapon(&simulation, ids, thrower).0 == WeaponStatus::ReloadingClip {
            let moved = position(&simulation, ids, thrower).minus(held_at).length();
            assert!(moved < 0.01, "walked {moved} during the reload");
        }
    }
    for _ in 0..SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let moved = position(&simulation, ids, thrower).minus(held_at).length();
    assert!(moved > 5.0, "still standing {moved} after the reload");
}

#[test]
fn a_weapon_set_toggle_switches_the_unit_between_its_sets() {
    let (mut simulation, ids, units) = scene_with_toggler();
    let toggler = units[2];
    simulation.tick().unwrap();
    let weapons = &simulation.rules().combat.weapons;
    let (fist, rock) = (weapons.id("Fist").unwrap(), weapons.id("Rock").unwrap());
    let wielded = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.armed, toggler).unwrap().weapon
    };
    assert_eq!(wielded(&simulation), fist);
    let press = RotwkCommand::ToggleWeaponSet {
        button: "Command_ToggleRocks".to_owned(),
        lead: None,
    };
    order(&mut simulation, toggler, press.clone());
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert_eq!(wielded(&simulation), rock);
    order(&mut simulation, toggler, press);
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert_eq!(wielded(&simulation), fist);
}

#[test]
fn a_weapon_set_toggle_raises_its_swap_condition_for_one_second_on_set_and_on_clear() {
    let (mut simulation, _ids, units) = scene_with_toggler();
    let toggler = units[2];
    simulation.tick().unwrap();
    let has_swap = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .upgrade_state_id()
            .and_then(|id| simulation.state().get(id, toggler))
            .is_some_and(|state| state.has_model_condition("SWAPPING_TO_WEAPONSET_1"))
    };
    let press = RotwkCommand::ToggleWeaponSet {
        button: "Command_ToggleRocks".to_owned(),
        lead: None,
    };
    order(&mut simulation, toggler, press.clone());
    simulation.tick().unwrap();
    assert!(
        has_swap(&simulation),
        "SWAPPING_TO_WEAPONSET_1 must rise with the raising press"
    );
    for _ in 0..29 {
        simulation.tick().unwrap();
        assert!(has_swap(&simulation), "must still be held inside 1 s");
    }
    simulation.tick().unwrap();
    assert!(
        !has_swap(&simulation),
        "must clear once the 1 s window elapses"
    );
    order(&mut simulation, toggler, press);
    simulation.tick().unwrap();
    assert!(
        has_swap(&simulation),
        "SWAPPING_TO_WEAPONSET_1 must rise again on the dropping press too"
    );
}
