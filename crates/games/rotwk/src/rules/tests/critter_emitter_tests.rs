use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
FXList ZFeathers\nEnd\n\
Object ZNest\n  KindOf = CRITTER_EMITTER\n  \
  Body = ImmortalBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = CritterEmitterUpdate Tag_Emit\n    FX = ZFeathers\n    ReloadTime = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 1\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(walker_x: f32) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZNest".to_owned(), "Walker".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZNest", Vec2::new(300.0, 300.0), OTHER),
        ("Walker", Vec2::new(walker_x, 300.0), PLAYER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(2);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let find = |name: &str| {
        let kind = kinds.kind(name).unwrap();
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .unwrap()
            .0
    };
    let (nest, walker) = (find("ZNest"), find("Walker"));
    (simulation, ids, nest, walker)
}

fn walk(simulation: &mut Simulation<RotwkRules>, walker: Entity, x: f32) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![walker],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
}

#[test]
fn a_unit_walking_into_an_emitter_spends_it_once_for_its_reload() {
    let (mut simulation, ids, nest, walker) = scene(200.0);
    walk(&mut simulation, walker, 400.0);
    let mut first: Option<(u64, u64)> = None;
    for step in 1..=120u64 {
        simulation.tick().unwrap();
        let Some(spent) = simulation.state().get(ids.critter_emitter_spent, nest) else {
            continue;
        };
        match first {
            None => {
                let x = simulation
                    .state()
                    .get(ids.body, walker)
                    .unwrap()
                    .0
                    .position
                    .x;
                assert!(
                    (x - 300.0).abs() <= 35.0,
                    "fired with the walker at x = {x}"
                );
                first = Some((step, spent.until.get()));
            }
            Some((_, until)) => assert_eq!(spent.until.get(), until, "re-fired at step {step}"),
        }
    }
    let (fired_at, until) = first.expect("the walk through fires the nest");
    assert_eq!(until, fired_at - 1 + 36, "fired on tick {}", fired_at - 1);
}

#[test]
fn a_unit_standing_still_inside_an_emitter_does_not_spend_it() {
    let (mut simulation, ids, nest, _walker) = scene(310.0);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().contains(ids.critter_emitter_spent, nest));
}
