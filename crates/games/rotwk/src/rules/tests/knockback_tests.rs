use super::*;
use crate::rules::knockback::{inside_arc, taper};

const DATA: &str = "\
GameData\n  Gravity = -64.0\nEnd\n\
ModifierList ZStandFast\n  Category = LEADERSHIP\n  \
  Modifier = RESIST_KNOCKBACK 100%\n  Duration = 0\nEnd\n\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon ZBlast\n  AttackRange = 300\n  DelayBetweenShots = 5000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = MAGIC\n    Radius = 200\n    DamageArc = 45\n  End\n  \
  MetaImpactNugget\n    ShockWaveAmount = 60\n    ShockWaveRadius = 200\n    \
    ShockWaveTaperOff = 1.0\n    ShockWaveZMult = 1.0\n  End\nEnd\n\
Object ZBlaster\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBlast\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZPost\n  Side = Men\n  KindOf = SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 9000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZPeon\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n    ShockStunnedTimeLow = 1000\n    \
    ShockStunnedTimeHigh = 1000\n    ShockStandingTime = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZWard ZPeon\nEnd\n\
ChildObject ZBehind ZPeon\nEnd\n\
ChildObject ZPinned ZPeon\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 0\n  End\nEnd\n\
Object ZStatue\n  Side = Men\n  KindOf = SELECTABLE IMMOBILE STRUCTURE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
    BonusName = ZStandFast\n    RefreshDelay = 1000\n    Range = 60\n    \
    ObjectFilter = ANY +INFANTRY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object ZBrick\n  Side = Men\n  KindOf = SELECTABLE\n  \
  ShockwaveResistance = 9999\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZChip\n  Side = Men\n  KindOf = SELECTABLE\n  \
  ShockwaveResistance = 20\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const BLASTER_AT: Vec2 = Vec2::new(300.0, 300.0);
const POST_AT: Vec2 = Vec2::new(400.0, 300.0);
const PEON_AT: Vec2 = Vec2::new(460.0, 300.0);
const WARD_AT: Vec2 = Vec2::new(540.0, 300.0);
const STATUE_AT: Vec2 = Vec2::new(580.0, 300.0);
const BEHIND_AT: Vec2 = Vec2::new(210.0, 300.0);
const PINNED_AT: Vec2 = Vec2::new(300.0, 700.0);
const BRICK_AT: Vec2 = Vec2::new(460.0, 340.0);
const CHIP_AT: Vec2 = Vec2::new(460.0, 260.0);

fn scene() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [
        "ZBlaster", "ZPost", "ZPeon", "ZWard", "ZStatue", "ZBehind", "ZPinned", "ZBrick", "ZChip",
    ];
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
    let spawns = [
        ("ZBlaster", BLASTER_AT, OTHER),
        ("ZPost", POST_AT, PLAYER),
        ("ZPeon", PEON_AT, PLAYER),
        ("ZWard", WARD_AT, PLAYER),
        ("ZStatue", STATUE_AT, PLAYER),
        ("ZBehind", BEHIND_AT, PLAYER),
        ("ZPinned", PINNED_AT, PLAYER),
        ("ZBrick", BRICK_AT, PLAYER),
        ("ZChip", CHIP_AT, PLAYER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            ["Neutral", "Ally", "Creeps"].map(str::to_owned).to_vec(),
            vec![
                crate::alliance::MapRelations::default(),
                crate::alliance::MapRelations::default(),
            ],
        );
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

fn throw_and_land(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    entity: Entity,
    frames: u32,
) -> (bool, Vec2) {
    let mut airborne = false;
    for _ in 0..frames {
        simulation.tick().unwrap();
        airborne |= simulation.state().get(ids.knocked, entity).is_some();
        if airborne && simulation.state().get(ids.knocked, entity).is_none() {
            break;
        }
    }
    let at = simulation.state().get(ids.body, entity).unwrap().0.position;
    (airborne, at)
}

#[test]
fn a_shock_wave_throws_a_unit_away_from_the_blast_and_lands_it() {
    let (mut simulation, ids, entities) = scene();
    let peon = entities[2];
    let (airborne, landed) = throw_and_land(&mut simulation, ids, peon, 240);
    assert!(airborne, "the peon was never thrown");
    assert!(
        landed.x > PEON_AT.x + 50.0,
        "thrown away from the blast: {landed:?}"
    );
    assert!(
        simulation.state().get(ids.ground_offset, peon).is_none(),
        "the landing leaves no height behind"
    );
}

#[test]
fn a_resist_knockback_leadership_keeps_a_unit_on_its_feet() {
    let (mut simulation, ids, entities) = scene();
    let ward = entities[3];
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .rules()
            .bonuses_of(simulation.state(), ids, ward)
            .resist_knockback,
        1.0,
        "the statue's leadership reached it"
    );
    let (airborne, _) = throw_and_land(&mut simulation, ids, ward, 200);
    assert!(!airborne, "a full resistance is never thrown");
}

