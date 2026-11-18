use super::*;

const DATA: &str = "\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Piece\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  CommandSet = PieceSet\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n  End\n  \
  Behavior = ReplaceSelfUpgrade Tag_Arch\n    ReplaceWith = Arch\n    \
    TriggeredBy = Upgrade_Arch\n    ConflictsWith = Upgrade_Turret Upgrade_Knot\n  End\n  \
  Behavior = ReplaceSelfUpgrade Tag_Turret\n    ReplaceWith = Turret\n    \
    TriggeredBy = Upgrade_Turret\n    ConflictsWith = Upgrade_Arch Upgrade_Knot\n  End\n  \
  Behavior = ReplaceSelfUpgrade Tag_Knot\n    ReplaceWith = Knot\n    \
    TriggeredBy = Upgrade_Knot\n    ConflictsWith = Upgrade_Arch Upgrade_Turret\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 5\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object Arch\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_UPGRADE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 9\n  GeometryMinorRadius = 60\n  \
  GeometryHeight = 30\nEnd\n\
Object Turret\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_UPGRADE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 15\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object Knot\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_HUB\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 30\nEnd\n\
CommandSet PieceSet\n  1 = Buy_Arch\n  2 = Buy_Turret\n  3 = Buy_Knot\nEnd\n\
CommandButton Buy_Arch\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Arch\nEnd\n\
CommandButton Buy_Turret\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Turret\nEnd\n\
CommandButton Buy_Knot\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Knot\nEnd\n\
Upgrade Upgrade_Arch\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Turret\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 3\nEnd\n\
Upgrade Upgrade_Knot\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n";

const PIECES: usize = 5;

fn wall() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Piece", "Arch", "Turret", "Knot"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let piece = kinds.kind("Piece").unwrap();
    #[allow(clippy::cast_precision_loss)]
    let spawns = (0..PIECES)
        .map(|index| {
            kinds
                .spawn(
                    &data.combat,
                    piece,
                    Vec2::new(300.0, 200.0 + 38.0 * index as f32),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let pieces = simulation.state().store(ids.body).unwrap().entities()[..PIECES].to_vec();
    (simulation, ids, pieces)
}

fn buy(simulation: &mut Simulation<RotwkRules>, on: Entity, button: u8) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![on],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button })
        .unwrap();
    simulation.tick().unwrap();
}

fn standing(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.state().is_live(entity)
        && !simulation
            .state()
            .get(ids.health, entity)
            .is_some_and(|health| health.0.is_dead())
}

fn money(simulation: &Simulation<RotwkRules>, ids: Ids) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
}

#[test]
fn a_long_conversion_takes_over_its_two_neighbours_only() {
    let (mut simulation, ids, pieces) = wall();
    buy(&mut simulation, pieces[2], 0);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().is_live(pieces[2]),
        "the middle piece was replaced"
    );
    assert!(!standing(&simulation, ids, pieces[1]) && !standing(&simulation, ids, pieces[3]));
    assert!(standing(&simulation, ids, pieces[0]) && standing(&simulation, ids, pieces[4]));
}

#[test]
fn a_conversion_is_refused_beside_a_standing_or_queued_wall_upgrade() {
    let (mut simulation, ids, pieces) = wall();
    buy(&mut simulation, pieces[0], 1);
    let affected = |simulation: &Simulation<RotwkRules>, piece: Entity| {
        simulation
            .rules()
            .object_upgrade_affected(simulation.state(), piece, "Upgrade_Turret")
    };
    assert!(!affected(&simulation, pieces[1]), "beside a queued turret");
    assert!(affected(&simulation, pieces[2]), "two pieces out is free");
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert!(
        !affected(&simulation, pieces[1]),
        "beside a standing turret"
    );
    let before = money(&simulation, ids);
    buy(&mut simulation, pieces[1], 1);
    assert_eq!(
        money(&simulation, ids),
        before,
        "the refused press charges nothing"
    );
}

#[test]
fn a_queued_conversion_a_new_hub_blocks_is_cancelled_and_refunded() {
    let (mut simulation, ids, pieces) = wall();
    let start = money(&simulation, ids);
    buy(&mut simulation, pieces[2], 1);
    buy(&mut simulation, pieces[3], 2);
    assert_eq!(money(&simulation, ids), start - 200, "both queued and paid");
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        standing(&simulation, ids, pieces[2]),
        "the turret never replaced its piece"
    );
    assert_eq!(
        money(&simulation, ids),
        start - 100,
        "the turret was refunded"
    );
}

#[test]
fn a_hub_conversion_is_refused_beside_a_standing_hub() {
    let (mut simulation, _, pieces) = wall();
    buy(&mut simulation, pieces[2], 2);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let affected = |piece: Entity| {
        simulation
            .rules()
            .object_upgrade_affected(simulation.state(), piece, "Upgrade_Knot")
    };
    assert!(!affected(pieces[3]), "beside a standing hub");
    assert!(affected(pieces[4]), "two pieces out is free");
}

#[test]
fn a_hub_conversion_is_allowed_beside_a_standing_tower() {
    let (mut simulation, _, pieces) = wall();
    buy(&mut simulation, pieces[0], 1);
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert!(simulation.rules().object_upgrade_affected(
        simulation.state(),
        pieces[1],
        "Upgrade_Knot"
    ));
}
