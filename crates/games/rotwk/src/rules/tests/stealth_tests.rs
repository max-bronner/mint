use super::*;

fn data(starts_active: bool) -> String {
    let active = if starts_active { "Yes" } else { "No" };
    format!(
        "Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 100\nEnd\n\
         Object ZHider\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
           LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
           Behavior = InvisibilityUpdate Tag_Camo\n    \
             InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      \
               DetectionRange = 100\n      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    \
             UpdatePeriod = 1000\n    StartsActive = {active}\n  End\n  \
           VisionRange = 400\n  ShroudClearingRange = 400\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object ZScout\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
           WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBow\n  End\n  \
           Behavior = AIUpdateInterface Tag_AI\n    \
             AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 200\n  End\n  \
           VisionRange = 400\n  ShroudClearingRange = 400\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Weapon ZBow\n  PrimaryDamage = 10\n  AttackRange = 350\n  \
           DelayBetweenShots = 500\n  WeaponSpeed = 999999\n  \
           DamageNugget\n    Damage = 10\n    Radius = 0\n    DelayTime = 0\n    \
             DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n"
    )
}

fn scene(starts_active: bool, apart: f32) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    scene_on_teams(starts_active, apart, Vec::new())
}

fn scene_on_teams(
    starts_active: bool,
    apart: f32,
    teams: Vec<Option<u8>>,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    scene_of(&data(starts_active), apart, teams, false)
}

fn scene_of(
    data: &str,
    apart: f32,
    teams: Vec<Option<u8>>,
    computer: bool,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZHider", "ZScout"];
    let (kinds, missing) = KindTable::closure(
        &game,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let places = [Vec2::new(300.0, 300.0), Vec2::new(300.0 + apart, 300.0)];
    let owners = [PLAYER, OTHER];
    let spawns = roots
        .iter()
        .zip(places)
        .zip(owners)
        .map(|((name, at), owner)| {
            kinds
                .spawn(
                    &game.combat,
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
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits))
        .with_teams(teams);
    let rules = if computer {
        rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: OTHER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        )
    } else {
        rules
    };
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities[0], entities[1])
}

#[test]
fn a_hidden_object_is_targetable_by_its_owner_alone_and_drawn_for_its_allies() {
    let check = |teams: Vec<Option<u8>>, allied: bool| {
        let (mut simulation, ids, hider, _) = scene_on_teams(true, 300.0, teams);
        settle(&mut simulation);
        assert!(hidden(&simulation, ids, hider), "never hid");
        let (world, rules) = (simulation.state(), simulation.rules());
        assert!(rules.sees(world, ids, PLAYER, hider), "owner targets");
        assert!(
            !rules.stealth_look_hides(world, ids, PLAYER, hider),
            "owner sees"
        );
        assert!(
            !rules.sees(world, ids, OTHER, hider),
            "allied {allied} targets"
        );
        assert_eq!(rules.stealth_look_hides(world, ids, OTHER, hider), !allied);
    };
    check(vec![Some(0), Some(0)], true);
    check(Vec::new(), false);
}

#[test]
fn a_computer_player_sees_a_hidden_object_while_it_attacks() {
    let data = data(true)
        .replace(
            "ForbiddenConditions = MOVING FIRING_ANY",
            "ForbiddenConditions = MOVING",
        )
        .replace(
            "MaxHealth = 400\n  End\n  LocomotorSet",
            "MaxHealth = 400\n  End\n  WeaponSet\n    Conditions = None\n    \
             Weapon = PRIMARY ZBow\n  End\n  LocomotorSet",
        );
    let seen_while_attacking = |computer: bool| {
        let (mut simulation, ids, hider, scout) = scene_of(&data, 300.0, Vec::new(), computer);
        settle(&mut simulation);
        assert!(hidden(&simulation, ids, hider), "never hid");
        assert!(!simulation
            .rules()
            .sees(simulation.state(), ids, OTHER, hider));
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![hider],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: scout })
            .unwrap();
        let mut seen = 0;
        for _ in 0..60 {
            simulation.tick().unwrap();
            assert!(hidden(&simulation, ids, hider), "firing unhid it");
            seen += usize::from(
                simulation
                    .rules()
                    .sees(simulation.state(), ids, OTHER, hider),
            );
        }
        seen
    };
    assert_eq!(
        seen_while_attacking(false),
        0,
        "a human player never sees it"
    );
    assert!(
        seen_while_attacking(true) >= 50,
        "a computer player sees it while it attacks"
    );
}

fn acquires(simulation: &mut Simulation<RotwkRules>, ids: Ids, scout: Entity, ticks: u32) -> bool {
    for _ in 0..ticks {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.armed, scout)
            .is_some_and(|armed| armed.target.is_some())
        {
            return true;
        }
    }
    false
}

