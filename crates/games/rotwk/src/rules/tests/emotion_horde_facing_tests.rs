use super::*;

const TEMPLATE: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\n@FEET@End\n\
EmotionNugget Nugget\n  Type = @TYPE@\n  AIState = @STATE@\n  \
  ModelConditions = EMOTION_AFRAID\n  InactiveDurationSameObject = 10000\nEnd\n\
Object Troll\n  Side = Neutral\n  KindOf = SELECTABLE @IMMOBILE@ SCARY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 2000\n  End\n  @TROLL_LOCOMOTOR@\
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeAIUpdate Tag_AI\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    FearScanDistance = 200\n    AfraidOf = NONE +Troll\n    AddEmotion = Nugget\n@TRACKER@  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n@CONTAIN@    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-30 Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:10 Y:30\n  \
  End\nEnd\n\
Locomotor Marcher\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  MinSpeed = 20\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = Marcher\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    FearScanDistance = 200\n    AfraidOf = NONE +Troll\n    AddEmotion = Nugget\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PikeHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_PikeHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n";

const HOME: Vec2 = Vec2::new(300.0, 300.0);
const TROLL_AT: Vec2 = Vec2::new(240.0, 360.0);

struct Setup {
    nugget_type: &'static str,
    state: &'static str,
    feet: &'static str,
    tracker: &'static str,
    contain: &'static str,
    walker: bool,
    roaming: bool,
}

fn scene(setup: &Setup) -> (Simulation<RotwkRules>, Ids, Entity, Option<Entity>) {
    let ranks = if setup.state == "QUARREL" {
        "Position:X:10 Y:0 Position:X:10 Y:-40 Position:X:10 Y:40 Position:X:10 Y:5"
    } else {
        "Position:X:10 Y:-30 Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:10 Y:30"
    };
    let text = TEMPLATE
        .replace(
            "Position:X:10 Y:-30 Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:10 Y:30",
            ranks,
        )
        .replace("@TYPE@", setup.nugget_type)
        .replace("@STATE@", setup.state)
        .replace("@FEET@", setup.feet)
        .replace("@TRACKER@", setup.tracker)
        .replace("@CONTAIN@", setup.contain)
        .replace("@IMMOBILE@", if setup.roaming { "" } else { "IMMOBILE" })
        .replace(
            "@TROLL_LOCOMOTOR@",
            if setup.roaming {
                "LocomotorSet
    Locomotor = Marcher
    Condition = SET_NORMAL
    Speed = 60
  End
  "
            } else {
                ""
            },
        );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (gamedata, _) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "PikeHorde", "Troll", "Walker"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&gamedata, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&gamedata);
    let troll = kinds
        .spawn(
            &gamedata.combat,
            kinds.kind("Troll").unwrap(),
            TROLL_AT,
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let mut spawns = vec![troll];
    if setup.walker {
        spawns.push(
            kinds
                .spawn(
                    &gamedata.combat,
                    kinds.kind("Walker").unwrap(),
                    HOME,
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap(),
        );
    }
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
    let walker = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .get(1)
        .copied()
        .filter(|_| setup.walker);
    (simulation, ids, horde, walker)
}

fn run_until_running(simulation: &mut Simulation<RotwkRules>, ids: Ids, horde: Entity) {
    for _ in 0..200 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.emotion, horde)
            .is_some_and(|emotion| emotion.state.is_some())
        {
            return;
        }
    }
    panic!("the horde never entered its emotion state");
}

