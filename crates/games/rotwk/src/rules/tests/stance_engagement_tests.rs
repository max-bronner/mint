use super::*;

const DATA: &str = "\
AIData\n  GuardOuterModifierHuman = 2.2\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Bow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon HordeSight\n  AttackRange = 80\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 0\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Blade\n  AttackRange = 20\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
ModifierList StanceSight\n  Category = LEADERSHIP\n  Modifier = VISION 100%\nEnd\n\
ModifierList StanceBlind\n  Category = LEADERSHIP\n  Modifier = VISION -90%\nEnd\n\
ModifierList StanceFarSight\n  Category = LEADERSHIP\n  Modifier = VISION 300%\nEnd\n\
StanceTemplate FighterTemplate\n  Stance Aggressive\n    AttributeModifier StanceSight\n  End\n  \
  Stance HoldGround\n    AttributeModifier StanceBlind\n  End\nEnd\n\
StanceTemplate ModTemplate\n  Stance Aggressive\n    AttributeModifier StanceFarSight\n  End\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  ShroudClearingRange = 900\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SpearHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  ShroudClearingRange = 900\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = FighterTemplate\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n    HoldGroundCloseRangeDistance = 60\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Bowman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  ShroudClearingRange = 900\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject BowHorde SpearHorde\n  VisionRange = 70\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY HordeSight\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Bowman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
ChildObject QuietHorde SpearHorde\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\nEnd\n\
ChildObject BriskHorde SpearHorde\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 250\n  End\nEnd\n\
ChildObject SlowHorde SpearHorde\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 5000\n  End\nEnd\n\
ChildObject ModHorde SpearHorde\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = ModTemplate\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 8000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet CampSet\n  1 = Build_SpearHorde\n  2 = Build_QuietHorde\n  3 = Build_BriskHorde\n  \
  4 = Build_SlowHorde\n  5 = Build_ModHorde\n  6 = Build_BowHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_SpearHorde\nEnd\n\
CommandButton Build_SpearHorde\n  Command = UNIT_BUILD\n  Object = SpearHorde\nEnd\n\
CommandButton Build_QuietHorde\n  Command = UNIT_BUILD\n  Object = QuietHorde\nEnd\n\
CommandButton Build_BriskHorde\n  Command = UNIT_BUILD\n  Object = BriskHorde\nEnd\n\
CommandButton Build_SlowHorde\n  Command = UNIT_BUILD\n  Object = SlowHorde\nEnd\n\
CommandButton Build_ModHorde\n  Command = UNIT_BUILD\n  Object = ModHorde\nEnd\n\
CommandButton Build_BowHorde\n  Command = UNIT_BUILD\n  Object = BowHorde\nEnd\n";

const HOME: Vec2 = Vec2::new(300.0, 300.0);

fn simulation(template: &str, distance: f32) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Camp",
        "Quarry",
        "SpearHorde",
        "QuietHorde",
        "BriskHorde",
        "SlowHorde",
        "ModHorde",
        "BowHorde",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let quarry = kinds
        .spawn(
            &data.combat,
            kinds.kind("Quarry").unwrap(),
            Vec2::new(HOME.x + distance, HOME.y),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![quarry])
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_melee_hordes(data.formations.melee().to_vec())
        .with_horde_rangefinders(data.formations.rangefinders().to_vec())
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_stances(Arc::clone(&data.stances))
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
        .find(|entry| entry.label == template)
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
    (simulation, ids)
}

fn horde(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation.state().store(ids.horde).unwrap().entities()[0]
}

