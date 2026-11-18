use super::*;

const DATA: &str = "\
DamageFX ZHitFX\n  AmountForMajorFX = DEFAULT 25.0\n  MajorFX = DEFAULT FX_ZBig\n  \
  MinorFX = DEFAULT FX_ZSmall\n  ThrottleTime = DEFAULT 2000\nEnd\n\
FXList FX_ZBig\nEnd\n\
FXList FX_ZSmall\nEnd\n\
Armor ZHide\n  Armor = DEFAULT 100%\nEnd\n\
Armor ZProof\n  Armor = DEFAULT 0%\nEnd\n\
Weapon ZHeavyBow\n  AttackRange = 400\n  DelayBetweenShots = 300\n  AcceptableAimDelta = 360\n  \
  DamageNugget\n    Damage = 40\n    DamageType = PIERCE\n    DamageFXType = ZARROW\n  End\nEnd\n\
Weapon ZLightBow\n  AttackRange = 400\n  DelayBetweenShots = 300\n  AcceptableAimDelta = 360\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n    DamageFXType = ZARROW\n  End\nEnd\n\
Object ZTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 20000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZHide\n    DamageFX = ZHitFX\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZProofTarget ZTarget\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZProof\n    DamageFX = ZHitFX\n  End\nEnd\n\
Object ZHeavyArcher\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 700\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZHeavyBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZLightArcher ZHeavyArcher\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZLightBow\n  End\nEnd\n";

fn scene(target: &str, archer: &str) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = vec![target.to_owned(), archer.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let places = [Vec2::new(300.0, 300.0), Vec2::new(500.0, 300.0)];
    let spawns = names
        .iter()
        .zip(places)
        .enumerate()
        .map(|(at, (name, place))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    place,
                    Facing::EAST,
                    if at == 0 { PLAYER } else { OTHER },
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    (simulation, ids)
}

fn played(target: &str, archer: &str, ticks: u32) -> Vec<String> {
    let (mut simulation, _ids) = scene(target, archer);
    let mut seen = Vec::new();
    for _ in 0..ticks {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            if let Some(name) = simulation.rules().fx_name(event.list) {
                seen.push(name.to_owned());
            }
        }
    }
    seen
}

#[test]
fn the_dealt_amount_chooses_the_major_list_over_the_minor() {
    let heavy = played("ZTarget", "ZHeavyArcher", 120);
    let light = played("ZTarget", "ZLightArcher", 120);
    assert!(
        heavy.iter().any(|name| name == "FX_ZBig") && !heavy.iter().any(|name| name == "FX_ZSmall"),
        "40 dealt is major: {heavy:?}"
    );
    assert!(
        light.iter().any(|name| name == "FX_ZSmall") && !light.iter().any(|name| name == "FX_ZBig"),
        "10 dealt is minor: {light:?}"
    );
}

#[test]
fn a_victim_plays_one_fx_per_throttle_time_and_none_for_a_hit_that_dealt_nothing() {
    let throttled = played("ZTarget", "ZHeavyArcher", 180);
    assert!(
        (1..=3).contains(&throttled.len()),
        "180 frames of arrows, 60-frame throttle: {throttled:?}"
    );
    assert!(
        played("ZProofTarget", "ZHeavyArcher", 180).is_empty(),
        "armour of 0% deals nothing, and zero plays nothing"
    );
}

#[test]
fn a_damage_fx_list_is_played_on_the_victim() {
    let (mut simulation, ids) = scene("ZTarget", "ZHeavyArcher");
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (target, archer) = (entities[0], entities[1]);
    let mut plays = Vec::new();
    for _ in 0..120 {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            if simulation.rules().fx_name(event.list) == Some("FX_ZBig") {
                plays.push((event.on_object, event.caller, event.victim));
            }
        }
    }
    assert!(!plays.is_empty(), "never played");
    assert!(
        plays
            .iter()
            .all(|play| *play == (true, target, Some(archer))),
        "{plays:?}"
    );
}
