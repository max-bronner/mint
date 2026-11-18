use super::*;

const DATA: &str = "\
GameData\n  SellPercentage = 50%\nEnd\n\
Object DefaultThingTemplate\n  Behavior = DestroyDie Tag_DefaultDie\n  End\nEnd\n\
Object Site\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE BASE_SITE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Behavior = CastleBehavior Tag_Castle\n    KeepDeathKillsEverything = Yes\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 1500\n  End\nEnd\n\
Object MapSite\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Behavior = CastleBehavior Tag_Castle\n  End\nEnd\n\
Object Keep\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE CASTLE_KEEP\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 1500\n  End\nEnd\n\
Object Pad\n  Side = Men\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE BASE_FOUNDATION UNATTACKABLE NO_COLLIDE EXPANSION_PAD\n  \
  CommandSet = PadSet\n  \
  Behavior = FoundationAIUpdate Tag_Foundation\n    BuildVariation = 2\n  End\n  \
  Body = ImmortalBody Tag_Body\n    MaxHealth = 15000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 5\n  GeometryMinorRadius = 5\n  GeometryHeight = 1\nEnd\n\
Object Tower\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = TowerSet\n  \
  BuildCost = 300\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 5\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    WorkerName = TowerWorkerNoSelect\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 12\n  GeometryMinorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object TowerWorkerNoSelect\n  Side = Men\n  KindOf = INFANTRY\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL -FADED\n    DestructionDelay = 1000\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Faded\n    DeathTypes = NONE +FADED\n    DestructionDelay = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 12\nEnd\n\
Object Soldiers\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 250\n  BuildTime = 30\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object FieldSite\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE BASE_SITE\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Behavior = CastleBehavior Tag_Castle\n    CastleToUnpackForFaction = Men FieldCastle\n    \
  FilterValidOwnedEntries = ANY +STRUCTURE\n    InstantUnpack = Yes\n  End\nEnd\n\
Object Rock\n  KindOf = IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\nEnd\n\
Object FieldPad\n  Side = Men\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE BASE_FOUNDATION UNATTACKABLE NO_COLLIDE EXPANSION_PAD\n  \
  CommandSet = FieldPadSet\n  \
  Behavior = FoundationAIUpdate Tag_Foundation\n  End\n  \
  Body = ImmortalBody Tag_Body\n    MaxHealth = 15000\n  End\nEnd\n\
CommandSet FieldPadSet\n  1 = Build_FieldSite\nEnd\n\
CommandButton Build_FieldSite\n  Command = FOUNDATION_CONSTRUCT\n  Object = FieldSite\nEnd\n\
CommandSet PadSet\n  1 = Build_Tower\nEnd\n\
CommandSet TowerSet\n  1 = Build_Soldiers\n  2 = Command_Sell\nEnd\n\
CommandButton Build_Tower\n  Command = FOUNDATION_CONSTRUCT\n  Object = Tower\n  Radial = Yes\nEnd\n\
CommandButton Build_Soldiers\n  Command = UNIT_BUILD\n  Object = Soldiers\nEnd\n\
CommandButton Command_Sell\n  Command = SELL\nEnd\n";

