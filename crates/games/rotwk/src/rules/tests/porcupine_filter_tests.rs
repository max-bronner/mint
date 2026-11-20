use super::*;

pub(super) const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 200\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
EmotionNugget PanicNugget\n  Type = FEAR\n  AIState = RUN_AWAY_PANIC\n  \
  ModelConditions = EMOTION_AFRAID\n  Duration = 500\n  InactiveDurationSameObject = 500\nEnd\n\
Object Troll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
StanceTemplate PikeTemplate\n  Stance Battle\n  End\n  Stance Porcupine\n  End\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PikeTemplate\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    FearScanDistance = 200\n    AfraidOf = NONE +Troll\n    AddEmotion = PanicNugget\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    RanksToReleaseWhenAttacking = 1\n    \
    ThisFormationIsTheMainFormation = Yes\n    AlternateFormation = PorcupineHorde\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
ChildObject PorcupineHorde PikeHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    IsPorcupineFormation = Yes\n    \
    AlternateFormation = PikeHorde\n    \
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

const UNDERSIZED_DATA: &str = "\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
StanceTemplate PikeTemplate\n  Stance Battle\n  End\n  Stance Porcupine\n  End\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PikeTemplate\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    MinimumHordeSize = 5\n    \
    AlternateFormation = PorcupineHorde\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
ChildObject PorcupineHorde PikeHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    IsPorcupineFormation = Yes\n    \
    AlternateFormation = PikeHorde\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10\n  \
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

pub(super) fn simulation(data: &str, enemies: &[(&str, Vec2)]) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    let (gamedata, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "PikeHorde", "PorcupineHorde"]
        .into_iter()
        .map(str::to_owned)
        .chain(enemies.iter().map(|(name, _)| (*name).to_owned()))
        .collect();
    let (kinds, missing) = KindTable::closure(&gamedata, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&gamedata);
    let spawns = enemies
        .iter()
        .map(|(template, at)| {
            kinds
                .spawn(
                    &gamedata.combat,
                    kinds.kind(template).unwrap(),
                    *at,
                    Facing::EAST,
                    OTHER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(gamedata.formations.tables().to_vec(), Vec::new())
        .with_combat(gamedata.combat.clone())
        .with_guard_ranges(gamedata.guard_ranges)
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
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
                x: HOME.x,
                y: HOME.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids)
}

pub(super) fn horde(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation.state().store(ids.horde).unwrap().entities()[0]
}

fn toggle_to_porcupine(simulation: &mut Simulation<RotwkRules>, ids: Ids) {
    simulation
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .rules()
            .stance_of(simulation.state(), ids, horde(simulation, ids)),
        Some(mint_sage_ini::Stance::Porcupine),
        "the fixture's premise: the toggle must have taken"
    );
}

#[test]
fn stop_drops_outright_on_a_porcupine_horde_rather_than_switching_it_back() {
    let (mut simulation, ids) = simulation(DATA, &[]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    let kind_before = simulation.state().get(ids.kind, horde).copied().unwrap();
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.state().get(ids.kind, horde).copied(),
        Some(kind_before),
        "Stop must not have switched the formation back"
    );
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Porcupine)
    );
    let guard = simulation.state().get(ids.guard, horde).copied().unwrap();
    assert_eq!(guard.mode, super::super::guard::GuardMode::HoldGround);
}

#[test]
fn a_plain_attack_order_is_obeyed_in_place_without_switching_back() {
    let (mut simulation, ids) = simulation(DATA, &[("Quarry", Vec2::new(HOME.x + 60.0, HOME.y))]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    let kind_before = simulation.state().get(ids.kind, horde).copied().unwrap();
    let enemy = simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
        })
        .unwrap()
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.state().get(ids.kind, horde).copied(),
        Some(kind_before),
        "a plain attack must not have switched the formation back"
    );
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Porcupine)
    );
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the attack must still have been taken up"
    );
}

#[test]
fn a_force_attack_switches_back_then_obeys() {
    let (mut simulation, ids) = simulation(DATA, &[("Quarry", Vec2::new(HOME.x + 60.0, HOME.y))]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    let enemy = simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
        })
        .unwrap()
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ForceAttackObject { target: enemy })
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Battle),
        "a force-attack must have switched the horde back first"
    );
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "and then obeyed the force-attack"
    );
}

#[test]
fn an_attack_move_switches_back_then_obeys() {
    let (mut simulation, ids) = simulation(DATA, &[]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: HOME.x + 400.0,
                y: HOME.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Battle),
        "attack-move must have switched the horde back first"
    );
    assert!(
        simulation.state().contains(ids.path, horde),
        "and then obeyed the attack-move"
    );
}

#[test]
fn a_cancelling_order_drops_when_the_horde_cannot_switch_back() {
    let (mut simulation, ids) = simulation(UNDERSIZED_DATA, &[]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    let kind_before = simulation.state().get(ids.kind, horde).copied().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: HOME.x + 400.0,
                y: HOME.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.state().get(ids.kind, horde).copied(),
        Some(kind_before),
        "two members under the main formation's own MinimumHordeSize of 5 must refuse the switch"
    );
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Porcupine),
        "the refused order must not have flipped the stance either"
    );
    assert!(
        !simulation.state().contains(ids.path, horde),
        "the dropped move must not have been obeyed"
    );
}

#[test]
fn run_away_panic_leaves_porcupine_formation_in_place() {
    let (mut simulation, ids) = simulation(DATA, &[("Troll", Vec2::new(HOME.x + 60.0, HOME.y))]);
    toggle_to_porcupine(&mut simulation, ids);
    let horde = horde(&simulation, ids);
    let toggled_at = simulation.frame();
    let mut retriggered = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, horde)
            .is_some_and(|emotion| emotion.activated > toggled_at)
        {
            retriggered = true;
            break;
        }
    }
    assert!(
        retriggered,
        "the fixture's premise: the troll must have frightened the horde again after the toggle"
    );
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Porcupine),
        "panic must not have touched the horde's formation"
    );
}
