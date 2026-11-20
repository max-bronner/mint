use super::ability_b1_tests::{run, scene, scene_on};
use super::*;

const AT: Vec2 = Vec2::new(100.0, 100.0);

const SOLDIER: &str = "\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn aura(name: &str, list: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
         Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
         Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
         BonusName = {list}\n    RefreshDelay = 2000\n    ObjectFilter = ANY +INFANTRY\n    \
         Range = 100\n  End\n  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  \
         GeometryHeight = 20\nEnd\n"
    )
}

fn played(simulation: &mut Simulation<RotwkRules>, ticks: u32) -> Vec<String> {
    let mut names = Vec::new();
    for _ in 0..ticks {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            names.extend(simulation.rules().fx_name(event.list).map(str::to_owned));
        }
    }
    names
}

#[test]
fn a_multi_level_list_plays_the_fx_its_categorys_counter_picks() {
    let lists: String = ["A", "B", "C"]
        .iter()
        .map(|n| {
            format!(
                "ModifierList L{n}\n  Category = LEADERSHIP\n  Modifier = DAMAGE_MULT 110%\n  \
                 Duration = 1000\n  MultiLevelFX = Yes\n  FX = Fx{n}1\n  FX2 = Fx{n}2\n  \
                 FX3 = Fx{n}3\nEnd\n"
            )
        })
        .collect();
    let auras: String = ["A", "B", "C"]
        .iter()
        .map(|n| aura(&format!("Aura{n}"), &format!("L{n}")))
        .collect();
    let data = format!("{SOLDIER}{lists}{auras}");
    let (mut simulation, _ids, _entities) = scene(
        &data,
        &[
            ("AuraA", AT, PLAYER),
            ("AuraB", AT, PLAYER),
            ("AuraC", AT, PLAYER),
            ("Soldier", AT, PLAYER),
        ],
    );
    let names = played(&mut simulation, 10);
    for want in ["FxA1", "FxB1", "FxC2"] {
        assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
    }
    for unwanted in ["FxB2", "FxC1", "FxC3"] {
        assert!(
            !names.iter().any(|n| n == unwanted),
            "{unwanted} in {names:?}"
        );
    }
}

fn has_condition(simulation: &Simulation<RotwkRules>, entity: Entity, name: &str) -> bool {
    simulation
        .rules()
        .holds_condition_in(simulation.state(), entity, name)
}

#[test]
fn an_ended_list_raises_snow_again_on_a_snowy_map_only() {
    let data = format!(
        "{SOLDIER}ModifierList Melt
  Category = SPELL
  Modifier = DAMAGE_MULT 110%
           Duration = 1000
  ModelCondition = SNOW
End
{}",
        aura("AuraMelt", "Melt")
    );
    for snowy in [true, false] {
        let (mut simulation, _ids, entities) = scene_on(
            &data,
            &[("AuraMelt", AT, PLAYER), ("Soldier", AT, PLAYER)],
            snowy,
        );
        let soldier = entities[1];
        run(&mut simulation, 40);
        assert_eq!(has_condition(&simulation, soldier, "SNOW"), snowy);
    }
}

const SCREECH_DATA: &str = "\
SpecialPower SpecialAbilityTestScreech\n  ReloadTime = 30000\n  Enum = SPECIAL_SCREECH\nEnd\n\
CommandSet ScreechSet\n  1 = Fire_Screech\nEnd\n\
CommandButton Fire_Screech\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityTestScreech\nEnd\n\
EmotionNugget ZTerror\n  Type = TERROR\n  ModelConditions = EMOTION_TERROR\n  Duration = 3000\nEnd\n\
Object Screamer\n  Side = Men\n  KindOf = HERO SELECTABLE\n  CommandSet = ScreechSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_Ability\n    \
    SpecialPowerTemplate = SpecialAbilityTestScreech\n    EffectRange = 150\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = ScreamFunctions\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Peasant\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = PeasantFunctions\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    AddEmotion = ZTerror\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Champion\n  Side = Mordor\n  KindOf = HERO SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 1000\n    \
    AddEmotion = ZTerror\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const SCREECH_EVENTS: &str = "<SageLuaScriptSection>\n\
    <Events><InternalEvent Name=\"BeScary\" /><InternalEvent Name=\"BeTerrified\" /></Events>\n\
    <EventList Name=\"ScreamFunctions\">\n\
    <EventHandler EventName=\"BeScary\" ScriptFunctionName=\"Scary\"/>\n</EventList>\n\
    <EventList Name=\"PeasantFunctions\">\n\
    <EventHandler EventName=\"BeTerrified\" ScriptFunctionName=\"Panic\"/>\n</EventList>\n\
    </SageLuaScriptSection>";

const SCREECH_HANDLERS: &str = "function Scary(self, other, range)\n  \
    ObjectBroadcastEventToEnemies(self, \"BeTerrified\", range)\nend\n\
function Panic(self, other)\n  ObjectEnterRunAwayPanicState(self, other)\nend\n";

fn screech_scene() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        SCREECH_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Screamer", "Peasant", "Champion", "Peasant"];
    let places = [
        Vec2::new(300.0, 300.0),
        Vec2::new(360.0, 300.0),
        Vec2::new(300.0, 360.0),
        Vec2::new(700.0, 300.0),
    ];
    let names: Vec<String> = roots.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let spawns = roots
        .iter()
        .zip(places)
        .enumerate()
        .map(|(at, (name, place))| {
            let owner = if at == 0 { PLAYER } else { OTHER };
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    place,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(
                SCREECH_EVENTS,
                SCREECH_HANDLERS,
            )
            .unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities)
}

#[test]
fn a_screech_terrifies_through_the_script_chain() {
    let (mut simulation, ids, entities) = screech_scene();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[0]],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    let mut felt = [false; 4];
    for _ in 0..120 {
        simulation.tick().unwrap();
        for (at, entity) in entities.iter().enumerate() {
            felt[at] |= simulation.state().get(ids.emotion, *entity).is_some();
        }
    }
    assert!(felt[1], "the peasant in range is terrified");
    assert!(!felt[2], "a champion with no BeTerrified handler is not");
    assert!(!felt[3], "a peasant beyond EffectRange is not");
}
