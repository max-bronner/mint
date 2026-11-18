use super::*;

const DATA: &str = "\
FXList FX_ZArrowDeath\nEnd\nFXList FX_ZArrowNormalDeath\nEnd\n\
Weapon ZBow\n  AttackRange = 300\n  DelayBetweenShots = 60000\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZArrow\n    WarheadTemplateName = ZHead\n  End\n  \
  ProjectileNugget\n    ProjectileTemplateName = ZArrowNormal\n    WarheadTemplateName = ZHead\n  \
  End\nEnd\n\
Weapon ZHead\n  HitStoredTarget = Yes\n  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object ZArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTarget\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZArrow\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DetonateCallsKill = Yes\n    DieOnImpact = Yes\n  End\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZArrowDeath\n    DeathTypes = NONE +DETONATED\n  End\nEnd\n\
Object ZArrowNormal\n  Side = Men\n  KindOf = PROJECTILE\n  \
  Behavior = BezierProjectileBehavior Tag_Flight\n    DetonateCallsKill = Yes\n    DieOnImpact = Yes\n  End\n  \
  Behavior = FXListDie Tag_Fx\n    DeathFX = FX_ZArrowNormalDeath\n    DeathTypes = NONE +NORMAL\n  End\nEnd\n";

fn plays_on_a_hit() -> Vec<String> {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZArcher", "ZTarget", "ZArrow", "ZArrowNormal"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZArcher", Vec2::new(300.0, 300.0), PLAYER),
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
    let mut seen = Vec::new();
    for _ in 0..90 {
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
    assert_eq!(plays_on_a_hit(), ["FX_ZArrowDeath"]);
}
