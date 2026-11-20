use super::*;
use crate::rules::knockback::bend_shock;

fn caster(name: &str, weapon: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
         Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    InitialHealth = 500\n  End\n  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY {weapon}\n  End\n  \
         Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

fn weapon(name: &str, nuggets: &str) -> String {
    format!(
        "Weapon {name}\n  AttackRange = 300\n  DelayBetweenShots = 60000\n  HitPercentage = 100\n  \
         {nuggets}\nEnd\n"
    )
}

fn data() -> String {
    let mut text = String::from(
        "GameData\n  Gravity = -64.0\nEnd\n\
         Object Dummy\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         ChildObject Hurt Dummy\n  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    \
           InitialHealth = 50\n  End\nEnd\n\
         ChildObject Mender Hurt\n  Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    \
           HealingAmount = 10\n    HealingDelay = 500\n  End\nEnd\n\
         Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 100\nEnd\n\
         ChildObject Walker Dummy\n  LocomotorSet\n    Locomotor = Feet\n    \
           Condition = SET_NORMAL\n    Speed = 60\n  End\nEnd\n\
         ChildObject Brittle Dummy\n  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    \
           MaxHealthDamaged = 60\n    EnteringDamagedTransitionTime = 1000\n  End\nEnd\n\
         Object Keep\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
           Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
           Behavior = HordeGarrisonContain Tag_Garrison\n    \
             ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
             PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
             EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
             ExitOffset = X:30.0 Y:0.0 Z:0.0\n    NumberOfExitPaths = 2\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 60\nEnd\n\
         ChildObject Heavy Dummy\n  ShockwaveResistance = 120\nEnd\n\
         ChildObject Hordish Dummy\n  KindOf = INFANTRY SELECTABLE HORDE\nEnd\n\
         ChildObject Mooring Dummy\n  KindOf = INFANTRY SELECTABLE IMMOBILE\nEnd\n\
         ChildObject Lurking Walker\n  Behavior = InvisibilityUpdate Tag_Camo\n    \
           InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      DetectionRange = 100\n      \
             ForbiddenConditions = MOVING FIRING_ANY\n    End\n    UpdatePeriod = 1000\n    \
           StartsActive = Yes\n  End\nEnd\n\
         Object ZGate\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
           Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
           Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
           Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 10\n    TriggerWidthY = 10\n  End\n  \
           Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
             ResetTimeInMilliseconds = 1000\n  End\n  \
           Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n\
         Object PassRider\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object PassBoat\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
           Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
           Behavior = TransportContain Tag_Boat\n    \
             ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    Slots = 1\n    \
             PassengerFilter = NONE +PassRider\n    InitialPayload = PassRider 1\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 60\nEnd\n",
    );
    let wave =
        "ShockWaveAmount = 150\n    ShockWaveRadius = 200\n    ShockWaveTaperOff = 1.0\n    \
                ShockWaveZMult = 1.0\n";
    for (name, weapon_name, nuggets) in [
        (
            "Drainer",
            "DrainBlade",
            "DamageNugget\n    Damage = 100\n    DamageType = MAGIC\n    DrainLife = Yes\n    \
             DrainLifeMultiplier = 0.5\n  End"
                .to_owned(),
        ),
        (
            "PlainDrainer",
            "PlainDrain",
            "DamageNugget\n    Damage = 100\n    DamageType = MAGIC\n    DrainLife = Yes\n  End"
                .to_owned(),
        ),
        (
            "Healer",
            "HealBolt",
            "DamageNugget\n    Damage = 20\n    DamageType = HEALING\n  End".to_owned(),
        ),
        (
            "LowShooter",
            "LowBolt",
            "DamageNugget\n    Damage = 10\n    DamageType = SLASH\n    \
             DamageMaxHeightAboveTerrain = 1\n  End"
                .to_owned(),
        ),
        (
            "Finisher",
            "FinishBlow",
            format!(
                "DamageNugget\n    Damage = 9999\n    DamageType = SLASH\n  End\n  \
                 MetaImpactNugget\n    OnlyWhenJustDied = Yes\n    {wave}  End"
            ),
        ),
        (
            "Slammer",
            "SlamBlow",
            format!("MetaImpactNugget\n    {wave}  End"),
        ),
        (
            "HordeSlammer",
            "HordeSlamBlow",
            format!("MetaImpactNugget\n    AffectHordes = Yes\n    {wave}  End"),
        ),
        (
            "Paralyser",
            "ParalyzeBolt",
            "ParalyzeNugget\n    Radius = 0\n    Duration = 2000\n  End".to_owned(),
        ),
        (
            "ParaSlammer",
            "ParaSlamBlow",
            format!(
                "MetaImpactNugget\n    {wave}  End\n  \
                 ParalyzeNugget\n    Radius = 0\n    Duration = 2000\n  End"
            ),
        ),
        (
            "Pounder",
            "PoundBlow",
            "DamageNugget\n    Damage = 50\n    DamageType = SLASH\n  End".to_owned(),
        ),
        (
            "SelfBlaster",
            "SelfBlow",
            "RadiusDamageAffects = SELF ALLIES ENEMIES\n  DamageNugget\n    Damage = 10\n    \
             DamageType = SLASH\n    Radius = 20\n  End"
                .to_owned(),
        ),
        (
            "Bombarder",
            "BombardShot",
            "PreAttackDelay = 1000\n  PreAttackType = PER_SHOT\n  NoVictimNeeded = Yes\n  \
             RadiusDamageAffects = ALLIES ENEMIES\n  DamageNugget\n    Damage = 10\n    \
             DamageType = SLASH\n    Radius = 8\n  End"
                .to_owned(),
        ),
        (
            "Freezer",
            "FreezeBolt",
            "ParalyzeNugget\n    Radius = 0\n    Duration = 2000\n    FreezeAnimation = Yes\n  End"
                .to_owned(),
        ),
        ("KeyHolder", "KeyTurn", "OpenGateNugget\n  End".to_owned()),
        (
            "Kicker",
            "KickBolt",
            "HitPassengerPercentage = 100%\n  DamageNugget\n    Damage = 10\n    \
             DamageType = PASSENGER\n    ForceKillObjectFilter = NONE +PassRider\n  End"
                .to_owned(),
        ),
    ] {
        text.push_str(&weapon(weapon_name, &nuggets));
        text.push_str(&caster(name, weapon_name));
    }
    text.push_str(&weapon(
        "GarrisonShot",
        "DamageNugget
    Damage = 10
    DamageType = SLASH
  End",
    ));
    text.push_str(&caster("Garrisoned", "GarrisonShot").replace(
        "  Geometry =",
        "  CommandSet = ReachSet
  Behavior = AttributeModifierUpgrade Tag_Reach
                 TriggeredBy = Upgrade_Reach
    AttributeModifier = DoubleReach
  End
               Geometry =",
    ));
    text.push_str(
        "Upgrade Upgrade_Reach
  Type = OBJECT
  BuildCost = 0
  BuildTime = 1
End
         ModifierList DoubleReach
  Category = BUFF
  Modifier = RANGE 100%
  Duration = 0
End
         CommandSet ReachSet
  1 = Buy_Reach
End
         CommandButton Buy_Reach
  Command = OBJECT_UPGRADE
  Upgrade = Upgrade_Reach
End
",
    );
    text.push_str(
        "Object PostKeep
  Side = Men
  KindOf = STRUCTURE SELECTABLE IMMOBILE
             Body = StructureBody Tag_Body
    MaxHealth = 1000
  End
             Behavior = HordeTransportContain Tag_Boat
                 ObjectStatusOfContained = CAN_ATTACK ENCLOSED INSIDE_GARRISON
    Slots = 1
                 PassengerFilter = NONE +Garrisoned
    InitialPayload = Garrisoned 1
  End
             Geometry = CYLINDER
  GeometryMajorRadius = 20
  GeometryHeight = 60
End
",
    );
    text.push_str(&weapon(
        "BombBlast",
        "DamageNugget\n    Damage = 30\n    DamageType = SLASH\n    Radius = 8\n  End",
    ));
    text.push_str(
        "Object Bomb\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
         Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
         Behavior = FireWeaponWhenDeadBehavior Tag_Dead\n    StartsActive = Yes\n    \
           DeathWeapon = BombBlast\n    DelayTime = 1000\n    WeaponOffset = X:20 Y:0 Z:0\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    );
    text
}

const CASTER_AT: Vec2 = Vec2::new(300.0, 300.0);
const TARGET_AT: Vec2 = Vec2::new(400.0, 300.0);
const NEIGHBOUR_AT: Vec2 = Vec2::new(440.0, 300.0);

fn scene(
    caster_name: &str,
    others: &[(&str, Vec2)],
    lift: f32,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_facing(caster_name, others, lift, Facing::EAST)
}

fn scene_facing(
    caster_name: &str,
    others: &[(&str, Vec2)],
    lift: f32,
    facing: Facing,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_for(caster_name, others, lift, facing, None)
}

fn scene_for(
    caster_name: &str,
    others: &[(&str, Vec2)],
    lift: f32,
    facing: Facing,
    computer: Option<PlayerId>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let text = data();
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let mut roots = vec![caster_name.to_owned()];
    roots.extend(others.iter().map(|(name, _)| (*name).to_owned()));
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let mut spawns = vec![(caster_name, CASTER_AT, PLAYER)];
    let faces = |index: usize| if index == 0 { facing } else { Facing::EAST };
    spawns.extend(others.iter().map(|(name, at)| (*name, *at, OTHER)));
    let mut spawns: Vec<_> = spawns
        .into_iter()
        .enumerate()
        .map(|(index, (name, at, player))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    faces(index),
                    player,
                )
                .unwrap()
        })
        .collect();
    if lift > 0.0 {
        if let Some(spawn) = spawns.get_mut(1) {
            spawn.ground_offset = Some(lift);
        }
    }
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let mut rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    if let Some(player) = computer {
        rules = rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    }
    let simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn attack(simulation: &mut Simulation<RotwkRules>, who: Entity, target: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![who],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |health| health.0.current())
}

