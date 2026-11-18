use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityRush\n  ReloadTime = 30000\nEnd\n\
ModifierList Rush\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 30000\nEnd\n\
Object Rusher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Rush\n    SpecialPowerTemplate = SpecialAbilityRush\n    \
    StartAbilityRange = 60\n    HeroAttributeModifier = Rush\n    HeroEffectDuration = 30000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Post\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RusherHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = RushSet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Rush\n    SpecialPowerTemplate = SpecialAbilityRush\n    \
    StartAbilityRange = 60\n    HeroAttributeModifier = Rush\n    HeroEffectDuration = 30000\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Rusher Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RusherHorde\nEnd\n\
CommandButton Build_RusherHorde\n  Command = UNIT_BUILD\n  Object = RusherHorde\nEnd\n\
CommandSet RushSet\n  1 = Fire_Rush\nEnd\n\
CommandButton Fire_Rush\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRush\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

fn simulation() -> Simulation<RotwkRules> {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Post", "RusherHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let post = kinds
        .spawn(
            &data.combat,
            kinds.kind("Post").unwrap(),
            Vec2::new(150.0, 200.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![post])
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn damage_mult(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(1.0, |bonuses| bonuses.0.damage_mult)
}

#[test]
fn a_hordes_ability_approach_moves_the_formation_and_triggers() {
    let mut simulation = simulation();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RusherHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 400.0,
                y: 200.0,
                facing: std::f32::consts::PI,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities()[0];
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 3, "the fixture's premise");
    let post = simulation.state().store(ids.kind).unwrap().entities()[0];
    let start = simulation.state().get(ids.body, horde).unwrap().0.position;
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![members[0]],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(post),
            at: None,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    let horde_ids = RotwkRules::horde_ids(ids);
    let mut worst_slot_error = 0.0f32;
    let mut triggered_at = None;
    for tick in 0..600u32 {
        simulation.tick().unwrap();
        for member in &members {
            assert!(
                !simulation.state().contains(ids.path, *member),
                "tick {tick}: member {} left its slot on a route of its own",
                member.index()
            );
        }
        if let Some((_, error)) = crate::horde::crowding(simulation.state(), horde_ids, horde) {
            worst_slot_error = worst_slot_error.max(error);
        }
        if members
            .iter()
            .chain([&horde])
            .all(|entity| (damage_mult(&simulation, ids, *entity) - 1.5).abs() < 1e-6)
        {
            triggered_at = Some(tick);
            break;
        }
    }
    let end = simulation.state().get(ids.body, horde).unwrap().0.position;
    let triggered_at = triggered_at
        .unwrap_or_else(|| panic!("never triggered; horde walked from {start:?} to {end:?}"));
    assert!(
        end.x < start.x - 100.0,
        "the horde itself walked: {start:?} -> {end:?}"
    );
    assert!(
        worst_slot_error < 20.0,
        "the formation held: worst mean slot error {worst_slot_error}, triggered at tick {triggered_at}"
    );
}
