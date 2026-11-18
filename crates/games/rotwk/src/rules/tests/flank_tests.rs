use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon SlowShot\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  FireFX = FX_Plain\n  FireFlankFX = FX_Flank\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n    DelayTime = 2000\n  End\nEnd\n\
Object Shooter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY SlowShot\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 5000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pikeman\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ShooterHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  VisionRange = 400\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY SlowShot\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 1\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Shooter Position:X:0 Y:0\n  End\nEnd\n\
Object PikeHorde\n  Side = Wild\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  VisionRange = 10\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    FrontAngle = 90\n    \
    FlankedDuration = 3000\n    \
    RankInfo = RankNumber:1 UnitType:Pikeman Position:X:0 Y:-10 Position:X:0 Y:10\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = HordeSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet HordeSet\n  1 = Build_ShooterHorde\n  2 = Build_PikeHorde\nEnd\n\
CommandButton Build_ShooterHorde\n  Command = UNIT_BUILD\n  Object = ShooterHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n";

fn simulation() -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (gamedata, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Camp", "ShooterHorde", "PikeHorde"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&gamedata, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&gamedata);
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(gamedata.formations.tables().to_vec(), Vec::new())
        .with_melee_hordes(gamedata.formations.melee().to_vec())
        .with_combat(gamedata.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = |label: &str| {
        simulation
            .rules()
            .buildable_catalogue()
            .iter()
            .find(|entry| entry.label == label)
            .unwrap()
            .index
    };
    let spawns = [
        (PLAYER, recipe("ShooterHorde"), 300.0, 0.0),
        (OTHER, recipe("PikeHorde"), 400.0, 0.0),
    ];
    for (player, recipe, x, facing) in spawns {
        simulation
            .submit(
                player,
                RotwkCommand::DebugSpawnHorde {
                    recipe,
                    x,
                    y: 300.0,
                    facing,
                },
            )
            .unwrap();
    }
    simulation.tick().unwrap();
    (simulation, ids)
}

fn members(simulation: &Simulation<RotwkRules>, ids: Ids, owner: PlayerId) -> Vec<Entity> {
    simulation
        .state()
        .store(ids.member)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .filter(|entity| simulation.state().get(ids.owner, *entity).map(|o| o.0) == Some(owner))
        .collect()
}

#[test]
fn a_shot_from_a_horde_that_has_died_out_lands_without_flanking() {
    let (mut simulation, ids) = simulation();
    let shooter = members(&simulation, ids, PLAYER)[0];
    let shooter_horde = simulation.state().get(ids.member, shooter).unwrap().horde;
    let pikes = members(&simulation, ids, OTHER);
    let target = pikes[0];
    let full = simulation
        .state()
        .get(ids.health, target)
        .unwrap()
        .0
        .current();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let in_flight = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .store(ids.delayed_hits)
            .is_some_and(|store| store.iter().any(|(_, hits)| !hits.0.is_empty()))
    };
    for _ in 0..120 {
        simulation.tick().unwrap();
        if in_flight(&simulation) {
            break;
        }
    }
    assert!(in_flight(&simulation), "the shooter never fired");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    assert!(
        !simulation.state().is_live(shooter_horde),
        "the shooter's horde outlived its last member"
    );
    assert!(
        simulation.state().is_live(shooter),
        "the corpse must still stand for its shot to name it"
    );

    for _ in 0..90 {
        simulation.tick().unwrap();
        if !in_flight(&simulation) {
            break;
        }
    }
    assert!(!in_flight(&simulation), "the delayed shot never landed");
    let now = simulation
        .state()
        .get(ids.health, target)
        .unwrap()
        .0
        .current();
    assert!(
        now < full,
        "the shot did not land on the pike ({now} of {full})"
    );
}

#[test]
fn a_shot_from_behind_plays_the_flank_fx_on_the_shooter_instead_of_the_fire_fx() {
    let (mut simulation, ids) = simulation();
    let shooter = members(&simulation, ids, PLAYER)[0];
    let target = members(&simulation, ids, OTHER)[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let mut plays = Vec::new();
    for _ in 0..240 {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            let name = simulation.rules().fx_name(event.list).unwrap_or("?");
            plays.push((name.to_owned(), event.on_object, event.caller, event.victim));
        }
    }
    let flank_run = plays.iter().take_while(|play| play.0 == "FX_Flank").count();
    assert!(
        flank_run >= 2,
        "the first shots are not flank plays: {plays:?}"
    );
    assert!(
        plays[flank_run..].iter().all(|play| play.0 == "FX_Plain"),
        "{plays:?}"
    );
    assert!(
        plays
            .iter()
            .all(|play| (play.1, play.2, play.3) == (true, shooter, Some(target))),
        "{plays:?}"
    );
}

#[test]
fn a_corpse_keeps_no_member_once_its_horde_is_gone() {
    let (mut simulation, ids) = simulation();
    let shooter = members(&simulation, ids, PLAYER)[0];
    let horde = simulation.state().get(ids.member, shooter).unwrap().horde;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    assert!(!simulation.state().is_live(horde));
    assert!(
        simulation.state().is_live(shooter),
        "the corpse still stands"
    );
    assert_eq!(simulation.state().get(ids.member, shooter), None);
}
