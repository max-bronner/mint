use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityDominate\n  ReloadTime = 0\nEnd\n\
Object PassRider\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = TemporarilyDefectUpdate Tag_Defect\n    DefectDuration = 30000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PassBoat\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Boat\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    Slots = 1\n    \
    PassengerFilter = NONE +PassRider\n    InitialPayload = PassRider 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 60\nEnd\n\
Object Dominator\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = DominatorSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 60\n    AttributeModifierAffects = ALL ENEMIES\n    PermanentlyConvert = Yes\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DominatorSet\n  1 = Cast_Dominate\nEnd\n\
CommandButton Cast_Dominate\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

fn scene(radius: f32) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Dominator", "PassBoat"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let dominator = kinds
        .spawn(
            &data.combat,
            kinds.kind("Dominator").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let boat = kinds
        .spawn(
            &data.combat,
            kinds.kind("PassBoat").unwrap(),
            Vec2::new(300.0 + radius - 1.0, 300.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![dominator, boat])
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(7, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let dominator_entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            kinds.get(**k).is_some_and(|e| e.record.name == "Dominator")
                && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let boat_entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            kinds.get(**k).is_some_and(|e| e.record.name == "PassBoat")
                && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let passenger = simulation
        .state()
        .get(ids.passengers, boat_entity)
        .and_then(|list| list.0.first().copied())
        .unwrap();
    (simulation, ids, dominator_entity, boat_entity, passenger)
}

fn take_boat(simulation: &mut Simulation<RotwkRules>, ids: Ids, dominator: Entity, boat: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![dominator],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(boat),
                at: None,
            },
        )
        .unwrap();
    let mut taken = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, boat).map(|o| o.0) == Some(PLAYER) {
            taken = true;
            break;
        }
    }
    assert!(
        taken,
        "the fixture's premise: the dominate must take the boat"
    );
}

#[test]
fn a_dominated_transports_passenger_always_defects_temporarily() {
    let (mut simulation, ids, dominator, boat, passenger) = scene(60.0);
    take_boat(&mut simulation, ids, dominator, boat);
    assert_eq!(
        simulation.state().get(ids.owner, boat).map(|o| o.0),
        Some(PLAYER),
        "PermanentlyConvert = Yes: the boat itself changes for good"
    );
    assert_eq!(
        simulation.state().get(ids.owner, passenger).map(|o| o.0),
        Some(PLAYER),
        "the passenger follows the container's new side"
    );
    let revert = simulation
        .state()
        .get(ids.defect_revert, passenger)
        .copied();
    assert_eq!(
        revert.map(|r| r.to),
        Some(OTHER),
        "the passenger's own defect is temporary and must queue a revert to its old side"
    );
}

#[test]
fn a_defected_objects_readout_drains_over_its_defect() {
    let (mut simulation, ids, dominator, boat, passenger) = scene(60.0);
    take_boat(&mut simulation, ids, dominator, boat);
    let revert = *simulation
        .state()
        .get(ids.defect_revert, passenger)
        .unwrap();
    assert_eq!(revert.until.get(), revert.since.get() + 900);
    while simulation.frame().get() < revert.since.get() + 225 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .rules()
            .rank_display(simulation.state(), &[passenger], simulation.frame()),
        Some(crate::experience::RankDisplay::TimeRemaining(0.75))
    );
}

const SLAVE_DATA: &str = "\
SpecialPower SpecialAbilityDominate\n  ReloadTime = 0\nEnd\n\
Object SlaveDen\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
    SpawnTemplateName = SlaveImp\n    SpawnReplaceDelay = 100000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object SlaveImp\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 60\n    GuardWanderRange = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dominator\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = DominatorSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 5\n    AttributeModifierAffects = ALL ENEMIES\n    PermanentlyConvert = Yes\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DominatorSet\n  1 = Cast_Dominate\nEnd\n\
CommandButton Cast_Dominate\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

#[test]
fn a_slave_follows_its_masters_dominated_side() {
    const WILDS: PlayerId = PlayerId::new(2);
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        SLAVE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(
        &data,
        &[String::from("SlaveDen"), String::from("Dominator")],
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let den = kinds
        .spawn(
            &data.combat,
            kinds.kind("SlaveDen").unwrap(),
            Vec2::new(400.0, 400.0),
            Facing::EAST,
            WILDS,
        )
        .unwrap();
    let dominator = kinds
        .spawn(
            &data.combat,
            kinds.kind("Dominator").unwrap(),
            Vec2::new(420.0, 400.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![den, dominator])
        .with_players(3)
        .with_teams(vec![None, None, None])
        .with_map_players(
            3,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let den_entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            kinds.get(**k).is_some_and(|e| e.record.name == "SlaveDen")
                && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let imp_entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            kinds.get(**k).is_some_and(|e| e.record.name == "SlaveImp")
                && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let dominator_entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            kinds.get(**k).is_some_and(|e| e.record.name == "Dominator")
                && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![dominator_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(den_entity),
                at: None,
            },
        )
        .unwrap();
    let mut den_taken = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, den_entity).map(|o| o.0) == Some(PLAYER) {
            den_taken = true;
            break;
        }
    }
    assert!(
        den_taken,
        "the fixture's premise: the dominate must take the den"
    );
    let mut imp_followed = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, imp_entity).map(|o| o.0) == Some(PLAYER) {
            imp_followed = true;
            break;
        }
    }
    assert!(
        imp_followed,
        "the slave must follow its master's dominated side"
    );
}
