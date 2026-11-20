use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityZap\n  ReloadTime = 30000\nEnd\n\
Weapon ZapWeapon\n  AttackRange = 500\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
CommandSet ZapSet\n  1 = Fire_Zap\nEnd\n\
CommandButton Fire_Zap\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityZap\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
Armor NoArmor\n  Armor = DEFAULT 100%\nEnd\n\
Object Zapper\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZapSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Zap\n    SpecialPowerTemplate = SpecialAbilityZap\n    \
    SpecialWeapon = ZapWeapon\n    WhichSpecialWeapon = 1\n    UnpackTime = 1\n    \
    PreparationTime = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Footman\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = NoArmor\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object FoeHorde\n  Side = Mordor\n  KindOf = SELECTABLE HORDE\n  BuildCost = 300\n  BuildTime = 2\n  \
  VisionRange = 400\n  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Footman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  End\nEnd\n\
Object Barracks\n  Side = Mordor\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_FoeHorde\nEnd\n\
CommandButton Build_FoeHorde\n  Command = UNIT_BUILD\n  Object = FoeHorde\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Zapper", "Barracks"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let caster = kinds
        .spawn(
            &data.combat,
            kinds.kind("Zapper").unwrap(),
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![caster])
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&data.formations, &kinds))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    (simulation, ids, caster)
}

fn spawn_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids) -> (Entity, Vec<Entity>) {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "FoeHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            OTHER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 150.0,
                y: 100.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities().first().copied().unwrap();
    (horde, store.get(horde).unwrap().members.clone())
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |pool| pool.0.current())
}

#[test]
fn toggling_hold_fire_on_a_horde_sets_it_on_every_member() {
    let (mut simulation, ids, _caster) = scene();
    let (horde, members) = spawn_horde(&mut simulation, ids);
    let press = |simulation: &mut Simulation<RotwkRules>| {
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: members.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::ToggleHoldFire {
                    lead: members.first().copied(),
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    };
    let held = |simulation: &Simulation<RotwkRules>| {
        std::iter::once(horde)
            .chain(members.iter().copied())
            .map(|entity| simulation.state().contains(ids.hold_fire, entity))
            .collect::<Vec<bool>>()
    };
    press(&mut simulation);
    assert_eq!(
        held(&simulation),
        vec![true; 4],
        "the horde and its members"
    );
    press(&mut simulation);
    assert_eq!(
        held(&simulation),
        vec![false; 4],
        "a second press clears all"
    );
}

#[test]
fn a_single_target_ability_aimed_at_a_horde_hits_its_first_member() {
    let (mut simulation, ids, caster) = scene();
    let (horde, members) = spawn_horde(&mut simulation, ids);
    assert_eq!(members.len(), 3, "the formation filled");
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
                target: Some(horde),
                at: None,
            },
        )
        .unwrap();
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    let hurt: Vec<bool> = members
        .iter()
        .map(|member| health(&simulation, ids, *member) < 1000.0)
        .collect();
    assert_eq!(hurt, vec![true, false, false], "only the first member");
}
