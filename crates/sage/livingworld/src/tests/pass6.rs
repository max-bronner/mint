use super::*;
use crate::model::{bonus, Rejected};

fn swap(a: ArmyId, b: ArmyId, a_members: Vec<u32>, b_members: Vec<u32>) -> Command {
    Command::SwapArmyMembers {
        a,
        b,
        a_members,
        b_members,
    }
}

fn member(s: &LivingWorldState, army: ArmyId, at: usize) -> u32 {
    s.armies[&army].roster[at].id
}

#[test]
fn a_swap_never_moves_the_source_armys_hero() {
    let mut s = session();
    let hero = hero_army(&mut s, P0, "A", 1);
    let post = garrison(&mut s, P0, "A", 1);
    let h = member(&s, hero, 0);
    assert_eq!(
        apply(&mut s, P0, swap(hero, post, vec![h], vec![])),
        Err(Rejected::MemberIsHero)
    );
}

#[test]
fn a_swap_needs_the_other_army_within_the_sources_reach() {
    let mut s = session();
    let hero = hero_army(&mut s, P0, "A", 1);
    let near = garrison(&mut s, P0, "C", 1);
    let far = garrison(&mut s, P0, "D", 1);
    let troop = member(&s, hero, 1);
    assert_eq!(
        apply(&mut s, P0, swap(hero, far, vec![troop], vec![])),
        Err(Rejected::OutOfRange)
    );
    let from_near = member(&s, near, 0);
    assert_eq!(
        apply(&mut s, P0, swap(hero, near, vec![], vec![from_near])),
        Err(Rejected::OutOfRange)
    );
    command(&mut s, P0, swap(hero, near, vec![troop, 999], vec![]));
    assert_eq!(s.armies[&near].roster.len(), 2);
}

#[test]
fn a_swap_tests_the_cp_cap_of_the_net_receiver_only() {
    let mut s = session();
    let hero = hero_army(&mut s, P0, "A", 1);
    let a = rid(&s, "A");
    let post = s.spawn_army(P0, a, None, &[("elf".to_owned(), 2), ("elf".to_owned(), 1)]);
    let (troop, small) = (member(&s, hero, 1), member(&s, post, 1));
    assert_eq!(
        apply(&mut s, P0, swap(hero, post, vec![troop], vec![])),
        Err(Rejected::InvalidSwap)
    );
    command(&mut s, P0, swap(hero, post, vec![troop], vec![small]));
    assert_eq!(s.army_cp(&s.armies[&post]), 3);
}

fn barracks(s: &mut LivingWorldState) {
    place_done(s, "A", 0, "Barracks");
}

#[test]
fn queueing_charges_the_first_members_price_and_a_cancel_refunds_at_the_current_price() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    let queue = Command::QueueArmy {
        region: a,
        plot: 0,
        entry: 0,
    };
    command(&mut s, P0, queue);
    assert_eq!(s.player(P0).unwrap().resources, 90);
    assert_eq!(s.player(P0).unwrap().reserved_cp, 2);
    s.player_mut(P0)
        .unwrap()
        .bonus_a
        .set(bonus::DISCOUNTED_BARRACKS_UNITS, 4);
    let cancel = Command::CancelQueuedArmy {
        region: a,
        plot: 0,
        index: 0,
    };
    command(&mut s, P0, cancel);
    assert_eq!(s.player(P0).unwrap().resources, 96);
    assert_eq!(s.player(P0).unwrap().reserved_cp, 0);
}

#[test]
fn queueing_needs_a_free_slot_resources_and_an_unfielded_hero() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    let queue = |entry| Command::QueueArmy {
        region: a,
        plot: 0,
        entry,
    };
    command(&mut s, P0, queue(0));
    command(&mut s, P0, queue(0));
    assert_eq!(apply(&mut s, P0, queue(0)), Err(Rejected::QueueFull));
    s.regions[a.index()].plots[0]
        .as_mut()
        .unwrap()
        .queue
        .clear();
    s.player_mut(P0).unwrap().resources = 5;
    assert_eq!(apply(&mut s, P0, queue(0)), Err(Rejected::CannotAfford));
    assert_eq!(apply(&mut s, P0, queue(7)), Err(Rejected::NoSuchEntry));
    command(&mut s, P0, queue(1));
    hero_army(&mut s, P0, "B", 1);
    s.regions[a.index()].plots[0]
        .as_mut()
        .unwrap()
        .queue
        .clear();
    assert_eq!(apply(&mut s, P0, queue(1)), Err(Rejected::HeroFielded));
}

#[test]
fn a_hero_already_queued_in_one_of_the_owners_buildings_cannot_be_queued_again() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    let queue = Command::QueueArmy {
        region: a,
        plot: 0,
        entry: 1,
    };
    command(&mut s, P0, queue.clone());
    assert_eq!(apply(&mut s, P0, queue), Err(Rejected::HeroQueued));
}

