use super::*;

const DATA: &str = "\
Weapon ZFireArrow\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 60\n    DamageType = FLAME\n    DeathType = NORMAL\n  End\nEnd\n\
Object ZFireArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZFireArrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZEnt\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = FlammableUpdate Tag_Flame\n    FlameDamageLimit = 50\n    BurnedDelay = 1000\n    \
    AflameDamageDelay = 200\n    AflameDamageAmount = 10\n    AflameDuration = 2000\n    \
    SetBurnedStatus = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZFireArcher", "ZEnt"];
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
    let placed = [
        ("ZFireArcher", 400.0, 400.0, PLAYER),
        ("ZEnt", 550.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let ent = {
        let kind = kinds.kind("ZEnt").unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, ent)
}

#[test]
fn a_flame_hit_past_the_limit_ignites() {
    let (simulation, ids, ent) = scene(10);
    let world = simulation.state();
    assert!(
        world.contains(ids.aflame, ent),
        "the ent should be AFLAME after the arrow lands"
    );
    let health = world.get(ids.health, ent).unwrap();
    assert!(
        health.0.current() < 1000.0,
        "the arrow and at least one self-damage pulse should have landed"
    );
}

#[test]
fn burned_is_worn_only_after_its_delay() {
    let (simulation, _ids, ent) = scene(10);
    let burned = |sim: &Simulation<RotwkRules>, entity: Entity| {
        sim.rules()
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(sim.state(), entity, "BURNED"))
    };
    assert!(
        !burned(&simulation, ent),
        "BurnedDelay has not elapsed by tick 10"
    );
    let (simulation, _ids, ent) = scene(45);
    assert!(
        burned(&simulation, ent),
        "BurnedDelay (1000 ms, 30 ticks after ignition) has elapsed by tick 45"
    );
}

#[test]
fn the_fire_goes_out_after_aflame_duration() {
    let (simulation, ids, ent) = scene(80);
    assert!(
        !simulation.state().contains(ids.aflame, ent),
        "AflameDuration (2000 ms, 60 ticks after ignition) has elapsed by tick 80"
    );
}

const CRUSH_IGNITE_DATA: &str = "\
Weapon ZCrushMace\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 60\n    DamageType = CRUSH\n    DeathType = NORMAL\n  End\nEnd\n\
Armor ZFlameWeak\n  Armor = DEFAULT 100%\n  Armor = FLAME 300%\nEnd\n\
Object ZMace\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZCrushMace\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTender\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZFlameWeak\n  End\n  \
  Behavior = FlammableUpdate Tag_Flame\n    DamageType = CRUSH\n    FlameDamageLimit = 50\n    \
    AflameDamageDelay = 200\n    AflameDamageAmount = 10\n    AflameDuration = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn the_self_damage_pulse_is_flame_even_when_the_ignition_filter_is_not() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        CRUSH_IGNITE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZMace", "ZTender"];
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
    let placed = [
        ("ZMace", 400.0, 400.0, PLAYER),
        ("ZTender", 550.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let tender = {
        let kind = kinds.kind("ZTender").unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let health = simulation.state().get(ids.health, tender).unwrap();
    assert!(
        health.0.current() < 900.0,
        "self-damage scaled by FLAME's 300% armour should have landed: {}",
        health.0.current()
    );
}
