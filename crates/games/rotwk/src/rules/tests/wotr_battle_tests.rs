use super::*;
use crate::rules::map_scripts::MapScriptEngine;
use crate::rules::wotr_battle::{WotrSeat, WotrStart};
use mint_sage_ini::{BlockTable, Document, MacroTable, ScienceIndex, UpgradeIndex};
use mint_sage_map::ScriptTree;

const SEAT: PlayerId = PlayerId::new(0);

const SCIENCES: &str = "\
Science SCIENCE_Fixed\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Science SCIENCE_Bought\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\nEnd\n";

const UPGRADES: &str =
    "Upgrade Upgrade_Kept\n  PersistsInCampaign = Yes\nEnd\nUpgrade Upgrade_Lost\nEnd\n";

pub(super) fn engine(start: WotrStart) -> MapScriptEngine {
    MapScriptEngine::build(
        &[ScriptTree::default()],
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        true,
    )
    .with_wotr(start)
}

fn battle(skill_points: f32) -> Simulation<RotwkRules> {
    battle_granting(skill_points, vec![0, 0, 0])
}

fn battle_granting(skill_points: f32, granted: Vec<u32>) -> Simulation<RotwkRules> {
    let parse = |name, text| Document::parse(name, text, &BlockTable::corpus_path()).unwrap();
    let mut sciences = ScienceIndex::new();
    sciences.insert_document(&parse("science.ini", SCIENCES), &MacroTable::new());
    let mut upgrades = UpgradeIndex::new();
    upgrades.insert_document(&parse("upgrade.ini", UPGRADES));
    let start = WotrStart {
        seats: vec![WotrSeat {
            player: SEAT,
            armies: Vec::new(),
            structures: Vec::new(),
            modifiers: None,
            purchase_points: 4,
            skill_points,
            sciences: vec!["SCIENCE_Fixed".to_owned(), "SCIENCE_Bought".to_owned()],
            upgrades: vec!["Upgrade_Kept".to_owned(), "Upgrade_Lost".to_owned()],
            fallen: Vec::new(),
        }],
        skill_scalars: Vec::new(),
        revival_multipliers: (1.0, 1.0),
    };
    let passability = Passability::build(&flat_map(32, 32), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_upgrades(
            crate::templates::UpgradeMenus::default(),
            Arc::new(mint_sage_modules::UpgradeBits::from_index(&upgrades)),
        )
        .with_spell_books(
            Arc::new(crate::sciences::ScienceTable::build(&sciences, false)),
            Arc::new(crate::sciences::Ranks::from_columns(
                vec![0, 60, 120],
                granted,
            )),
            vec![crate::sciences::SpellPlan::default(); 2],
            Vec::new(),
        )
        .with_map_scripts(Some(engine(start)), vec![(0, SEAT)]);
    Simulation::new(5, rules).unwrap()
}

#[test]
fn a_battle_starts_from_the_carried_in_rank_state() {
    let mut simulation = battle(90.0);
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let rules = simulation.rules();
    let player = crate::economy::player_entity(world, RotwkRules::economy_ids(ids), SEAT).unwrap();
    let science = world.get(ids.player_science, player).unwrap();
    assert_eq!(science.rank, 2);
    assert_eq!(science.purchase_points, 4);
    assert!(science.holds(rules.sciences.id("SCIENCE_Fixed").unwrap()));
    assert!(science.holds(rules.sciences.id("SCIENCE_Bought").unwrap()));
    let held = &world.get(ids.player_upgrades, player).unwrap().0;
    for name in ["Upgrade_Kept", "Upgrade_Lost"] {
        assert!(
            held.contains(rules.upgrade_bits.bit(name).unwrap()),
            "{name}"
        );
    }
    assert_eq!(
        rules.wotr_run(world).unwrap().start_ranks,
        vec![(SEAT, 2u16)]
    );
}

#[test]
fn a_battle_hands_back_its_points_fixed_sciences_and_persisting_upgrades() {
    let mut simulation = battle_granting(90.0, vec![5, 2, 3]);
    simulation.tick().unwrap();
    let carry = simulation
        .rules()
        .wotr_player_carry(simulation.state(), SEAT)
        .unwrap();
    assert_eq!(carry.skill_points, 90.0);
    assert_eq!(carry.rank_points, 7);
    assert_eq!(carry.sciences, vec!["SCIENCE_Fixed".to_owned()]);
    assert_eq!(carry.upgrades, vec!["upgrade_kept".to_owned()]);
}

#[test]
fn a_forfeited_player_stays_defeated_when_a_dozer_arrives_later() {
    use crate::rules::wotr_battle::WotrArmy;
    const HUMAN: PlayerId = PlayerId::new(0);
    const AI: PlayerId = PlayerId::new(1);
    let (data, kinds) = construction_kinds();
    let spawn = |name: &str, at: f32, player| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(at, 100.0),
                Facing::EAST,
                player,
            )
            .unwrap()
    };
    let objects = vec![spawn("Shed", 100.0, HUMAN), spawn("Shed", 300.0, AI)];
    let start = WotrStart {
        seats: vec![WotrSeat {
            player: AI,
            armies: vec![WotrArmy {
                army: 1,
                units: vec![("Porter".to_owned(), 1, 1)],
                arrives: 300,
                stand: [500.0, 100.0, 0.0],
                face: [600.0, 100.0, 0.0],
                walk_to: None,
            }],
            structures: Vec::new(),
            modifiers: None,
            purchase_points: 0,
            skill_points: 0.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            fallen: Vec::new(),
        }],
        skill_scalars: Vec::new(),
        revival_multipliers: (1.0, 1.0),
    };
    let structure =
        crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let tuning = mint_sage_ini::game_data::RetreatTuning {
        farming_threshold: 1000,
        army_quality_bias: 2,
        army_quantity_bias: 1,
        hero_quality_bias: 1,
        map_control_bias: 2,
        base_strength_bias: 1,
        ring_ownership_bias: 2,
        frames_till_checks_start: 10,
        frames_between_checks: 5,
        frames_till_self_destruct: 4,
        disable_tactical_ai: false,
    };
    let seeds = vec![crate::skirmish_ai::AiSeed {
        player: AI,
        candidates: Vec::new(),
    }];
    let mut simulation = Simulation::new(
        64,
        RotwkRules::new(passability, objects)
            .with_combat(data.combat.clone())
            .with_victory(Arc::clone(&kinds), structure)
            .with_defeat_mode(crate::defeat::DefeatMode::Dozer)
            .with_players(2)
            .with_teams(vec![Some(0), Some(1)])
            .with_skirmish_ai(seeds, crate::combat::AiTiming::default(), Vec::new())
            .with_ai_power_tuning([(1, 1); 4], vec![(AI, crate::lobby::Difficulty::Medium)])
            .with_forfeit(Some(tuning))
            .with_map_scripts(Some(engine(start)), vec![(0, HUMAN)]),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    assert!(simulation
        .rules()
        .player_defeated(simulation.state(), ids, AI));
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let world = simulation.state();
    let held = world
        .store(ids.owner)
        .unwrap()
        .iter()
        .filter(|(entity, owner)| owner.0 == AI && world.is_live(*entity))
        .count();
    assert_eq!(held, 1, "the army arrived");
    assert!(simulation.rules().player_defeated(world, ids, AI));
}