#[test]
fn queueing_in_an_unowned_region_is_free() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    s.regions[a.index()].owner = None;
    let queue = Command::QueueArmy {
        region: a,
        plot: 0,
        entry: 0,
    };
    command(&mut s, P0, queue);
    assert_eq!(s.player(P0).unwrap().resources, 100);
    assert_eq!(s.player(P0).unwrap().reserved_cp, 0);
}

#[test]
fn only_the_queue_head_advances_and_a_spawn_waits_while_the_region_has_no_room() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    for _ in 0..2 {
        command(
            &mut s,
            P0,
            Command::QueueArmy {
                region: a,
                plot: 0,
                entry: 0,
            },
        );
    }
    let queued = |s: &LivingWorldState| s.regions[a.index()].plots[0].as_ref().unwrap().queue.len();
    end_planning(&mut s);
    assert_eq!((s.armies.len(), queued(&s)), (0, 2));
    end_planning(&mut s);
    assert_eq!((s.armies.len(), queued(&s)), (1, 1));
    assert_eq!(s.player(P0).unwrap().reserved_cp, 2);
    for _ in 0..3 {
        end_planning(&mut s);
    }
    assert_eq!((s.armies.len(), queued(&s)), (1, 1));
    s.armies.clear();
    end_planning(&mut s);
    assert_eq!((s.armies.len(), queued(&s)), (1, 0));
    assert_eq!(s.player(P0).unwrap().reserved_cp, 0);
}

#[test]
fn a_spawned_garrison_joins_the_existing_one_and_goes_to_the_regions_current_owner() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    let post = garrison(&mut s, P0, "A", 1);
    command(
        &mut s,
        P0,
        Command::QueueArmy {
            region: a,
            plot: 0,
            entry: 0,
        },
    );
    end_planning(&mut s);
    let events = end_planning(&mut s);
    assert!(events.contains(&Event::ArmySpawned {
        army: post,
        region: a
    }));

    s.regions[a.index()].owner = Some(P1);
    command(
        &mut s,
        P1,
        Command::QueueArmy {
            region: a,
            plot: 0,
            entry: 0,
        },
    );
    s.regions[a.index()].owner = Some(P0);
    s.armies.clear();
    end_planning(&mut s);
    end_planning(&mut s);
    let spawned: Vec<_> = s.armies.values().map(|x| x.player).collect();
    assert_eq!(spawned, vec![P0]);
}

#[test]
fn each_building_upgrades_its_quota_highest_level_first_and_the_count_resets_after_planning() {
    let mut s = session();
    place_done(&mut s, "A", 1, "Armoury");
    let army = garrison(&mut s, P0, "A", 1);
    let a = rid(&s, "A");
    s.armies
        .get_mut(&army)
        .unwrap()
        .roster
        .push(crate::model::Member {
            id: 900,
            unit: s.rules.intern("elf"),
            level: 3,
            count: 1,
            experience: 0,
            battles: 0,
            kills: 0,
            kills_battle: 0,
            upgraded_at: None,
        });
    s.troop_upgrades();
    let flags = |s: &LivingWorldState| -> Vec<bool> {
        s.armies[&army]
            .roster
            .iter()
            .map(|m| m.is_upgraded())
            .collect()
    };
    assert_eq!(flags(&s), vec![false, true]);
    s.troop_upgrades();
    assert_eq!(flags(&s), vec![false, true]);
    end_planning(&mut s);
    assert_eq!(flags(&s), vec![true, true]);
    assert_eq!(
        s.regions[a.index()].plots[1]
            .as_ref()
            .unwrap()
            .upgrades_used,
        1
    );
}

#[test]
fn a_garrison_merge_moves_every_member_whatever_the_cap() {
    let mut s = session();
    let first = garrison(&mut s, P0, "B", 2);
    let b = rid(&s, "B");
    let second = s.spawn_army(P0, b, None, &[("elf".to_owned(), 2), ("elf".to_owned(), 1)]);
    s.garrison_merge(&mut Vec::new());
    assert_eq!(s.army_cp(&s.armies[&first]), 5);
    assert!(!s.armies.contains_key(&second));
}

#[test]
fn the_snap_leaves_no_path_behind() {
    let mut s = session();
    let hero = hero_army(&mut s, P0, "A", 1);
    let b = rid(&s, "B");
    command(
        &mut s,
        P0,
        Command::MoveArmy {
            army: hero,
            dest: b,
        },
    );
    s.snap_orders();
    assert_eq!(s.armies[&hero].order, None);
    assert_eq!(s.armies[&hero].region, b);
}

#[test]
fn two_teamless_survivors_end_a_session_that_began_with_a_defeated_team() {
    let mut s = session();
    let mut extra = s.players[0].clone();
    extra.id = PlayerId(2);
    extra.team = 0;
    extra.defeated = true;
    for p in &mut s.players {
        p.team = -1;
    }
    s.players.push(extra);
    let mut events = Vec::new();
    s.check_victory(&mut events);
    assert_eq!(s.ended, Some(None));
}

