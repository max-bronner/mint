use super::*;

const DATA: &str = "\
AIData\n  GuardChaseUnitsDuration = 2000\n  GuardEnemyScanRate = 3000\n  \
  GuardEnemyReturnScanRate = 1000\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
StanceTemplate PikeTemplate\n  Stance HoldGround\n  End\n  Stance Porcupine\n  End\nEnd\n\
EmotionNugget ZTaunt\n  Type = TAUNT\n  IgnoreIfUnitBusy = Yes\n  AIState = FACE_OBJECT\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  BuildCost = 1\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 20000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object QuarryCamp\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = QuarryCampSet\n  BuildCost = 1\n  BuildTime = 1\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet QuarryCampSet\n  1 = Build_Quarry\nEnd\n\
CommandButton Build_Quarry\n  Command = UNIT_BUILD\n  Object = Quarry\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PikeTemplate\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emo\n    TauntAndPointUpdateDelay = 500\n    \
    TauntAndPointDistance = 150\n    AddEmotion = ZTaunt\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PikeHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_PikeHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n";

const HOME: Vec2 = Vec2::new(300.0, 300.0);
const ENEMY_AT: Vec2 = Vec2::new(330.0, 330.0);

fn scene(data: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "QuarryCamp", "Quarry", "PikeHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_stances(Arc::clone(&data.stances))
        .with_emotions(Arc::clone(&data.emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let catalogue = simulation.rules().buildable_catalogue();
    let horde_recipe = catalogue
        .iter()
        .find(|entry| entry.label == "PikeHorde")
        .unwrap()
        .index;
    let quarry_recipe = catalogue
        .iter()
        .find(|entry| entry.label == "Quarry")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe: horde_recipe,
                x: HOME.x,
                y: HOME.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            OTHER,
            RotwkCommand::DebugSpawnUnit {
                buildable: quarry_recipe,
                x: ENEMY_AT.x,
                y: ENEMY_AT.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids, horde)
}

#[test]
fn a_taunted_guarding_horde_still_attacks_and_drops_the_taunt_on_inner() {
    use super::super::guard::GuardState;

    let (mut simulation, ids, horde) = scene(DATA);
    let mut taunted = false;
    let mut inner_at: Option<u64> = None;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, horde)
            .is_some_and(|emotion| emotion.kind == mint_sage_ini::EmotionType::Taunt.code())
        {
            taunted = true;
        }
        if inner_at.is_none()
            && simulation.state().get(ids.guard, horde).map(|g| g.state) == Some(GuardState::Inner)
        {
            inner_at = Some(simulation.frame().get());
        }
        if inner_at.is_some() {
            break;
        }
    }
    assert!(taunted, "the horde never picked up the taunt");
    let inner_at = inner_at.expect("the horde never entered INNER - it stayed pinned by the taunt");
    let mut dropped = false;
    for _ in 0..10 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, horde)
            .is_none_or(|emotion| emotion.kind != mint_sage_ini::EmotionType::Taunt.code())
        {
            dropped = true;
            break;
        }
    }
    assert!(
        dropped,
        "the taunt did not drop once guard went INNER at frame {inner_at}"
    );
}

#[test]
fn an_idle_horde_does_not_taunt_an_enemy_structure() {
    let data = DATA.replace(
        "Object Quarry
  Side = Wild
  KindOf = INFANTRY SELECTABLE",
        "Object Quarry
  Side = Wild
  KindOf = STRUCTURE SELECTABLE IMMOBILE",
    );
    assert_ne!(data, DATA);
    let (mut simulation, ids, horde) = scene(&data);
    for _ in 0..90 {
        simulation.tick().unwrap();
        assert!(
            simulation
                .state()
                .get(ids.emotion, horde)
                .is_none_or(|emotion| emotion.kind != mint_sage_ini::EmotionType::Taunt.code()),
            "the horde taunted a structure at frame {}",
            simulation.frame().get()
        );
    }
}
