use super::*;
use crate::screen_queries::{
    button_mask, BattleType, MaskInputs, AUTO_RESOLVE, REAL_TIME, RETREAT,
};
use mint_sage_ini::living_world::players::BuildingType;

fn inputs(choice: BattleType, local_game: bool, everyone: bool, retreat: bool) -> u8 {
    button_mask(MaskInputs {
        battle_type: choice,
        campaign_map: false,
        local_game,
        every_human_in_battle: everyone,
        can_retreat: retreat,
    })
}

#[test]
fn the_mask_offers_auto_resolve_unless_real_time_only_and_real_time_unless_auto_only() {
    use BattleType::{AutoResolveOnly, Both, RealTimeOnly};
    assert_eq!(inputs(Both, true, true, false), AUTO_RESOLVE | REAL_TIME);
    assert_eq!(
        inputs(Both, true, true, true),
        AUTO_RESOLVE | RETREAT | REAL_TIME
    );
    assert_eq!(inputs(AutoResolveOnly, true, true, false), AUTO_RESOLVE);
    assert_eq!(inputs(RealTimeOnly, true, true, false), REAL_TIME);
    assert_eq!(inputs(RealTimeOnly, true, true, true), RETREAT | REAL_TIME);
}

#[test]
fn real_time_needs_a_local_game_or_every_human_in_the_battle_else_the_result_is_auto_resolve() {
    use BattleType::{Both, RealTimeOnly};
    assert_eq!(inputs(Both, false, true, false), AUTO_RESOLVE | REAL_TIME);
    assert_eq!(inputs(Both, false, false, false), AUTO_RESOLVE);
    assert_eq!(inputs(Both, false, false, true), AUTO_RESOLVE | RETREAT);
    assert_eq!(inputs(RealTimeOnly, false, false, false), AUTO_RESOLVE);
}

fn siege() -> (LivingWorldState, crate::model::Battle) {
    let mut s = session();
    let army = hero_army(&mut s, P0, "D", 2);
    let e = rid(&s, "E");
    command(&mut s, P0, Command::MoveArmy { army, dest: e });
    end_planning(&mut s);
    let battle = s.battles[0].clone();
    (s, battle)
}

#[test]
fn a_player_may_retreat_when_a_neighbour_of_the_battle_is_its_own_or_an_ally() {
    let (mut s, battle) = siege();
    assert!(s.can_retreat_from(&battle, P0), "D is P0's");
    assert!(
        !s.can_retreat_from(&battle, P1),
        "E's only neighbour is P0's"
    );
    let d = rid(&s, "D");
    s.regions[d.index()].owner = Some(P1);
    assert!(!s.can_retreat_from(&battle, P0));
    assert!(
        !s.can_retreat_from(&battle, P1),
        "P1 has only garrisons at E"
    );
    hero_army(&mut s, P1, "E", 1);
    assert!(s.can_retreat_from(&battle, P1));
    assert!(
        !s.can_retreat_from(&battle, PlayerId(7)),
        "not in the battle"
    );
}

#[test]
fn a_battle_in_a_campaign_map_region_offers_real_time_only() {
    let (mut s, battle) = siege();
    let all = AUTO_RESOLVE | RETREAT | REAL_TIME;
    assert_eq!(s.battle_buttons(&battle, P0, BattleType::Both, true), all);
    s.rules.regions[battle.region.index()].campaign_map = true;
    assert_eq!(
        s.battle_buttons(&battle, P0, BattleType::Both, true),
        REAL_TIME
    );
    assert_eq!(
        s.battle_buttons(&battle, P0, BattleType::AutoResolveOnly, true),
        REAL_TIME,
        "the region wins over the option"
    );
}

#[test]
fn map_is_campaign_map_is_read_from_the_region() {
    let s = session();
    let campaign_map = |name: &str| s.rules.region_rules(rid(&s, name)).unwrap().campaign_map;
    assert!(campaign_map("B"));
    assert!(!campaign_map("A"));
    assert!(!campaign_map("E"));
}

#[test]
fn a_side_of_garrisons_cannot_retreat() {
    let (mut s, battle) = siege();
    assert!(s.can_retreat_from(&battle, P0));
    for army in s.armies.values_mut().filter(|a| a.player == P0) {
        army.hero = None;
    }
    assert!(!s.can_retreat_from(&battle, P0));
    assert_eq!(
        s.battle_buttons(&battle, P0, BattleType::Both, true),
        AUTO_RESOLVE | REAL_TIME
    );
}