struct Base {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Base {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Site", "MapSite", "Keep", "Pad", "FieldPad", "Rock"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(b: &Base, site: &str) -> (Simulation<RotwkRules>, Ids) {
    let placed = [
        (site, 300.0, 300.0),
        ("Keep", 300.0, 300.0),
        ("Pad", 364.0, 300.0),
        ("Pad", 236.0, 300.0),
    ];
    let production = b.kinds.production(&b.data);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y)| {
            let kind = b.kinds.kind(name).unwrap();
            b.kinds
                .spawn(
                    &b.data.combat,
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
        .with_players(2)
        .with_castles(vec![(1, 0), (2, 0), (3, 0)], Arc::default())
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone())
        .with_sell_percentage(b.data.sell_percentage);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn start_contested(b: &Base) -> (Simulation<RotwkRules>, Ids) {
    let placed = [
        ("Site", 300.0, 300.0, PLAYER),
        ("Keep", 300.0, 300.0, PLAYER),
        ("Pad", 364.0, 300.0, PLAYER),
        ("Pad", 236.0, 300.0, PLAYER),
        ("MapSite", 100.0, 100.0, OTHER),
    ];
    let production = b.kinds.production(&b.data);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, player)| {
            let kind = b.kinds.kind(name).unwrap();
            b.kinds
                .spawn(
                    &b.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *player,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let filter = crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_castles(vec![(1, 0), (2, 0), (3, 0)], Arc::default())
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone())
        .with_victory(Arc::clone(&b.kinds), filter)
        .with_sell_percentage(b.data.sell_percentage);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
}

fn of_kind(b: &Base, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = b.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
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
    run(simulation, 1);
}

fn build_tower(b: &Base, simulation: &mut Simulation<RotwkRules>, ids: Ids, pad: Entity) -> Entity {
    press(
        simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    let tower = of_kind(b, simulation, ids, "Tower")
        .into_iter()
        .find(|tower| {
            simulation
                .state()
                .get(ids.pad_occupant, pad)
                .is_some_and(|occupant| occupant.0 == *tower)
        })
        .unwrap();
    run(simulation, 80);
    assert!(simulation.state().get(ids.construction, tower).is_none());
    tower
}

fn doomed(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.state().contains(ids.doomed, entity)
}

fn decided(simulation: &Simulation<RotwkRules>, ids: Ids) -> bool {
    crate::clock_entity(simulation.state(), ids)
        .is_some_and(|clock| simulation.state().contains(ids.outcome, clock))
}

#[test]
fn a_pad_builds_on_itself_and_offers_the_build_again_once_it_is_sold() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pad = of_kind(&b, &simulation, ids, "Pad")[0];
    let site = of_kind(&b, &simulation, ids, "Site")[0];
    let before = purse(&simulation, ids);
    press(
        &mut simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    assert_eq!(
        purse(&simulation, ids),
        before - 300,
        "charged at the order"
    );
    let tower = of_kind(&b, &simulation, ids, "Tower")[0];
    let world = simulation.state();
    let construction = world.get(ids.construction, tower).copied().unwrap();
    assert_eq!(construction.builder, None, "a pad build has no porter");
    assert_eq!(
        world.get(ids.body, tower).unwrap().0.position,
        world.get(ids.body, pad).unwrap().0.position,
        "on the pad"
    );
    assert_eq!(world.get(ids.castle_member, tower).unwrap().castle, site);
    let look = simulation.rules().upgrade_state_id().unwrap();
    assert_eq!(
        world.get(look, tower).unwrap().model_condition_names(),
        ["BUILD_VARIATION_TWO"],
        "the pad's BuildVariation"
    );
    assert!(world.get(ids.selected, pad).is_none(), "deselected");
    let rules = simulation.rules();
    assert!(rules.pad_is_occupied(world, ids, pad));
    assert_eq!(
        rules.foundation_target(world, ids, PLAYER, pad, 0),
        Err("already built on")
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![pad],
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        simulation.state().get(ids.selected, pad).is_none(),
        "an occupied pad is unselectable"
    );
    run(&mut simulation, 80);
    assert!(
        simulation.state().get(ids.construction, tower).is_none(),
        "the worker's walk and BuildTime 1 s run with no porter"
    );

    press(&mut simulation, tower, RotwkCommand::Sell);
    run(&mut simulation, 185);
    assert!(!simulation.state().is_live(tower), "sold");
    let world = simulation.state();
    assert!(!simulation.rules().pad_is_occupied(world, ids, pad));
    assert!(simulation
        .rules()
        .foundation_target(world, ids, PLAYER, pad, 0)
        .is_ok());
    let rebuilt = purse(&simulation, ids);
    press(
        &mut simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    assert_eq!(purse(&simulation, ids), rebuilt - 300, "built again");
}

#[test]
fn one_click_builds_on_one_pad_of_a_selection() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let before = purse(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: pads.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::FoundationConstruct { button: 0 })
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(purse(&simulation, ids), before - 300);
    assert_eq!(of_kind(&b, &simulation, ids, "Tower").len(), 1);
    assert!(simulation
        .rules()
        .pad_is_occupied(simulation.state(), ids, pads[0]));
}

#[test]
fn a_destroyed_pad_structure_frees_its_pad() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pad = of_kind(&b, &simulation, ids, "Pad")[0];
    let tower = build_tower(&b, &mut simulation, ids, pad);
    press(&mut simulation, tower, RotwkCommand::DebugKill);
    assert!(doomed(&simulation, ids, tower));
    assert!(!simulation
        .rules()
        .pad_is_occupied(simulation.state(), ids, pad));
    assert!(!doomed(&simulation, ids, pad), "the pad stays");
}

#[test]
fn a_keeps_death_kills_its_castle_and_what_stands_on_its_pads() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let tower = build_tower(&b, &mut simulation, ids, pads[0]);
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    let site = of_kind(&b, &simulation, ids, "Site")[0];
    press(&mut simulation, keep, RotwkCommand::DebugKill);
    assert!(doomed(&simulation, ids, keep), "the keep starts dying");
    for entity in [site, tower] {
        assert!(
            doomed(&simulation, ids, entity),
            "{} dies with the keep's death blow",
            entity.index()
        );
    }
    for pad in [pads[0], pads[1]] {
        assert!(
            !simulation.state().is_live(pad),
            "a pad declares no death of its own: its inherited DestroyDie removes it on the frame"
        );
    }
    run(&mut simulation, DEFAULT_DESTRUCTION_DELAY.get());
    for entity in [keep, site, pads[0], pads[1]] {
        assert!(!simulation.state().is_live(entity), "{}", entity.index());
    }
    assert!(
        simulation.state().is_live(tower),
        "the tower's own 3 s slow death outlasts the keep"
    );
    run(&mut simulation, 90 - DEFAULT_DESTRUCTION_DELAY.get());
    assert!(!simulation.state().is_live(tower));
}

#[test]
fn a_keeps_death_kills_its_pads_after_the_castle_object_is_gone() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    let site = of_kind(&b, &simulation, ids, "Site")[0];
    press(&mut simulation, site, RotwkCommand::DebugKill);
    run(&mut simulation, 300);
    assert!(
        !simulation.state().is_live(site),
        "the castle object is gone"
    );
    assert!(!doomed(&simulation, ids, pads[0]), "its pads are not");
    press(&mut simulation, keep, RotwkCommand::DebugKill);
    for entity in [pads[0], pads[1]] {
        assert!(!simulation.state().is_live(entity), "{}", entity.index());
    }
}

