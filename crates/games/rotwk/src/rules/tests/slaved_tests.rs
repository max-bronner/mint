use super::*;

fn data() -> String {
    let mut out = String::from(
        "Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
         TurnTime = 100\nEnd\n",
    );
    for (den, door, slave, extra) in [
        ("ZDen", "", "ZImp", ""),
        ("ZKennel", "", "ZHound", "    LeashRange = 90\n"),
        ("ZPit", "", "ZThrall", "    DieOnMastersDeath = Yes\n"),
        ("ZWarren", "", "ZScout", "    MarkUnselectable = Yes\n    LeashRange = 40\n"),
        (
            "ZBurrow",
            "  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:5 Y:0 Z:0\n    \
             NaturalRallyPoint = X:70 Y:0 Z:0\n  End\n",
            "ZGrub",
            "",
        ),
    ] {
        out.push_str(&format!(
            "Object {den}\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
             Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n{door}  \
             Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
             SpawnTemplateName = {slave}\n    SpawnReplaceDelay = 100000\n  End\n  \
             Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
             GeometryHeight = 30\nEnd\n\
             Object {slave}\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
             Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
             LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
             Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 60\n    \
             GuardWanderRange = 30\n{extra}  End\n  \
             Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
        ));
    }
    out.push_str(
        "Object ZTree\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
         Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
         Behavior = SpawnPointProductionExitUpdate Tag_Exit\n    SpawnPointBoneName = PERCH_\n  End\n  \
         Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 2\n    InitialBurst = 2\n    \
         SpawnTemplateName = ZSentinel\n    SpawnReplaceDelay = 100000\n  End\n  \
         Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
         GeometryHeight = 30\nEnd\n\
         Object ZSentinel\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
         Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
         LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n  \
         Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 0\n    GuardWanderRange = 0\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    );
    out
}

const PERCHES: [crate::perches::Perch; 2] = [
    crate::perches::Perch {
        position: [-20.0, 5.0, 30.0],
        facing: Facing::EAST,
    },
    crate::perches::Perch {
        position: [10.0, -15.0, 40.0],
        facing: Facing::EAST,
    },
];

const DEN_AT: Vec2 = Vec2::new(300.0, 300.0);

