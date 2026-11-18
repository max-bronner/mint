use super::*;

const DATA: &str = "\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Hub\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_HUB\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\n  \
  Behavior = WallHubBehavior Tag_Hub\n    Options = OPTION_ONE\n    \
    SegmentTemplateName = Segment\n    HubCapTemplateName = Hub\n    MaxBuildoutDistance = 500\n  End\nEnd\n\
Object Segment\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  CommandSet = SegmentSet\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n  End\n  \
  Behavior = ReplaceSelfUpgrade Tag_Knot\n    ReplaceWith = Knot\n    \
    TriggeredBy = Upgrade_Knot\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Knot\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_HUB\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 20\nEnd\n\
Object CC\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
CommandSet SegmentSet\n  1 = Buy_Knot\nEnd\n\
CommandButton Buy_Knot\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Knot\nEnd\n\
Upgrade Upgrade_Knot\n  Type = OBJECT\n  BuildCost = 1\n  BuildTime = 1\nEnd\n";

fn select(simulation: &mut Simulation<RotwkRules>, entity: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
}

#[test]
fn a_span_links_its_hubs_and_a_conversion_carries_the_links() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Hub", "Segment", "Knot", "CC"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: Vec2| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn("CC", Vec2::new(100.0, 130.0)),
        spawn("Hub", Vec2::new(50.0, 100.0)),
        spawn("Hub", Vec2::new(170.0, 100.0)),
        spawn("Hub", Vec2::new(290.0, 100.0)),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 50_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let bodies = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (a, b, c) = (bodies[1], bodies[2], bodies[3]);
    for (from, to) in [(a, 170.0), (b, 290.0)] {
        select(&mut simulation, from);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::WallSpan {
                    option: 0,
                    x: to,
                    y: 100.0,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    }
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let linked = |x: Entity, y: Entity| {
        simulation
            .rules()
            .hubs_linked(simulation.state(), ids, x, y)
    };
    assert!(linked(a, b) && linked(b, c), "each span joins its two hubs");
    assert!(!linked(a, c), "a chain does not link its ends");
    let segment = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|entity| {
            simulation.state().get(ids.kind, *entity).copied() == kinds.kind("Segment")
                && simulation
                    .state()
                    .get(ids.body, *entity)
                    .is_some_and(|body| body.0.position.x < 160.0)
        })
        .expect("a segment of the first span stands");
    select(&mut simulation, segment);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    let knot = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|entity| simulation.state().get(ids.kind, *entity).copied() == kinds.kind("Knot"))
        .expect("the segment became a knot");
    let linked = |x: Entity, y: Entity| {
        simulation
            .rules()
            .hubs_linked(simulation.state(), ids, x, y)
    };
    assert!(
        linked(knot, a) && linked(knot, b),
        "the knot inherits the segment's links"
    );
}
