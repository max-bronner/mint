use super::*;

const DATA: &str = "\
Weapon ZTorch\n  AttackRange = 40\n  DelayBetweenShots = 100\n  DeathType = BURNED\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = FLAME\n  End\nEnd\n\
FXList FX_ZDeath\nEnd\n\
Object ZBurner\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZTorch\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZOriented\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZDeath\n    DeathTypes = ALL\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZFlat ZOriented\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZDeath\n    OrientToObject = No\n    \
    DeathTypes = ALL\n  End\nEnd\n\
ChildObject ZGated ZOriented\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZDeath\n    DeathTypes = NONE +CRUSHED\n  End\nEnd\n";

fn scene(target: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [target.to_owned(), "ZBurner".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        (target, Vec2::new(300.0, 300.0), PLAYER),
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
    let victim = simulation.state().store(ids.body).unwrap().entities()[0];
    (simulation, ids, victim)
}

fn played(target: &str) -> Vec<(bool, String)> {
    let (mut simulation, ids, _victim) = scene(target);
    let mut seen = Vec::new();
    for _ in 0..120 {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            if let Some(name) = simulation.rules().fx_name(event.list) {
                seen.push((event.on_object, name.to_owned()));
            }
        }
        if simulation.state().get(ids.body, _victim).is_none() {
            break;
        }
    }
    seen
}

#[test]
fn a_death_fx_plays_once_on_death() {
    let seen = played("ZOriented");
    assert_eq!(
        seen,
        vec![(true, "FX_ZDeath".to_owned())],
        "a declared DeathFX plays exactly once, on the dying object by default"
    );
}

#[test]
fn on_object_follows_orient_to_object() {
    let seen = played("ZFlat");
    assert_eq!(
        seen,
        vec![(false, "FX_ZDeath".to_owned())],
        "OrientToObject = No plays at the object's position, not on it"
    );
}

#[test]
fn a_death_types_mismatch_plays_nothing() {
    assert!(
        played("ZGated").is_empty(),
        "DeathTypes = NONE +CRUSHED must not admit a BURNED death"
    );
}