#[test]
fn a_cascade_victim_refunds_its_queue() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let tower = build_tower(&b, &mut simulation, ids, pads[0]);
    let before = purse(&simulation, ids);
    press(
        &mut simulation,
        tower,
        RotwkCommand::QueueUnit { button: 0 },
    );
    let queued = purse(&simulation, ids);
    assert_eq!(queued, before - 250, "charged at the order");
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    press(&mut simulation, keep, RotwkCommand::DebugKill);
    assert!(doomed(&simulation, ids, tower), "the cascade killed it");
    assert_eq!(
        purse(&simulation, ids),
        queued + 250,
        "the queued soldier's cost came back"
    );
}

#[test]
fn a_castle_cascade_finishes_after_the_match_is_decided() {
    use crate::mode::{steps_simulation, transition, Event, Mode};

    let b = load();
    let (mut simulation, ids) = start_contested(&b);
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    let site = of_kind(&b, &simulation, ids, "Site")[0];
    press(&mut simulation, keep, RotwkCommand::DebugKill);
    let mut mode = Mode::Match;
    for _ in 0..=DEFAULT_DESTRUCTION_DELAY.get() {
        if mode == Mode::Match && decided(&simulation, ids) {
            mode = transition(mode, Event::MatchEnded).unwrap();
        }
        if steps_simulation(mode) {
            simulation.tick().unwrap();
        }
    }
    assert_eq!(mode, Mode::Results, "losing the castle decided the match");
    for entity in pads.iter().copied().chain([site, keep]) {
        assert!(
            !simulation.state().is_live(entity),
            "{} still stands behind the banner",
            entity.index()
        );
    }
}