fn dens(names: &[&str]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data(), &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = names
        .iter()
        .zip(0u8..)
        .map(|(name, row)| {
            let at = Vec2::new(DEN_AT.x, DEN_AT.y + 200.0 * f32::from(row));
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let spawn_points = kinds
        .iter()
        .map(|entry| {
            if entry.record.name == "ZTree" {
                PERCHES.to_vec()
            } else {
                Vec::new()
            }
        })
        .collect();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_spawn_points(spawn_points);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let den_entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let mut slaves = Vec::new();
    for _ in 0..60 {
        simulation.tick().unwrap();
        slaves = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .filter(|entity| !den_entities.contains(entity))
            .collect();
        if slaves.len() >= names.len() {
            break;
        }
    }
    assert!(slaves.len() >= names.len(), "every den made its slave");
    (simulation, ids, den_entities, slaves)
}

fn order(simulation: &mut Simulation<RotwkRules>, units: Vec<Entity>, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
}

fn run(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    den: Entity,
    slave: Entity,
    ticks: u32,
) -> (f32, f32, f32, Vec<u64>) {
    let home = position(simulation, ids, den);
    let (mut farthest, mut walked) = (0.0f32, 0.0f32);
    let mut last = position(simulation, ids, slave);
    let mut hashes = Vec::new();
    for _ in 0..ticks {
        simulation.tick().unwrap();
        hashes.push(simulation.state_hash());
        let here = position(simulation, ids, slave);
        farthest = farthest.max(here.minus(home).length());
        walked += here.minus(last).length();
        last = here;
    }
    (farthest, last.minus(home).length(), walked, hashes)
}

#[test]
fn an_idle_slave_paces_within_its_wander_range_of_its_master() {
    let (mut simulation, ids, dens, slaves) = dens(&["ZDen"]);
    let (farthest, _, walked, _) = run(&mut simulation, ids, dens[0], slaves[0], 1200);
    assert!(
        farthest <= 30.0 + 15.0,
        "strayed {farthest:.0} from its master"
    );
    assert!(walked > 200.0, "walked only {walked:.0} in 40 s");
}

#[test]
fn a_spawn_appears_at_the_create_point_and_walks_out_to_the_rally_point() {
    let (mut simulation, ids, dens, slaves) = dens(&["ZBurrow"]);
    let home = position(&simulation, ids, dens[0]);
    let first = position(&simulation, ids, slaves[0]).minus(home);
    assert!(
        first.minus(Vec2::new(5.0, 0.0)).length() < 5.0,
        "appeared at {first:?} from the burrow"
    );
    let rally = home.plus(Vec2::new(70.0, 0.0));
    let mut nearest = f32::MAX;
    for _ in 0..150 {
        simulation.tick().unwrap();
        nearest = nearest.min(position(&simulation, ids, slaves[0]).minus(rally).length());
    }
    assert!(
        nearest < 10.0,
        "came only {nearest:.0} from the rally point"
    );
}

#[test]
fn an_ordered_slave_is_turned_back_at_its_stray_limit() {
    for (den, limit) in [("ZDen", 60.0), ("ZKennel", 90.0)] {
        let (mut simulation, ids, dens, slaves) = dens(&[den]);
        let goal = DEN_AT.plus(Vec2::new(500.0, 0.0));
        order(
            &mut simulation,
            vec![slaves[0]],
            RotwkCommand::MoveTo {
                x: goal.x,
                y: goal.y,
                queue: false,
            },
        );
        let (farthest, end, _, _) = run(&mut simulation, ids, dens[0], slaves[0], 900);
        assert!(
            (limit..=limit + 40.0).contains(&farthest),
            "{den}: got {farthest:.0} from its master, limit {limit}"
        );
        assert!(end <= 100.0, "{den}: ended {end:.0} from its master");
    }
}

#[test]
fn a_slave_dies_with_its_master_only_when_its_data_says_so() {
    let (mut simulation, ids, dens, slaves) = dens(&["ZDen", "ZPit"]);
    order(&mut simulation, dens, RotwkCommand::DebugKill);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let dead = |slave: Entity| {
        let world = simulation.state();
        !world.is_live(slave) || world.contains(ids.doomed, slave)
    };
    assert_eq!(
        (dead(slaves[0]), dead(slaves[1])),
        (false, true),
        "(free slave dead, bound slave dead)"
    );
}

#[test]
fn mark_unselectable_raises_on_attach_and_clears_on_release() {
    let (mut simulation, _ids, dens, slaves) = dens(&["ZDen", "ZWarren"]);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let unselectable = |sim: &Simulation<RotwkRules>, entity: Entity| {
        sim.rules()
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(sim.state(), entity, "UNSELECTABLE"))
    };
    assert!(
        !unselectable(&simulation, slaves[0]),
        "ZImp declares no MarkUnselectable"
    );
    assert!(
        unselectable(&simulation, slaves[1]),
        "ZScout's MarkUnselectable should have raised UNSELECTABLE on attach"
    );
    order(&mut simulation, vec![dens[1]], RotwkCommand::DebugKill);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(
        !unselectable(&simulation, slaves[1]),
        "freed by its master's death (no DieOnMastersDeath), UNSELECTABLE should clear"
    );
}

#[test]
fn leashed_returning_marks_a_busy_slave_walking_back_from_beyond_its_leash() {
    let (mut simulation, _ids, _dens, slaves) = dens(&["ZKennel"]);
    order(
        &mut simulation,
        vec![slaves[0]],
        RotwkCommand::MoveTo {
            x: 800.0,
            y: 300.0,
            queue: false,
        },
    );
    let leashed_returning = |sim: &Simulation<RotwkRules>, entity: Entity| {
        sim.rules()
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(sim.state(), entity, "LEASHED_RETURNING"))
    };
    let mut saw_it = false;
    for _ in 0..450 {
        simulation.tick().unwrap();
        if leashed_returning(&simulation, slaves[0]) {
            saw_it = true;
        }
    }
    assert!(
        saw_it,
        "the hound should have been marked LEASHED_RETURNING while walked back from beyond its leash"
    );
    assert!(
        !leashed_returning(&simulation, slaves[0]),
        "it should have settled back near its master and cleared the mark by now"
    );
}

#[test]
fn a_leashed_slave_replays_to_the_same_hash_sequence() {
    let hashes = || {
        let (mut simulation, ids, dens, slaves) = dens(&["ZKennel"]);
        order(
            &mut simulation,
            vec![slaves[0]],
            RotwkCommand::MoveTo {
                x: 800.0,
                y: 300.0,
                queue: false,
            },
        );
        run(&mut simulation, ids, dens[0], slaves[0], 450).3
    };
    assert_eq!(hashes(), hashes());
}

#[test]
fn a_spawn_point_exit_stands_each_spawn_on_its_own_bone_where_it_stays() {
    let (mut simulation, ids, dens, slaves) = dens(&["ZTree"]);
    assert_eq!(slaves.len(), 2, "one spawn per bone");
    let home = position(&simulation, ids, dens[0]);
    for (slave, perch) in slaves.iter().zip(PERCHES) {
        let offset = position(&simulation, ids, *slave).minus(home);
        let bone = Vec2::new(perch.position[0], perch.position[1]);
        assert!(
            offset.minus(bone).length() < 0.01,
            "stood at {offset:?} from the tree, bone at {bone:?}"
        );
        assert_eq!(
            simulation
                .state()
                .get(ids.ground_offset, *slave)
                .map(|g| g.0),
            Some(perch.position[2])
        );
    }
    let before: Vec<Vec2> = slaves
        .iter()
        .map(|slave| position(&simulation, ids, *slave))
        .collect();
    for tick in 0..600 {
        simulation.tick().unwrap();
        for slave in &slaves {
            assert!(
                !simulation.state().contains(ids.path, *slave),
                "tick {tick}: a spawn on its spawn point holds a route"
            );
        }
    }
    let after: Vec<Vec2> = slaves
        .iter()
        .map(|slave| position(&simulation, ids, *slave))
        .collect();
    assert_eq!(before, after);
}