#[test]
fn drain_life_heals_the_source_by_a_share_of_the_damage_dealt() {
    let (mut simulation, ids, e) = scene("Drainer", &[("Dummy", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    run(&mut simulation, 90);
    assert_eq!(health(&simulation, ids, e[1]), 0.0);
    assert_eq!(health(&simulation, ids, e[0]), 550.0);
}

#[test]
fn drain_life_without_a_multiplier_heals_all_the_damage_dealt() {
    let (mut simulation, ids, e) = scene("PlainDrainer", &[("Dummy", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    run(&mut simulation, 90);
    assert_eq!(health(&simulation, ids, e[0]), 600.0);
}

#[test]
fn a_garrisoned_shooters_boosted_reach_is_capped_at_its_scan_radius() {
    let reach = |caster: &str| {
        let (mut simulation, ids, e) = scene(caster, &[("Dummy", Vec2::new(800.0, 300.0))], 0.0);
        run(&mut simulation, 1);
        let shooter = if caster == "Garrisoned" {
            e[0]
        } else {
            simulation.state().get(ids.passengers, e[0]).unwrap().0[0]
        };
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
            .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
            .unwrap();
        run(&mut simulation, 60);
        simulation
            .rules()
            .range_with_factor(simulation.state(), ids, shooter, 300.0)
    };
    assert_eq!(reach("Garrisoned"), 600.0, "the loose control");
    assert_eq!(reach("PostKeep"), 405.0);
}

#[test]
fn a_healing_nugget_heals_its_victim() {
    let (mut simulation, ids, e) = scene("Healer", &[("Hurt", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    run(&mut simulation, 90);
    assert_eq!(health(&simulation, ids, e[1]), 70.0);
}

#[test]
fn a_victim_above_the_terrain_height_gate_is_skipped() {
    let mut dealt = Vec::new();
    for offset in [5.0, 0.001] {
        let (mut simulation, ids, e) = scene("LowShooter", &[("Dummy", TARGET_AT)], offset);
        attack(&mut simulation, e[0], e[1]);
        run(&mut simulation, 90);
        dealt.push(100.0 - health(&simulation, ids, e[1]));
    }
    assert_eq!(dealt, [0.0, 10.0]);
}

#[test]
fn a_force_kill_blow_removes_a_passenger_from_its_container() {
    let (mut simulation, ids, e) = scene("Kicker", &[("PassBoat", TARGET_AT)], 0.0);
    run(&mut simulation, 1);
    let rider = simulation
        .state()
        .get(ids.passengers, e[1])
        .and_then(|list| list.0.first().copied())
        .unwrap();
    attack(&mut simulation, e[0], e[1]);
    run(&mut simulation, 90);
    assert!(!simulation.state().contains(ids.contained, rider));
}

fn thrown(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.state().contains(ids.knocked, entity)
}

fn ever_thrown(simulation: &mut Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    let mut seen = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        seen |= thrown(simulation, ids, entity);
    }
    seen
}

#[test]
fn only_when_just_died_throws_the_victim_the_shot_killed() {
    let (mut simulation, ids, e) = scene(
        "Finisher",
        &[("Dummy", TARGET_AT), ("Dummy", NEIGHBOUR_AT)],
        0.0,
    );
    attack(&mut simulation, e[0], e[1]);
    let mut killed_thrown = false;
    let mut neighbour_thrown = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        killed_thrown |= thrown(&simulation, ids, e[1]);
        neighbour_thrown |= thrown(&simulation, ids, e[2]);
    }
    assert_eq!(health(&simulation, ids, e[1]), 0.0);
    assert!(killed_thrown, "the killed victim was never thrown");
    assert!(!neighbour_thrown, "a survivor was thrown");
}

#[test]
fn a_shockwave_resistance_of_100_or_more_is_never_thrown() {
    let (mut simulation, ids, e) = scene(
        "Slammer",
        &[("Dummy", TARGET_AT), ("Heavy", NEIGHBOUR_AT)],
        0.0,
    );
    attack(&mut simulation, e[0], e[1]);
    let mut heavy = false;
    let mut plain = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        plain |= thrown(&simulation, ids, e[1]);
        heavy |= thrown(&simulation, ids, e[2]);
    }
    assert!(plain, "the control was never thrown");
    assert!(!heavy, "resistance 120 against an amount of 150 was thrown");
}

#[test]
fn a_horde_victim_is_thrown_only_with_affect_hordes() {
    let mut results = Vec::new();
    for caster in ["Slammer", "HordeSlammer"] {
        let (mut simulation, ids, e) = scene(
            caster,
            &[("Dummy", TARGET_AT), ("Hordish", NEIGHBOUR_AT)],
            0.0,
        );
        attack(&mut simulation, e[0], e[1]);
        results.push(ever_thrown(&mut simulation, ids, e[2]));
    }
    assert_eq!(results, [false, true]);
}

#[test]
fn a_cyclonic_factor_bends_the_shock_vector() {
    let bent = bend_shock(Vec2::new(100.0, 0.0), 0.5);
    assert_eq!((bent.x, bent.y), (50.0, -25.0));
    let plain = bend_shock(Vec2::new(100.0, 0.0), 0.0);
    assert_eq!((plain.x, plain.y), (100.0, 0.0));
}

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation.state().get(ids.body, entity).unwrap().0.position
}

fn disabled(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.state().contains(ids.disabled, entity)
}

#[test]
fn a_paralysed_unit_stops_walking_and_resumes_after() {
    let (mut simulation, ids, e) = scene("Paralyser", &[("Walker", TARGET_AT)], 0.0);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![e[1]],
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
    run(&mut simulation, 5);
    assert!(
        position(&simulation, ids, e[1]).x > TARGET_AT.x,
        "it never set off"
    );
    attack(&mut simulation, e[0], e[1]);
    let mut seen = false;
    for _ in 0..30 {
        simulation.tick().unwrap();
        if disabled(&simulation, ids, e[1]) {
            seen = true;
            break;
        }
    }
    assert!(seen, "the nugget never paralysed it");
    let held = position(&simulation, ids, e[1]);
    run(&mut simulation, 20);
    assert_eq!(
        position(&simulation, ids, e[1]),
        held,
        "it walked while paralysed"
    );
    for _ in 0..200 {
        simulation.tick().unwrap();
        if !disabled(&simulation, ids, e[1]) {
            break;
        }
    }
    run(&mut simulation, 20);
    assert!(
        position(&simulation, ids, e[1]).x > held.x,
        "it never resumed"
    );
}

#[test]
fn a_paralysed_unit_with_a_stealth_declaration_does_not_hide_inside_the_window() {
    let hidden = |simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity| {
        simulation
            .state()
            .get(ids.stealth, entity)
            .is_some_and(|stealth| stealth.hidden)
    };
    let (mut simulation, ids, e) = scene("Paralyser", &[("Lurking", Vec2::new(540.0, 300.0))], 0.0);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![e[1]],
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
    run(&mut simulation, 31);
    assert!(
        !hidden(&simulation, ids, e[1]),
        "walking, it was not revealed on the pass at frame 30"
    );
    simulation.submit(OTHER, RotwkCommand::Stop).unwrap();
    attack(&mut simulation, e[0], e[1]);
    let mut paralysed = false;
    for _ in 0..28 {
        simulation.tick().unwrap();
        if disabled(&simulation, ids, e[1]) {
            paralysed = true;
            break;
        }
    }
    assert!(paralysed, "the nugget never paralysed it");
    assert_eq!(
        simulation.state().get(ids.body, e[1]).unwrap().0.speed,
        0.0,
        "it was still walking, so its own conditions forbid hiding"
    );
    let mut window = 0_u32;
    while disabled(&simulation, ids, e[1]) && window < 200 {
        assert!(!hidden(&simulation, ids, e[1]), "hid while paralysed");
        simulation.tick().unwrap();
        window += 1;
    }
    assert!(
        window >= 30,
        "the window ({window}) held no one-second pass"
    );
}

#[test]
fn a_paralysed_garrison_empties_and_a_transport_does_not() {
    let mut left = Vec::new();
    for container in ["Keep", "PassBoat"] {
        let walkers = [
            ("Walker", Vec2::new(TARGET_AT.x + 40.0, TARGET_AT.y)),
            ("Walker", Vec2::new(TARGET_AT.x + 40.0, TARGET_AT.y + 20.0)),
        ];
        let mut others = vec![(container, TARGET_AT)];
        if container == "Keep" {
            others.extend(walkers);
        }
        let (mut simulation, ids, e) = scene("Paralyser", &others, 0.0);
        let riders: Vec<Entity> = if container == "Keep" {
            simulation
                .submit(
                    OTHER,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: e[2..].to_vec(),
                    },
                )
                .unwrap();
            simulation
                .submit(OTHER, RotwkCommand::Enter { target: e[1] })
                .unwrap();
            run(&mut simulation, 240);
            e[2..].to_vec()
        } else {
            run(&mut simulation, 1);
            simulation
                .state()
                .get(ids.passengers, e[1])
                .unwrap()
                .0
                .clone()
        };
        assert!(
            riders
                .iter()
                .all(|rider| simulation.state().contains(ids.contained, *rider)),
            "{container} never took its riders"
        );
        attack(&mut simulation, e[0], e[1]);
        run(&mut simulation, 90);
        left.push(
            riders
                .iter()
                .filter(|rider| !simulation.state().contains(ids.contained, **rider))
                .count(),
        );
    }
    assert_eq!(left, [2, 0]);
}

#[test]
fn a_paralysed_unit_does_not_auto_heal_while_the_window_is_open() {
    let mut gained = Vec::new();
    for paralysed in [false, true] {
        let (mut simulation, ids, e) = scene("Paralyser", &[("Mender", TARGET_AT)], 0.0);
        if paralysed {
            attack(&mut simulation, e[0], e[1]);
            let mut seen = false;
            for _ in 0..90 {
                simulation.tick().unwrap();
                if disabled(&simulation, ids, e[1]) {
                    seen = true;
                    break;
                }
            }
            assert!(seen, "the nugget never paralysed it");
        }
        let before = health(&simulation, ids, e[1]);
        run(&mut simulation, 40);
        gained.push(health(&simulation, ids, e[1]) - before);
    }
    assert!(gained[0] > 0.0, "the control never healed: {gained:?}");
    assert_eq!(gained[1], 0.0);
}

#[test]
fn a_body_paralysed_in_flight_hangs_until_the_window_ends() {
    let (mut simulation, ids, e) = scene("ParaSlammer", &[("Dummy", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    let mut seen = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if disabled(&simulation, ids, e[1]) {
            seen = true;
            break;
        }
    }
    assert!(seen, "the nugget never paralysed it");
    run(&mut simulation, 4);
    let lift = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.ground_offset, e[1])
            .map_or(0.0, |offset| offset.0)
    };
    let held = (position(&simulation, ids, e[1]), lift(&simulation));
    for _ in 0..40 {
        simulation.tick().unwrap();
        assert!(disabled(&simulation, ids, e[1]), "window ended early");
        let now = (position(&simulation, ids, e[1]), lift(&simulation));
        assert_eq!(now, held, "it flew while held");
    }
    let mut moved = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        moved |= (position(&simulation, ids, e[1]), lift(&simulation)) != held;
        if !thrown(&simulation, ids, e[1]) {
            break;
        }
    }
    assert!(moved, "it never flew after the window");
    assert!(!thrown(&simulation, ids, e[1]), "it never landed");
}

#[test]
fn entering_the_damaged_rung_holds_the_object_busy_for_the_transition_time() {
    let (mut simulation, ids, e) = scene("Pounder", &[("Brittle", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    let mut opened = None;
    let mut closed = None;
    for frame in 0..200_u32 {
        simulation.tick().unwrap();
        let busy = disabled(&simulation, ids, e[1]);
        if busy && opened.is_none() {
            opened = Some(frame);
        }
        if !busy && opened.is_some() && closed.is_none() {
            closed = Some(frame);
        }
    }
    let (opened, closed) = (opened.unwrap(), closed.unwrap());
    assert!((29..=31).contains(&(closed - opened)), "{opened}..{closed}");
}

#[test]
fn a_hit_on_the_firer_itself_lands_a_frame_late() {
    let (mut simulation, ids, e) = scene(
        "SelfBlaster",
        &[("Dummy", Vec2::new(CASTER_AT.x + 10.0, CASTER_AT.y))],
        0.0,
    );
    attack(&mut simulation, e[0], e[1]);
    let (mut target_hit, mut firer_hit) = (None, None);
    for frame in 0..90_u32 {
        simulation.tick().unwrap();
        if target_hit.is_none() && health(&simulation, ids, e[1]) < 100.0 {
            target_hit = Some(frame);
        }
        if firer_hit.is_none() && health(&simulation, ids, e[0]) < 500.0 {
            firer_hit = Some(frame);
        }
    }
    let gap = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
    let target_hit = target_hit.expect("the target was never hit");
    let firer_hit = firer_hit.expect("the firer was never hit");
    assert_eq!(firer_hit, target_hit + gap.max(1));
}

#[test]
fn a_no_victim_needed_wind_up_lands_where_the_victim_began_it() {
    let (mut simulation, ids, e) = scene(
        "Bombarder",
        &[
            ("Walker", TARGET_AT),
            ("Dummy", Vec2::new(TARGET_AT.x, TARGET_AT.y + 3.0)),
        ],
        0.0,
    );
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![e[1]],
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
    attack(&mut simulation, e[0], e[1]);
    run(&mut simulation, 120);
    assert!(
        position(&simulation, ids, e[1]).x > TARGET_AT.x + 30.0,
        "the victim never walked off"
    );
    assert!(
        health(&simulation, ids, e[2]) < 100.0,
        "nothing hit the spot"
    );
    assert_eq!(health(&simulation, ids, e[1]), 100.0);
}

#[test]
fn a_death_weapon_waits_its_delay_and_goes_off_at_its_offset() {
    let north = Facing::from_vector(Vec2::new(0.0, 1.0)).unwrap();
    let ahead = Vec2::new(CASTER_AT.x, CASTER_AT.y + 20.0);
    let beside = Vec2::new(CASTER_AT.x + 20.0, CASTER_AT.y);
    let (mut simulation, ids, e) =
        scene_facing("Bomb", &[("Dummy", ahead), ("Dummy", beside)], 0.0, north);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![e[0]],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    let mut struck = None;
    for frame in 0..120_u32 {
        simulation.tick().unwrap();
        if struck.is_none() && health(&simulation, ids, e[1]) < 100.0 {
            struck = Some(frame);
        }
    }
    let struck = struck.expect("the death weapon never went off");
    assert!(
        struck >= 28,
        "fired after {struck} frames, not after its DelayTime"
    );
    assert_eq!(
        health(&simulation, ids, e[2]),
        100.0,
        "it went off beside the object"
    );
}

#[test]
fn an_immobile_victim_with_physics_is_thrown_by_a_meta_impact() {
    let (mut simulation, ids, e) = scene("Slammer", &[("Mooring", TARGET_AT)], 0.0);
    attack(&mut simulation, e[0], e[1]);
    assert!(ever_thrown(&mut simulation, ids, e[1]));
}

fn gate_open(simulation: &Simulation<RotwkRules>, ids: Ids, gate: Entity) -> bool {
    simulation
        .state()
        .get(ids.gate_cycle, gate)
        .is_some_and(crate::rules::gates::GateCycle::fully_open)
}

fn open_after_a_hit(computer: Option<PlayerId>) -> (bool, bool) {
    let (mut simulation, ids, e) = scene_for(
        "KeyHolder",
        &[("ZGate", TARGET_AT)],
        0.0,
        Facing::EAST,
        computer,
    );
    attack(&mut simulation, e[0], e[1]);
    let mut opened = false;
    for _ in 0..120 {
        simulation.tick().unwrap();
        if gate_open(&simulation, ids, e[1]) {
            opened = true;
            break;
        }
    }
    run(&mut simulation, 120);
    (opened, gate_open(&simulation, ids, e[1]))
}

#[test]
fn a_gate_opened_by_a_hit_stays_open_unless_a_skirmish_ai_box_shuts_it() {
    assert_eq!(open_after_a_hit(None), (true, true));
    assert_eq!(open_after_a_hit(Some(OTHER)), (true, false));
}

#[test]
fn a_frozen_gate_moves_and_a_paralysed_one_does_not() {
    let mut moved = Vec::new();
    for caster in ["Freezer", "Paralyser"] {
        let (mut simulation, ids, e) = scene(caster, &[("ZGate", TARGET_AT)], 0.0);
        attack(&mut simulation, e[0], e[1]);
        let mut seen = false;
        for _ in 0..90 {
            simulation.tick().unwrap();
            if disabled(&simulation, ids, e[1]) {
                seen = true;
                break;
            }
        }
        assert!(seen, "the nugget never disabled the gate");
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![e[1]],
                },
            )
            .unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::GateCommand {
                    action: crate::rules::GateAction::Open,
                },
            )
            .unwrap();
        run(&mut simulation, 15);
        assert!(disabled(&simulation, ids, e[1]), "window ended early");
        moved.push(
            simulation
                .state()
                .get(ids.gate_cycle, e[1])
                .is_some_and(|cycle| !cycle.may_operate(false, false)),
        );
    }
    assert_eq!(moved, [true, false]);
}