#[test]
fn selling_a_keep_ends_its_castle() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let site = of_kind(&b, &simulation, ids, "Site")[0];
    let tower = build_tower(&b, &mut simulation, ids, pads[0]);
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    press(&mut simulation, keep, RotwkCommand::Sell);
    assert!(doomed(&simulation, ids, keep), "sold");
    for entity in [site, tower] {
        assert!(doomed(&simulation, ids, entity), "{}", entity.index());
    }
    for entity in [pads[0], pads[1]] {
        assert!(!simulation.state().is_live(entity), "{}", entity.index());
    }
    run(&mut simulation, 90);
    for entity in [pads[0], pads[1], site, tower, keep] {
        assert!(!simulation.state().is_live(entity), "{}", entity.index());
    }
}

#[test]
fn a_castle_without_the_keep_rule_keeps_its_members() {
    let b = load();
    let (mut simulation, ids) = start(&b, "MapSite");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let tower = build_tower(&b, &mut simulation, ids, pads[0]);
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    press(&mut simulation, keep, RotwkCommand::DebugKill);
    assert!(doomed(&simulation, ids, keep));
    for entity in [pads[0], pads[1], tower] {
        assert!(
            simulation.state().is_live(entity) && !doomed(&simulation, ids, entity),
            "{}",
            entity.index()
        );
    }
}

#[test]
fn only_the_keep_takes_the_castle_with_it() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pads = of_kind(&b, &simulation, ids, "Pad");
    let tower = build_tower(&b, &mut simulation, ids, pads[0]);
    press(&mut simulation, tower, RotwkCommand::DebugKill);
    let keep = of_kind(&b, &simulation, ids, "Keep")[0];
    assert!(!doomed(&simulation, ids, keep));
    assert!(!doomed(&simulation, ids, pads[1]));
}

fn pad_build(b: &Base, simulation: &mut Simulation<RotwkRules>, ids: Ids) -> (Entity, Entity) {
    let pad = of_kind(b, simulation, ids, "Pad")[0];
    press(
        simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    let tower = of_kind(b, simulation, ids, "Tower")[0];
    let worker = of_kind(b, simulation, ids, "TowerWorkerNoSelect")[0];
    (tower, worker)
}

fn worker_stage(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    worker: Entity,
) -> Option<crate::construction::WorkerStage> {
    simulation
        .state()
        .get(ids.site_worker, worker)
        .map(|state| state.stage)
}

#[test]
fn a_pad_builds_worker_starts_on_the_towers_centre_and_walks_out() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let (tower, worker) = pad_build(&b, &mut simulation, ids);
    let at = |simulation: &Simulation<RotwkRules>, entity| {
        simulation.state().get(ids.body, entity).unwrap().0.position
    };
    let centre = at(&simulation, tower);
    let first = at(&simulation, worker).minus(centre).length();
    run(&mut simulation, 20);
    let later = at(&simulation, worker).minus(centre).length();
    assert!(
        first < 1.0 && later > 5.0,
        "{first} from the centre at first, {later} after the walk"
    );
}

#[test]
fn a_pad_builds_worker_faces_its_tower_before_it_hammers() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let (tower, worker) = pad_build(&b, &mut simulation, ids);
    let hammering = Some(crate::construction::WorkerStage::Constructing);
    for _ in 0..32 {
        if worker_stage(&simulation, ids, worker) == hammering {
            break;
        }
        run(&mut simulation, 1);
    }
    assert_eq!(
        worker_stage(&simulation, ids, worker),
        hammering,
        "the fixture's premise: it hammers before the tower completes"
    );
    let body = |entity| simulation.state().get(ids.body, entity).unwrap().0;
    let (site, standing) = (body(tower), body(worker));
    let towards = site.position.minus(standing.position);
    let heading = standing.facing.to_vector();
    let cosine = (heading.x * towards.x + heading.y * towards.y) / towards.length();
    assert!(
        cosine > 0.99,
        "it hammers facing its tower: heading {heading:?}, tower lies {towards:?}"
    );
}

#[test]
fn a_pad_built_tower_grows_only_once_its_worker_hammers() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let (tower, worker) = pad_build(&b, &mut simulation, ids);
    let hammering = Some(crate::construction::WorkerStage::Constructing);
    let elapsed = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.construction, tower)
            .map(|site| site.elapsed)
    };
    let mut before = elapsed(&simulation);
    while worker_stage(&simulation, ids, worker) != hammering {
        before = elapsed(&simulation);
        run(&mut simulation, 1);
        assert!(simulation.frame().get() < 60, "the worker never hammered");
    }
    assert_eq!((before, elapsed(&simulation)), (Some(0), Some(1)));
}

