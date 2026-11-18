use super::*;
use crate::unit::Kind;

const DATA: &str = "\
SpecialPower SpecialAbilityZap\n  ReloadTime = 30000\nEnd\n\
Weapon ZapWeapon\n  AttackRange = 500\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n    Radius = 20\n  End\nEnd\n\
CommandSet ZapSet\n  1 = Fire_Zap\nEnd\n\
CommandButton Fire_Zap\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityZap\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
Object Zapper\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZapSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Zap\n    SpecialPowerTemplate = SpecialAbilityZap\n    \
    SpecialWeapon = ZapWeapon\n    WhichSpecialWeapon = 1\n    UnpackTime = 1\n    \
    PreparationTime = 1\n    PersistentCount = 3\n    PersistentPrepTime = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Armor NoArmor\n  Armor = DEFAULT 100%\nEnd\n\
Object ZapTarget\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = NoArmor\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn load() -> (GameData, Arc<KindTable>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Zapper", "ZapTarget"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    (data, Arc::new(kinds))
}

fn simulation(data: &GameData, kinds: &Arc<KindTable>) -> (Simulation<RotwkRules>, Entity, Entity) {
    let production = kinds.production(data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let caster_kind = kinds.kind("Zapper").unwrap();
    let target_kind = kinds.kind("ZapTarget").unwrap();
    let caster_spawn = kinds
        .spawn(
            &data.combat,
            caster_kind,
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let target_spawn = kinds
        .spawn(
            &data.combat,
            target_kind,
            Vec2::new(150.0, 100.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![caster_spawn, target_spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let find = |kind: Kind| {
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, held)| **held == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let caster = find(caster_kind);
    let target = find(target_kind);
    (simulation, caster, target)
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |pool| pool.0.current())
}

#[test]
fn a_trigger_before_reload_does_not_fire_one_after_it_does() {
    let (data, kinds) = load();
    let (mut simulation, caster, target) = simulation(&data, &kinds);
    let ids = simulation.rules().ids().unwrap();
    let max_health = health_of(&simulation, ids, target);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(target),
                at: None,
            },
        )
        .unwrap();
    let mut hits_at = Vec::new();
    let mut last_health = max_health;
    for frame in 0..300u64 {
        simulation.tick().unwrap();
        let health = health_of(&simulation, ids, target);
        if health < last_health {
            hits_at.push(frame);
            last_health = health;
        }
    }
    assert_eq!(
        hits_at.len(),
        2,
        "two of the three triggers must land (the second must be gated): {hits_at:?}"
    );
}
