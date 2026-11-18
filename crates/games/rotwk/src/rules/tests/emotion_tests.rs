use super::*;

fn data() -> String {
    String::from(
        "Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 100\nEnd\n\
         EmotionNugget ZFearIdle\n  \
           Type = FEAR\n  IgnoreIfUnitBusy = Yes\n  AIState = BACK_AWAY\n  \
           AILockDuration = 4000\n  ModelConditions = EMOTION_AFRAID\n  \
           InactiveDurationSameObject = 10000\n  StartFXList = FX_ZCower\n  \
           EndFXList = FX_ZRegroup\nEnd\n\
         EmotionNugget ZFearTimed\n  \
           Type = FEAR\n  ModelConditions = EMOTION_AFRAID\n  Duration = 1000\n  \
           InactiveDurationSameObject = 10000\nEnd\n\
         Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZFighter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_SCARED\n    Speed = 999\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 100\n    \
             AfraidOf = NONE +ZTroll\n    AddEmotion = ZFearIdle\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object ZWatcher\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 100\n    \
             AfraidOf = NONE +ZTroll\n    AddEmotion = ZFearTimed\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

const TROLL_AT: Vec2 = Vec2::new(300.0, 300.0);
const FIGHTER_AT: Vec2 = Vec2::new(390.0, 300.0);
const WATCHER_AT: Vec2 = Vec2::new(300.0, 390.0);

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data(), &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    assert_eq!(emotions.len(), 2);
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter", "ZWatcher"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = roots
        .iter()
        .zip([
            (TROLL_AT, OTHER),
            (FIGHTER_AT, PLAYER),
            (WATCHER_AT, PLAYER),
        ])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter, watcher) = (entities[0], entities[1], entities[2]);
    simulation.tick().unwrap();
    (simulation, ids, troll, fighter, watcher)
}

fn holds(simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str) -> bool {
    simulation
        .rules()
        .modules
        .as_ref()
        .and_then(|wiring| simulation.state().get(wiring.state, entity))
        .is_some_and(|state| {
            state
                .model_condition_names()
                .iter()
                .any(|held| held.eq_ignore_ascii_case(flag))
        })
}

#[test]
fn a_loose_unit_backing_away_turns_to_face_the_scarer_and_stays() {
    let (mut simulation, ids, troll, fighter, _) = scene();
    let mut started = None;
    for _ in 0..30 {
        let at = simulation.frame().get();
        simulation.tick().unwrap();
        if started.is_none() && simulation.state().get(ids.emotion, fighter).is_some() {
            started = Some(at);
        }
    }
    let started = started.expect("an emotion within one TauntAndPointUpdateDelay");
    let emotion = simulation.state().get(ids.emotion, fighter).unwrap();
    assert_eq!(emotion.cause, Some(troll));
    assert_eq!(emotion.kind, mint_sage_ini::EmotionType::Fear.code());
    assert_eq!(
        emotion.lock_until.get() - emotion.activated.get(),
        120,
        "AILockDuration 4000 ms is 120 frames"
    );
    assert!(emotion.activated.get() <= started + 1);
    assert!(holds(&simulation, fighter, "EMOTION_AFRAID"));
    assert!(
        simulation.state().get(ids.path, fighter).is_none(),
        "a loose unit backing away does not walk"
    );
    let facing = simulation.state().get(ids.body, fighter).unwrap().0.facing;
    let towards = Facing::from_vector(TROLL_AT.minus(FIGHTER_AT)).unwrap();
    assert!(
        facing.delta_to(towards).abs() < 1.0e-3,
        "it faces the troll: {}",
        facing.radians()
    );
}

#[test]
fn a_loose_unit_backing_away_keeps_its_own_locomotor_set() {
    let (mut simulation, ids, _, fighter, _) = scene();
    for _ in 0..30 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.emotion, fighter).is_some() {
            break;
        }
    }
    assert!(
        simulation.state().get(ids.emotion, fighter).is_some(),
        "an emotion within one TauntAndPointUpdateDelay"
    );
    assert!(
        simulation.state().get(ids.locomotion, fighter).is_none(),
        "SET_SCARED must not apply during BACK_AWAY"
    );
}