#[test]
fn a_pad_built_towers_worker_fades_out_when_it_completes() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let (tower, worker) = pad_build(&b, &mut simulation, ids);
    run(&mut simulation, 80);
    assert!(
        simulation.state().get(ids.construction, tower).is_none(),
        "the fixture's premise: the tower completed"
    );
    assert_eq!(
        simulation
            .state()
            .get(ids.doomed, worker)
            .map(|doomed| doomed.death_index),
        Some(1),
        "the worker dies through its FADED rung"
    );
}

#[test]
fn cancelling_a_pad_build_fades_its_worker_too() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let (tower, worker) = pad_build(&b, &mut simulation, ids);
    press(&mut simulation, tower, RotwkCommand::CancelConstruction);
    run(&mut simulation, 1);
    assert!(!simulation.state().is_live(tower), "the site is gone");
    assert!(doomed(&simulation, ids, worker), "its worker is dying");
}

#[test]
fn a_castle_finished_in_play_unpacks_owned_and_neutral_members() {
    let b = load();
    let entry = |name: &str, x: f32| mint_sage_map::BaseEntry {
        template_name: name.to_owned(),
        offset: [x, 0.0],
        angle: 0.0,
        build_order: None,
    };
    let layouts: crate::castle::CastleLayouts = [(
        String::from("fieldcastle"),
        mint_sage_map::BaseLayout {
            entries: vec![entry("Tower", 40.0), entry("Rock", -40.0)],
            skipped: 0,
            refused: 0,
        },
    )]
    .into();
    let kind = b.kinds.kind("FieldPad").unwrap();
    let pad = b
        .kinds
        .spawn(
            &b.data.combat,
            kind,
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let production = b.kinds.production(&b.data);
    let neutral = PlayerId::new(2);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![pad])
        .with_players(2)
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_attack_ai(
            crate::combat::AttackUrgencyValues::default(),
            Vec::new(),
            vec![(PLAYER, String::from("Men"))],
        )
        .with_castles(Vec::new(), Arc::new(layouts))
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let pad = of_kind(&b, &simulation, ids, "FieldPad")[0];
    press(
        &mut simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    run(&mut simulation, 80);
    let site = of_kind(&b, &simulation, ids, "FieldSite")[0];
    let world = simulation.state();
    let held = |name: &str| {
        let entity = of_kind(&b, &simulation, ids, name)[0];
        (
            world.get(ids.owner, entity).unwrap().0,
            world.get(ids.castle_member, entity).unwrap().castle,
        )
    };
    assert_eq!(
        [held("Tower"), held("Rock")],
        [(PLAYER, site), (neutral, site)]
    );
}

#[test]
fn a_debug_complete_finishes_a_site_still_building() {
    let b = load();
    let (mut simulation, ids) = start(&b, "Site");
    let pad = of_kind(&b, &simulation, ids, "Pad")[0];
    press(
        &mut simulation,
        pad,
        RotwkCommand::FoundationConstruct { button: 0 },
    );
    let tower = of_kind(&b, &simulation, ids, "Tower")[0];
    assert!(simulation.state().get(ids.construction, tower).is_some());
    let kind = b.kinds.kind("Tower").unwrap().0;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::DebugCommand::Complete {
                target: crate::rules::DebugTarget::Kind { kind, side: None },
            }),
        )
        .unwrap();
    run(&mut simulation, 1);
    assert!(simulation.state().get(ids.construction, tower).is_none());
}

#[test]
fn a_debug_capture_hands_an_object_to_the_issuing_player() {
    let b = load();
    let (mut simulation, ids) = start_contested(&b);
    let theirs = of_kind(&b, &simulation, ids, "MapSite")[0];
    let kind = b.kinds.kind("MapSite").unwrap().0;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::DebugCommand::Capture {
                target: crate::rules::DebugTarget::Kind { kind, side: None },
            }),
        )
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(
        simulation
            .state()
            .get(ids.owner, theirs)
            .map(|owner| owner.0),
        Some(PLAYER)
    );
}
