use super::*;

const DATA: &str = "\
GameData\n  SellPercentage = 50%\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object Shed\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 2\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    WorkerName = ShedWorkerNoSelect\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 40\n  GeometryMinorRadius = 40\n  GeometryHeight = 30\nEnd\n\
Object ShedWorkerNoSelect\n  Side = Men\n  KindOf = INFANTRY\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 12\nEnd\n\
CommandSet PorterSet\n  1 = Build_Shed\nEnd\n\
CommandButton Build_Shed\n  Command = DOZER_CONSTRUCT\n  Object = Shed\nEnd\n";

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
    let seeds: Vec<String> = ["Porter", "Shed"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(b: &Base, porter_at: (f32, f32)) -> (Simulation<RotwkRules>, Ids, Entity) {
    start_as(b, porter_at, false)
}

fn start_as(
    b: &Base,
    porter_at: (f32, f32),
    computer: bool,
) -> (Simulation<RotwkRules>, Ids, Entity) {
    let production = b.kinds.production(&b.data);
    let porter_kind = b.kinds.kind("Porter").unwrap();
    let spawn = b
        .kinds
        .spawn(
            &b.data.combat,
            porter_kind,
            Vec2::new(porter_at.0, porter_at.1),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_players(1)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_sell_percentage(b.data.sell_percentage);
    let rules = if computer {
        rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        )
    } else {
        rules
    };
    let simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    (simulation, ids, porter)
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

#[test]
fn a_porter_built_site_spawns_no_worker() {
    let b = load();
    let (mut simulation, ids, porter) = start(&b, (150.0, 200.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: b.kinds.kind("Shed").unwrap().0,
                x: 150.0,
                y: 200.0,
                angle: 0.0,
            },
        )
        .unwrap();
    let mut site = None;
    for _ in 0..600 {
        simulation.tick().unwrap();
        site = of_kind(&b, &simulation, ids, "Shed").first().copied();
        if site.is_some() {
            break;
        }
    }
    let site = site.expect("the site appeared");
    let construction = simulation.state().get(ids.construction, site).copied();
    assert_eq!(construction.and_then(|c| c.builder), Some(porter));
    assert_eq!(construction.and_then(|c| c.worker), None);
    assert!(
        of_kind(&b, &simulation, ids, "ShedWorkerNoSelect").is_empty(),
        "no worker beside a porter's site"
    );
}

fn construct_sounds(computer: bool) -> Vec<(&'static str, PlayerId, f32, f32)> {
    let b = load();
    let (mut simulation, _, porter) = start_as(&b, (150.0, 200.0), computer);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![porter],
        },
        RotwkCommand::Construct {
            kind: b.kinds.kind("Shed").unwrap().0,
            x: 150.0,
            y: 200.0,
            angle: 0.0,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    let frame = Frame::new(simulation.frame().get() - 1);
    simulation
        .rules()
        .misc_sounds(simulation.state(), frame)
        .iter()
        .map(|e| (e.sound.field(), e.player, e.at.x, e.at.y))
        .collect()
}

#[test]
fn a_construct_order_raises_the_placement_sound_for_a_player_only() {
    assert_eq!(
        construct_sounds(false),
        [("BuildingPlacementSound", PLAYER, 150.0, 200.0)]
    );
    assert_eq!(construct_sounds(true), []);
}
