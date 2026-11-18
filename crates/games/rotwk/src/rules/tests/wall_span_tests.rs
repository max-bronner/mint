use super::*;

const DATA: &str = "\
Object Hub\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_HUB\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\n  \
  Behavior = WallHubBehavior Tag_Hub\n    Options = OPTION_ONE\n    \
    SegmentTemplateName = Segment\n    HubCapTemplateName = HubCap\n    \
    CliffCapTemplateName = CliffCap\n    MaxBuildoutDistance = 200\n  End\nEnd\n\
Object Segment\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  BuildCost = 50\n  BuildTime = 1\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object HubCap\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_HUB\n  \
  BuildCost = 50\n  BuildTime = 1\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object CliffCap\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  BuildCost = 50\n  BuildTime = 1\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object CC\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n";

fn stepped(width: u32, height: u32, threshold: u32) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: (0..width * height)
            .map(|i| if i % width >= threshold { 2_000 } else { 0 })
            .collect(),
    }
}

fn setup_with_hubs(
    height_map: HeightMapData,
    cash: i64,
    other_hubs: &[Vec2],
) -> (
    Simulation<RotwkRules>,
    Ids,
    Entity,
    Vec<Entity>,
    Arc<KindTable>,
) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(
        &data,
        &[
            "Hub".to_owned(),
            "Segment".to_owned(),
            "HubCap".to_owned(),
            "CliffCap".to_owned(),
            "CC".to_owned(),
        ],
    );
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let hub = kinds
        .spawn(
            &data.combat,
            kinds.kind("Hub").unwrap(),
            Vec2::new(50.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let command_center = kinds
        .spawn(
            &data.combat,
            kinds.kind("CC").unwrap(),
            Vec2::new(50.0, 130.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let other_spawns: Vec<_> = other_hubs
        .iter()
        .map(|position| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Hub").unwrap(),
                    *position,
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&height_map, 5.0 / 128.0).unwrap();
    let mut units = vec![hub, command_center];
    units.extend(other_spawns);
    let mut simulation = Simulation::new(
        71,
        RotwkRules::new(passability, units)
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), cash),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation.state().store(ids.body).unwrap().entities();
    let hub_entity = entities[0];
    let others = entities[2..].to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hub_entity],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids, hub_entity, others, kinds)
}

fn setup(
    height_map: HeightMapData,
    cash: i64,
) -> (Simulation<RotwkRules>, Ids, Entity, Arc<KindTable>) {
    let (simulation, ids, hub, _others, kinds) = setup_with_hubs(height_map, cash, &[]);
    (simulation, ids, hub, kinds)
}

#[test]
fn a_piece_on_a_cliff_becomes_the_cap_pair_and_ends_the_span() {
    let (simulation, ids, _hub, kinds) = setup(stepped(40, 20, 15), 100_000);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 300.0, 100.0)
        .expect("flat approach, affordable - a plan");
    let segment = kinds.kind("Segment").unwrap();
    let hub_cap = kinds.kind("HubCap").unwrap();
    let cliff_cap = kinds.kind("CliffCap").unwrap();
    let kinds_laid: Vec<_> = plan.pieces.iter().map(|piece| piece.kind).collect();
    assert_eq!(
        kinds_laid.last().copied(),
        Some(cliff_cap),
        "the span ends on the cliff cap: {kinds_laid:?}"
    );
    assert_eq!(
        kinds_laid[kinds_laid.len() - 2],
        hub_cap,
        "the piece before the cliff cap becomes the hub cap: {kinds_laid:?}"
    );
    assert!(
        kinds_laid[..kinds_laid.len() - 2]
            .iter()
            .all(|kind| *kind == segment),
        "every earlier piece is a plain segment: {kinds_laid:?}"
    );
    assert!(
        kinds_laid.len() < 12,
        "the span must have stopped well short of the full drag: {kinds_laid:?}"
    );
}

#[test]
fn a_piece_beyond_the_buildout_distance_from_every_command_center_refuses_the_whole_span() {
    let (simulation, ids, _hub, _kinds) = setup(flat_map(60, 60), 100_000);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 50.0, 400.0)
        .expect("a refused span still plans, for the preview");
    assert_eq!(plan.code(), 9, "{:?}", plan.pieces);
    assert_eq!(plan.pieces.last().map(|piece| piece.code), Some(9));
}

