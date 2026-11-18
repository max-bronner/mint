use super::super::guard::GuardMode;
use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Sword\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
SpecialPower SpecialAbilityRampage\n  ReloadTime = 30000\nEnd\n\
StanceTemplate BasicStances\n  Stance Battle\n  End\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = BasicStances\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Rager\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  CommandSet = RageSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = BasicStances\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRampage\n    \
    Duration = 30000\n    WeaponSetFlags = RAMPAGE\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = BasicStances\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PikeHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_PikeHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n\
CommandSet RageSet\n  1 = Fire_Rampage\nEnd\n\
CommandButton Fire_Rampage\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRampage\nEnd\n";

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
    let seeds: Vec<String> = ["Camp", "Soldier", "Rager", "Quarry", "PikeHorde"]
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
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_teams(vec![None, None]);
    Simulation::new(5, rules).unwrap()
}

fn of_kind(m: &Match, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Entity {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
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

#[test]
fn a_loose_mobile_unit_guards_at_its_own_spot_when_stopped() {
    let m = load();
    let mut simulation = simulation(&m, &[("Soldier", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let soldier = of_kind(&m, &simulation, ids, "Soldier");
    select_and_submit(
        &mut simulation,
        soldier,
        RotwkCommand::MoveTo {
            x: 900.0,
            y: 100.0,
            queue: false,
        },
    );
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let at_stop = simulation
        .state()
        .get(ids.body, soldier)
        .unwrap()
        .0
        .position;
    assert!(at_stop.x < 400.0, "the fixture's premise: still marching");
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.path, soldier),
        "a path must not survive Stop"
    );
    let guard = simulation.state().get(ids.guard, soldier).copied().unwrap();
    assert_eq!(guard.mode, GuardMode::Normal);
    let now = simulation
        .state()
        .get(ids.body, soldier)
        .unwrap()
        .0
        .position;
    assert!(
        (guard.point.x - now.x).abs() < 1.0 && (guard.point.y - now.y).abs() < 1.0,
        "guards {:?}, stands at {:?}",
        guard.point,
        now
    );
    assert!(
        (guard.point.x - 900.0).abs() > 50.0,
        "must not have finished the march to the old goal"
    );
}

#[test]
fn a_unit_stopped_at_speed_stands_still() {
    let m = load();
    let mut simulation = simulation(&m, &[("Soldier", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let soldier = of_kind(&m, &simulation, ids, "Soldier");
    select_and_submit(
        &mut simulation,
        soldier,
        RotwkCommand::MoveTo {
            x: 900.0,
            y: 100.0,
            queue: false,
        },
    );
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let speed = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.body, soldier).unwrap().0.speed
    };
    assert!(speed(&simulation) > 1.0, "the fixture's premise: running");
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert_eq!(speed(&simulation), 0.0);
    assert_eq!(
        simulation
            .state()
            .get(ids.motive, soldier)
            .map(|motive| motive.0),
        Some(mint_movement::MotionPhase::NONE)
    );
}

#[test]
fn stop_clears_a_direct_attack_order_not_only_the_guard_scan() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Soldier", 100.0, 100.0, PLAYER),
            ("Quarry", 130.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let soldier = of_kind(&m, &simulation, ids, "Soldier");
    let quarry = of_kind(&m, &simulation, ids, "Quarry");
    select_and_submit(
        &mut simulation,
        soldier,
        RotwkCommand::AttackObject { target: quarry },
    );
    let mut fighting = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.attack_order, soldier) {
            fighting = true;
            break;
        }
    }
    assert!(fighting, "the fixture's premise: the order took");
    assert!(
        simulation
            .state()
            .get(ids.attack_order, soldier)
            .is_some_and(|order| !order.guard),
        "the fixture's premise: a direct order, not a guard-acquired one"
    );
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .get(ids.attack_order, soldier)
            .is_none_or(|order| order.guard),
        "Stop must clear the direct attack order"
    );
}

#[test]
fn a_rampaging_unit_ignores_stop() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Rager", 100.0, 100.0, PLAYER),
            ("Quarry", 130.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let rager = of_kind(&m, &simulation, ids, "Rager");
    let quarry = of_kind(&m, &simulation, ids, "Quarry");
    select_and_submit(
        &mut simulation,
        rager,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: None,
        },
    );
    simulation.tick().unwrap();
    assert!(
        simulation.rules().is_rampaging(simulation.state(), rager),
        "the fixture's premise: the power raised RAMPAGE"
    );
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: quarry })
        .unwrap();
    let mut fighting = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.attack_order, rager) {
            fighting = true;
            break;
        }
    }
    assert!(fighting, "the fixture's premise: the order took");
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, rager),
        "a rampaging unit's standing order must survive Stop"
    );
    assert!(
        !simulation.state().contains(ids.guard, rager),
        "a rampaging unit must not be guarded by Stop either"
    );
}

#[test]
fn a_fighting_hordes_own_order_is_cleared_by_stop() {
    let m = load();
    let mut simulation = simulation(&m, &[("Quarry", 260.0, 200.0, OTHER)]);
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "PikeHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let quarry = of_kind(&m, &simulation, ids, "Quarry");
    select_and_submit(
        &mut simulation,
        horde,
        RotwkCommand::AttackObject { target: quarry },
    );
    let mut fighting = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.attack_order, horde)
            .is_some_and(|order| !order.guard)
        {
            fighting = true;
            break;
        }
    }
    assert!(fighting, "the fixture's premise: the horde's order took");
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .get(ids.attack_order, horde)
            .is_none_or(|order| order.guard),
        "Stop must clear the horde's own direct order"
    );
    assert!(simulation.state().contains(ids.guard, horde));
}
