use std::collections::BTreeMap;

use mint_sage_map::{Dictionary, TeamDef, Value};

use super::*;
use crate::map_keys::ObjectKeys;
use crate::map_players::{MapEntity, Relation};
use crate::rules::map_scripts::{MapScriptEngine, ScriptStatusBit, TeamProtos};

const RANK_POSITIONS: &str = "Position:X:10 Y:-30 Position:X:10 Y:-10 Position:X:10 Y:10 \
    Position:X:10 Y:30 Position:X:-10 Y:-30 Position:X:-10 Y:-10 Position:X:-10 Y:10 \
    Position:X:-10 Y:30";

fn data_text() -> String {
    format!(
        "Upgrade Upgrade_A\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_B\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_Parent\n  Type = OBJECT\n  SubUpgradeTemplateNames = Upgrade_Sub\nEnd\n\
Upgrade Upgrade_Sub\n  Type = OBJECT\nEnd\n\
ExperienceLevel PawnLevel1\n  TargetNames = Pawn PawnHorde\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel PawnLevel2\n  TargetNames = Pawn PawnHorde\n  RequiredExperience = 7\n  Rank = 2\nEnd\n\
ExperienceLevel PawnLevel3\n  TargetNames = Pawn PawnHorde\n  RequiredExperience = 20\n  Rank = 3\nEnd\n\
ExperienceLevel PawnLevel4\n  TargetNames = Pawn PawnHorde\n  RequiredExperience = 40\n  Rank = 4\nEnd\n\
Object Pawn\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PawnHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  VisionRange = 400\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 8\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Pawn {RANK_POSITIONS}\n  End\nEnd\n"
    )
}