#[test]
fn a_free_for_all_of_teamless_players_ends_when_one_is_left() {
    let mut s = session();
    for p in &mut s.players {
        p.team = -1;
    }
    let mut events = Vec::new();
    s.check_victory(&mut events);
    assert_eq!(s.ended, None);
    s.defeat_player(P1, &mut events);
    s.check_victory(&mut events);
    assert_eq!(s.ended, Some(None));
}

#[test]
fn a_slots_start_region_maps_through_its_ownership_set_to_the_sets_start_region() {
    let extra = " OwnershipSet\n  Regions = A B\n  StartRegion = B\n  SpawnBuildings\n   Buildings = Farm\n   Region = A\n  End\n End\n";
    let data = super::data_with(DEFAULT_VICTORY, extra);
    let (_, setup) = setup(DEFAULT_VICTORY, 1);
    let s = LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    assert_eq!(s.player(P0).unwrap().base_region, Some(rid(&s, "B")));
    assert_eq!(s.owner(rid(&s, "A")), Some(P0));
    assert_eq!(s.owner(rid(&s, "B")), Some(P0));
    assert!(s.regions[rid(&s, "A").index()].plots[0].is_some());
    assert_eq!(s.player(P1).unwrap().base_region, Some(rid(&s, "E")));
}

fn escort(s: &mut LivingWorldState) {
    let hero = s.rules.intern("hero_a");
    let elf = s.rules.intern("elf");
    let id = s.rules.building_id("Barracks").unwrap();
    let queue = s.rules.buildings[usize::from(id.0)].queue.as_mut().unwrap();
    queue.entries.push(crate::rules::QueuedArmy {
        build_time: 1,
        hero: Some(hero),
        roster: vec![(elf, 1), (hero, 1)],
        is_city: false,
    });
}

fn queue_at(region: RegionId, plot: u8, entry: u8) -> Command {
    Command::QueueArmy {
        region,
        plot,
        entry,
    }
}

#[test]
fn a_hero_named_by_the_descriptor_is_unique_though_a_troop_leads_the_roster() {
    let mut s = session();
    barracks(&mut s);
    escort(&mut s);
    let a = rid(&s, "A");
    command(&mut s, P0, queue_at(a, 0, 2));
    assert_eq!(
        apply(&mut s, P0, queue_at(a, 0, 2)),
        Err(Rejected::HeroQueued)
    );
    assert_eq!(
        apply(&mut s, P0, queue_at(a, 0, 1)),
        Err(Rejected::HeroQueued)
    );
    s.regions[a.index()].plots[0]
        .as_mut()
        .unwrap()
        .queue
        .clear();
    hero_army(&mut s, P0, "B", 1);
    assert_eq!(
        apply(&mut s, P0, queue_at(a, 0, 2)),
        Err(Rejected::HeroFielded)
    );
}

#[test]
fn a_fallen_hero_is_revived_at_the_recruit_price_and_time_and_leaves_the_fallen_list() {
    let mut s = session();
    barracks(&mut s);
    let a = rid(&s, "A");
    s.rules.regions[a.index()].cp_limit = 10;
    s.rules.revival_cost_multiplier = 2.0;
    s.rules.revival_time_multiplier = 3.0;
    let hero = s.rules.intern("hero_a");
    s.rules.units[usize::from(hero.0)].build_cost = 40;
    s.player_mut(P0)
        .unwrap()
        .fallen_heroes
        .push(crate::model::FallenHero {
            unit: hero,
            level: 1,
            experience: 0,
        });
    command(&mut s, P0, queue_at(a, 0, 1));
    assert_eq!(s.player(P0).unwrap().resources, 60);
    assert_eq!(
        apply(&mut s, P0, queue_at(a, 0, 1)),
        Err(Rejected::HeroQueued)
    );
    assert!(s.hero_fallen(P0, hero) && !s.hero_fielded(P0, hero));
    end_planning(&mut s);
    assert!(s.hero_fielded(P0, hero) && !s.hero_fallen(P0, hero));
}

#[test]
fn a_queued_hero_does_not_spawn_beside_the_same_hero_the_owner_fields() {
    let mut s = session();
    barracks(&mut s);
    escort(&mut s);
    let a = rid(&s, "A");
    s.rules.regions[a.index()].cp_limit = 10;
    command(&mut s, P0, queue_at(a, 0, 2));
    hero_army(&mut s, P0, "B", 1);
    let before = s.armies.len();
    for _ in 0..3 {
        end_planning(&mut s);
    }
    assert_eq!(s.armies.len(), before);
    assert_eq!(s.regions[a.index()].plots[0].as_ref().unwrap().queue, [2]);
}