#[test]
fn a_locked_emotion_refuses_orders_until_its_lock_lifts() {
    let (mut simulation, ids, _troll, fighter, _) = scene();
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    let order = |simulation: &mut Simulation<RotwkRules>| {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![fighter],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 600.0,
                    y: 300.0,
                    queue: false,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    };
    order(&mut simulation);
    assert!(
        simulation.state().get(ids.path, fighter).is_none(),
        "the locked state refused the move"
    );
    let lifts = simulation
        .state()
        .get(ids.emotion, fighter)
        .unwrap()
        .lock_until
        .get();
    while simulation.frame().get() <= lifts + 6 {
        simulation.tick().unwrap();
    }
    order(&mut simulation);
    assert!(
        simulation.state().get(ids.path, fighter).is_some(),
        "an unlocked state lets the order through"
    );
}

#[test]
fn a_cooldown_keeps_the_same_scarer_from_starting_it_again() {
    let (mut simulation, ids, _troll, _fighter, watcher) = scene();
    let mut felt: Vec<u64> = Vec::new();
    for _ in 0..480u64 {
        let at = simulation.frame().get();
        simulation.tick().unwrap();
        if simulation.state().get(ids.emotion, watcher).is_some() {
            felt.push(at);
        }
    }
    let first = *felt.first().expect("it is frightened at all");
    let ended = felt
        .windows(2)
        .find(|pair| pair[1] > pair[0] + 1)
        .map(|pair| pair[0])
        .expect("and stops being frightened");
    assert!(
        ended - first <= 36,
        "Duration 1000 ms is 30 frames, ended at the next retail-frame update, not {}",
        ended - first
    );
    assert!(
        !holds(&simulation, watcher, "EMOTION_AFRAID"),
        "the condition is dropped when the feeling ends"
    );
    let again = *felt
        .iter()
        .find(|frame| **frame > ended + 1)
        .expect("and is frightened again once the cooldown runs out");
    assert!(
        again >= ended + 300,
        "InactiveDurationSameObject 10000 ms is 300 frames; it started again after {}",
        again - ended
    );
}

fn face_data() -> String {
    String::from(
        "Locomotor ZSlowFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 2000\nEnd\n\
         EmotionNugget ZFearFace\n  \
           Type = FEAR\n  AIState = FACE_OBJECT\n  ModelConditions = EMOTION_AFRAID\n  \
           InactiveDurationSameObject = 10000\nEnd\n\
         Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZFighter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           LocomotorSet\n    Locomotor = ZSlowFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 100\n    \
             AfraidOf = NONE +ZTroll\n    AddEmotion = ZFearFace\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

const FACE_TROLL_AT: Vec2 = Vec2::new(300.0, 300.0);
const FACE_FIGHTER_AT: Vec2 = Vec2::new(300.0, 390.0);

fn face_scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &face_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = roots
        .iter()
        .zip([(FACE_TROLL_AT, OTHER), (FACE_FIGHTER_AT, PLAYER)])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter) = (entities[0], entities[1]);
    simulation.tick().unwrap();
    (simulation, ids, troll, fighter)
}

#[test]
fn face_object_ends_with_success_once_within_tolerance() {
    let (mut simulation, ids, troll, fighter) = face_scene();
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    let mut ended_at = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, fighter)
            .is_some_and(|emotion| emotion.state.is_none())
        {
            ended_at = Some(());
            break;
        }
    }
    assert!(ended_at.is_some(), "FACE_OBJECT never ended on its own");
    let facing = simulation.state().get(ids.body, fighter).unwrap().0.facing;
    let towards = Facing::from_vector(FACE_TROLL_AT.minus(FACE_FIGHTER_AT)).unwrap();
    assert!(
        facing.delta_to(towards).abs() < 0.035,
        "ended {} rad off its scarer",
        facing.delta_to(towards).abs()
    );
    assert!(
        simulation.state().get(ids.emotion, fighter).is_some(),
        "the nugget deactivated instead of staying active"
    );
    assert_eq!(simulation.state().get(ids.emotion, troll), None);
}

