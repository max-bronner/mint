use super::*;

const DATA: &str = "\
Locomotor RiderLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon HoofStrike\n  AttackRange = 10\n  \
  DamageNugget\n    Damage = 30\n    DamageType = CRUSH\n  End\nEnd\n\
Weapon Hoof\n  AttackRange = 10\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object SlowAttacker\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  VisionRange = 400\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  MinCrushVelocityPercent = 150%\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hoof\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object UnarmedBlocker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  \
  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SquishCollide Tag_Squish\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object FarTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object HiddenBlocker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  \
  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SquishCollide Tag_Squish\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Camo\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n    End\n    \
    UpdatePeriod = 1\n  StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StealthedAttacker\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  VisionRange = 400\n  \
  CrusherLevel = 1\n  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  \
  MinCrushVelocityPercent = 150%\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hoof\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    \
    AutoAcquireEnemiesWhenIdle = Yes STEALTHED\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Camo\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n    End\n    \
    UpdatePeriod = 1\n  StartsActive = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n";

fn blocked_encounter_between(attacker: &str, blocker: &str) -> (bool, bool) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [attacker, blocker, "FarTarget"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [
        (attacker, 300.0, PLAYER),
        (blocker, 340.0, OTHER),
        ("FarTarget", 420.0, OTHER),
    ]
    .iter()
    .map(|(name, x, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(*x, 300.0),
                Facing::EAST,
                *owner,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec!["Creeps".to_owned()],
            vec![crate::alliance::MapRelations {
                universal_hostile: true,
                ..crate::alliance::MapRelations::default()
            }],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let targets = |entity: Entity, other: Entity| {
        simulation
            .state()
            .get(ids.attack_order, entity)
            .is_some_and(|order| order.target == other)
    };
    (
        targets(entities[0], entities[1]),
        targets(entities[1], entities[0]),
    )
}

#[test]
fn a_too_slow_crusher_attacks_its_blocker() {
    let (attacker_targets_blocker, _) = blocked_encounter_between("SlowAttacker", "UnarmedBlocker");
    assert!(
        attacker_targets_blocker,
        "the crusher never attacked its blocker"
    );
}

#[test]
fn a_crusher_never_attacks_a_blocker_it_cannot_see() {
    let (attacker_targets_blocker, _) = blocked_encounter_between("SlowAttacker", "HiddenBlocker");
    assert!(
        !attacker_targets_blocker,
        "the crusher attacked a blocker it could not see"
    );
}

#[test]
fn a_stealthed_crusher_never_attacks_its_blocker() {
    let (attacker_targets_blocker, _) =
        blocked_encounter_between("StealthedAttacker", "UnarmedBlocker");
    assert!(
        !attacker_targets_blocker,
        "the stealthed crusher attacked its blocker"
    );
}

mod horde_release {
    use super::*;

    const DATA: &str = "\
Locomotor RiderLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon Hoof\n  AttackRange = 10\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Weapon MemberBlade\n  AttackRange = 30\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object SlowAttacker\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  VisionRange = 400\n  \
  CrusherLevel = 1\n  CrushableLevel = 2\n  MinCrushVelocityPercent = 150%\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hoof\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object FarTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Decoy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object HordeMember\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  \
  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY MemberBlade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object HordeMemberHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  \
  BuildTime = 1\n  CommandPoints = 1\n  \
  Behavior = HordeContain Tag_01\n    Slots = 1\n    \
    RankInfo = RankNumber:1 UnitType:HordeMember Position:X:0 Y:0\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_HordeMemberHorde\nEnd\n\
CommandButton Build_HordeMemberHorde\n  Command = UNIT_BUILD\n  Object = HordeMemberHorde\nEnd\n";

    const HORDE_AT: Vec2 = Vec2::new(340.0, 300.0);
    const ATTACKER_AT: Vec2 = Vec2::new(300.0, 300.0);
    const FAR_TARGET_AT: Vec2 = Vec2::new(420.0, 300.0);
    const DECOY_AT: Vec2 = Vec2::new(340.0, 325.0);

    fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity) {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            DATA,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let (data, _) =
            crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
        let seeds: Vec<String> = ["SlowAttacker", "FarTarget", "Decoy", "Camp"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let (kinds, missing) = KindTable::closure(&data, &seeds);
        assert!(missing.is_empty(), "missing from the closure: {missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let spawns = [
            ("SlowAttacker", ATTACKER_AT, PLAYER),
            ("FarTarget", FAR_TARGET_AT, OTHER),
            ("Decoy", DECOY_AT, PLAYER),
            ("Camp", Vec2::new(340.0, 500.0), OTHER),
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
            .with_hordes(data.formations.tables().to_vec(), Vec::new())
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_economy(production.buildables.clone(), 100_000)
            .with_menus(production.menus.clone())
            .with_teams(vec![None, None])
            .with_map_players(
                2,
                vec!["Creeps".to_owned()],
                vec![crate::alliance::MapRelations {
                    universal_hostile: true,
                    ..crate::alliance::MapRelations::default()
                }],
            );
        let mut simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        simulation.tick().unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetMovementModes(Modes {
                    formation: crate::horde::FormationMode::Off,
                    ..Modes::default()
                }),
            )
            .unwrap();
        let recipe = simulation
            .rules()
            .buildable_catalogue()
            .iter()
            .find(|entry| entry.label == "HordeMemberHorde")
            .unwrap()
            .index;
        simulation
            .submit(
                OTHER,
                RotwkCommand::DebugSpawnHorde {
                    recipe,
                    x: HORDE_AT.x,
                    y: HORDE_AT.y,
                    facing: 0.0,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
        let attacker = simulation
            .state()
            .store(ids.armed)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .find(|entity| simulation.state().get(ids.owner, *entity).unwrap().0 == PLAYER)
            .unwrap();
        let decoy = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .find(|entity| {
                simulation.state().get(ids.owner, *entity).unwrap().0 == PLAYER
                    && !simulation.state().contains(ids.armed, *entity)
            })
            .unwrap();
        let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .clone();
        assert_eq!(members.len(), 1, "the fixture's premise");
        let member = members[0];
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![member],
                },
            )
            .unwrap();
        simulation
            .submit(OTHER, RotwkCommand::AttackObject { target: decoy })
            .unwrap();
        (simulation, ids, attacker, member, horde)
    }

    #[test]
    fn a_busy_member_is_force_idled_and_released_to_swing_at_its_blocker() {
        let (mut simulation, ids, attacker, member, horde) = scene();
        let mut member_was_busy = false;
        let mut member_struck_back = false;
        for _ in 0..3 * SIMULATION_HZ {
            simulation.tick().unwrap();
            if !member_was_busy {
                member_was_busy = simulation
                    .state()
                    .get(ids.attack_order, member)
                    .is_some_and(|order| order.target != attacker);
            }
            if simulation
                .state()
                .get(ids.attack_order, horde)
                .is_some_and(|order| order.target == attacker)
            {
                member_struck_back = true;
                break;
            }
        }
        assert!(member_was_busy, "the member never took its own decoy order");
        assert!(
            member_struck_back,
            "the busy member was never released to swing at its blocker"
        );
    }
}