fn set_stance(simulation: &mut Simulation<RotwkRules>, ids: Ids, stance: mint_sage_ini::Stance) {
    let horde = horde(simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetStance {
                stance: stance.index(),
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

fn engagement(simulation: &mut Simulation<RotwkRules>, ids: Ids, ticks: u32) -> (Option<u32>, f32) {
    let horde = horde(simulation, ids);
    let mut acquired = None;
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    for frame in 0..ticks {
        simulation.tick().unwrap();
        let held = simulation
            .state()
            .get(ids.horde_target, horde)
            .is_some_and(|held| held.target.is_some())
            || simulation.state().contains(ids.attack_order, horde);
        if acquired.is_none() && held {
            acquired = Some(frame);
        }
    }
    let at = simulation.state().get(ids.body, horde).unwrap().0.position;
    let (dx, dy) = (at.x - HOME.x, at.y - HOME.y);
    (acquired, (dx * dx + dy * dy).sqrt())
}

#[test]
fn a_horde_whose_ai_module_writes_no_still_guards_but_never_mood_scans() {
    let (mut battle, ids) = simulation("QuietHorde", 30.0);
    assert!(
        engagement(&mut battle, ids, 90).0.is_some(),
        "Battle guard acquired nothing"
    );
    let (mut aggressive, ids) = simulation("QuietHorde", 30.0);
    set_stance(&mut aggressive, ids, mint_sage_ini::Stance::Aggressive);
    assert_eq!(
        engagement(&mut aggressive, ids, 90).0,
        None,
        "Aggressive with NO acquired"
    );
}

#[test]
fn a_hordes_own_mood_rate_decides_how_often_it_looks() {
    let next_look = |template: &str| {
        let (mut simulation, ids) = simulation(template, 150.0);
        set_stance(&mut simulation, ids, mint_sage_ini::Stance::Aggressive);
        let horde = horde(&simulation, ids);
        simulation.tick().unwrap();
        simulation
            .state()
            .get(ids.horde_target, horde)
            .unwrap()
            .next_acquire
            .get()
    };
    let (brisk, slow) = (next_look("BriskHorde"), next_look("SlowHorde"));
    assert!(slow > brisk + 100, "brisk {brisk}, slow {slow}");
}

#[test]
fn battle_charges_beyond_weapon_range_and_hold_ground_stands() {
    let (mut battle, ids) = simulation("SpearHorde", 110.0);
    let (acquired, travelled) = engagement(&mut battle, ids, 120);
    assert!(acquired.is_some(), "Battle never acquired at 110");
    assert!(travelled > 30.0, "Battle stayed put: {travelled}");
    let (mut held, ids) = simulation("SpearHorde", 110.0);
    set_stance(&mut held, ids, mint_sage_ini::Stance::HoldGround);
    let (acquired, travelled) = engagement(&mut held, ids, 120);
    assert_eq!(acquired, None, "Hold Ground acquired at 110");
    assert!(travelled < 1.0, "Hold Ground walked {travelled}");
}

#[test]
fn a_mods_aggressive_list_widens_the_reach_by_what_it_writes() {
    let (mut battle, ids) = simulation("ModHorde", 350.0);
    assert_eq!(
        engagement(&mut battle, ids, 60).0,
        None,
        "Battle acquired at 350, past its guard radius"
    );
    let (mut wide, ids) = simulation("ModHorde", 350.0);
    set_stance(&mut wide, ids, mint_sage_ini::Stance::Aggressive);
    assert!(
        engagement(&mut wide, ids, 60).0.is_some(),
        "Aggressive did not acquire at 350, inside its 400"
    );
}

#[test]
fn a_human_aggressive_ranged_horde_takes_only_what_its_own_weapon_reaches() {
    let first_find = |distance: f32| {
        let (mut simulation, ids) = simulation("BowHorde", distance);
        set_stance(&mut simulation, ids, mint_sage_ini::Stance::Aggressive);
        engagement(&mut simulation, ids, 60).0
    };
    assert_eq!(first_find(130.0), None, "acquired past its own weapon");
    assert!(
        first_find(90.0).is_some(),
        "nothing acquired inside its own weapon"
    );
}

#[test]
fn an_engagement_replays_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, ids) = simulation("SpearHorde", 150.0);
        set_stance(&mut simulation, ids, mint_sage_ini::Stance::Aggressive);
        let mut hashes = Vec::new();
        for _ in 0..90 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn a_horde_drops_a_victim_handed_to_a_non_enemy() {
    use mint_sage_map::{
        Condition, Dictionary, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
        ScriptTree, TeamDef,
    };

    use crate::rules::map_scripts::{MapScriptEngine, TeamProtos};

    let text = |value: &str| Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    };
    let real = |value: f32| Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: value,
        string_value: String::new(),
    };
    let act = |name: &str, params: Vec<Parameter>| ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    };
    let script =
        |name: &str, condition: (&str, Vec<Parameter>), actions: Vec<ScriptAction>| Script {
            name: name.to_owned(),
            comment: String::new(),
            condition_comment: String::new(),
            action_comment: String::new(),
            active: true,
            one_shot: true,
            easy: true,
            normal: true,
            hard: true,
            subroutine: false,
            delay_evaluation_seconds: 0,
            or_conditions: vec![OrCondition {
                conditions: vec![Condition {
                    condition_type: 0,
                    inverted: false,
                    enabled: true,
                    name: condition.0.to_owned(),
                    params: condition.1,
                }],
            }],
            actions,
            actions_false: Vec::new(),
            sequence: Default::default(),
        };
    let scripts = vec![
        script(
            "Make",
            ("CONDITION_TRUE", Vec::new()),
            vec![
                act(
                    "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
                    vec![text("Prey"), text("Quarry"), text("teamWilds"), text("P1")],
                ),
                act("SET_MILLISECOND_TIMER", vec![text("Hand"), real(4.0)]),
            ],
        ),
        script(
            "Hand",
            ("TIMER_EXPIRED", vec![text("Hand")]),
            vec![act(
                "NAMED_TRANSFER_OWNERSHIP_PLAYER",
                vec![text("Prey"), text("Tame")],
            )],
        ),
    ];
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("P1"), (Vec2::new(HOME.x + 40.0, HOME.y), 0.0))].into();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let team = TeamDef {
        owner: String::from("Wilds"),
        name: String::from("teamWilds"),
        singleton: true,
        properties: Dictionary::from_pairs(Vec::new()),
    };
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), false)
        .with_team_prototypes(TeamProtos::from_defs(&[team]).unwrap());

    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Quarry", "SpearHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_melee_hordes(data.formations.melee().to_vec())
        .with_horde_rangefinders(data.formations.rangefinders().to_vec())
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds"), String::from("Tame")],
            vec![
                crate::alliance::MapRelations {
                    enemies: vec![PLAYER],
                    ..Default::default()
                },
                crate::alliance::MapRelations::default(),
            ],
        )
        .with_map_scripts(Some(engine), vec![(0, PLAYER), (1, OTHER)]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "SpearHorde")
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
    for _ in 0..12 {
        simulation.tick().unwrap();
    }
    let prey = simulation
        .state()
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, tag)| tag.0 == "Prey")
                .map(|(entity, _)| entity)
        })
        .expect("the script made the prey");
    let horde = horde(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: prey })
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the fixture's premise: the horde took the order on an enemy"
    );
    let mut handed = false;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, prey).map(|o| o.0) == Some(PlayerId::new(3)) {
            handed = true;
            break;
        }
    }
    assert!(
        handed,
        "the fixture's premise: the script hands the prey to Tame"
    );
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "the order must end once its victim is no longer an enemy"
    );
}