fn settle(simulation: &mut Simulation<RotwkRules>) {
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
}

fn hidden(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.stealth, entity)
        .is_some_and(|stealth| stealth.hidden)
}

#[test]
fn a_hidden_object_is_not_acquired_from_300_and_the_same_scene_without_hiding_is() {
    let (mut simulation, ids, hider, scout) = scene(true, 300.0);
    settle(&mut simulation);
    assert!(hidden(&simulation, ids, hider), "never hid");
    assert!(!acquires(&mut simulation, ids, scout, 120), "hidden");

    let (mut simulation, ids, _, scout) = scene(false, 300.0);
    settle(&mut simulation);
    assert!(acquires(&mut simulation, ids, scout, 120), "not hidden");
}

#[test]
fn an_enemy_inside_the_detection_range_reveals_a_camouflaged_object() {
    let (mut simulation, ids, hider, scout) = scene(true, 80.0);
    settle(&mut simulation);
    assert!(hidden(&simulation, ids, hider), "still hiding, just seen");
    assert!(acquires(&mut simulation, ids, scout, 120), "inside 100");
}

#[test]
fn a_move_order_unhides_at_once_and_the_hiding_returns_after_one_update_period() {
    let (mut simulation, ids, hider, _) = scene(true, 300.0);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(hidden(&simulation, ids, hider), "hid while idle");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hider],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 460.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut unhid = None;
    for step in 0..30u32 {
        simulation.tick().unwrap();
        if !hidden(&simulation, ids, hider) {
            unhid = Some(step);
            break;
        }
    }
    assert!(unhid.is_some(), "moving did not unhide it");
    let mut stopped_at = None;
    for _ in 0..300 {
        simulation.tick().unwrap();
        let moving = simulation
            .state()
            .get(ids.body, hider)
            .is_some_and(|body| body.0.speed > 0.0);
        if !moving {
            stopped_at = Some(simulation.frame().get());
            break;
        }
        assert!(!hidden(&simulation, ids, hider), "hid while walking");
    }
    let stopped_at = stopped_at.expect("the walk ends");
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        hidden(&simulation, ids, hider),
        "never hid again after stopping at frame {stopped_at}"
    );
}

const DETECT_DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
SpecialPower ZCloakPower\n  ReloadTime = 1000\nEnd\n\
CommandSet ZCloakSet\n  1 = ZFire_Cloak\nEnd\n\
CommandButton ZFire_Cloak\n  Command = SPECIAL_POWER\n  SpecialPower = ZCloakPower\nEnd\n\
Object ZStealthHider\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Stealth\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      \
      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = Yes\n    \
    EvaEventDetectedEnemy = DetectedEnemy\n    EvaEventDetectedAlly = DetectedAlly\n    \
    EvaEventDetectedOwner = DetectedOwner\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZCloaked\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZCloakSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Cloak\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      \
      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = No\n  End\n  \
  Behavior = ToggleHiddenSpecialAbilityUpdate Tag_CloakButton\n    \
    SpecialPowerTemplate = ZCloakPower\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTimedCloak\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZCloakSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Cloak\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      \
      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = No\n  End\n  \
  Behavior = ToggleHiddenSpecialAbilityUpdate Tag_CloakButton\n    \
    SpecialPowerTemplate = ZCloakPower\n    EffectDuration = 4000\n    ShowPalantirTimer = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZQuietCloak ZTimedCloak\n  \
  Behavior = ToggleHiddenSpecialAbilityUpdate Tag_CloakButton\n    \
    SpecialPowerTemplate = ZCloakPower\n    EffectDuration = 4000\n  End\nEnd\n\
Object ZSeeker\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = StealthDetectorUpdate Tag_Detect\n    DetectionRate = 500\n  End\n  \
  VisionRange = 500\n  ShroudClearingRange = 500\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZHurtHider\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_HurtStealth\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      \
      ForbiddenConditions = TAKING_DAMAGE\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = Yes\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon ZStalkBow\n  PrimaryDamage = 10\n  AttackRange = 600\n  \
  DelayBetweenShots = 500\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 10\n    Radius = 0\n    DelayTime = 0\n    \
    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n\
Object ZStalker\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZStalkBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    \
    AutoAcquireEnemiesWhenIdle = Yes STEALTHED\n    MoodAttackCheckRate = 200\n  End\n  \
  Behavior = StealthUpdate Tag_Stalk\n    StealthDelay = 1000\n    StartsActive = Yes\n    \
    RevealDistanceFromTarget = 1000\n  End\n  \
  VisionRange = 700\n  ShroudClearingRange = 700\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn detect_scene(
    roots: &[&str],
    places: &[Vec2],
    owners: &[PlayerId],
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DETECT_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(
        &game,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let spawns = roots
        .iter()
        .zip(places.iter().copied())
        .zip(owners.iter().copied())
        .map(|((name, at), owner)| {
            kinds
                .spawn(
                    &game.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&game.special_powers);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits))
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities)
}

