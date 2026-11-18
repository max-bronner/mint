use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 50\n    DamageType = PIERCE\n  End\nEnd\n\
EmotionNugget ZCheer\n  Type = CHEER\n  IgnoreIfUnitBusy = Yes\n  \
  ModelConditions = EMOTION_CELEBRATING\n  Duration = 1000\nEnd\n\
Object Sentry\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    AddEmotion = ZCheer\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Victim\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 40\n    CheerRadius = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(data: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    scene_on(
        data,
        passability,
        [Vec2::new(300.0, 300.0), Vec2::new(330.0, 300.0)],
        false,
    )
}

fn scene_on(
    data: &str,
    passability: Passability,
    places: [Vec2; 2],
    computers: bool,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Sentry", "Victim"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [("Sentry", places[0], PLAYER), ("Victim", places[1], OTHER)]
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let rules = if computers {
        let against = |player| crate::alliance::MapRelations {
            enemies: vec![player],
            ..Default::default()
        };
        rules.with_map_players(0, Vec::new(), vec![against(OTHER), against(PLAYER)])
    } else {
        rules
    };
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities[0], entities[1])
}

fn cheering(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.emotion, entity)
        .is_some_and(|emotion| emotion.kind == mint_sage_ini::EmotionType::Cheer.code())
}

#[test]
fn a_kill_makes_the_killer_cheer_for_the_nuggets_duration() {
    let (mut simulation, ids, sentry, victim) = scene(DATA);
    let mut killed_at = None;
    let mut started = None;
    let mut ended = None;
    for _ in 0..600 {
        simulation.tick().unwrap();
        let frame = simulation.frame().get();
        if killed_at.is_none()
            && simulation
                .state()
                .get(ids.health, victim)
                .is_none_or(|health| health.0.is_dead())
        {
            killed_at = Some(frame);
        }
        let now = cheering(&simulation, ids, sentry);
        if now && started.is_none() {
            started = Some(frame);
        }
        if !now && started.is_some() {
            ended = Some(frame);
            break;
        }
    }
    let killed_at = killed_at.expect("the sentry never killed the victim");
    let started = started.expect("the sentry never cheered");
    let ended = ended.expect("the cheer never ended");
    assert!(
        started >= killed_at,
        "cheered at {started} before the kill at {killed_at}"
    );
    let lasted = ended - started;
    assert!(
        (25..=40).contains(&lasted),
        "Duration 1000 ms is 30 ticks; the cheer lasted {lasted}"
    );
}

#[test]
fn no_cheer_radius_no_cheer() {
    let data = DATA.replace("    CheerRadius = 100\n", "");
    assert_ne!(data, DATA);
    let (mut simulation, ids, sentry, _) = scene(&data);
    for _ in 0..600 {
        simulation.tick().unwrap();
        assert!(!cheering(&simulation, ids, sentry));
    }
}

#[test]
fn a_move_order_ends_an_idle_cheer() {
    let (mut simulation, ids, sentry, _) = scene(DATA);
    let mut started = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if cheering(&simulation, ids, sentry) {
            started = true;
            break;
        }
    }
    assert!(started, "the sentry never cheered");
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![sentry],
        },
        RotwkCommand::MoveTo {
            x: 300.0,
            y: 500.0,
            queue: false,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    let mut ended = None;
    for tick in 0..30 {
        simulation.tick().unwrap();
        if !cheering(&simulation, ids, sentry) {
            ended = Some(tick);
            break;
        }
    }
    let ended = ended.expect("the cheer outlived the move order");
    assert!(
        ended <= 10,
        "the cheer took {ended} ticks to end after the order"
    );
}

#[test]
fn a_kill_across_the_map_edge_raises_no_cheer() {
    let cheered = |victim_y: f32| {
        let mut map = flat_map(61, 61);
        map.border_size = 10;
        map.boundaries = vec![mint_sage_map::Boundary { x: 39, y: 39 }];
        let passability = Passability::builder(&map, 5.0 / 128.0)
            .playable_border(true)
            .build()
            .unwrap();
        let places = [Vec2::new(200.0, 20.0), Vec2::new(200.0, victim_y)];
        let (mut simulation, ids, sentry, _) = scene_on(DATA, passability, places, true);
        (0..300).any(|_| {
            simulation.tick().unwrap();
            cheering(&simulation, ids, sentry)
        })
    };
    assert!(cheered(60.0), "the control did not cheer");
    assert!(!cheered(-20.0), "cheered for a kill beyond the edge");
}
