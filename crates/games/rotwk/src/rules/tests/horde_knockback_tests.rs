use super::*;
use crate::rules::knockback::Knocked;

const DATA: &str = "\
GameData\n  Gravity = -64.0\nEnd\n\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon ZBlast\n  AttackRange = 300\n  DelayBetweenShots = 1000000\n  \
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
Object ZPeon\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n    ShockStunnedTimeLow = 3000\n    \
    ShockStunnedTimeHigh = 3000\n    ShockStandingTime = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZPeonHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 10\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:ZPeon Position:X:-150 Y:0 Position:X:150 Y:0 \
      Position:X:0 Y:300\n  End\nEnd\n\
Object ZCamp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = ZCampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet ZCampSet\n  1 = Build_ZPeonHorde\nEnd\n\
CommandButton Build_ZPeonHorde\n  Command = UNIT_BUILD\n  Object = ZPeonHorde\nEnd\n";

const BLASTER_AT: Vec2 = Vec2::new(300.0, 300.0);
const POST_AT: Vec2 = Vec2::new(330.0, 300.0);
const HORDE_AT: Vec2 = Vec2::new(500.0, 300.0);

fn scene() -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["ZBlaster", "ZPost", "ZCamp"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [("ZBlaster", BLASTER_AT, OTHER), ("ZPost", POST_AT, PLAYER)]
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
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
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
    simulation.tick().unwrap();
    (simulation, ids)
}

#[test]
fn a_thrown_horde_member_is_not_steered_by_the_formation_pass_while_off_its_feet() {
    let (mut simulation, ids) = scene();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "ZPeonHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: HORDE_AT.x,
                y: HORDE_AT.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let members = store.get(store.entities()[0]).unwrap().members.clone();
    assert_eq!(members.len(), 3, "the fixture's premise");

    let mut thrown = None;
    let mut down_at = None;
    let mut stood_up_at = None;
    for tick in 0..300 {
        simulation.tick().unwrap();
        if thrown.is_none() {
            let hit: Vec<Entity> = members
                .iter()
                .copied()
                .filter(|member| simulation.state().get(ids.knocked, *member).is_some())
                .collect();
            if !hit.is_empty() {
                assert_eq!(hit.len(), 1, "more than one member was thrown: {hit:?}");
                thrown = Some(hit[0]);
            }
        }
        let Some(peon) = thrown else { continue };
        let knocked = simulation.state().get(ids.knocked, peon).copied();
        if down_at.is_none()
            && knocked
                .is_some_and(|knocked| knocked.phase == crate::rules::knockback::Phase::Stunned)
        {
            down_at = Some(position(&simulation, ids, peon));
        }
        if let Some(down_at) = down_at {
            if let Some(knocked) = knocked.filter(Knocked::off_its_feet) {
                let now = position(&simulation, ids, peon);
                assert!(
                    (now.x - down_at.x).abs() < 1e-3 && (now.y - down_at.y).abs() < 1e-3,
                    "tick {tick}: the formation pass dragged a member off its feet ({:?}): \
                     {down_at:?} -> {now:?}",
                    knocked.phase
                );
            } else if stood_up_at.is_none() {
                stood_up_at = Some((tick, position(&simulation, ids, peon)));
            }
        }
    }
    assert!(thrown.is_some(), "no member was ever thrown");
    assert!(down_at.is_some(), "the thrown member never landed stunned");
    let (stood_up_at, at_standing) =
        stood_up_at.expect("the thrown member never made it back onto its feet");
    let mut moved = false;
    for _ in 0..30 {
        simulation.tick().unwrap();
        if position(&simulation, ids, thrown.unwrap())
            .minus(at_standing)
            .length()
            > 1.0
        {
            moved = true;
            break;
        }
    }
    assert!(
        moved,
        "tick {stood_up_at}: the member never resumed formation steering once back on its feet"
    );
}
