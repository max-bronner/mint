use super::*;

const DATA: &str = "\
Object ToggleMember\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ToggleHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = ToggleHordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 2\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:ToggleMember Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_ToggleHorde\nEnd\n\
CommandButton Build_ToggleHorde\n  Command = UNIT_BUILD\n  Object = ToggleHorde\nEnd\n\
CommandSet ToggleHordeSet\n  1 = Command_ToggleMounted\nEnd\n\
CommandButton Command_ToggleMounted\n  Command = TOGGLE_WEAPONSET\n  \
  FlagsUsedForToggle = MOUNTED\nEnd\n";

fn simulation() -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "ToggleMember", "ToggleHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

#[test]
fn a_toggle_press_on_a_horde_reaches_every_member() {
    let (mut simulation, ids) = simulation();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "ToggleHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 100.0,
                y: 100.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities()[0];
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 2, "the fixture's premise");

    let state_id = simulation.rules().upgrade_state_id().unwrap();
    let has_flag = |simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str| {
        simulation
            .state()
            .get(state_id, entity)
            .is_some_and(|state| state.has_weapon_set_flag(flag))
    };

    let press = RotwkCommand::ToggleWeaponSet {
        button: "Command_ToggleMounted".to_owned(),
        lead: None,
    };
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![horde],
        },
        press.clone(),
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    for entity in members.iter().chain([&horde]) {
        assert!(
            has_flag(&simulation, *entity, "WEAPONSET_TOGGLE_1"),
            "entity {} must hold the toggle flag",
            entity.index()
        );
        assert!(
            !has_flag(&simulation, *entity, "MOUNTED"),
            "the button's own face name must never be the flag applied"
        );
    }

    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![horde],
        },
        press,
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    for entity in members.iter().chain([&horde]) {
        assert!(
            !has_flag(&simulation, *entity, "WEAPONSET_TOGGLE_1"),
            "entity {} must drop the flag on the second press",
            entity.index()
        );
    }
}