struct Fixture {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn fixture() -> Fixture {
    let data = crate::game::tests::data_from(&data_text());
    let roots = [String::from("Pawn"), String::from("PawnHorde")];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    Fixture {
        data,
        kinds: Arc::new(kinds),
    }
}

fn base_rules(fixture: &Fixture, spawns: Vec<Spawn>) -> RotwkRules {
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    RotwkRules::new(passability, spawns)
        .with_hordes(fixture.data.formations.tables().to_vec(), Vec::new())
        .with_combat(fixture.data.combat.clone())
        .with_construction(
            Arc::clone(&fixture.kinds),
            fixture.kinds.production(&fixture.data),
        )
        .with_upgrades(
            fixture.kinds.upgrades(&fixture.data),
            Arc::clone(&fixture.data.upgrade_bits),
        )
        .with_experience(
            Arc::clone(&fixture.data.experience),
            fixture.data.armor_bonus_cap,
        )
}

fn entity(template: &str, keys: ObjectKeys) -> MapEntity {
    MapEntity {
        object: 0,
        template: template.to_owned(),
        owner: Relation::Contestant(PLAYER),
        at: Vec2::new(40.0, 40.0),
        angle: 0.0,
        z: 0.0,
        script_name: None,
        team: None,
        indestructible: false,
        keys,
    }
}

fn horde_sim(keys: ObjectKeys) -> (Simulation<RotwkRules>, Fixture) {
    let fixture = fixture();
    let kind = fixture.kinds.kind("PawnHorde").unwrap();
    let placed = crate::skirmish::placed_horde(
        &fixture.data,
        &fixture.kinds,
        &entity("PawnHorde", keys),
        kind,
        Facing::EAST,
        PLAYER,
        None,
    )
    .unwrap();
    let rules = base_rules(&fixture, Vec::new()).with_placed_hordes(vec![placed]);
    (Simulation::new(3, rules).unwrap(), fixture)
}

fn pawn_spawn(fixture: &Fixture) -> Spawn {
    let kind = fixture.kinds.kind("Pawn").unwrap();
    fixture
        .kinds
        .spawn(
            &fixture.data.combat,
            kind,
            Vec2::new(40.0, 40.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap()
}

fn pawn_sim(keys: ObjectKeys) -> (Simulation<RotwkRules>, Fixture) {
    let fixture = fixture();
    let rules = base_rules(&fixture, vec![pawn_spawn(&fixture)]).with_placed_keys(vec![(0, keys)]);
    (Simulation::new(3, rules).unwrap(), fixture)
}

fn holds(
    simulation: &Simulation<RotwkRules>,
    fixture: &Fixture,
    entity: Entity,
    name: &str,
) -> bool {
    let ids = simulation.rules().ids().unwrap();
    let bit = fixture.data.upgrade_bits.bit(name).unwrap();
    simulation
        .state()
        .get(ids.object_upgrades, entity)
        .is_some_and(|held| held.0.contains(bit))
}

fn upgrades(names: &[&str]) -> ObjectKeys {
    ObjectKeys {
        upgrades: names.iter().map(|name| (*name).to_owned()).collect(),
        ..ObjectKeys::default()
    }
}

#[test]
fn a_placed_hordes_upgrade_list_reaches_the_horde_and_every_member() {
    let (simulation, fixture) = horde_sim(upgrades(&["Upgrade_A", "Upgrade_Nope", "Upgrade_B"]));
    let ids = simulation.rules().ids().unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 8);
    for holder in std::iter::once(horde).chain(members) {
        for name in ["Upgrade_A", "Upgrade_B"] {
            assert!(
                holds(&simulation, &fixture, holder, name),
                "{holder:?} {name}"
            );
        }
    }
}

#[test]
fn an_upgrade_that_lists_sub_upgrades_grants_them_and_not_itself() {
    let (simulation, fixture) = pawn_sim(upgrades(&["Upgrade_Parent"]));
    let ids = simulation.rules().ids().unwrap();
    let pawn = simulation.state().store(ids.kind).unwrap().entities()[0];
    assert!(holds(&simulation, &fixture, pawn, "Upgrade_Sub"));
    assert!(!holds(&simulation, &fixture, pawn, "Upgrade_Parent"));
}

#[test]
fn an_experience_level_raises_a_placed_horde_to_it_and_never_lowers() {
    for (value, expected) in [(3, 3), (1, 1), (0, 1)] {
        let (simulation, _) = horde_sim(ObjectKeys {
            experience: Some(value),
            ..ObjectKeys::default()
        });
        let ids = simulation.rules().ids().unwrap();
        let horde = horde_entity(&simulation, ids).unwrap();
        assert_eq!(
            simulation.rules().rank_of(simulation.state(), horde),
            Some(expected),
            "value {value}"
        );
    }
}

fn health_of(simulation: &Simulation<RotwkRules>) -> (f32, f32) {
    let ids = simulation.rules().ids().unwrap();
    let (_, health) = simulation
        .state()
        .store(ids.health)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    (health.0.current(), health.0.max())
}

#[test]
fn an_initial_health_percent_sets_health_and_above_100_the_maximum() {
    for (percent, expected) in [(50, (50.0, 100.0)), (500, (500.0, 500.0))] {
        let (simulation, _) = pawn_sim(ObjectKeys {
            initial_health: Some(percent),
            ..ObjectKeys::default()
        });
        assert_eq!(health_of(&simulation), expected, "{percent}");
    }
}

#[test]
fn an_initial_health_percent_trims_a_placed_horde_but_never_empties_it() {
    for (percent, left) in [(25, 2), (100, 8), (0, 8)] {
        let (simulation, _) = horde_sim(ObjectKeys {
            initial_health: Some(percent),
            ..ObjectKeys::default()
        });
        let ids = simulation.rules().ids().unwrap();
        assert_eq!(members_of(&simulation, ids).len(), left, "{percent}");
    }
}

fn team_sim(keys: ObjectKeys) -> (Simulation<RotwkRules>, Entity) {
    let fixture = fixture();
    let team = TeamDef {
        owner: String::from("Wilds"),
        name: String::from("Squad"),
        singleton: true,
        properties: Dictionary::from_pairs(vec![(
            String::from("teamAggressiveness"),
            Value::Int(1),
        )]),
    };
    let tree = mint_sage_map::ScriptTree {
        player_slots: vec![Vec::new()],
    };
    let engine = MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Wilds")]],
        &BTreeMap::new(),
        &BTreeMap::new(),
        false,
    )
    .with_team_prototypes(TeamProtos::from_defs(&[team]).unwrap());
    let rules = base_rules(&fixture, vec![pawn_spawn(&fixture)])
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_script_teams(vec![(0, String::from("Wilds/Squad"))])
        .with_placed_keys(vec![(0, keys)]);
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let pawn = simulation.state().store(ids.kind).unwrap().entities()[0];
    (simulation, pawn)
}

#[test]
fn a_placed_aggressiveness_outranks_its_teams_attitude() {
    let (simulation, pawn) = team_sim(ObjectKeys {
        aggressiveness: Some(-2),
        ..ObjectKeys::default()
    });
    let ids = simulation.rules().ids().unwrap();
    assert_eq!(
        simulation
            .rules()
            .stored_attitude(simulation.state(), ids, pawn),
        -2
    );
}

#[test]
fn a_targetable_placement_carries_the_targetable_status_bit() {
    let (simulation, pawn) = team_sim(ObjectKeys {
        targetable: true,
        ..ObjectKeys::default()
    });
    let ids = simulation.rules().ids().unwrap();
    let state = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap();
    assert!(state.rt.statuses.has(pawn, ScriptStatusBit::Targetable));
}

#[test]
fn a_placed_objects_team_is_active() {
    let (simulation, pawn) = team_sim(ObjectKeys::default());
    let ids = simulation.rules().ids().unwrap();
    let state = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap();
    let seated: Vec<bool> = state
        .rt
        .teams
        .instances()
        .filter(|instance| instance.members().contains(&pawn))
        .map(|instance| instance.active)
        .collect();
    assert_eq!(seated, [true]);
}