fn members(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> Vec<Entity> {
    simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone()
}

fn off(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity, point: Vec2) -> f32 {
    let body = simulation.state().get(ids.body, entity).unwrap().0;
    body.facing
        .delta_to(Facing::from_vector(point.minus(body.position)).unwrap())
        .abs()
}

const BACK_AWAY: Setup = Setup {
    nugget_type: "FEAR",
    state: "BACK_AWAY",
    feet: "",
    tracker: "",
    walker: false,
    roaming: false,
    contain: "    BackupPercentage = 50%\n    BackUpMinDelayTime = 600000\n    \
              BackUpMaxDelayTime = 600000\n",
};

#[test]
fn a_backing_horde_turns_every_member_without_an_entry_to_the_scarer() {
    let (mut simulation, ids, horde, _) = scene(&BACK_AWAY);
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let members = members(&simulation, ids, horde);
    let facing_scarer = members
        .iter()
        .filter(|member| off(&simulation, ids, **member, TROLL_AT) < 0.05)
        .count();
    let waiting = members
        .iter()
        .filter(|member| off(&simulation, ids, **member, TROLL_AT) > 0.5)
        .count();
    assert!(
        facing_scarer >= 1 && facing_scarer + waiting == members.len(),
        "{facing_scarer} of {} members face the scarer, {waiting} wait untouched",
        members.len()
    );
}

#[test]
fn a_member_waiting_out_its_delay_gets_no_formation_move() {
    let (mut simulation, ids, horde, _) = scene(&Setup {
        contain: "    BackupPercentage = 100%\n    BackUpMinDelayTime = 600000\n    \
                  BackUpMaxDelayTime = 600000\n",
        ..BACK_AWAY
    });
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    for member in members(&simulation, ids, horde) {
        assert!(
            off(&simulation, ids, member, TROLL_AT) > 0.5,
            "a waiting member turned to the scarer"
        );
    }
}

const FACE: Setup = Setup {
    nugget_type: "FEAR",
    state: "FACE_OBJECT",
    feet: "",
    tracker: "",
    contain: "",
    walker: false,
    roaming: false,
};

#[test]
fn a_horde_in_face_object_turns_its_members_and_not_its_body() {
    let (mut simulation, ids, horde, _) = scene(&FACE);
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    for member in members(&simulation, ids, horde) {
        assert!(
            off(&simulation, ids, member, TROLL_AT) < 0.05,
            "a member does not face the target"
        );
    }
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    assert!(
        body.facing.delta_to(Facing::EAST).abs() < 1e-3,
        "the horde body turned"
    );
}

#[test]
fn a_porcupine_horde_ignores_the_face_point() {
    let porcupine = Setup {
        contain: "    IsPorcupineFormation = Yes
",
        ..FACE
    };
    let (mut simulation, ids, horde, _) = scene(&porcupine);
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    for member in members(&simulation, ids, horde) {
        assert!(
            off(&simulation, ids, member, TROLL_AT) > 0.5,
            "a porcupine member turned to the face point"
        );
    }
}

const QUARREL: Setup = Setup {
    nugget_type: "QUARRELSOME",
    state: "QUARREL",
    feet: "",
    tracker: "    QuarrelProbability = 100%\n",
    contain: "",
    walker: false,
    roaming: false,
};

#[test]
fn a_quarrel_places_the_second_member_and_rings_the_bystanders() {
    let (mut simulation, ids, horde, _) = scene(&QUARREL);
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let members = members(&simulation, ids, horde);
    let spots: Vec<Vec2> = members
        .iter()
        .map(|member| {
            simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position
        })
        .collect();
    let placed = (0..members.len()).any(|first| {
        let distance = |other: usize| spots[other].minus(spots[first]).length();
        let others: Vec<usize> = (0..members.len()).filter(|i| *i != first).collect();
        let facing_a = |other: usize| off(&simulation, ids, members[other], spots[first]) < 0.005;
        let second = others
            .iter()
            .filter(|i| (distance(**i) - 20.0).abs() < 0.5 && facing_a(**i))
            .count();
        let ring = others
            .iter()
            .filter(|i| (49.5..=60.5).contains(&distance(**i)) && facing_a(**i))
            .count();
        second == 1 && ring == 2
    });
    assert!(
        placed,
        "no member has the quarrel's pair and ring round it: {spots:?}"
    );
}

#[test]
fn a_unit_that_cannot_turn_in_place_walks_toward_the_face_target() {
    let (mut simulation, ids, _, walker) = scene(&Setup {
        walker: true,
        roaming: false,
        ..FACE
    });
    let walker = walker.unwrap();
    run_until_running(&mut simulation, ids, walker);
    let start = simulation.state().get(ids.body, walker).unwrap().0.position;
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let now = simulation.state().get(ids.body, walker).unwrap().0.position;
    assert!(
        now.minus(TROLL_AT).length() + 5.0 < start.minus(TROLL_AT).length(),
        "it stayed {start:?} -> {now:?}"
    );
}

#[test]
fn the_quarrels_first_member_is_the_nearest_the_mean_within_a_distance_of_100() {
    use super::super::emotion_states::quarrel_first;
    let spots = [(0.0, 0.0), (80.0, 0.0), (60.0, 0.0)].map(|(x, y)| Vec2::new(x, y));
    assert_eq!(quarrel_first(&spots), 2, "13 from the mean, not within 10");
    let tie = [(0.0, 0.0), (60.0, 0.0)].map(|(x, y)| Vec2::new(x, y));
    assert_eq!(quarrel_first(&tie), 0, "a tie goes to the first");
    let apart = [(0.0, 0.0), (400.0, 0.0), (200.0, 0.0)].map(|(x, y)| Vec2::new(x, y));
    assert_eq!(quarrel_first(&apart), 2, "the member on the mean");
    let none = [(0.0, 0.0), (500.0, 0.0)].map(|(x, y)| Vec2::new(x, y));
    assert_eq!(quarrel_first(&none), 0, "none within 100: the first");
}

#[test]
fn a_member_counts_as_selected_only_for_the_player_who_controls_it() {
    let (simulation, ids, horde, _) = scene(&QUARREL);
    let member = members(&simulation, ids, horde)[0];
    let mut world = World::new();
    world.restore(&simulation.state().snapshot());
    let held_by =
        |player| crate::unit::Selected(crate::unit::Selected::bit(player).unwrap(), [0; 8]);
    world.insert(ids.selected, member, held_by(OTHER)).unwrap();
    assert!(!super::super::emotion_states::controller_selected(
        &world, ids, member
    ));
    world.insert(ids.selected, member, held_by(PLAYER)).unwrap();
    assert!(super::super::emotion_states::controller_selected(
        &world, ids, member
    ));
}

#[test]
fn the_back_away_distance_counts_the_height_between_member_and_scarer() {
    let (simulation, ids, horde, _) = scene(&QUARREL);
    let member = members(&simulation, ids, horde)[0];
    let troll = simulation.state().store(ids.body).unwrap().entities()[0];
    let mut world = World::new();
    world.restore(&simulation.state().snapshot());
    world
        .insert(ids.ground_offset, troll, crate::unit::GroundOffset(100.0))
        .unwrap();
    let at = world.get(ids.body, member).unwrap().0.position;
    let scarer = world.get(ids.body, troll).unwrap().0.position;
    let (away, length) = simulation
        .rules()
        .separation(&world, ids, member, at, troll, scarer);
    let flat = at.minus(scarer).length();
    assert_eq!(away, at.minus(scarer));
    assert!((length - (flat * flat + 100.0 * 100.0).sqrt()).abs() < 0.01);
}

#[test]
fn a_walk_toward_the_face_target_follows_the_target_as_it_moves() {
    let (mut simulation, ids, _, walker) = scene(&Setup {
        walker: true,
        roaming: true,
        ..FACE
    });
    let walker = walker.unwrap();
    let troll = simulation.state().store(ids.body).unwrap().entities()[0];
    run_until_running(&mut simulation, ids, walker);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![troll],
        },
        RotwkCommand::MoveTo {
            x: TROLL_AT.x,
            y: 520.0,
            queue: false,
        },
    ] {
        simulation.submit(OTHER, command).unwrap();
    }
    for _ in 0..8 {
        simulation.tick().unwrap();
    }
    let troll_at = simulation.state().get(ids.body, troll).unwrap().0.position;
    assert!(
        troll_at.y > TROLL_AT.y + 1.0,
        "the target never moved: {troll_at:?}"
    );
    let aimed = simulation.state().get(ids.path, walker).unwrap().goal;
    assert!(
        aimed.y > TROLL_AT.y + 0.5,
        "aimed at {aimed:?}, the target is at {troll_at:?}"
    );
}

