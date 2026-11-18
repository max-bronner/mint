use std::collections::BTreeMap;

use mint_sage_ini::Stance;
use mint_sage_map::{ScriptTree, Value};

use super::*;
use crate::map_keys::ObjectKeys;
use crate::rules::map_scripts::{MapScriptEngine, ScriptStatusBit};

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
StanceTemplate Plain\n  Stance Aggressive\n  End\n  Stance HoldGround\n  End\n  \
  Stance Porcupine\n  End\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = StancesBehavior Tag_Stance\n    StanceTemplate = Plain\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  BuildCost = 500\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object Brute\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    RampageTime = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const EVENTS: &str = "<SageLuaScriptSection>\n\
                      <Events><InternalEvent Name=\"OnCreated\" /></Events>\n\
                      <EventList Name=\"TestCreatedFunctions\">\n\
                      <EventHandler EventName=\"OnCreated\" ScriptFunctionName=\"Rage\"/>\n\
                      </EventList>\n\
                      </SageLuaScriptSection>";

const HANDLERS: &str = "function Rage(self)\n  ObjectEnterRampageState(self)\nend\n";

fn scene(props: Vec<(usize, ObjectKeys)>) -> (Simulation<RotwkRules>, Ids, [Entity; 3]) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier", "Camp", "Brute"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = ["Soldier", "Camp", "Brute"]
        .iter()
        .zip([100.0, 200.0, 300.0])
        .map(|(name, x)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(x, 200.0),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let engine = MapScriptEngine::build(
        &[ScriptTree {
            player_slots: vec![Vec::new()],
        }],
        &[vec![String::new()]],
        &BTreeMap::new(),
        &BTreeMap::new(),
        false,
    );
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production)
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ))
        .with_map_scripts(Some(engine), vec![(0, PLAYER)])
        .with_placed_keys(props);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let all = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, [all[0], all[1], all[2]])
}

fn props(edit: impl FnOnce(&mut ObjectKeys)) -> ObjectKeys {
    let mut props = ObjectKeys::default();
    edit(&mut props);
    props
}

#[test]
fn an_object_placed_unselectable_drops_out_of_a_selection() {
    let (mut simulation, ids, [soldier, camp, _]) =
        scene(vec![(1, props(|p| p.selectable = Some(false)))]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![soldier, camp],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(simulation.state().contains(ids.selected, soldier));
    assert!(!simulation.state().contains(ids.selected, camp));
}

#[test]
fn a_placed_stance_ordinal_names_the_engine_tables_stance() {
    for (ordinal, stance) in [(2, Stance::Aggressive), (3, Stance::HoldGround)] {
        let (simulation, ids, [soldier, _, _]) =
            scene(vec![(0, props(|p| p.initial_stance = Some(ordinal)))]);
        assert_eq!(
            simulation
                .rules()
                .stance_of(simulation.state(), ids, soldier),
            Some(stance),
            "ordinal {ordinal}"
        );
    }
}

#[test]
fn placed_enabled_false_and_unsellable_set_the_script_status_bits() {
    let (simulation, ids, [_, camp, _]) = scene(vec![(
        1,
        props(|p| {
            p.enabled = Some(false);
            p.unsellable = Some(true);
        }),
    )]);
    let has = |bit| {
        simulation
            .rules()
            .script_status_bit(simulation.state(), ids, camp, bit)
    };
    assert!(has(ScriptStatusBit::Disabled));
    assert!(has(ScriptStatusBit::Unsellable));
    assert_eq!(
        simulation
            .rules()
            .sell_refusal(simulation.state(), ids, PLAYER, camp),
        Some("unsellable (script status)")
    );
}

#[test]
fn a_placed_events_list_runs_for_an_object_whose_template_names_none() {
    let rampaging = |list: Option<&str>| {
        let placed = list.map_or_else(Vec::new, |name| {
            vec![(2, props(|p| p.events_list = Some(name.to_owned())))]
        });
        let (mut simulation, ids, [_, _, brute]) = scene(placed);
        simulation.tick().unwrap();
        simulation.state().contains(ids.rampaging, brute)
    };
    assert!(!rampaging(None), "the control rampaged");
    assert!(rampaging(Some("TestCreatedFunctions")));
}

#[test]
fn object_keys_read_only_a_key_of_the_right_type() {
    let object = mint_sage_map::Object {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        angle: 0.0,
        flags: 0,
        template_name: String::new(),
        properties: mint_sage_map::Dictionary::from_pairs(vec![
            ("objectInitialStance".to_owned(), Value::Int(3)),
            ("objectSelectable".to_owned(), Value::Int(1)),
            ("objectEnabled".to_owned(), Value::Bool(false)),
        ]),
    };
    let read = ObjectKeys::from_object(&object);
    assert_eq!(read.initial_stance, Some(3));
    assert_eq!(read.selectable, None);
    assert_eq!(read.enabled, Some(false));
}