fn seen_by(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    player: PlayerId,
    entity: Entity,
) -> bool {
    simulation
        .rules()
        .sees(simulation.state(), ids, player, entity)
}

#[test]
fn a_detector_marks_a_hidden_enemy_within_its_own_vision_and_the_mark_lapses_when_it_leaves() {
    let (mut simulation, ids, entities) = detect_scene(
        &["ZStealthHider", "ZSeeker"],
        &[Vec2::new(300.0, 300.0), Vec2::new(700.0, 300.0)],
        &[PLAYER, OTHER],
    );
    let (hider, seeker) = (entities[0], entities[1]);
    settle(&mut simulation);
    assert!(hidden(&simulation, ids, hider), "never hid");
    let mut marked = false;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if seen_by(&simulation, ids, OTHER, hider) {
            marked = true;
            break;
        }
    }
    assert!(marked, "the detector never marked the hider at 400");

    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![seeker],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut lapsed = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if !seen_by(&simulation, ids, OTHER, hider) {
            lapsed = true;
            break;
        }
    }
    assert!(
        lapsed,
        "the mark never lapsed after the detector walked off"
    );
}

#[test]
fn a_detector_mark_reports_the_detected_edge_once_not_its_extension() {
    let (mut simulation, ids, entities) = detect_scene(
        &["ZStealthHider", "ZSeeker"],
        &[Vec2::new(300.0, 300.0), Vec2::new(700.0, 300.0)],
        &[PLAYER, OTHER],
    );
    let hider = entities[0];
    let mut reports = 0usize;
    let mut first_at = None;
    for _ in 0..150 {
        let frame = simulation.tick().unwrap().frame;
        let found = simulation
            .rules()
            .combat_facts(simulation.state())
            .map_or(0, |facts| facts.detections(frame).len());
        reports += found;
        if found > 0 && first_at.is_none() {
            first_at = Some(frame.get());
        }
    }
    assert!(hidden(&simulation, ids, hider), "never hid");
    first_at.expect("the mark never reported a detection");
    assert_eq!(
        reports, 1,
        "reported more than once while the mark stayed held"
    );
    assert!(
        seen_by(&simulation, ids, OTHER, hider),
        "stayed marked after reporting"
    );
}

#[test]
fn at_its_own_victim_reveals_everyone_but_reports_no_detection() {
    let (mut simulation, ids, entities) = detect_scene(
        &["ZStalker", "ZSeeker"],
        &[Vec2::new(300.0, 300.0), Vec2::new(700.0, 300.0)],
        &[PLAYER, OTHER],
    );
    let stalker = entities[0];
    let bystander = PlayerId::new(3);
    let mut settled = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if hidden(&simulation, ids, stalker) {
            settled = true;
            break;
        }
    }
    assert!(settled, "never started hiding");
    let mut revealed_to_everyone = false;
    for _ in 0..300 {
        let frame = simulation.tick().unwrap().frame;
        let found = simulation
            .rules()
            .combat_facts(simulation.state())
            .map_or(0, |facts| facts.detections(frame).len());
        assert_eq!(found, 0, "at_its_own_victim must report no detection");
        if hidden(&simulation, ids, stalker) && seen_by(&simulation, ids, bystander, stalker) {
            revealed_to_everyone = true;
            break;
        }
    }
    assert!(
        revealed_to_everyone,
        "the stalker never reached its own-victim reveal while still hidden"
    );
}

#[test]
fn a_cloak_that_does_not_start_active_hides_only_after_its_button_is_pressed() {
    let (mut simulation, ids, entities) =
        detect_scene(&["ZCloaked"], &[Vec2::new(300.0, 300.0)], &[PLAYER]);
    let cloaked = entities[0];
    settle(&mut simulation);
    assert!(
        !hidden(&simulation, ids, cloaked),
        "hid without the button being pressed"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![cloaked],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    let mut hid = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        if hidden(&simulation, ids, cloaked) {
            hid = true;
            break;
        }
    }
    assert!(hid, "the cloak button never turned the hiding on");
}

