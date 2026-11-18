use super::*;
use crate::death_phase::DeathPhase;

const DATA: &str = "\
Weapon ZBlade\n  AttackRange = 40\n  DelayBetweenShots = 100\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = SLASH\n  End\nEnd\n\
Weapon ZBoom\n  AttackRange = 60\n  RadiusDamageAffects = ENEMIES\n  \
  DamageNugget\n    Damage = 7\n    Radius = 60\n    DamageType = SLASH\n  End\nEnd\n\
ObjectCreationList ZListA\n  CreateObject\n    ObjectNames = ZMarkA\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList ZListB\n  CreateObject\n    ObjectNames = ZMarkB\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
Object ZKiller\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBlade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZVictim\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL\n    \
    DestructionDelay = 2000\n    FX = INITIAL ZFxA ZFxB\n    Sound = MIDPOINT ZSound\n    \
    FX = FINAL ZFxC\n    FX = HIT_GROUND ZFxD\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSpawner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL\n    \
    DestructionDelay = 2000\n    OCL = INITIAL ZListA ZListB\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZBomb\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL\n    \
    DestructionDelay = 1000\n    Weapon = FINAL ZBoom\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZMarkA\n  Side = Men\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object ZMarkB\n  Side = Men\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n";

struct Scene {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    kinds: Arc<KindTable>,
    victim: Entity,
    killer: Entity,
}

fn scene(seed: u64, victim: &str) -> Scene {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZKiller", victim, "ZMarkA", "ZMarkB"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        (victim, Vec2::new(300.0, 300.0), PLAYER),
        ("ZKiller", Vec2::new(320.0, 300.0), OTHER),
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
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None])
        .with_creation_lists(Arc::clone(&data.ocls));
    let simulation = Simulation::new(seed, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let bodies = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    Scene {
        simulation,
        ids,
        kinds,
        victim: bodies[0],
        killer: bodies[1],
    }
}

fn phases(scene: &mut Scene, ticks: u32) -> Vec<(DeathPhase, u64)> {
    let mut seen = Vec::new();
    for _ in 0..ticks {
        scene.simulation.tick().unwrap();
        let frame = Frame::new(scene.simulation.frame().get() - 1);
        let rules = scene.simulation.rules();
        for event in rules.death_phase_events(scene.simulation.state(), frame) {
            assert_eq!(event.entity, scene.victim);
            seen.push((event.phase, frame.get()));
        }
    }
    seen
}

#[test]
fn each_declared_death_phase_reaches_presentation_on_its_frame() {
    let mut scene = scene(5, "ZVictim");
    let seen = phases(&mut scene, 200);
    let died = seen.first().expect("the victim must die in the window").1;
    let kinds: Vec<DeathPhase> = seen.iter().map(|(phase, _)| *phase).collect();
    assert_eq!(
        kinds,
        [
            DeathPhase::Initial,
            DeathPhase::HitGround,
            DeathPhase::Midpoint,
            DeathPhase::Final
        ]
    );
    assert_eq!((seen[1].1, seen[3].1), (died, died + 60));
}

#[test]
fn the_midpoint_is_a_logic_draw_within_its_band() {
    let mut offsets = Vec::new();
    for seed in 1..=8 {
        let mut scene = scene(seed, "ZVictim");
        let seen = phases(&mut scene, 200);
        let died = seen.first().unwrap().1;
        let midpoint = seen
            .iter()
            .find(|(phase, _)| *phase == DeathPhase::Midpoint)
            .unwrap()
            .1;
        offsets.push(midpoint - died);
    }
    assert!(
        offsets.iter().all(|offset| (21..=39).contains(offset)),
        "{offsets:?}"
    );
    offsets.sort_unstable();
    offsets.dedup();
    assert!(offsets.len() > 1, "{offsets:?}");
}

#[test]
fn a_death_phase_runs_one_drawn_ocl() {
    let mut made = Vec::new();
    for seed in 1..=8 {
        let mut scene = scene(seed, "ZSpawner");
        for _ in 0..60 {
            scene.simulation.tick().unwrap();
        }
        let count = |name: &str| {
            let kind = scene.kinds.kind(name).unwrap();
            scene
                .simulation
                .state()
                .store(scene.ids.kind)
                .unwrap()
                .iter()
                .filter(|(_, k)| **k == kind)
                .count()
        };
        made.push((count("ZMarkA"), count("ZMarkB")));
    }
    assert!(made.iter().all(|(a, b)| a + b == 1), "{made:?}");
    assert!(made.contains(&(1, 0)) && made.contains(&(0, 1)), "{made:?}");
}

#[test]
fn a_final_death_weapon_fires_after_its_object_is_gone() {
    let mut scene = scene(5, "ZBomb");
    for _ in 0..120 {
        scene.simulation.tick().unwrap();
    }
    let world = scene.simulation.state();
    assert!(!world.is_live(scene.victim));
    let health = world
        .get(scene.ids.health, scene.killer)
        .map(|pool| pool.0.current())
        .unwrap();
    assert!(health < 4000.0, "{health}");
}

const FALL_DATA: &str = "\
GameData\n  Gravity = -64.0\nEnd\n\
Weapon ZBlast\n  AttackRange = 300\n  DelayBetweenShots = 5000\n  \
  DamageNugget\n    Damage = 5\n    DamageType = MAGIC\n    Radius = 200\n  End\n  \
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
Object ZFaller\n  Side = Men\n  KindOf = INFANTRY SELECTABLE NOT_AUTOACQUIRABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 8\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 600\n    MaxLifetime = 600\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL\n    \
    DestructionDelay = 5000\n    FX = HIT_GROUND ZFxD\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn hit_ground_runs_when_a_thrown_corpse_comes_within_a_unit_of_the_terrain() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        FALL_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZBlaster", "ZPost", "ZFaller"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZFaller", Vec2::new(460.0, 300.0), PLAYER),
        ("ZBlaster", Vec2::new(300.0, 300.0), OTHER),
        ("ZPost", Vec2::new(400.0, 300.0), PLAYER),
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
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let faller = simulation.state().store(ids.body).unwrap().entities()[0];
    let mut heights = Vec::new();
    let mut hit_ground = None;
    for _ in 0..300 {
        simulation.tick().unwrap();
        let frame = Frame::new(simulation.frame().get() - 1);
        let world = simulation.state();
        if world.contains(ids.doomed, faller) {
            heights.push((
                frame.get(),
                world.get(ids.ground_offset, faller).map_or(0.0, |o| o.0),
            ));
        }
        for event in simulation.rules().death_phase_events(world, frame) {
            if event.phase == DeathPhase::HitGround {
                hit_ground = Some(frame.get());
            }
        }
    }
    let (died, height_at_death) = heights.first().copied().expect("the faller must die");
    assert!(height_at_death >= 1.0, "{heights:?}");
    let landed = heights
        .iter()
        .find(|(_, h)| *h < 1.0)
        .map(|(frame, _)| *frame);
    assert!(landed.is_some_and(|landed| landed > died), "{heights:?}");
    assert_eq!(hit_ground, landed, "{heights:?}");
}
