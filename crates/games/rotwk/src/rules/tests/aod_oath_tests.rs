use super::*;

const DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  ZAxisBehavior = FLOATING_Z\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  MaxTurnWithoutReform = 45\nEnd\n\
Object Ghost\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = AODCrushCollide Tag_Crush\n    DamageType = MAGIC_WOUND\n    \
    DeathType = SUPERNATURAL\n    Damage = 1\n  End\n  \
  LocomotorSet\n    Locomotor = MemberFeet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Column\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = AODHordeContain Tag_Contain\n    Slots = 2\n    \
    ScatterSpeedFactor = 50\n    ScatterRandomness = 0\n    OathFulfilledZFactor = 2\n    \
    LargeUnitHeightFactor = 1\n    LargeUnitMinHeight = 10\n    LargeUnitMaxHeight = 180\n    \
    LargeUnitTimeout = 9000\n    LargeUnitTailOff = 1.5\n    \
    RankInfo = RankNumber:1 UnitType:Ghost Position:X:0 Y:20 Position:X:0 Y:-20\n  \
  End\nEnd\n\
Object Troll\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  DeadCollideSize = LARGE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 200\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Column\nEnd\n\
CommandButton Build_Column\n  Command = UNIT_BUILD\n  Object = Column\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Column", "Troll"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind("Troll").unwrap(),
            Vec2::new(300.0, 420.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap()];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Column")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 400.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = *simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .unwrap();
    (simulation, ids, horde)
}

fn select(simulation: &mut Simulation<RotwkRules>, ids: Ids, horde: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    let _ = ids;
}

fn member_holds_oath(simulation: &Simulation<RotwkRules>, member: Entity) -> bool {
    simulation.rules().modules.as_ref().is_some_and(|wiring| {
        simulation
            .state()
            .get(wiring.state, member)
            .is_some_and(|state| {
                state
                    .model_condition_names()
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case("OATH_FULLFILLED"))
            })
    })
}

#[test]
fn debug_oath_sets_the_horde_and_copies_down_to_its_members() {
    let (mut simulation, ids, horde) = scene();
    select(&mut simulation, ids, horde);
    simulation.submit(PLAYER, RotwkCommand::DebugOath).unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        member_holds_oath(&simulation, horde),
        "the horde itself should hold OATH_FULLFILLED"
    );
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(members.len(), 2);
    for member in members {
        assert!(
            member_holds_oath(&simulation, member),
            "member {member:?} never got OATH_FULLFILLED"
        );
    }
}

#[test]
fn a_held_member_scatters_outward_and_rises() {
    let (mut simulation, ids, horde) = scene();
    select(&mut simulation, ids, horde);
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let before = simulation.state().get(ids.body, member).unwrap().0.position;
    simulation.submit(PLAYER, RotwkCommand::DebugOath).unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let after = simulation.state().get(ids.body, member).unwrap().0.position;
    let drift = after.minus(before).length();
    assert!(drift > 20.0, "expected real scatter, found {drift}");
    let height = simulation
        .state()
        .get(ids.elevation, member)
        .map_or(0.0, |elevation| elevation.0);
    assert!(
        height > 0.5,
        "expected the oath rise to lift it, found {height}"
    );
}

#[test]
fn a_member_over_a_large_unit_floats_above_its_untouched_neighbour() {
    let (mut simulation, ids, horde) = scene();
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let heights: Vec<f32> = members
        .iter()
        .map(|member| {
            simulation
                .state()
                .get(ids.elevation, *member)
                .map_or(0.0, |elevation| elevation.0)
        })
        .collect();
    assert_eq!(heights.len(), 2);
    let (touched, untouched) = (heights[0].max(heights[1]), heights[0].min(heights[1]));
    assert!(
        untouched < 0.1,
        "the far member should have no lift: {heights:?}"
    );
    assert!(touched > 2.0, "the near member should flow up: {heights:?}");
}

#[test]
fn a_joining_member_fades_in_over_five_seconds() {
    let (simulation, ids, horde) = scene();
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let world = simulation.state();
    let rules = simulation.rules();
    let start = rules.join_fade_opacity(world, member, mint_sim::Frame::new(0));
    assert!(start < 0.1, "expected transparent at join, found {start}");
    let later = rules.join_fade_opacity(world, member, mint_sim::Frame::new(150));
    assert!(later > 0.99, "expected opaque after 5s, found {later}");
}
