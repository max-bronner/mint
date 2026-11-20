use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityDominate\n  ReloadTime = 0\nEnd\n\
Weapon Claw\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Raider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RaiderHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = EmptySet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Raider Position:X:20 Y:-20 Position:X:20 Y:20\n  \
  End\nEnd\n\
CommandSet EmptySet\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RaiderHorde\nEnd\n\
CommandButton Build_RaiderHorde\n  Command = UNIT_BUILD\n  Object = RaiderHorde\nEnd\n\
Object DominatorFiltered\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = FilteredSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 200\n    AttributeModifierAffects = NONE +INFANTRY ENEMIES\n    \
    PermanentlyConvert = Yes\n    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet FilteredSet\n  1 = Cast_Area\nEnd\n\
CommandButton Cast_Area\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object DominatorBroad\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = BroadSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 200\n    AttributeModifierAffects = ALL ENEMIES\n    \
    PermanentlyConvert = Yes\n    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet BroadSet\n  1 = Cast_Object\nEnd\n\
CommandButton Cast_Object\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
Object DominatorSweep\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = SweepSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 200\n    AttributeModifierAffects = ALL ENEMIES NEUTRAL\n    \
    PermanentlyConvert = Yes\n    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet SweepSet\n  1 = Cast_Area\n  2 = Cast_Neutral\nEnd\n\
CommandButton Cast_Neutral\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_NEUTRAL_OBJECT\nEnd\n\
Locomotor HighFlyer\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\n  \
  PreferredHeight = 60\n  AirborneTargetingHeight = 25\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Locomotor LowFlyer\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\n  \
  PreferredHeight = 60\n  AirborneTargetingHeight = 500\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Object HighBird\n  Side = Other\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = HighFlyer\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object LowBird\n  Side = Other\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = LowFlyer\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Victim\n  Side = Other\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = TemporarilyDefectUpdate Tag_Defect\n    DefectDuration = 30000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object DominatorDefect\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = DefectSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 200\n    AttributeModifierAffects = ALL ENEMIES\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DefectSet\n  1 = Cast_Defect\nEnd\n\
CommandButton Cast_Defect\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Camp",
        "Raider",
        "RaiderHorde",
        "DominatorFiltered",
        "DominatorBroad",
        "DominatorDefect",
        "DominatorSweep",
        "HighBird",
        "LowBird",
        "Victim",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(160, 160);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_map_players(
            2,
            vec!["Civilians".to_owned()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn of_kind_owned(
    m: &Match,
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    name: &str,
    owner: PlayerId,
) -> Entity {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            **k == kind
                && simulation.state().is_live(*entity)
                && simulation
                    .state()
                    .get(ids.owner, *entity)
                    .is_some_and(|o| o.0 == owner)
        })
        .map(|(entity, _)| entity)
        .unwrap()
}

fn select_and_submit(simulation: &mut Simulation<RotwkRules>, unit: Entity, command: RotwkCommand) {
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

fn spawn_horde(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    owner: PlayerId,
    x: f32,
    y: f32,
) -> (Entity, Vec<Entity>) {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RaiderHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            owner,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = *store
        .entities()
        .iter()
        .find(|entity| simulation.state().get(ids.owner, **entity).map(|o| o.0) == Some(owner))
        .unwrap();
    let members = store.get(horde).unwrap().members.clone();
    (horde, members)
}

#[test]
fn area_cast_never_converts_a_horde_member_on_its_own() {
    let m = load();
    let mut simulation = simulation(&m, &[("DominatorFiltered", 200.0, 200.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, OTHER, 230.0, 200.0);
    assert_eq!(members.len(), 2, "the fixture's premise");

    let caster = of_kind_owned(&m, &simulation, ids, "DominatorFiltered", PLAYER);
    select_and_submit(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((230.0, 200.0)),
        },
    );
    for _ in 0..5 {
        simulation.tick().unwrap();
    }

    assert_eq!(
        simulation.state().get(ids.owner, horde).map(|o| o.0),
        Some(OTHER),
        "the horde container fails the INFANTRY filter, so it must stay OTHER's"
    );
    for member in members {
        assert_eq!(
            simulation.state().get(ids.owner, member).map(|o| o.0),
            Some(OTHER),
            "a horde member is never converted on its own, whatever the filter admits"
        );
    }
}

#[test]
fn a_clicked_member_converts_its_whole_horde_and_drops_its_order() {
    let m = load();
    let mut simulation = simulation(&m, &[("DominatorBroad", 200.0, 200.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, OTHER, 230.0, 200.0);
    assert_eq!(members.len(), 2, "the fixture's premise");
    let clicked = members[0];
    let other_member = members[1];
    let caster = of_kind_owned(&m, &simulation, ids, "DominatorBroad", PLAYER);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: caster })
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the fixture's premise: the horde must be attacking before the dominate"
    );
    select_and_submit(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(clicked),
            at: None,
        },
    );
    for _ in 0..5 {
        simulation.tick().unwrap();
    }

    assert_eq!(
        simulation.state().get(ids.owner, horde).map(|o| o.0),
        Some(PLAYER),
        "a clicked member's whole horde converts"
    );
    assert_eq!(
        simulation.state().get(ids.owner, other_member).map(|o| o.0),
        Some(PLAYER),
        "the other member follows its horde"
    );
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "the change of owner drops the unit's own orders"
    );
}

