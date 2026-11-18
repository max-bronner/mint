use super::*;

const DATA: &str = "\
Weapon ZTorch\n  AttackRange = 40\n  DelayBetweenShots = 100\n  DeathType = BURNED\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = FLAME\n  End\nEnd\n\
Weapon ZClub\n  AttackRange = 40\n  DelayBetweenShots = 100\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
Object ZBurner\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZTorch\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZClubber ZBurner\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZClub\n  End\nEnd\n\
Object ZVictim\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Plain\n    DeathTypes = ALL -BURNED -FADED\n    \
    DestructionDelay = 60000\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Burned\n    DeathTypes = NONE +BURNED\n    \
    DestructionDelay = 60000\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Faded\n    DeathTypes = NONE +FADED\n    \
    DestructionDelay = 60000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZEphemeral ZVictim\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 500\n    MaxLifetime = 500\n    \
    DeathType = FADED\n  End\nEnd\n";

const PLAIN: u32 = 0;
const BURNED: u32 = 1;
const FADED: u32 = 2;

fn scene() -> (Simulation<RotwkRules>, Ids, [Entity; 3]) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZBurner", "ZClubber", "ZVictim", "ZEphemeral"];
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
        ("ZVictim", Vec2::new(300.0, 300.0), PLAYER),
        ("ZVictim", Vec2::new(300.0, 700.0), PLAYER),
        ("ZEphemeral", Vec2::new(700.0, 500.0), PLAYER),
        ("ZBurner", Vec2::new(320.0, 300.0), OTHER),
        ("ZClubber", Vec2::new(320.0, 700.0), OTHER),
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
    let entities = simulation.state().store(ids.body).unwrap().entities();
    let victims = [entities[0], entities[1], entities[2]];
    (simulation, ids, victims)
}

fn rung_after(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    victim: Entity,
    ticks: u32,
) -> Option<u32> {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation
        .state()
        .get(ids.doomed, victim)
        .map(|doomed| doomed.death_index)
}

#[test]
fn a_rung_the_death_forbids_is_skipped_at_the_roll_and_taken_on_the_death_it_names() {
    let (mut simulation, ids, [burned, clubbed, _]) = scene();
    assert_eq!(rung_after(&mut simulation, ids, burned, 30), Some(BURNED));
    assert_eq!(
        simulation
            .state()
            .get(ids.doomed, clubbed)
            .map(|d| d.death_index),
        Some(PLAIN)
    );
}

#[test]
fn a_lifetime_death_is_the_type_its_lifetime_update_names() {
    let (mut simulation, ids, [_, _, ephemeral]) = scene();
    assert_eq!(rung_after(&mut simulation, ids, ephemeral, 30), Some(FADED));
}

#[test]
fn combat_facts_record_every_blow_kill_and_death_with_the_faded_bit_right() {
    let (mut simulation, _ids, [burned, clubbed, ephemeral]) = scene();
    let mut struck = Vec::new();
    let mut kills = Vec::new();
    let mut deaths = Vec::new();
    for _ in 0..60 {
        simulation.tick().unwrap();
        let frame = Frame::new(simulation.frame().get() - 1);
        if let Some(facts) = simulation.rules().combat_facts(simulation.state()) {
            struck.extend_from_slice(facts.struck(frame));
            kills.extend_from_slice(facts.kills(frame));
            deaths.extend_from_slice(facts.deaths(frame));
        }
    }
    assert!(!struck.is_empty(), "the fight landed no recorded blow");
    assert!(
        kills.iter().any(|(victim, _)| *victim == burned),
        "the burned victim's kill was not recorded"
    );
    assert!(
        kills.iter().any(|(victim, _)| *victim == clubbed),
        "the clubbed victim's kill was not recorded"
    );
    assert_eq!(
        deaths.iter().find(|(entity, _)| *entity == burned),
        Some(&(burned, false))
    );
    assert_eq!(
        deaths.iter().find(|(entity, _)| *entity == clubbed),
        Some(&(clubbed, false))
    );
    assert_eq!(
        deaths.iter().find(|(entity, _)| *entity == ephemeral),
        Some(&(ephemeral, true)),
        "the lifetime death should be recorded as FADED"
    );
}