fn stance_face_scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let data = face_data().replace("TurnTime = 2000", "TurnTime = 1000000")
        + "\
         StanceTemplate ZStance\n  Stance Aggressive\n    AttributeModifier ZSight\n  End\nEnd\n\
         ModifierList ZSight\n  Category = LEADERSHIP\n  Modifier = VISION 100%\nEnd\n";
    let data = data.replace(
        "Object ZFighter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n",
        "Object ZFighter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = StancesBehavior Tag_S\n    StanceTemplate = ZStance\n  End\n",
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (game_data, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter"];
    let (kinds, missing) = KindTable::closure(
        &game_data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game_data);
    let spawns = roots
        .iter()
        .zip([(FACE_TROLL_AT, OTHER), (FACE_FIGHTER_AT, PLAYER)])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &game_data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game_data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(
            kinds.upgrades(&game_data),
            Arc::clone(&game_data.upgrade_bits),
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter) = (entities[0], entities[1]);
    simulation.tick().unwrap();
    (simulation, ids, troll, fighter)
}

#[test]
fn face_object_gives_up_once_per_retail_frame_not_every_tick() {
    let (mut simulation, ids, _troll, fighter) = stance_face_scene();
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    for _ in 0..5 {
        simulation.tick().unwrap();
        assert!(
            simulation
                .state()
                .get(ids.emotion, fighter)
                .is_some_and(|emotion| emotion.state.is_some()),
            "the overlay ended within 5 ticks - counted every tick, not once a retail frame"
        );
    }
    let mut ended_at = None;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, fighter)
            .is_some_and(|emotion| emotion.state.is_none())
        {
            ended_at = Some(simulation.frame().get());
            break;
        }
    }
    assert!(ended_at.is_some(), "the overlay never gave up at all");
}

#[test]
fn an_order_during_face_object_deactivates_the_nugget() {
    let (mut simulation, ids, _troll, fighter) = face_scene();
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![fighter],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 390.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().get(ids.emotion, fighter).is_none(),
        "the order ended the overlay without deactivating the nugget"
    );
    assert!(
        simulation.state().get(ids.path, fighter).is_some(),
        "the move order itself was refused"
    );
}