#[test]
fn a_shockwave_resistance_of_9999_never_moves() {
    let (mut simulation, ids, entities) = scene();
    let brick = entities[7];
    let (airborne, landed) = throw_and_land(&mut simulation, ids, brick, 60);
    assert!(!airborne, "a 9999 resistance was thrown");
    assert!(
        (landed.x - BRICK_AT.x).abs() < 1e-3 && (landed.y - BRICK_AT.y).abs() < 1e-3,
        "it moved without ever leaving the ground: {BRICK_AT:?} -> {landed:?}"
    );
}

#[test]
fn a_shockwave_resistance_under_the_shot_amount_still_moves() {
    let (mut simulation, ids, entities) = scene();
    let chip = entities[8];
    let (airborne, _) = throw_and_land(&mut simulation, ids, chip, 240);
    assert!(airborne, "a resistance below the amount was never thrown");
}

#[test]
fn shock_wave_taper_off_is_the_share_left_at_the_rim() {
    assert_eq!(taper(0.0, 100.0, 0.25), 1.0);
    assert_eq!(taper(100.0, 100.0, 0.25), 0.25);
    assert_eq!(taper(50.0, 100.0, 0.0), 0.5);
    assert_eq!(taper(100.0, 100.0, 1.0), 1.0);
    assert_eq!(taper(400.0, 100.0, 0.0), 0.0, "clamped past the rim");
}

#[test]
fn shock_wave_arc_is_measured_to_each_side_of_the_facing() {
    let east = Vec2::new(1.0, 0.0);
    let off_by = |degrees: f32| {
        let (sin, cos) = mint_math::f32::sin_cos(degrees.to_radians());
        Vec2::new(cos, sin)
    };
    assert!(inside_arc(east, off_by(40.0), Some(45.0), false));
    assert!(!inside_arc(east, off_by(60.0), Some(45.0), false));
    assert!(inside_arc(east, off_by(179.0), None, false));
    assert!(
        inside_arc(east, off_by(60.0), Some(45.0), true),
        "ShockWaveArcInverted names what is spared"
    );
}

#[test]
fn a_damage_arc_spares_what_stands_behind_the_shooter() {
    let (mut simulation, ids, entities) = scene();
    let (peon, behind) = (entities[2], entities[5]);
    let full = health_of(&simulation, ids, behind);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        health_of(&simulation, ids, peon) < full,
        "the bystander in front was hit"
    );
    assert_eq!(
        health_of(&simulation, ids, behind),
        full,
        "the bystander behind the shooter is outside the 45 degree arc"
    );
}

#[test]
fn a_landed_object_lies_stunned_for_its_own_shock_times_before_it_takes_an_order() {
    let (mut simulation, ids, entities) = scene();
    let peon = entities[2];
    let mut landed: Option<u64> = None;
    let mut up: Option<u64> = None;
    for _ in 0..240 {
        simulation.tick().unwrap();
        let frame = simulation.frame().get();
        let phase = simulation.state().get(ids.knocked, peon).map(|k| k.phase);
        if landed.is_none() && phase == Some(crate::rules::knockback::Phase::Stunned) {
            landed = Some(frame);
        }
        if landed.is_some() && phase.is_none() {
            up = Some(frame);
            break;
        }
    }
    let (landed, up) = (landed.expect("never landed"), up.expect("never got up"));
    assert_eq!(up - landed, 48, "landed on {landed}, up on {up}");
}

#[test]
fn a_stunned_object_takes_no_move_order_until_it_is_back_on_its_feet() {
    let (mut simulation, ids, entities) = scene();
    let peon = entities[2];
    for _ in 0..240 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.knocked, peon)
            .is_some_and(|knocked| knocked.phase == crate::rules::knockback::Phase::Stunned)
        {
            break;
        }
    }
    let down_at = position(&simulation, ids, peon);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![peon],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 700.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let still = position(&simulation, ids, peon);
    assert!(
        simulation.state().get(ids.path, peon).is_none(),
        "a route was granted to an object still on the ground"
    );
    assert!(
        (still.x - down_at.x).abs() < 1e-3 && (still.y - down_at.y).abs() < 1e-3,
        "it walked off while stunned: {down_at:?} -> {still:?}"
    );
}

