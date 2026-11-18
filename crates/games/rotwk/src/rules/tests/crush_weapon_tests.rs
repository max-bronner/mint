use super::*;

const DATA: &str = "\
Locomotor RiderLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Locomotor GhostLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  NonDirtyTransform = Yes\nEnd\n\
Weapon HoofStrike\n  AttackRange = 10\n  \
  DamageNugget\n    Damage = 30\n    DamageType = CRUSH\n  End\nEnd\n\
Weapon SpikeBack\n  AttackRange = 10\n  \
  DamageNugget\n    Damage = 7\n    DamageType = SLASH\n  End\nEnd\n\
Weapon PikeWall\n  AttackRange = 10\n  \
  DamageNugget\n    Damage = 11\n    DamageType = SLASH\n  End\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object PlainRider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object GhostRider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = GhostLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object SlowRider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  MinCrushVelocityPercent = 150%\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object ChargeRider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  CrushWeapon = HoofStrike\n  CrushOnlyWhileCharging = Yes\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = RiderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object Pikeman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  CrushRevengeWeapon = SpikeBack\n  \
  Body = PorcupineFormationBodyModule Tag_01\n    MaxHealth = 1000\n    \
    CrusherLevelResisted = 1\n    CrushDamageWeaponTemplate = PikeWall\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  CrushRevengeWeapon = SpikeBack\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SquishCollide Tag_Squish\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Hoof\n  AttackRange = 10\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
ChildObject Trampler PlainRider\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hoof\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\nEnd\n\
ChildObject TamedTrampler Trampler\n  UseCrushAttack = No\nEnd\n\
ChildObject Stampeder Trampler\n  VisionRange = 20\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 5000\n  End\nEnd\n\
Object Rock\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn ride_through(crusher: &str, victim: &str) -> (f32, f32) {
    encounter(crusher, victim, 340.0, false)
}

fn encounter(crusher: &str, victim: &str, victim_x: f32, attack: bool) -> (f32, f32) {
    let (mut simulation, ids, entities) = scene(&[
        (crusher, Vec2::new(300.0, 300.0), PLAYER),
        (victim, Vec2::new(victim_x, 300.0), OTHER),
    ]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[0]],
            },
        )
        .unwrap();
    let order = if attack {
        RotwkCommand::AttackObject {
            target: entities[1],
        }
    } else {
        RotwkCommand::MoveTo {
            x: 420.0,
            y: 300.0,
            queue: false,
        }
    };
    simulation.submit(PLAYER, order).unwrap();
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let lost = |entity: Entity| {
        simulation
            .state()
            .get(ids.health, entity)
            .map_or(1000.0, |health| 1000.0 - health.0.current())
    };
    (lost(entities[0]), lost(entities[1]))
}

fn scene(units: &[(&str, Vec2, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Rider",
        "PlainRider",
        "GhostRider",
        "SlowRider",
        "ChargeRider",
        "Trampler",
        "TamedTrampler",
        "Stampeder",
        "Pikeman",
        "Spearman",
        "Rock",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = units
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
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

#[test]
fn a_crush_fires_the_crush_weapon_at_the_crushed_and_the_revenge_weapon_back() {
    let (rider_lost, spearman_lost) = ride_through("Rider", "Spearman");
    assert_eq!(spearman_lost, 30.0, "the crush weapon's damage, once");
    assert_eq!(rider_lost, 7.0, "the revenge weapon's damage, once");
}

#[test]
fn a_crusher_with_no_crush_weapon_kills_outright() {
    let (_, spearman_lost) = ride_through("PlainRider", "Spearman");
    assert_eq!(spearman_lost, 1000.0);
}

#[test]
fn a_crusher_does_nothing_to_what_it_cannot_crush() {
    assert_eq!(ride_through("Rider", "Rock"), (0.0, 0.0));
}

#[test]
fn a_crusher_too_slow_or_not_charging_crushes_nothing() {
    assert_eq!(ride_through("SlowRider", "Spearman"), (0.0, 0.0));
    assert_eq!(ride_through("ChargeRider", "Spearman"), (0.0, 0.0));
}

#[test]
fn a_crusher_kept_out_of_the_collision_manager_crushes_nothing() {
    assert_eq!(ride_through("GhostRider", "Spearman"), (0.0, 0.0));
}

#[test]
fn a_porcupine_body_is_crushed_like_any_other_resistance_never_fires() {
    assert_eq!(ride_through("Rider", "Pikeman"), (7.0, 30.0));
}

#[test]
fn a_melee_crusher_runs_its_victim_down_unless_use_crush_attack_is_off() {
    let (_, trampled) = encounter("Trampler", "Spearman", 450.0, true);
    assert_eq!(trampled, 1000.0, "the crush run did not kill");
    let (_, struck) = encounter("TamedTrampler", "Spearman", 450.0, true);
    assert!(
        struck < 100.0,
        "UseCrushAttack = No still crushed: lost {struck}"
    );
}

#[test]
fn a_crusher_whose_victim_dies_takes_the_next_one_at_once() {
    let (mut simulation, ids, entities) = scene(&[
        ("Stampeder", Vec2::new(300.0, 300.0), PLAYER),
        ("Spearman", Vec2::new(450.0, 300.0), OTHER),
        ("Spearman", Vec2::new(450.0, 345.0), OTHER),
    ]);
    let (crusher, first, second) = (entities[0], entities[1], entities[2]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![crusher],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: first })
        .unwrap();
    let mut died = None;
    for frame in 0..4 * SIMULATION_HZ {
        simulation.tick().unwrap();
        let dead = simulation
            .state()
            .get(ids.health, first)
            .is_none_or(|health| health.0.current() <= 0.0);
        if died.is_none() && dead {
            died = Some(frame);
        }
        let target = simulation.state().get(ids.armed, crusher).unwrap().target;
        if let (Some(at), Some(target)) = (died, target) {
            assert_eq!(target, second);
            assert!(
                frame <= at + 2,
                "re-targeted {} frames after the kill",
                frame - at
            );
            return;
        }
    }
    panic!("no re-target (victim died at {died:?})");
}