#[test]
fn the_first_piece_touches_the_hubs_own_radius_with_no_gap() {
    let (simulation, ids, hub, kinds) = setup(flat_map(60, 60), 100_000);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 200.0, 100.0)
        .expect("flat approach, affordable - a plan");
    assert!(
        (plan.step - 18.0).abs() < 0.01,
        "step is 2 x (GeometryMinorRadius - 1) = 2 x (10 - 1): {}",
        plan.step
    );
    let segment = kinds.kind("Segment").unwrap();
    let first = plan.pieces.first().expect("at least one piece");
    assert_eq!(first.kind, segment);
    let hub_position = simulation
        .state()
        .get(ids.body, hub)
        .expect("the hub has a body")
        .0
        .position;
    assert!(
        (first.centre.x - (hub_position.x + 19.0)).abs() < 0.01,
        "first centre at origin (hub rim, 10) + h (9): {:?}",
        first.centre
    );
}

#[test]
fn an_unaffordable_span_is_refused_in_full() {
    let (simulation, ids, _hub, _kinds) = setup(flat_map(60, 60), 50);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 50.0, 150.0)
        .expect("a refused span still plans, for the preview");
    let codes: Vec<u8> = plan.pieces.iter().map(|piece| piece.code).collect();
    assert_eq!(
        codes.first(),
        Some(&0),
        "the first 50 is covered: {codes:?}"
    );
    assert_eq!(codes.get(1), Some(&7), "the second is not: {codes:?}");
    assert_eq!(plan.code(), 7);
}

#[test]
fn a_span_snapped_to_an_existing_hub_lays_no_cap_there_and_is_legal() {
    let (simulation, ids, _hub, others, kinds) =
        setup_with_hubs(flat_map(60, 60), 100_000, &[Vec2::new(110.0, 100.0)]);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 105.0, 100.0)
        .expect("a drag ending 5 units from an own hub snaps and joins, legal");
    let segment = kinds.kind("Segment").unwrap();
    let hub_cap = kinds.kind("HubCap").unwrap();
    let kinds_laid: Vec<_> = plan.pieces.iter().map(|piece| piece.kind).collect();
    assert!(
        kinds_laid.iter().all(|kind| *kind == segment),
        "a joined span lays no hub cap of its own: {kinds_laid:?}"
    );
    assert_ne!(
        kinds_laid.last().copied(),
        Some(hub_cap),
        "the span ends at the existing hub, not under a new cap"
    );
    let last = plan.pieces.last().expect("at least one piece");
    let hub2 = simulation
        .state()
        .get(ids.body, others[0])
        .expect("the second hub has a body")
        .0
        .position;
    assert!(
        last.centre.minus(hub2).length() <= plan.step + 10.0 + 0.01,
        "the last piece stands at the joined hub: {:?} vs {hub2:?}",
        last.centre
    );
}

#[test]
fn the_nearer_of_two_qualifying_hubs_wins_the_snap() {
    let (simulation, ids, _hub, _others, _kinds) = setup_with_hubs(
        flat_map(60, 60),
        100_000,
        &[Vec2::new(130.0, 115.0), Vec2::new(130.0, 100.0)],
    );
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 130.0, 105.0)
        .expect("a drag between two qualifying hubs snaps to the nearer");
    for piece in &plan.pieces {
        assert!(
            (piece.centre.y - 100.0).abs() < 0.5,
            "every piece stays on the line to the nearer hub at y=100: {:?}",
            piece.centre
        );
    }
}

#[test]
fn a_join_over_clear_ground_is_untinted_and_its_last_piece_undrawn() {
    let (simulation, ids, _hub, _others, _kinds) =
        setup_with_hubs(flat_map(60, 60), 100_000, &[Vec2::new(110.0, 100.0)]);
    let plan = simulation
        .rules()
        .span_plan(simulation.state(), ids, PLAYER, 0, 105.0, 100.0)
        .expect("a drag ending near an own hub joins it");
    assert_eq!(plan.code(), 0);
    assert!(plan.pieces.iter().all(|piece| piece.code == 0));
    assert_eq!(plan.drawn().len(), plan.pieces.len() - 1);
}