#[test]
fn a_locomotor_set_of_zero_speed_refuses_a_move_order() {
    let (mut simulation, ids, entities) = scene();
    let pinned = entities[6];
    let start = position(&simulation, ids, pinned);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![pinned],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 700.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let now = position(&simulation, ids, pinned);
    assert!(
        (now.x - start.x).abs() < 1e-3 && (now.y - start.y).abs() < 1e-3,
        "a pinned object was moved: {start:?} -> {now:?}"
    );
}

mod kill_object_filter {
    use super::*;

    const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon ZKillBlast\n  AttackRange = 300\n  DelayBetweenShots = 5000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = MAGIC\n    Radius = 200\n  End\n  \
  MetaImpactNugget\n    ShockWaveAmount = 60\n    ShockWaveRadius = 200\n    \
    ShockWaveTaperOff = 1.0\n    ShockWaveZMult = 1.0\n    KillObjectFilter = ANY +ZFragile\n  \
  End\nEnd\n\
Object ZKillBlaster\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZKillBlast\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFragile\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSturdy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

    const BLASTER_AT: Vec2 = Vec2::new(300.0, 300.0);
    const FRAGILE_AT: Vec2 = Vec2::new(400.0, 300.0);
    const STURDY_AT: Vec2 = Vec2::new(460.0, 300.0);

    fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            DATA,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let (data, _) =
            crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
        let roots = ["ZKillBlaster", "ZFragile", "ZSturdy"].map(str::to_owned);
        let (kinds, missing) = KindTable::closure(&data, &roots);
        assert!(missing.is_empty(), "{missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let spawns = [
            ("ZKillBlaster", BLASTER_AT, OTHER),
            ("ZFragile", FRAGILE_AT, PLAYER),
            ("ZSturdy", STURDY_AT, PLAYER),
        ]
        .into_iter()
        .map(|(name, at, player)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    player,
                )
                .unwrap()
        })
        .collect();
        let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
        let rules = RotwkRules::new(passability, spawns)
            .with_combat(data.combat.clone())
            .with_gravity(data.gravity)
            .with_construction(Arc::clone(&kinds), production)
            .with_teams(vec![None, None]);
        let mut simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let entities = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .to_vec();
        simulation.tick().unwrap();
        (simulation, ids, entities[1], entities[2])
    }

    #[test]
    fn a_matching_victim_dies_outright_instead_of_being_thrown() {
        let (mut simulation, ids, fragile, _) = scene();
        let mut died = false;
        for _ in 0..120 {
            simulation.tick().unwrap();
            if simulation
                .state()
                .get(ids.health, fragile)
                .is_some_and(|health| health.0.is_dead())
            {
                died = true;
                break;
            }
        }
        assert!(died, "the filter's own target was never killed");
        assert!(
            simulation.state().get(ids.knocked, fragile).is_none(),
            "a killed victim is never also thrown"
        );
    }

    #[test]
    fn a_victim_the_filter_does_not_name_is_thrown_as_usual() {
        let (mut simulation, ids, _, sturdy) = scene();
        let mut airborne = false;
        for _ in 0..120 {
            simulation.tick().unwrap();
            airborne |= simulation.state().get(ids.knocked, sturdy).is_some();
        }
        assert!(airborne, "a victim outside the filter should be thrown");
        assert!(
            simulation
                .state()
                .get(ids.health, sturdy)
                .is_some_and(|health| !health.0.is_dead()),
            "the filter must not kill what it does not name"
        );
    }
}

#[test]
fn a_shockwave_sounds_the_impact_of_what_it_reaches_not_its_death() {
    let (mut simulation, _, entities) = scene();
    let (post, peon, ward) = (entities[1], entities[2], entities[3]);
    let mut impacts = Vec::new();
    for _ in 0..240 {
        simulation.tick().unwrap();
        let frame = Frame::new(simulation.frame().get() - 1);
        impacts.extend(
            simulation
                .rules()
                .voice_events(simulation.state(), frame)
                .iter()
                .filter(|event| event.kind == crate::voice::VoiceKind::Impact)
                .map(|event| event.entity),
        );
    }
    assert!(
        impacts.contains(&peon),
        "the thrown peon's impact: {impacts:?}"
    );
    assert!(
        impacts.contains(&ward),
        "the impact sounds before RESIST_KNOCKBACK keeps the ward standing: {impacts:?}"
    );
    assert!(
        !impacts.contains(&post),
        "an immobile post has nothing to throw"
    );
}