#[test]
fn duration_rounds_up_to_the_retail_two_hundred_millisecond_frame() {
    let data = String::from(
        "EmotionNugget ZFearQuick\n  Type = FEAR\n  Duration = 250\n  \
           ModelConditions = EMOTION_AFRAID\n  InactiveDurationSameObject = 10000\nEnd\n\
         Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZWatcher\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 100\n    \
             AfraidOf = NONE +ZTroll\n    AddEmotion = ZFearQuick\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (game_data, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll".to_owned(), "ZWatcher".to_owned()];
    let (kinds, missing) = KindTable::closure(&game_data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game_data);
    let spawns = roots
        .iter()
        .zip([(TROLL_AT, OTHER), (WATCHER_AT, PLAYER)])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &game_data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game_data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(
            kinds.upgrades(&game_data),
            Arc::clone(&game_data.upgrade_bits),
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let watcher = entities[1];
    simulation.tick().unwrap();
    while simulation.state().get(ids.emotion, watcher).is_none() {
        simulation.tick().unwrap();
    }
    let emotion = simulation.state().get(ids.emotion, watcher).unwrap();
    assert_eq!(
        emotion.ends.get() - emotion.activated.get(),
        12,
        "250 ms must ceil to 2 retail frames (400 ms, 12 ticks)"
    );
}

fn kill_data() -> String {
    String::from(
        "Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 100\nEnd\n\
         Weapon BigHit\n  AttackRange = 200\n  DelayBetweenShots = 500\n  WeaponSpeed = 99999\n  \
           HitPercentage = 100\n  \
           DamageNugget\n    Damage = 5000\n    DamageType = CRUSH\n  End\nEnd\n\
         EmotionNugget ZFearIdle\n  \
           Type = FEAR\n  IgnoreIfUnitBusy = Yes\n  AIState = BACK_AWAY\n  \
           AILockDuration = 4000\n  ModelConditions = EMOTION_AFRAID\n  \
           InactiveDurationSameObject = 10000\nEnd\n\
         Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZFighter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 100\n    \
             AfraidOf = NONE +ZTroll\n    AddEmotion = ZFearIdle\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object ZSlayer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
           WeaponSet\n    Conditions = None\n    Weapon = PRIMARY BigHit\n  End\n  \
           Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

const KILL_TROLL_AT: Vec2 = Vec2::new(300.0, 300.0);
const KILL_FIGHTER_AT: Vec2 = Vec2::new(390.0, 300.0);
const KILL_SLAYER_AT: Vec2 = Vec2::new(300.0, 250.0);

#[test]
fn back_away_ends_when_the_scarer_is_really_killed() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &kill_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter", "ZSlayer"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = roots
        .iter()
        .zip([
            (KILL_TROLL_AT, OTHER),
            (KILL_FIGHTER_AT, PLAYER),
            (KILL_SLAYER_AT, PLAYER),
        ])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter, slayer) = (entities[0], entities[1], entities[2]);
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "fighter enters BACK_AWAY"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slayer],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: troll })
        .unwrap();
    let mut troll_dead = false;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.health, troll)
            .is_some_and(|health| health.0.is_dead())
        {
            troll_dead = true;
        }
    }
    assert!(
        troll_dead,
        "the troll must actually die for this test to mean anything"
    );
    assert!(
        !holds(&simulation, fighter, "EMOTION_AFRAID"),
        "BACK_AWAY must end once its scarer is dead, not merely doomed"
    );
}

#[test]
fn back_away_ends_when_a_horde_scarer_is_really_killed() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &kill_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter", "ZSlayer"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = ["ZFighter", "ZSlayer"]
        .iter()
        .zip([(KILL_FIGHTER_AT, PLAYER), (KILL_SLAYER_AT, PLAYER)])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let troll_member = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZTroll").unwrap(),
            KILL_TROLL_AT,
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let formation = table_of(&[Vec2::ZERO]);
    let horde_spawn = HordeSpawn {
        formation: 0,
        origin: KILL_TROLL_AT,
        facing: Facing::EAST,
        member: troll_member,
        count: 1,
        vision: 0.0,
    };
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2)
        .with_hordes(vec![formation], vec![horde_spawn]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let troll = members_of(&simulation, ids)
        .first()
        .copied()
        .expect("the horde spawned its one member");
    let (fighter, slayer) = {
        let entities: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .filter(|entity| *entity != troll)
            .collect();
        (entities[0], entities[1])
    };
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "fighter enters BACK_AWAY"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slayer],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: troll })
        .unwrap();
    let mut troll_dead = false;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.health, troll)
            .is_some_and(|health| health.0.is_dead())
        {
            troll_dead = true;
        }
    }
    assert!(
        troll_dead,
        "the horde's one member must actually die for this test to mean anything"
    );
    assert!(
        !holds(&simulation, fighter, "EMOTION_AFRAID"),
        "BACK_AWAY must end once a horde scarer's member is dead"
    );
}

fn burning_kill_data() -> String {
    kill_data()
        .replace("DamageType = CRUSH", "DamageType = FLAME")
        .replace(
            "Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  ",
            "Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n    BurningDeathBehavior = Yes\n  End\n  \
           Behavior = AIUpdateInterface Tag_AI\n    BurningDeathTime = 1000\n  End\n  ",
        )
}

