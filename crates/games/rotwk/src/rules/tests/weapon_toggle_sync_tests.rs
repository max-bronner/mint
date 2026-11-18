use super::*;

const DATA: &str = "\
Weapon Fist\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Weapon Rock\n  AttackRange = 300\n  DelayBetweenShots = 6000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object TogglerA\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  CommandSet = TogglerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  \
  WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Rock\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object TogglerB\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  CommandSet = TogglerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  \
  WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Rock\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet TogglerSet\n  1 = Command_ToggleRocks\nEnd\n\
CommandButton Command_ToggleRocks\n  Command = TOGGLE_WEAPONSET\n  \
  FlagsUsedForToggle = WEAPONSET_TOGGLE_1\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["TogglerA".to_owned(), "TogglerB".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("TogglerA", Vec2::new(300.0, 300.0)),
        ("TogglerA", Vec2::new(350.0, 300.0)),
        ("TogglerB", Vec2::new(400.0, 300.0)),
    ]
    .iter()
    .map(|(name, at)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                *at,
                Facing::EAST,
                PLAYER,
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

fn has_flag(simulation: &Simulation<RotwkRules>, entity: Entity) -> bool {
    let state_id = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state_id, entity)
        .is_some_and(|state| state.has_weapon_set_flag("WEAPONSET_TOGGLE_1"))
}

fn select_and_press(simulation: &mut Simulation<RotwkRules>, units: &[Entity], lead: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: units.to_vec(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::ToggleWeaponSet {
                button: "Command_ToggleRocks".to_owned(),
                lead: Some(lead),
            },
        )
        .unwrap();
}

#[test]
fn a_player_press_syncs_every_equivalent_unit_to_the_opposite_of_the_lead() {
    let (mut simulation, _ids, units) = scene();
    let [a1, a2, b] = [units[0], units[1], units[2]];

    select_and_press(&mut simulation, &[a2], a2);
    simulation.tick().unwrap();
    assert!(has_flag(&simulation, a2), "the fixture's premise");
    assert!(!has_flag(&simulation, a1));

    select_and_press(&mut simulation, &[a1, a2, b], a1);
    simulation.tick().unwrap();

    assert!(has_flag(&simulation, a1), "the lead itself raises");
    assert!(
        has_flag(&simulation, a2),
        "already at the lead's target state - stays raised, not flipped off"
    );
    assert!(
        !has_flag(&simulation, b),
        "a different template is untouched"
    );
}
