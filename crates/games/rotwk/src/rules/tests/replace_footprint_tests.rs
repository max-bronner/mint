use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityOpen\n  ReloadTime = 1000\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = GateSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = SpecialPowerModule Tag_OpenStarter\n    \
    SpecialPowerTemplate = SpecialAbilityOpen\n  End\n  \
  Behavior = ToggleMountedSpecialAbilityUpdate Tag_Open\n    \
    SpecialPowerTemplate = SpecialAbilityOpen\n    MountedTemplate = OpenGate\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object OpenGate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
CommandSet GateSet\n  1 = Open_Gate\nEnd\n\
CommandButton Open_Gate\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityOpen\nEnd\n";

const GATE_AT: Vec2 = Vec2::new(300.0, 300.0);

fn gate_match() -> (Simulation<RotwkRules>, Ids, Entity, usize) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec!["Gate".to_owned(), "OpenGate".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let kind = kinds.kind("Gate").unwrap();
    let spawn = kinds
        .spawn(&data.combat, kind, GATE_AT, Facing::EAST, PLAYER)
        .unwrap();
    let record = &kinds.get(kind).unwrap().record;
    let footprints = vec![(0, crate::construction::footprint_of(record))];
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let cell = passability.cell_at(GATE_AT).unwrap();
    let index = usize::try_from(passability.grid.extent().index(cell).unwrap()).unwrap();
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_structure_footprints(footprints)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let gate = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    (simulation, ids, gate, index)
}

fn blocked(simulation: &Simulation<RotwkRules>, ids: Ids, index: usize) -> bool {
    crate::obstacles::counts_of(simulation.state(), ids.obstacles)
        .get(index)
        .is_some_and(|count| *count > 0)
}

fn press(simulation: &mut Simulation<RotwkRules>, on: Entity, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![on],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
    simulation.tick().unwrap();
}

#[test]
fn the_ground_under_a_replaced_structure_is_free_once_the_replacement_dies() {
    let (mut simulation, ids, gate, index) = gate_match();
    assert!(
        blocked(&simulation, ids, index),
        "the gate blocks its cells"
    );
    press(
        &mut simulation,
        gate,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: None,
        },
    );
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().is_live(gate), "the gate was replaced");
    let replacement = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    assert!(
        blocked(&simulation, ids, index),
        "the replacement blocks the same cells"
    );
    press(&mut simulation, replacement, RotwkCommand::DebugKill);
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().is_live(replacement),
        "the replacement is gone"
    );
    assert!(
        !blocked(&simulation, ids, index),
        "and its ground is walkable again"
    );
}
