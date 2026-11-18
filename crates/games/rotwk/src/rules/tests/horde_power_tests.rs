use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityRage\n  ReloadTime = 30000\nEnd\n\
ModifierList Rage\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 3000\nEnd\n\
Weapon Claw\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Raider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Loner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = RageSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RaiderHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = RageSet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Raider Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RaiderHorde\nEnd\n\
CommandButton Build_RaiderHorde\n  Command = UNIT_BUILD\n  Object = RaiderHorde\nEnd\n\
CommandSet RageSet\n  1 = Fire_Rage\nEnd\n\
CommandButton Fire_Rage\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Loner", "RaiderHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match, placed: &[(&str, f32, f32)]) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn press(simulation: &mut Simulation<RotwkRules>, unit: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

fn damage_mult(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(1.0, |bonuses| bonuses.0.damage_mult)
}

#[test]
fn a_hordes_power_reaches_each_member_that_declares_it() {
    let m = load();
    let mut simulation = simulation(&m, &[]);
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RaiderHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities()[0];
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 3, "the fixture's premise");
    press(&mut simulation, members[0]);
    for entity in members.iter().chain([&horde]) {
        assert!((damage_mult(&simulation, ids, *entity) - 1.5).abs() < 1e-6);
    }
}

#[test]
fn a_lone_objects_power_reaches_only_itself() {
    let m = load();
    let mut simulation = simulation(&m, &[("Loner", 100.0, 100.0), ("Loner", 150.0, 100.0)]);
    let ids = simulation.rules().ids().unwrap();
    let loner = m.kinds.kind("Loner").unwrap();
    let mut loners: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, kind)| **kind == loner)
        .map(|(entity, _)| entity)
        .collect();
    loners.sort_unstable_by_key(|entity| entity.index());
    press(&mut simulation, loners[0]);
    assert!((damage_mult(&simulation, ids, loners[0]) - 1.5).abs() < 1e-6);
    assert_eq!(damage_mult(&simulation, ids, loners[1]), 1.0);
}
