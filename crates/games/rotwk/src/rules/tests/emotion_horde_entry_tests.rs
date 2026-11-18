use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Sword\n  AttackRange = 15\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
EmotionNugget PanicNugget\n  Type = FEAR\n  AIState = RUN_AWAY_PANIC\n  \
  ModelConditions = EMOTION_AFRAID\n  Duration = 5000\n  InactiveDurationSameObject = 10000\nEnd\n\
Object Troll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    FearScanDistance = 200\n    AfraidOf = NONE +Troll\n    AddEmotion = PanicNugget\n  End\n  \
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

fn simulation(enemies: &[(&str, Vec2)]) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    let (gamedata, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "PikeHorde"]
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
        .with_melee_hordes(gamedata.formations.melee().to_vec())
        .with_combat(gamedata.combat.clone())
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

#[test]
fn panic_ends_the_hordes_melee_and_drops_every_members_own_order() {
    let (mut simulation, ids) = simulation(&[
        ("Quarry", Vec2::new(HOME.x + 12.0, HOME.y)),
        ("Troll", Vec2::new(HOME.x - 60.0, HOME.y)),
    ]);
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let mut enemies: Vec<Entity> = simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .filter(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap();
    enemies.sort_unstable_by_key(|entity| entity.index());
    let target = enemies[0];
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let mut in_melee = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.horde_melee, horde) {
            in_melee = true;
            break;
        }
    }
    assert!(
        in_melee,
        "the fixture's premise: the horde must have closed into melee"
    );
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert!(
        members
            .iter()
            .any(|member| simulation.state().contains(ids.member_melee, *member)),
        "the fixture's premise: a member must be recorded as fighting in melee"
    );

    let mut panicked = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.emotion, horde).is_some() {
            panicked = true;
            break;
        }
    }
    assert!(
        panicked,
        "the fixture's premise: the troll must have frightened the horde"
    );
    assert!(
        !simulation.state().contains(ids.horde_melee, horde),
        "panic must have ended the horde's melee"
    );
    assert!(
        members
            .iter()
            .all(|member| !simulation.state().contains(ids.member_melee, *member)),
        "every member's melee record must have cleared with it"
    );
    assert!(
        members
            .iter()
            .all(|member| !simulation.state().contains(ids.attack_order, *member)),
        "every member's own attack order must have been dropped on panic entry"
    );
}
