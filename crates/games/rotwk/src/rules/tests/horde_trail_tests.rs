use super::*;

const DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  ZAxisBehavior = FLOATING_Z\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  MaxTurnWithoutReform = 45\nEnd\n\
Object Ghost\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = MemberFeet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Column\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = AODHordeContain Tag_Contain\n    Slots = 6\n    \
    RankInfo = RankNumber:1 UnitType:Ghost Position:X:0 Y:20 Position:X:0 Y:-20\n    \
    RankInfo = RankNumber:2 UnitType:Ghost Position:X:-60 Y:20 Position:X:-60 Y:-20\n    \
    RankInfo = RankNumber:3 UnitType:Ghost Position:X:-120 Y:20 Position:X:-120 Y:-20\n  \
  End\nEnd\n\
Object Swaying\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = AODHordeContain Tag_Contain\n    Slots = 1\n    FrequencyScale = 0.5\n    \
    AmplitudeScale = 10\n    StillAmplitude = 1\n    AmplitudeScaleZ = 6\n    \
    StillAmplitudeZ = 1\n    FrequencyScaleZ = 0.4\n    \
    RankInfo = RankNumber:1 UnitType:Ghost Position:X:0 Y:0\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Column\n  2 = Build_Swaying\nEnd\n\
CommandButton Build_Column\n  Command = UNIT_BUILD\n  Object = Column\nEnd\n\
CommandButton Build_Swaying\n  Command = UNIT_BUILD\n  Object = Swaying\nEnd\n\
Weapon Zap\n  AttackRange = 500\n  DelayBetweenShots = 300\n  \
  DamageNugget\n    Damage = 150\n    DamageType = SLASH\n  End\nEnd\n\
Object Reaper\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Zap\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn start(label: &str, x: f32) -> (Simulation<RotwkRules>, Ids, Entity) {
    start_on(label, x, flat_map(96, 96))
}

fn start_on(label: &str, x: f32, map: HeightMapData) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Column", "Swaying"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == label)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y: 400.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = *simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .unwrap();
    let units = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    (simulation, ids, horde)
}

