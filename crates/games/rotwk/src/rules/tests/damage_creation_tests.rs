use super::*;

const DATA: &str = "\
Weapon RockThrow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  FXTrigger = CATAPULT_ROCK\n  \
  ProjectileNugget\n    WarheadTemplateName = RockHead\n  End\nEnd\n\
Weapon PlainThrow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  \
  ProjectileNugget\n    WarheadTemplateName = RockHead\n  End\nEnd\n\
Weapon RockHead\n  DamageNugget\n    Damage = 100\n    DamageType = CRUSH\n  End\nEnd\n\
ObjectCreationList OCL_Bits\n  CreateObject\n    ObjectNames = DebrisBit\n    Count = 2\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList OCL_Front\n  CreateObject\n    ObjectNames = FrontChunk\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList OCL_Back\n  CreateObject\n    ObjectNames = BackChunk\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
Object Thrower\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY RockThrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainThrower\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PlainThrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Hut\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    MaxHealthDamaged = 950\n    \
    MaxHealthReallyDamaged = 850\n    \
    DamageCreationList = OCL_Bits CATAPULT_ROCK\n    \
    DamageCreationList = OCL_Front CATAPULT_ROCK FRONT_DESTROYED\n    \
    DamageCreationList = OCL_Back CATAPULT_ROCK BACK_DESTROYED\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
ObjectCreationList OCL_Fly\n  CreateObject\n    ObjectNames = FlyingBit\n    \
  Offset = X:0 Y:0 Z:30\n    Disposition = SEND_IT_FLYING LIKE_EXISTING\n    \
  DispositionIntensity = 60\n  End\nEnd\n\
Object Shed\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    \
    DamageCreationList = OCL_Fly CATAPULT_ROCK\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Object FlyingBit\n  KindOf = NO_COLLIDE INERT\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n    AllowBouncing = Yes\n  End\nEnd\n\
Object DebrisBit\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n\
Object FrontChunk\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n\
Object BackChunk\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n";

fn scene(thrower: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Arc<KindTable>) {
    scene_at(thrower, "Hut")
}

fn scene_at(
    thrower: &str,
    target: &str,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Arc<KindTable>) {
    scene_facing(thrower, target, Facing::EAST)
}

fn scene_facing(
    thrower: &str,
    target: &str,
    target_facing: Facing,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Arc<KindTable>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [thrower, target].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        (thrower, Vec2::new(300.0, 300.0), PLAYER, Facing::EAST),
        (target, Vec2::new(500.0, 300.0), OTHER, target_facing),
    ]
    .into_iter()
    .map(|(name, at, player, facing)| {
        kinds
            .spawn(&data.combat, kinds.kind(name).unwrap(), at, facing, player)
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_creation_lists(Arc::clone(&data.ocls))
        .with_gravity(Some(-64.0))
        .with_players(2);
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (me, hut) = (entities[0], entities[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![me],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: hut })
        .unwrap();
    (simulation, ids, me, hut, kinds)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn count(simulation: &Simulation<RotwkRules>, ids: Ids, kinds: &KindTable, name: &str) -> usize {
    let kind = kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, held)| **held == kind)
        .count()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn landings(simulation: &Simulation<RotwkRules>, ids: Ids, hut: Entity) -> usize {
    let health = simulation.state().get(ids.health, hut).unwrap().0.current();
    ((1000.0 - health) / 100.0).round() as usize
}

#[test]
fn every_triggered_landing_throws_the_bodys_per_hit_list() {
    let (mut simulation, ids, _, hut, kinds) = scene("Thrower");
    run(&mut simulation, 120);
    let landed = landings(&simulation, ids, hut);
    assert!(landed >= 3, "only {landed} rock(s) landed");
    assert_eq!(count(&simulation, ids, &kinds, "DebrisBit"), 2 * landed);
}

#[test]
fn a_damage_state_list_makes_its_pieces_face_the_damaged_body() {
    let north = Facing::from_radians(core::f32::consts::FRAC_PI_2);
    let (mut simulation, ids, _, _, kinds) = scene_facing("Thrower", "Hut", north);
    run(&mut simulation, 120);
    let bit = kinds.kind("DebrisBit").unwrap();
    let world = simulation.state();
    let faces: Vec<f32> = world
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, held)| **held == bit)
        .filter_map(|(entity, _)| world.get(ids.body, entity))
        .map(|body| body.0.facing.radians())
        .collect();
    assert!(!faces.is_empty(), "no piece was made");
    assert!(
        faces.iter().all(|f| (f - north.radians()).abs() < 1e-3),
        "{faces:?}"
    );
}

#[test]
fn a_launcher_with_no_trigger_throws_nothing() {
    let (mut simulation, ids, _, hut, kinds) = scene("PlainThrower");
    run(&mut simulation, 120);
    assert!(landings(&simulation, ids, hut) >= 3);
    let thrown =
        ["DebrisBit", "FrontChunk", "BackChunk"].map(|name| count(&simulation, ids, &kinds, name));
    assert_eq!(thrown, [0, 0, 0]);
}

#[test]
fn the_struck_side_comes_off_once_on_the_first_rung_drop() {
    let (mut simulation, ids, _, hut, kinds) = scene("Thrower");
    run(&mut simulation, 120);
    assert!(landings(&simulation, ids, hut) >= 3);
    let chunks = ["BackChunk", "FrontChunk"].map(|name| count(&simulation, ids, &kinds, name));
    assert_eq!(chunks, [1, 0]);
}

#[test]
fn the_body_wears_the_struck_sides_destroyed_condition() {
    let (mut simulation, _, _, hut, _) = scene("Thrower");
    run(&mut simulation, 120);
    let state = simulation.rules().upgrade_state_id().unwrap();
    let held = &simulation.state().get(state, hut).unwrap().model_conditions;
    assert!(
        held.iter().any(|name| &**name == "DESTROYED_BACK"),
        "{held:?}"
    );
}

#[test]
fn a_flying_bit_is_thrown_from_its_offset_height_and_comes_down_on_the_ground() {
    let (mut simulation, ids, _, _, kinds) = scene_at("Thrower", "Shed");
    let bit = kinds.kind("FlyingBit").unwrap();
    let debris = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .store(ids.flight)
            .unwrap()
            .iter()
            .find(|(_, flight)| flight.warhead.is_none())
            .map(|(_, flight)| *flight)
    };
    let mut thrown = None;
    for _ in 0..120 {
        simulation.tick().unwrap();
        thrown = debris(&simulation);
        if thrown.is_some() {
            break;
        }
    }
    let flight = thrown.expect("no bit was thrown");
    let carried = flight.carried.unwrap();
    assert_eq!(simulation.state().get(ids.kind, carried), Some(&bit));
    let (from, to) = (flight.flight.start(), flight.flight.end());
    assert_eq!(from.z, 30.0, "thrown from its Offset height");
    assert_eq!(to.z, 0.0, "it comes down on the ground");
    assert!(flight.flight.frames() > 1);
    run(&mut simulation, u32::from(flight.flight.frames()) + 300);
    let rest = simulation
        .state()
        .get(ids.body, carried)
        .unwrap()
        .0
        .position;
    let reach = |x: f32, y: f32| (x - from.x) * (x - from.x) + (y - from.y) * (y - from.y);
    assert!(
        reach(rest.x, rest.y) >= reach(to.x, to.y),
        "it settles at or past where it came down, away from the hit"
    );
    assert!(!simulation.state().contains(ids.ground_offset, carried));
}
