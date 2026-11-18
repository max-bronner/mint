use super::*;

const DATA: &str = "\
Weapon ZTorch\n  AttackRange = 40\n  DelayBetweenShots = 100\n  DeathType = BURNED\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = FLAME\n  End\nEnd\n\
Object ZBurner\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZTorch\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSinker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Sink\n    DestructionDelay = 60000\n    \
    SinkDelay = 1000\n    SinkRate = 3.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZBurner", "ZSinker"];
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
        ("ZSinker", Vec2::new(300.0, 300.0), PLAYER),
        ("ZBurner", Vec2::new(320.0, 300.0), OTHER),
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
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let sinker = simulation.state().store(ids.body).unwrap().entities()[0];
    (simulation, ids, sinker)
}

#[test]
fn a_sinking_corpse_waits_its_delay_then_drops_by_the_rate_per_tick() {
    let (mut simulation, ids, sinker) = scene();
    let ground = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.ground_offset, sinker)
            .map_or(0.0, |offset| offset.0)
    };

    let mut death_tick = None;
    let mut onset = None;
    for t in 0..300 {
        simulation.tick().unwrap();
        if death_tick.is_none() && simulation.state().get(ids.doomed, sinker).is_some() {
            death_tick = Some(t);
        }
        if death_tick.is_some() && ground(&simulation) != 0.0 {
            onset = Some((t, ground(&simulation)));
            break;
        }
    }
    let death_tick = death_tick.expect("the sinker must die within the test window");
    let (onset_tick, onset) = onset.expect("the corpse must start sinking within the test window");
    assert_eq!(
        onset_tick - death_tick,
        30,
        "SinkDelay = 1000ms is exactly 30 ticks at SIMULATION_HZ = 30"
    );
    let per_tick = 3.0 / 30.0;
    assert!(
        (onset - -per_tick).abs() < 1e-6,
        "onset offset {onset}, expected {}",
        -per_tick
    );

    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let after = ground(&simulation);
    assert!(
        (after - -(6.0 * per_tick)).abs() < 1e-5,
        "offset after 6 sinking ticks {after}, expected {}",
        -(6.0 * per_tick)
    );
}