#[test]
fn back_away_ends_on_the_lethal_flame_hit_not_the_burn_end() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &burning_kill_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter", "ZSlayer"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = roots
        .iter()
        .zip([
            (KILL_TROLL_AT, OTHER),
            (KILL_FIGHTER_AT, PLAYER),
            (KILL_SLAYER_AT, PLAYER),
        ])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter, slayer) = (entities[0], entities[1], entities[2]);
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "fighter enters BACK_AWAY"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slayer],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: troll })
        .unwrap();
    let mut burning_started = false;
    let mut cower_ended_frame = None;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.burning, troll) {
            burning_started = true;
        }
        if cower_ended_frame.is_none() && !holds(&simulation, fighter, "EMOTION_AFRAID") {
            cower_ended_frame = Some(simulation.frame());
        }
    }
    assert!(
        burning_started,
        "the troll must actually start its slow death for this test to mean anything"
    );
    assert!(
        cower_ended_frame.is_some(),
        "BACK_AWAY must end once the lethal FLAME hit lands, not merely once the burn finishes"
    );
}

#[test]
fn back_away_keeps_going_while_the_scarer_is_only_doomed() {
    let (mut simulation, ids, troll, fighter, _) = scene();
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "fighter enters BACK_AWAY"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![troll],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.doomed, troll),
        "the troll must actually be doomed for this test to mean anything"
    );
    assert!(
        !simulation
            .state()
            .get(ids.health, troll)
            .is_some_and(|health| health.0.is_dead()),
        "DebugKill must not flip the dead flag for this test to mean anything"
    );
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "BACK_AWAY must not end while its scarer is merely doomed"
    );
}

fn spell_book_kill_data() -> String {
    kill_data().replace(
        "Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  ",
        "Object ZTroll\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY SPELL_BOOK\n  ",
    )
}

#[test]
fn back_away_keeps_going_once_a_spell_book_scarer_is_dead() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &spell_book_kill_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTroll", "ZFighter", "ZSlayer"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = roots
        .iter()
        .zip([
            (KILL_TROLL_AT, OTHER),
            (KILL_FIGHTER_AT, PLAYER),
            (KILL_SLAYER_AT, PLAYER),
        ])
        .map(|(name, (at, owner))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (troll, fighter, slayer) = (entities[0], entities[1], entities[2]);
    while simulation.state().get(ids.emotion, fighter).is_none() {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "fighter enters BACK_AWAY"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slayer],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: troll })
        .unwrap();
    let mut troll_dead = false;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.health, troll)
            .is_some_and(|health| health.0.is_dead())
        {
            troll_dead = true;
        }
    }
    assert!(
        troll_dead,
        "the SPELL_BOOK troll must actually die for this test to mean anything"
    );
    assert!(
        holds(&simulation, fighter, "EMOTION_AFRAID"),
        "BACK_AWAY must keep going once a dead SPELL_BOOK scarer dies"
    );
}

