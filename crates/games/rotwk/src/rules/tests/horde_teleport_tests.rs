use super::*;

const DATA: &str = "\
SpecialPower Blink\n  ReloadTime = 0\nEnd\n\
Object TeleportMember\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TeleportHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = TeleportHordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 2\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:TeleportMember Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\n  \
  Behavior = TeleportSpecialAbilityUpdate Tag_Blink\n    SpecialPowerTemplate = Blink\n    \
    MaxDistance = 2000\n    PreparationTime = 1\n    IgnoreFacingCheck = Yes\n  End\nEnd\n\
CommandSet TeleportHordeSet\n  1 = Fire_Blink\nEnd\n\
CommandButton Fire_Blink\n  Command = SPECIAL_POWER\n  SpecialPower = Blink\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_TeleportHorde\nEnd\n\
CommandButton Build_TeleportHorde\n  Command = UNIT_BUILD\n  Object = TeleportHorde\nEnd\n";

fn simulation() -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "TeleportMember", "TeleportHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

#[test]
fn a_teleported_horde_carries_its_members_onto_their_slots_the_same_frame() {
    let (mut simulation, ids) = simulation();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "TeleportHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 300.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities()[0];
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 2, "the fixture's premise");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: Some((2300.0, 300.0)),
            },
        )
        .unwrap();
    let mut landed = false;
    for _ in 0..20 {
        simulation.tick().unwrap();
        let at = simulation.state().get(ids.body, horde).unwrap().0.position;
        if (at.x - 2300.0).abs() < 1.0 {
            landed = true;
            break;
        }
    }
    assert!(landed, "the fixture's premise");

    let horde_at = simulation.state().get(ids.body, horde).unwrap().0.position;
    for member in members {
        let at = simulation.state().get(ids.body, member).unwrap().0.position;
        let gap = at.minus(horde_at).length();
        assert!(
            gap < 20.0,
            "member {} at {at:?} is not on its slot by the horde's new spot {horde_at:?}",
            member.index()
        );
    }
}