#[test]
fn a_timed_cloak_drains_the_palantir_readout_and_one_without_the_flag_shows_none() {
    let (mut simulation, _, entities) = detect_scene(
        &["ZTimedCloak", "ZQuietCloak"],
        &[Vec2::new(300.0, 300.0), Vec2::new(500.0, 300.0)],
        &[PLAYER, PLAYER],
    );
    let shown = |simulation: &Simulation<RotwkRules>, entity: Entity| {
        simulation
            .rules()
            .rank_display(simulation.state(), &[entity], simulation.frame())
    };
    settle(&mut simulation);
    for &cloak in &entities {
        assert_eq!(shown(&simulation, cloak), None, "before the press");
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![cloak],
                },
            )
            .unwrap();
        simulation.tick().unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::FireSpecialPower {
                    button: 0,
                    target: None,
                    at: None,
                },
            )
            .unwrap();
    }
    let mut on_at = None;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if shown(&simulation, entities[0]).is_some() {
            on_at = Some(simulation.frame().get());
            break;
        }
    }
    let on_at = on_at.expect("the timed cloak never showed its time");
    while simulation.frame().get() < on_at + 29 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        shown(&simulation, entities[0]),
        Some(crate::experience::RankDisplay::TimeRemaining(0.75))
    );
    assert_eq!(
        shown(&simulation, entities[1]),
        None,
        "no ShowPalantirTimer"
    );
    while simulation.frame().get() < on_at + 125 {
        simulation.tick().unwrap();
    }
    assert_eq!(shown(&simulation, entities[0]), None, "the cloak ran out");
}

#[test]
fn a_hit_reveals_a_stealthed_object_forbidding_taking_damage_and_it_hides_again_after_the_period() {
    let (mut simulation, ids, entities) =
        detect_scene(&["ZHurtHider"], &[Vec2::new(300.0, 300.0)], &[PLAYER]);
    let hider = entities[0];
    settle(&mut simulation);
    assert!(hidden(&simulation, ids, hider), "never hid");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hider],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Damage { percent: 10 }),
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !hidden(&simulation, ids, hider),
        "stayed hidden after a hit"
    );

    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        hidden(&simulation, ids, hider),
        "never hid again after the window"
    );
}

#[test]
fn a_hidden_object_is_stealthed_only_from_an_enemy_that_has_not_revealed_it() {
    let (mut far, far_ids, far_hider, _) = scene(true, 300.0);
    settle(&mut far);
    let (mut near, near_ids, near_hider, _) = scene(true, 80.0);
    settle(&mut near);
    assert!(hidden(&far, far_ids, far_hider) && hidden(&near, near_ids, near_hider));
    let cases = [
        (
            &far,
            far_ids,
            far_hider,
            OTHER,
            true,
            "an enemy that has not revealed it",
        ),
        (&far, far_ids, far_hider, PLAYER, false, "its own player"),
        (
            &near,
            near_ids,
            near_hider,
            OTHER,
            false,
            "an enemy inside DetectionRange",
        ),
    ];
    for (simulation, ids, entity, player, stealthed, who) in cases {
        let rules = simulation.rules();
        assert_eq!(
            rules.stealthed_from(simulation.state(), ids, player, entity),
            stealthed,
            "{who}"
        );
    }
}

#[test]
fn a_nugget_refuses_a_primary_hidden_by_stealth_not_one_merely_out_of_sight() {
    let (mut simulation, ids, hider, scout) = scene(true, 600.0);
    settle(&mut simulation);
    assert!(hidden(&simulation, ids, hider));
    let unseen = simulation
        .state()
        .get(ids.seen, scout)
        .is_some_and(|seen| seen.visibility(PLAYER) != mint_sage_shroud::ObjectVisibility::Clear);
    assert!(
        unseen,
        "the scout must stand where the hider's player cannot see it"
    );
    let rules = simulation.rules();
    let invisible = |entity: Entity, player: PlayerId| {
        rules
            .fx_facts(simulation.state(), entity, player)
            .unwrap()
            .invisible_to_local
    };
    assert!(invisible(hider, OTHER));
    assert!(!invisible(hider, PLAYER));
    assert!(!invisible(scout, PLAYER));
}

fn voiced(simulation: &Simulation<RotwkRules>, entity: Entity) -> Vec<crate::voice::VoiceKind> {
    let frame = Frame::new(simulation.frame().get() - 1);
    simulation
        .rules()
        .voice_events(simulation.state(), frame)
        .iter()
        .filter(|event| event.entity == entity)
        .map(|event| event.kind)
        .collect()
}

#[test]
fn hiding_and_its_end_are_each_heard_once_on_their_own_tick() {
    let (mut simulation, ids, hider, _) = scene(true, 300.0);
    let mut on = Vec::new();
    for _ in 0..60 {
        simulation.tick().unwrap();
        on.extend(voiced(&simulation, hider));
    }
    assert!(hidden(&simulation, ids, hider));
    assert_eq!(on, [crate::voice::VoiceKind::StealthOn]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hider],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 460.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut off = Vec::new();
    for _ in 0..30 {
        simulation.tick().unwrap();
        off.extend(
            voiced(&simulation, hider)
                .into_iter()
                .filter(|kind| *kind != crate::voice::VoiceKind::Move),
        );
        if !hidden(&simulation, ids, hider) {
            break;
        }
    }
    assert_eq!(off, [crate::voice::VoiceKind::StealthOff]);
}
