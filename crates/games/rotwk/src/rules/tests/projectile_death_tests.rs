use super::*;

const DATA: &str = "\
FXList FX_ZArrowDeath\nEnd\nFXList FX_ZArrowNormalDeath\nEnd\n\
Weapon ZBow\n  AttackRange = 300\n  DelayBetweenShots = 60000\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZArrow\n    WarheadTemplateName = ZHead\n  End\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZArrowNormal\n    WarheadTemplateName = ZHead\n  \
  End\nEnd\n\
Weapon ZMissBow\n  AttackRange = 300\n  DelayBetweenShots = 60000\n  WeaponSpeed = 600\n  \
  HitPercentage = 0\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZStick\n    WarheadTemplateName = ZHead\n  End\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZArrowNormal\n    WarheadTemplateName = ZHead\n  \
  End\nEnd\n\
Weapon ZSplashBow\n  AttackRange = 300\n  DelayBetweenShots = 60000\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZQuiet\n    WarheadTemplateName = ZSplash\n  End\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZQuiet\n    WarheadTemplateName = ZSplash\n  End\nEnd\n\
Weapon ZSplash\n  RadiusDamageAffects = ALLIES ENEMIES NEUTRAL\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n    Radius = 60\n  End\nEnd\n\
Weapon ZHead\n  HitStoredTarget = Yes\n  ProjectileCollidesWith = STRUCTURES\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object ZArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSplashArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZSplashBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZQuiet\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DetonateCallsKill = No\n    DieOnImpact = Yes\n  End\nEnd\n\
Object ZMissArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZMissBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTarget\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZArrow\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DetonateCallsKill = Yes\n    DieOnImpact = Yes\n  End\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZArrowDeath\n    DeathTypes = NONE +DETONATED\n  End\nEnd\n\
Object ZArrowNormal\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DetonateCallsKill = Yes\n    DieOnImpact = Yes\n  End\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZArrowNormalDeath\n    DeathTypes = NONE +NORMAL\n  End\nEnd\n\
Object ZStick\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DieOnImpact = No\n    FinalStuckTime = 2000\n  End\nEnd\n";

const STUCK_SECONDS: f32 = 2.0;

fn scene(archer: &str) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [
        archer,
        "ZTarget",
        "ZArrow",
        "ZArrowNormal",
        "ZStick",
        "ZQuiet",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        (archer, Vec2::new(300.0, 300.0), PLAYER),
        ("ZTarget", Vec2::new(400.0, 300.0), OTHER),
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
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[0]],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackObject {
                target: entities[1],
            },
        )
        .unwrap();
    (simulation, ids)
}

fn arrow_fx(simulation: &mut Simulation<RotwkRules>, frames: u32) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..frames {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            if let Some(name) = simulation.rules().fx_name(event.list) {
                if name.starts_with("FX_ZArrow") {
                    seen.push(name.to_owned());
                }
            }
        }
    }
    seen
}

#[test]
fn a_hit_kills_the_projectile_detonated_so_only_that_filtered_fx_plays() {
    let (mut simulation, _) = scene("ZArcher");
    assert_eq!(arrow_fx(&mut simulation, 90), ["FX_ZArrowDeath"]);
}

#[test]
fn a_landing_with_die_on_impact_kills_the_projectile_normal() {
    let (mut simulation, _) = scene("ZMissArcher");
    assert_eq!(arrow_fx(&mut simulation, 90), ["FX_ZArrowNormalDeath"]);
}

fn objects_of(simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = simulation.rules().kinds.kind(name);
    simulation
        .state()
        .store(ids.kind)
        .map(|store| {
            store
                .iter()
                .filter(|(_, held)| Some(**held) == kind)
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_landed_projectile_without_die_on_impact_stays_busy_for_its_final_stuck_time() {
    let (mut simulation, ids) = scene("ZMissArcher");
    let hz = RotwkRules::ticks_per_second();
    let mut landed = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        let flying = simulation
            .state()
            .store(ids.flight)
            .is_some_and(|store| store.iter().any(|(_, flight)| flight.object));
        if !flying && !objects_of(&simulation, ids, "ZStick").is_empty() {
            landed = Some(());
            break;
        }
    }
    landed.expect("the stick never landed");
    let stick = objects_of(&simulation, ids, "ZStick")[0];
    let busy =
        |simulation: &Simulation<RotwkRules>| simulation.state().contains(ids.ai_busy, stick);
    assert!(busy(&simulation), "busy on landing");
    assert!(
        simulation.state().contains(ids.no_attack, stick),
        "stays unattackable"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let halfway = (STUCK_SECONDS * 0.5 * hz) as u32;
    for _ in 0..halfway {
        simulation.tick().unwrap();
    }
    assert!(busy(&simulation), "still busy a second on");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let after = (STUCK_SECONDS * 1.5 * hz) as u32;
    for _ in 0..after {
        simulation.tick().unwrap();
    }
    assert!(!busy(&simulation), "the busy window ends");
    assert!(
        simulation.state().is_live(stick),
        "nothing removes it afterwards"
    );
}

#[test]
fn a_blast_does_not_damage_the_projectile_objects_in_it() {
    let (mut simulation, ids) = scene("ZSplashArcher");
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    let hurt = simulation.state().store(ids.health).is_some_and(|store| {
        store
            .iter()
            .any(|(_, pool)| pool.0.current() < pool.0.max())
    });
    assert!(hurt, "the shots never landed");
}
