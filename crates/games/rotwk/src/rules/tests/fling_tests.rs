use super::*;

const DATA: &str = "\
SpecialPower ZDrop\n  Enum = SPECIAL_GENERAL_TARGETLESS\n  ReloadTime = 0\nEnd\n\
Weapon ZDropHead\n  DamageNugget\n    Damage = 25\n    DamageType = MAGIC\n  End\nEnd\n\
Object ZHolder\n  Side = Wild\n  KindOf = SELECTABLE\n  CommandSet = ZHolderSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialPowerModule Tag_DropStart\n    SpecialPowerTemplate = ZDrop\n    \
    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = FlingPassengerSpecialAbilityUpdate Tag_Drop\n    SpecialPowerTemplate = ZDrop\n    \
    UnpackTime = 300\n    FlingPassengerVelocity = X:0 Y:0 Z:0\n    \
    FlingPassengerLandingWarhead = ZDropHead\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +INFANTRY\n    Slots = 1\n    InitialPayload = ZVictim 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object ZVictim\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ZHolderSet\n  1 = Command_ZDrop\nEnd\n\
CommandButton Command_ZDrop\n  Command = SPECIAL_POWER\n  SpecialPower = ZDrop\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["ZHolder".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let holder = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZHolder").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![holder])
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let holder = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation.tick().unwrap();
    let passenger = simulation
        .state()
        .get(ids.passengers, holder)
        .and_then(|list| list.0.first().copied())
        .unwrap();
    (simulation, ids, holder, passenger)
}

#[test]
fn a_fling_drops_its_passenger_where_it_is_held_and_fires_the_landing_warhead() {
    let (mut simulation, ids, holder, passenger) = scene();
    simulation.tick().unwrap();
    let held_at = simulation
        .state()
        .get(ids.body, passenger)
        .unwrap()
        .0
        .position;
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![holder],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: None,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().contains(ids.contained, passenger));
    let health = simulation.state().get(ids.health, passenger).unwrap().0;
    assert_eq!(health.current(), 475.0);
    let landed_at = simulation
        .state()
        .get(ids.body, passenger)
        .unwrap()
        .0
        .position;
    assert!(
        landed_at.minus(held_at).length() < 0.01,
        "{held_at:?} -> {landed_at:?}"
    );
}
