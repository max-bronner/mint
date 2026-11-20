use super::*;

const DATA: &str = "\
Weapon TogglePrimary\n  AttackRange = 80\n  DelayBetweenShots = 3000\n  ShareTimers = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon ToggleSecondary\n  AttackRange = 80\n  DelayBetweenShots = 500\n  ShareTimers = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Toggler\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TogglePrimary\n    \
    Weapon = SECONDARY ToggleSecondary\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Toggler", "Dummy"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 100.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn("Toggler", 100.0, PLAYER),
        spawn("Dummy", 140.0, OTHER),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(7, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let find = |name: &str| {
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, kind)| kinds.get(**kind).is_some_and(|e| e.record.name == name))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let (toggler, dummy) = (find("Toggler"), find("Dummy"));
    (simulation, ids, toggler, dummy)
}

#[test]
fn a_weapon_toggle_locks_the_next_slot_permanently_and_shares_the_reload() {
    let (mut simulation, ids, toggler, dummy) = scene();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![toggler],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: dummy })
        .unwrap();
    let mut fired = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.health, dummy)
            .is_some_and(|health| health.0.current() < health.0.max())
        {
            fired = true;
            break;
        }
    }
    assert!(fired, "the fixture's premise: the primary fires first");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::ToggleWeapon {
                toggles: [0, 1, u8::MAX],
                lead: Some(toggler),
            },
        )
        .unwrap();
    for _ in 0..2 {
        simulation.tick().unwrap();
    }
    let wiring = simulation.rules().modules.as_ref().unwrap();
    let state = simulation.state().get(wiring.state, toggler).unwrap();
    assert_eq!(state.weapon_lock, Some(1));
    assert!(!state.weapon_lock_temporary);
    let armed = simulation.state().get(ids.armed, toggler).unwrap();
    let slots = simulation.state().get(ids.weapon_slots, toggler).unwrap();
    let other = slots.states.iter().flatten().next().unwrap();
    assert_eq!(
        armed.state.when_we_can_fire_again(),
        other.when_we_can_fire_again()
    );
}