#[test]
fn no_retreat_from_a_capital_whose_loss_loses_the_game() {
    let (mut s, battle) = siege();
    s.players[0].base_region = Some(battle.region);
    assert!(s.can_retreat_from(&battle, P0), "no condition names it");
    let team = u32::try_from(s.players[0].team).unwrap();
    s.rules
        .conditions
        .player_defeat
        .push(crate::rules::PlayerDefeat {
            teams: vec![team],
            lose_if_capital_lost: true,
            max_regions: None,
        });
    assert!(!s.can_retreat_from(&battle, P0));
    s.players[0].base_region = None;
    assert!(s.can_retreat_from(&battle, P0), "not its BaseRegion");
}

#[test]
fn the_prompt_mask_of_a_battle_follows_the_retreat_path_the_option_and_the_humans() {
    let (mut s, battle) = siege();
    let all = AUTO_RESOLVE | RETREAT | REAL_TIME;
    assert_eq!(s.battle_buttons(&battle, P0, BattleType::Both, true), all);
    assert_eq!(
        s.battle_buttons(&battle, P0, BattleType::AutoResolveOnly, true),
        AUTO_RESOLVE | RETREAT
    );
    assert_eq!(
        s.battle_buttons(&battle, P1, BattleType::Both, true),
        AUTO_RESOLVE | REAL_TIME,
        "nowhere to retreat to"
    );
    s.players[1].kind = PlayerKind::Human;
    let mut outside = battle.clone();
    outside.players.retain(|p| *p != P1);
    assert_eq!(
        s.battle_buttons(&outside, P0, BattleType::Both, false),
        AUTO_RESOLVE | RETREAT,
        "a human outside the battle in a networked game"
    );
    assert_eq!(s.battle_buttons(&outside, P0, BattleType::Both, true), all);
}

#[test]
fn an_empty_plot_offers_the_available_structures_in_list_order_capped_ones_included() {
    let mut s = session();
    let a = rid(&s, "A");
    let names = |s: &LivingWorldState, list: Vec<crate::model::BuildingId>| -> Vec<String> {
        list.iter()
            .map(|id| s.rules.building(*id).unwrap().name.clone())
            .collect()
    };
    let all = names(&s, s.buildable_here(P0));
    assert_eq!(all, ["Farm", "Barracks", "Armoury", "Hall", "Keep"]);
    let barracks = s.rules.building_id("Barracks").unwrap();
    s.rules.buildings[usize::from(barracks.0)].available_to = vec![crate::model::TemplateId(1)];
    s.rules.regions[a.index()].restrict = vec![(vec![BuildingType::Fortress], Some(0))];
    let keep = s.rules.building_id("Keep").unwrap();
    assert!(s.build_restricted(a, keep), "the Keep is capped here");
    assert_eq!(
        names(&s, s.buildable_here(P0)),
        ["Farm", "Armoury", "Hall", "Keep"],
        "Barracks is Orcs'; the capped Keep keeps its place"
    );
}

#[test]
fn the_battle_type_is_in_the_hash_and_the_save() {
    let both = session();
    for kind in [BattleType::AutoResolveOnly, BattleType::RealTimeOnly] {
        let mut other = both.clone();
        other.battle_type = kind;
        assert_ne!(other.state_hash(), both.state_hash(), "{kind:?}");
        let payload = other.save_payload(None).unwrap();
        let (loaded, _) = LivingWorldState::load_payload(other.rules.clone(), &payload, 0).unwrap();
        assert_eq!(loaded.battle_type, kind);
        assert_eq!(loaded.state_hash(), other.state_hash());
    }
}

#[test]
fn a_started_session_keeps_the_setups_battle_type() {
    let (data, mut setup) = super::setup(super::DEFAULT_VICTORY, 1);
    setup.battle_type = BattleType::RealTimeOnly;
    let state = LivingWorldState::start(&data, &super::catalog(), &setup).unwrap();
    assert_eq!(state.battle_type, BattleType::RealTimeOnly);
}

#[test]
fn the_battle_type_ordinals_are_the_rules_values() {
    for value in 0..=2 {
        assert_eq!(BattleType::from_ordinal(value).unwrap().ordinal(), value);
    }
    assert_eq!(BattleType::from_ordinal(3), None);
}