#[test]
fn a_defection_ends_early_when_its_dominator_dies_and_does_not_refresh() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Victim", 230.0, 200.0, OTHER),
            ("DominatorDefect", 200.0, 200.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let victim = of_kind_owned(&m, &simulation, ids, "Victim", OTHER);
    let dominator = of_kind_owned(&m, &simulation, ids, "DominatorDefect", PLAYER);

    select_and_submit(
        &mut simulation,
        dominator,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(victim),
            at: None,
        },
    );
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.state().get(ids.owner, victim).map(|o| o.0),
        Some(PLAYER),
        "the fixture's premise: an unwritten PermanentlyConvert still takes the target"
    );
    let first = *simulation
        .state()
        .get(ids.defect_revert, victim)
        .expect("a TemporarilyDefectUpdate target queues a revert");
    assert_eq!(first.by, dominator);

    select_and_submit(
        &mut simulation,
        dominator,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(victim),
            at: None,
        },
    );
    simulation.tick().unwrap();
    let again = *simulation.state().get(ids.defect_revert, victim).unwrap();
    assert_eq!(
        again, first,
        "the second dominate must not refresh the defection"
    );

    select_and_submit(&mut simulation, dominator, RotwkCommand::DebugKill);
    let mut reverted = false;
    for _ in 0..20 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, victim).map(|o| o.0) == Some(OTHER) {
            reverted = true;
            break;
        }
    }
    assert!(
        reverted,
        "the defection must end early, long before its 900-frame DefectDuration, once its \
         dominator is dead"
    );
    assert!(simulation.frame().get() < first.until.get());
}

const NEUTRAL: PlayerId = PlayerId::new(2);

fn owner_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<PlayerId> {
    simulation.state().get(ids.owner, entity).map(|o| o.0)
}

fn fire(
    simulation: &mut Simulation<RotwkRules>,
    caster: Entity,
    button: u8,
    target: Option<Entity>,
    at: Option<(f32, f32)>,
) {
    select_and_submit(
        simulation,
        caster,
        RotwkCommand::FireSpecialPower { button, target, at },
    );
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
}

#[test]
fn an_area_dominate_takes_enemies_only_but_a_click_takes_a_neutral() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("DominatorSweep", 200.0, 200.0, PLAYER),
            ("Raider", 230.0, 200.0, OTHER),
            ("Raider", 230.0, 230.0, NEUTRAL),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let enemy = of_kind_owned(&m, &simulation, ids, "Raider", OTHER);
    let neutral = of_kind_owned(&m, &simulation, ids, "Raider", NEUTRAL);
    let caster = of_kind_owned(&m, &simulation, ids, "DominatorSweep", PLAYER);
    fire(&mut simulation, caster, 0, None, Some((230.0, 215.0)));
    assert_eq!(owner_of(&simulation, ids, enemy), Some(PLAYER));
    assert_eq!(owner_of(&simulation, ids, neutral), Some(NEUTRAL));
    fire(&mut simulation, caster, 1, Some(neutral), None);
    assert_eq!(owner_of(&simulation, ids, neutral), Some(PLAYER));
}

#[test]
fn a_dominate_passes_over_a_flyer_above_its_targeting_height() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("DominatorSweep", 200.0, 200.0, PLAYER),
            ("HighBird", 230.0, 200.0, OTHER),
            ("LowBird", 230.0, 230.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let high = of_kind_owned(&m, &simulation, ids, "HighBird", OTHER);
    let low = of_kind_owned(&m, &simulation, ids, "LowBird", OTHER);
    let caster = of_kind_owned(&m, &simulation, ids, "DominatorSweep", PLAYER);
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    fire(&mut simulation, caster, 0, None, Some((230.0, 215.0)));
    assert_eq!(owner_of(&simulation, ids, high), Some(OTHER));
    assert_eq!(owner_of(&simulation, ids, low), Some(PLAYER));
}