#[test]
fn a_backing_member_is_walked_to_its_point_by_the_formation_pass() {
    let (mut simulation, ids, horde, _) = scene(&Setup {
        contain: "    BackupPercentage = 100%\n    BackUpMinDelayTime = 0\n    \
                  BackUpMaxDelayTime = 0\n    BackUpMinDistance = 5\n    BackUpMaxDistance = 5\n",
        ..BACK_AWAY
    });
    let members = members(&simulation, ids, horde);
    let distance = |simulation: &Simulation<RotwkRules>, member: Entity| {
        let at = simulation.state().get(ids.body, member).unwrap().0.position;
        at.minus(TROLL_AT).length()
    };
    let before: Vec<f32> = members
        .iter()
        .map(|member| distance(&simulation, *member))
        .collect();
    run_until_running(&mut simulation, ids, horde);
    for _ in 0..240 {
        simulation.tick().unwrap();
    }
    for (member, before) in members.iter().zip(before) {
        let moved = distance(&simulation, *member) - before;
        assert!(moved > 30.0, "a member moved {moved} away from the scarer");
        assert!(
            simulation
                .state()
                .get(ids.path, *member)
                .is_none_or(|path| path.kind != crate::unit::PathKind::Emotion),
            "a member walks an emotion route"
        );
    }
}
