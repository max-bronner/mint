use super::*;

const DATA: &str = "\
Object Field\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = TerrainResourceBehavior Tag_02\n    Radius = 100\n    MaxIncome = 100\n    \
    IncomeInterval = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n";

fn two_fields() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Field".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind("Field").unwrap();
    let spawns: Vec<Spawn> = [300.0, 400.0]
        .iter()
        .map(|x| {
            kinds
                .spawn(
                    &data.combat,
                    kind,
                    Vec2::new(*x, 300.0),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_claim_cell_size(Some(20.0));
    let mut simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let fields: Vec<Entity> = simulation
        .state()
        .store(ids.claimant)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, fields[0], fields[1])
}

#[test]
fn a_claim_half_over_another_pays_its_share_of_max_income() {
    let (simulation, ids, first, second) = two_fields();
    let share = |field| {
        simulation
            .state()
            .get(ids.claimant, field)
            .unwrap()
            .share
            .unwrap()
    };
    let payment = |field| {
        simulation
            .state()
            .get(ids.income, field)
            .unwrap()
            .0
            .payment()
    };
    assert_eq!((share(first), payment(first)), (1.0, 100));
    let later = share(second);
    assert!((0.5..0.75).contains(&later), "share {later}");
    #[allow(clippy::cast_possible_truncation)]
    let scaled = (100.0 * later) as i64;
    assert_eq!(payment(second), scaled, "share {later}");
}

const PLAN_DATA: &str = "\
Object Porter\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  ShroudClearingRange = 3000\n  Body = ActiveBody Tag_01\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
CommandSet PorterSet\n  1 = Build_Field\nEnd\n\
CommandButton Build_Field\n  Command = DOZER_CONSTRUCT\n  Object = Field\nEnd\n\
Object Field\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  BuildCost = 100\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = TerrainResourceBehavior Tag_02\n    Radius = 100\n    MaxIncome = 100\n    \
    IncomeInterval = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n";

fn share_left(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    viewer: PlayerId,
    centre: Vec2,
) -> f32 {
    let rules = simulation.rules();
    let field = rules.kinds.kind("Field").unwrap();
    let claimant = crate::templates::claimant(&rules.kinds.get(field).unwrap().record).unwrap();
    rules
        .preview_claim(simulation.state(), ids, &claimant, centre, viewer)
        .unwrap()
        .share()
}

fn porters(x: f32, other_x: f32) -> (Simulation<RotwkRules>, Ids, Entity, Entity, u16) {
    let data = crate::game::tests::data_from(PLAN_DATA);
    let (kinds, missing) = KindTable::closure(&data, &["Porter".to_owned(), "Field".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let porter = |x: f32, owner| {
        kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(x, 300.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![porter(x, PLAYER), porter(other_x, OTHER)])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_economy(Vec::new(), 1_000)
        .with_claim_cell_size(Some(20.0));
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let field = kinds.kind("Field").unwrap().0;
    (simulation, ids, units[0], units[1], field)
}

fn order(simulation: &mut Simulation<RotwkRules>, ids: Ids, porter: Entity, field: u16, at: Vec2) {
    let owner = simulation.state().get(ids.owner, porter).unwrap().0;
    simulation
        .submit(
            owner,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter],
            },
        )
        .unwrap();
    simulation
        .submit(
            owner,
            RotwkCommand::Construct {
                kind: field,
                x: at.x,
                y: at.y,
                angle: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

fn share_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.claimant, entity)
        .and_then(|claimant| claimant.share)
        .unwrap()
}

#[test]
fn a_plan_claims_from_the_order_and_its_site_keeps_the_claim() {
    let (mut simulation, ids, porter, _, field) = porters(100.0, 1500.0);
    let spot = Vec2::new(500.0, 300.0);
    order(&mut simulation, ids, porter, field, spot);
    assert!(simulation.state().contains(ids.build_task, porter));
    assert_eq!(
        share_left(&simulation, ids, PLAYER, spot),
        0.0,
        "the plan holds its ground"
    );

    let elsewhere = Vec2::new(280.0, 300.0);
    order(&mut simulation, ids, porter, field, elsewhere);
    assert_eq!(
        share_left(&simulation, ids, PLAYER, spot),
        1.0,
        "a dropped plan frees its ground"
    );
    assert_eq!(share_left(&simulation, ids, PLAYER, elsewhere), 0.0);

    let site = tick_until_site(&mut simulation, ids);
    simulation.tick().unwrap();
    assert_eq!(
        share_of(&simulation, ids, site),
        1.0,
        "the site keeps the plan's share"
    );
    assert_eq!(
        share_left(&simulation, ids, PLAYER, elsewhere),
        0.0,
        "and its ground"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![site],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(site) {
            break;
        }
    }
    assert!(!simulation.state().is_live(site), "the site never died");
    assert_eq!(
        share_left(&simulation, ids, PLAYER, elsewhere),
        1.0,
        "the cells are held under the site's own key, so killing it frees them"
    );
}

#[test]
fn an_enemy_plan_shares_my_ground_until_the_first_site_is_built() {
    let (mut simulation, ids, mine, theirs, field) = porters(300.0, 800.0);
    let (my_spot, their_spot) = (Vec2::new(430.0, 300.0), Vec2::new(530.0, 300.0));
    order(&mut simulation, ids, theirs, field, their_spot);
    assert_eq!(
        share_left(&simulation, ids, PLAYER, my_spot),
        1.0,
        "an enemy's plan is not in my preview"
    );
    assert!(
        share_left(&simulation, ids, OTHER, my_spot) < 0.75,
        "but is in theirs"
    );
    order(&mut simulation, ids, mine, field, my_spot);
    let registered = crate::claims::layer_of(simulation.state(), ids.claims)
        .unwrap()
        .registered()
        .to_vec();
    assert_eq!(registered.len(), 2);
    assert!(
        registered.iter().all(|claimant| claimant.share == 1.0),
        "{registered:?}"
    );

    let mut sites = Vec::new();
    for _ in 0..6000 {
        simulation.tick().unwrap();
        sites = simulation
            .state()
            .store(ids.claimant)
            .unwrap()
            .entities()
            .to_vec();
        let built = |site: &Entity| !simulation.state().contains(ids.construction, *site);
        if sites.len() == 2 && sites.iter().all(built) {
            break;
        }
    }
    simulation.tick().unwrap();
    let owner = |site| simulation.state().get(ids.owner, site).unwrap().0;
    let my_site = *sites.iter().find(|site| owner(**site) == PLAYER).unwrap();
    let their_site = *sites.iter().find(|site| owner(**site) == OTHER).unwrap();
    assert_eq!(
        share_of(&simulation, ids, my_site),
        1.0,
        "built first, it took the shared ground"
    );
    let theirs = share_of(&simulation, ids, their_site);
    assert!(
        (0.3..0.75).contains(&theirs),
        "built later, they keep the rest: {theirs}"
    );
}

#[test]
fn a_claimant_gone_lets_its_neighbour_claim_its_ground() {
    let (mut simulation, ids, first, second) = two_fields();
    assert!(share_of(&simulation, ids, second) < 0.75);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![first],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(first) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(first),
        "the first field is gone"
    );
    simulation.tick().unwrap();
    assert_eq!(share_of(&simulation, ids, second), 1.0);
}