const TAUNT_DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
EmotionNugget ZTaunt\n  Type = TAUNT\n  Duration = 1000\nEnd\n\
Object ZTaunter\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
    TauntAndPointUpdateDelay = 1000\n    TauntAndPointDistance = 100\n    AddEmotion = ZTaunt\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFoe\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn taunts_at(distance: f32) -> bool {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        TAUNT_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTaunter".to_owned(), "ZFoe".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let spawns = [
        (&roots[0], Vec2::new(300.0, 300.0), PLAYER),
        (&roots[1], Vec2::new(300.0 + distance, 300.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let taunter = simulation.state().store(ids.body).unwrap().entities()[0];
    (0..70).any(|_| {
        simulation.tick().unwrap();
        simulation
            .state()
            .get(ids.emotion, taunter)
            .is_some_and(|emotion| emotion.kind == mint_sage_ini::EmotionType::Taunt.code())
    })
}

#[test]
fn the_taunt_scan_measures_centre_to_centre() {
    assert!(
        taunts_at(95.0),
        "an enemy inside the distance was not taunted"
    );
    assert!(!taunts_at(105.0), "an enemy outside by centre was taunted");
}

#[test]
fn the_ground_line_walk_includes_both_ends() {
    use crate::rules::emotions::ground_line_clear;
    let blocked = |at: (i32, i32)| move |cell: (i32, i32)| cell != at;
    assert!(ground_line_clear((0, 0), (4, 0), |_| true));
    assert!(!ground_line_clear((0, 0), (4, 0), blocked((2, 0))));
    assert!(!ground_line_clear((0, 0), (4, 0), blocked((4, 0))));
    assert!(!ground_line_clear((0, 0), (4, 0), blocked((0, 0))));
    assert!(ground_line_clear((0, 0), (4, 0), blocked((2, 1))));
    assert!(!ground_line_clear((0, 0), (6, 3), blocked((3, 2))));
    assert!(ground_line_clear((2, 2), (2, 2), |_| true));
}

fn list_data() -> String {
    String::from(
        "EmotionNugget ZFearIdle\n  \
           Type = FEAR\n  IgnoreIfUnitBusy = Yes\n  AIState = BACK_AWAY\n  \
           AILockDuration = 4000\n  InactiveDurationSameObject = 10000\nEnd\n\
         EmotionNugget ZTaunt\n  Type = TAUNT\n  Duration = 1000\nEnd\n\
         Object ZOne\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    AddEmotion = ZTaunt\n    \
             AddEmotion = OVERRIDE ZFearIdle\n      Duration = 7000\n    End\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object ZTwo\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    AddEmotion = ZFearIdle\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

fn read_per_update(
    index: &mint_sage_ini::EmotionIndex,
    tracker: &mint_sage_modules::EmotionTrackerUpdate,
) -> Vec<mint_sage_ini::EmotionNuggetDef> {
    let errors = mint_sage_ini::ValueErrors::new();
    tracker
        .emotions()
        .iter()
        .filter_map(|declaration| index.resolve(declaration, &errors))
        .collect()
}

#[test]
fn the_per_kind_emotion_list_is_what_the_update_resolved() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &list_data(),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZOne".to_owned(), "ZTwo".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let spawns = [
        (&roots[0], Vec2::new(300.0, 300.0)),
        (&roots[1], Vec2::new(400.0, 300.0)),
    ]
    .into_iter()
    .map(|(name, at)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    assert_eq!(entities.len(), 2);
    let rules = simulation.rules();
    let column = rules.modules.as_ref().unwrap().emotion_state;
    let tracker_of = |world: &World, entity: Entity| {
        world
            .get(column, entity)
            .unwrap()
            .0
            .first()
            .unwrap()
            .state
            .clone()
    };
    for &entity in &entities {
        let tracker = tracker_of(simulation.state(), entity);
        let read = rules.nuggets_of(simulation.state(), ids, entity, &tracker);
        assert!(
            matches!(read, std::borrow::Cow::Borrowed(_)),
            "a tracker carrying its kind's declaration reads the list built at setup"
        );
        assert_eq!(*read, read_per_update(&rules.emotions, &tracker)[..]);
    }
    let one = tracker_of(simulation.state(), entities[0]);
    let built = rules.nuggets_of(simulation.state(), ids, entities[0], &one);
    assert_eq!(built.len(), 2);
    assert_eq!(
        built[0].emotion_type,
        Some(mint_sage_ini::EmotionType::Taunt)
    );
    assert_eq!(
        built[1].duration_ms,
        Some(7000),
        "the override body applies"
    );
    assert_eq!(
        built[1].ai_lock_duration_ms,
        Some(4000),
        "over the named nugget"
    );

    let two = tracker_of(simulation.state(), entities[1]);
    let read = rules.nuggets_of(simulation.state(), ids, entities[0], &two);
    assert!(matches!(read, std::borrow::Cow::Owned(_)));
    assert_eq!(*read, read_per_update(&rules.emotions, &two)[..]);
}