fn by_slot(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> Vec<(Vec2, Vec2)> {
    let table = simulation.state().get(ids.horde, horde).unwrap();
    table
        .members
        .iter()
        .map(|member| {
            let slot = simulation.state().get(ids.member, *member).unwrap().slot;
            let offset = table.offset(slot.unwrap()).unwrap();
            (
                offset,
                simulation
                    .state()
                    .get(ids.body, *member)
                    .unwrap()
                    .0
                    .position,
            )
        })
        .collect()
}

fn off_path(point: Vec2, path: &[Vec2]) -> f32 {
    path.windows(2)
        .map(|leg| {
            let (a, b) = (leg[0], leg[1]);
            let along = b.minus(a);
            let share = (point.minus(a).dot(along) / along.length_squared()).clamp(0.0, 1.0);
            point.minus(a.plus(along.scale(share))).length()
        })
        .fold(f32::MAX, f32::min)
}

#[test]
fn a_turned_column_keeps_its_rear_rank_on_its_own_path() {
    let (mut simulation, ids, horde) = start("Column", 300.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..330 {
        simulation.tick().unwrap();
    }
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
    let path = [
        Vec2::new(180.0, 400.0),
        Vec2::new(600.0, 400.0),
        Vec2::new(600.0, 700.0),
    ];
    let mut worst = 0.0f32;
    let mut fastest = 0.0f32;
    let mut before = by_slot(&simulation, ids, horde);
    for i in 0..240 {
        simulation.tick().unwrap();
        let now = by_slot(&simulation, ids, horde);
        let rear: Vec<Vec2> = now
            .iter()
            .filter(|(offset, _)| offset.x == -60.0)
            .map(|(_, at)| *at)
            .collect();
        assert_eq!(rear.len(), 2);
        let middle = rear[0].plus(rear[1]).scale(0.5);
        if i >= 35 {
            worst = worst.max(off_path(middle, &path));
        }
        for ((_, was), (_, is)) in before.iter().zip(&now) {
            fastest = fastest.max(is.minus(*was).length());
        }
        before = now;
    }
    let turned = simulation.state().get(ids.body, horde).unwrap().0.position;
    assert!(turned.y > 500.0, "the horde went north: {turned:?}");
    assert!(worst < 10.0, "the rear rank strayed {worst} from the path");
    let tick = 1.0 / 30.0;
    assert!(
        fastest > 45.0 * tick * 1.1,
        "fastest step {fastest} - no glide past its speed"
    );
    assert!(
        fastest <= 1.25 * 45.0 * tick + 1e-3,
        "fastest step {fastest} past 1.25 x speed"
    );
}

#[test]
fn a_standing_trail_member_sways_and_floats_without_moving() {
    let (mut simulation, ids, horde) = start("Swaying", 400.0);
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    let mut floats = Vec::new();
    for _ in 0..120 {
        simulation.tick().unwrap();
        let body = simulation.state().get(ids.body, member).unwrap().0;
        assert_eq!(
            body.speed, 0.0,
            "a swaying member of a standing horde is not MOVING"
        );
        low = low.min(body.position.y);
        high = high.max(body.position.y);
        floats.push(
            simulation
                .state()
                .get(ids.elevation, member)
                .map_or(0.0, |elevation| elevation.0),
        );
    }
    assert!(high - low > 2.0, "no sway: {low}..{high}");
    assert!(
        high - low < 2.0 * 10.0 * 1.0 + 1e-3,
        "sway past its amplitude: {low}..{high}"
    );
    assert!(
        floats.iter().any(|height| *height > 0.5),
        "no float: {floats:?}"
    );
}

#[test]
fn a_dead_front_rank_member_is_never_replaced_by_a_rear_one() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Column", "Swaying", "Reaper"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let reaper = kinds
        .spawn(
            &data.combat,
            kinds.kind("Reaper").unwrap(),
            Vec2::new(340.0, 400.0),
            Facing::from_radians(core::f32::consts::PI),
            OTHER,
        )
        .unwrap();
    let rules = RotwkRules::new(passability, vec![reaper])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Column")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 400.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = *simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .unwrap();
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let before: std::collections::BTreeMap<Entity, Option<u16>> = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .iter()
        .map(|member| {
            (
                *member,
                simulation.state().get(ids.member, *member).unwrap().slot,
            )
        })
        .collect();
    assert_eq!(before.len(), 6, "every slot filled before the kill");
    let mut after_count = before.len();
    for _ in 0..600 {
        simulation.tick().unwrap();
        after_count = simulation
            .state()
            .get(ids.horde, horde)
            .map_or(0, |horde| horde.members.len());
        if after_count < before.len() {
            break;
        }
    }
    assert_eq!(after_count, before.len() - 1, "exactly one member died");
    let after = simulation.state().get(ids.horde, horde).unwrap();
    for member in &after.members {
        let slot = simulation.state().get(ids.member, *member).unwrap().slot;
        assert_eq!(
            Some(slot),
            before.get(member).copied(),
            "a survivor's own slot must not change"
        );
    }
}

#[test]
fn a_gliding_member_follows_a_downhill_slope() {
    let (width, height) = (96u32, 96u32);
    let samples = (0..height)
        .flat_map(|_| (0..width).map(|x| u16::try_from(2000 - 12 * x).unwrap()))
        .collect();
    let map = HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples,
    };
    let (mut simulation, ids, horde) = start_on("Column", 300.0, map);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    let mut worst = 0.0f32;
    let mut descent = f32::MAX;
    let mut start_z = None;
    for _ in 0..400 {
        simulation.tick().unwrap();
        for member in &members {
            let at = simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position;
            let z = simulation.state().get(ids.elevation, *member).unwrap().0;
            let ground = simulation
                .rules()
                .passability
                .heights
                .height_at_point(at)
                .unwrap();
            worst = worst.max((z - ground).abs());
            start_z.get_or_insert(z);
            descent = descent.min(z);
        }
    }
    assert!(
        start_z.unwrap() - descent > 5.0,
        "no real descent: {start_z:?} {descent}"
    );
    assert!(worst < 0.5, "a member left the surface by {worst}");
}
