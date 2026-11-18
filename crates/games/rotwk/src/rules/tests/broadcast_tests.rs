use super::*;
use mint_sage_modules::object_scripts::BroadcastKind;

const DATA: &str = "\
Object Standee\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ALLY: PlayerId = PlayerId::new(1);
const ENEMY_NEAR: PlayerId = PlayerId::new(2);
const ENEMY_FAR: PlayerId = PlayerId::new(3);
const ENEMY_OUTSIDE: PlayerId = PlayerId::new(4);

fn placed() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Standee".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |at: Vec2, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind("Standee").unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn(Vec2::new(0.0, 0.0), PLAYER),
        spawn(Vec2::new(10.0, 0.0), ALLY),
        spawn(Vec2::new(50.0, 0.0), ENEMY_NEAR),
        spawn(Vec2::new(30.0, 0.0), ENEMY_FAR),
        spawn(Vec2::new(500.0, 0.0), ENEMY_OUTSIDE),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![Some(0), Some(0), Some(1), Some(1), Some(1)]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities[0])
}

#[test]
fn enemies_broadcast_reaches_only_the_near_enemies_in_distance_order() {
    let (simulation, ids, broadcaster) = placed();
    let receivers = simulation.rules().broadcast_receivers(
        simulation.state(),
        ids,
        broadcaster,
        BroadcastKind::Enemies,
        100.0,
    );
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    assert_eq!(receivers, vec![entities[3], entities[2]]);
}

#[test]
fn allies_broadcast_reaches_the_broadcaster_and_the_ally() {
    let (simulation, ids, broadcaster) = placed();
    let receivers = simulation.rules().broadcast_receivers(
        simulation.state(),
        ids,
        broadcaster,
        BroadcastKind::Allies,
        1000.0,
    );
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    assert_eq!(receivers, vec![entities[0], entities[1]]);
}

const LOOP_EVENTS: &str = "<SageLuaScriptSection>\n\
                      <Events><InternalEvent Name=\"OnCreated\" /></Events>\n\
                      <EventList Name=\"TestLoopFunctions\">\n\
                      <EventHandler EventName=\"OnCreated\" ScriptFunctionName=\"Loop\"/>\n\
                      </EventList>\n\
                      </SageLuaScriptSection>";

const LOOP_HANDLERS: &str = "function Loop(self)\n  \
    ObjectEnterRampageState(self)\n  \
    ObjectBroadcastEventToAllies(self, \"OnCreated\", 1000)\n\
end\n";

const LOOP_DATA: &str = "\
Object ZLooper\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = TestLoopFunctions\n    \
    RampageTime = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_self_targeting_allies_broadcast_terminates_at_the_depth_cap() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        LOOP_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["ZLooper".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZLooper").unwrap(),
            Vec2::new(0.0, 0.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(LOOP_EVENTS, LOOP_HANDLERS)
                .unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let looper = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .entities()
        .to_vec()[0];
    assert!(
        simulation.state().is_live(looper),
        "the handler ran at least once without the loop crashing the tick"
    );
}
