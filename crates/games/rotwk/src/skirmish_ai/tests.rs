use super::*;
use mint_sim::World;

fn dummy_entity() -> Entity {
    let mut world = World::new();
    world.spawn().expect("a fresh world can always spawn one")
}

fn task(priority: i32, phase: BasePhase) -> AiTask {
    AiTask {
        kind: Kind(0),
        position: Vec2::ZERO,
        angle: 0.0,
        priority,
        phase,
    }
}

#[test]
fn sorted_tasks_is_ascending_and_stable() {
    let tasks = vec![
        task(300, BasePhase::One),
        task(20, BasePhase::One),
        task(20, BasePhase::Two),
        task(150, BasePhase::One),
    ];
    let sorted = sorted_tasks(tasks);
    let priorities: Vec<i32> = sorted.iter().map(|t| t.priority).collect();
    assert_eq!(priorities, [20, 20, 150, 300]);
    assert_eq!(sorted[0].phase, BasePhase::One);
    assert_eq!(sorted[1].phase, BasePhase::Two);
}

#[test]
fn phase_one_is_always_allowed_and_two_unlocks_once_one_is_built() {
    let tasks = vec![task(1, BasePhase::One), task(2, BasePhase::Two)];
    let mut states = vec![AiTaskState::Pending, AiTaskState::Pending];
    assert_eq!(allowed_phase(&tasks, &states), BasePhase::One);
    states[0] = AiTaskState::Built(dummy_entity());
    assert_eq!(allowed_phase(&tasks, &states), BasePhase::Two);
}

#[test]
fn a_phase_absent_from_the_plan_does_not_block_the_next_one() {
    let tasks = vec![task(1, BasePhase::One), task(2, BasePhase::Three)];
    let states = vec![AiTaskState::Built(dummy_entity()), AiTaskState::Pending];
    assert_eq!(allowed_phase(&tasks, &states), BasePhase::Three);
}

#[test]
fn structure_timer_scales_by_wealth_and_reads_as_zero_when_base_is_zero() {
    assert_eq!(
        structure_timer_frames(0.0, 2.0, 0.6, 7_000, 2_000, 10_000),
        0
    );
    let wealthy = structure_timer_frames(10.0, 2.0, 0.6, 7_000, 2_000, 8_000);
    let normal = structure_timer_frames(10.0, 2.0, 0.6, 7_000, 2_000, 5_000);
    let poor = structure_timer_frames(10.0, 2.0, 0.6, 7_000, 2_000, 1_000);
    assert!(wealthy < normal, "{wealthy} should be less than {normal}");
    assert!(poor > normal, "{poor} should be more than {normal}");
}

#[test]
fn rebuild_is_ready_only_after_the_delay_has_elapsed() {
    let delay = mint_sim::LogicFrames::from_seconds(30.0);
    let destroyed_at = Frame::new(100);
    assert!(!rebuild_ready(destroyed_at, Frame::new(100), delay));
    assert!(!rebuild_ready(
        destroyed_at,
        Frame::new(100 + u64::from(delay.get()) - 1),
        delay
    ));
    assert!(rebuild_ready(
        destroyed_at,
        Frame::new(100 + u64::from(delay.get())),
        delay
    ));
}

#[test]
fn the_retry_spiral_tries_the_original_spot_first_and_never_repeats() {
    let spiral = retry_spiral(40.0);
    assert_eq!(spiral[0], Vec2::new(0.0, 0.0));
    let mut seen = std::collections::BTreeSet::new();
    for offset in &spiral {
        let key = (offset.x.to_bits(), offset.y.to_bits());
        assert!(seen.insert(key), "duplicate offset {offset:?}");
    }
}

#[test]
fn army_phase_reads_rush_then_mid_game_from_end_of_rush_then_end_game() {
    assert_eq!(army_phase(0.0, 270.0, 280.0), 1);
    assert_eq!(army_phase(269.9, 270.0, 280.0), 1);
    assert_eq!(
        army_phase(270.0, 270.0, 280.0),
        2,
        "rush ends, phase 2 begins"
    );
    assert_eq!(army_phase(549.9, 270.0, 280.0), 2, "270 + 280 - 0.1");
    assert_eq!(army_phase(550.0, 270.0, 280.0), 3, "everything after");
    assert_eq!(army_phase(100_000.0, 270.0, 280.0), 3);
}

#[test]
fn army_phase_clamps_a_hostile_duration_rather_than_propagating_it() {
    assert_eq!(army_phase(10.0, f32::NAN, 280.0), 2, "NaN rush reads as 0");
    assert_eq!(
        army_phase(10.0, -50.0, 280.0),
        2,
        "negative rush reads as 0"
    );
}

fn member(phase1: f32, phase2: f32, phase3: f32) -> ArmyMemberPlan {
    ArmyMemberPlan {
        kind: Some(Kind(0)),
        phase: [phase1, phase2, phase3],
    }
}

fn most_deviant_member(members: &[ArmyMemberPlan], counts: &[u32], phase: u8) -> Option<usize> {
    deviation_order(members, counts, phase).into_iter().next()
}

#[test]
fn the_deviation_query_picks_whichever_resolved_member_is_furthest_under_target() {
    let members = [member(40.0, 40.0, 40.0), member(60.0, 60.0, 60.0)];
    assert_eq!(most_deviant_member(&members, &[0, 0], 1), Some(1));
    assert_eq!(most_deviant_member(&members, &[10, 0], 1), Some(1));
    assert_eq!(most_deviant_member(&members, &[10, 20], 1), Some(0));
}

#[test]
fn deviation_order_falls_back_past_the_most_deviant_member() {
    let members = [member(40.0, 40.0, 40.0), member(60.0, 60.0, 60.0)];
    assert_eq!(deviation_order(&members, &[0, 0], 1), vec![1, 0]);
}

#[test]
fn a_zero_percent_phase_target_is_never_picked() {
    let members = [member(0.0, 25.0, 30.0)];
    assert_eq!(most_deviant_member(&members, &[0], 1), None);
    assert_eq!(most_deviant_member(&members, &[0], 2), Some(0));
}

#[test]
fn an_unresolved_member_is_never_picked() {
    let members = [ArmyMemberPlan {
        kind: None,
        phase: [100.0, 100.0, 100.0],
    }];
    assert_eq!(most_deviant_member(&members, &[0], 1), None);
}

#[test]
fn the_command_point_floor_reads_used_and_queued_against_the_cap() {
    assert!(below_command_point_floor(80, 100, 0.9), "80 < 90");
    assert!(
        !below_command_point_floor(90, 100, 0.9),
        "90 is not below 90"
    );
    assert!(!below_command_point_floor(95, 100, 0.9));
    assert!(!below_command_point_floor(0, 0, 0.9), "no cap, no floor");
}

#[test]
fn the_purse_holds_back_a_percentage_unless_the_floor_forces_spending() {
    assert_eq!(spendable_purse(1_000, 0.05, false), 950);
    assert_eq!(
        spendable_purse(1_000, 0.05, true),
        1_000,
        "the floor overrides the save-a-fraction rule"
    );
    assert_eq!(
        spendable_purse(1_000, 1.5, false),
        0,
        "a percent above 1.0 clamps"
    );
    assert_eq!(spendable_purse(-50, 0.1, false), 0, "never negative");
}

fn manager(candidates: &[u8]) -> EnemyManager {
    EnemyManager {
        candidates: candidates.iter().map(|&p| PlayerId::new(p)).collect(),
        target: None,
    }
}

fn choose(near: f32, far: f32, roll: u32) -> Option<PlayerId> {
    let mut manager = manager(&[2, 1]);
    let distance = |p: PlayerId| if p.get() == 1 { near } else { far };
    manager.update(|_| false, distance, || roll);
    manager.target
}

#[test]
fn the_second_nearest_is_drawn_half_the_time_only_within_the_gap() {
    let (one, two) = (Some(PlayerId::new(1)), Some(PlayerId::new(2)));
    assert_eq!(choose(300.0, 1500.0, 1), one);
    assert_eq!(choose(300.0, 1500.0, 100), one);
    assert_eq!(choose(300.0, 900.0, 50), two);
    assert_eq!(choose(300.0, 900.0, 51), one);
}

#[test]
fn a_target_is_kept_until_its_player_is_defeated() {
    let mut manager = manager(&[1, 2, 3]);
    let distance = |p: PlayerId| f32::from(p.get()) * 100.0;
    manager.update(|_| false, distance, || 100);
    assert_eq!(manager.target, Some(PlayerId::new(1)));

    let farther_first = |p: PlayerId| 1000.0 - f32::from(p.get()) * 100.0;
    let changed = manager.update(|_| false, farther_first, || unreachable!("no draw"));
    assert!(!changed);
    assert_eq!(manager.target, Some(PlayerId::new(1)));

    let changed = manager.update(|p| p.get() == 1, distance, || 100);
    assert!(changed);
    assert_eq!(manager.target, Some(PlayerId::new(2)));
    assert_eq!(manager.candidates, [PlayerId::new(2), PlayerId::new(3)]);
}

#[test]
fn a_computer_player_joins_the_group_of_the_first_ally() {
    let players: Vec<PlayerId> = (1..=4).map(PlayerId::new).collect();
    let allied = |a: PlayerId, b: PlayerId| {
        matches!(
            (a.get(), b.get()),
            (3, 1) | (1, 3) | (4, 2) | (2, 4) | (4, 3) | (3, 4)
        )
    };
    let groups = enemy_groups(&players, allied);
    let ids = |group: &Vec<PlayerId>| group.iter().map(|p| p.get()).collect::<Vec<_>>();
    assert_eq!(
        groups.iter().map(ids).collect::<Vec<_>>(),
        [vec![1, 3], vec![2, 4]]
    );
}

fn urgency_values() -> crate::combat::AttackUrgencyValues {
    crate::combat::AttackUrgencyValues::default()
}

#[test]
fn advance_attack_urgency_launches_exactly_once_per_crossing() {
    let values = urgency_values();
    let mut urgency = AttackUrgency::new(&values);
    let mut launched_at = Vec::new();
    for i in 0..60u32 {
        let (next, launched) = advance_attack_urgency(&values, urgency, true);
        urgency = next;
        if launched {
            launched_at.push(i);
        }
    }
    assert!(
        launched_at.len() >= 2,
        "expected more than one crossing over 60 raises: {launched_at:?}"
    );
    for pair in launched_at.windows(2) {
        assert!(
            pair[1] > pair[0] + 1,
            "two launches with no raise-below-threshold in between: {launched_at:?}"
        );
    }
}

#[test]
fn advance_attack_urgency_does_not_relaunch_every_check_while_still_armed() {
    let values = urgency_values();
    let upper = values.upper_bound_high_base - values.change_high;
    let mut urgency = AttackUrgency {
        attack: values.launch,
        defense: upper,
        economy: upper,
        armed: false,
    };
    let (after_first, first_launch) = advance_attack_urgency(&values, urgency, false);
    assert!(first_launch, "the crossing is detected on the first check");
    assert_eq!(
        after_first.attack, values.launch,
        "no headroom for the reset to move anything"
    );
    urgency = after_first;
    for _ in 0..10 {
        let (next, launched) = advance_attack_urgency(&values, urgency, false);
        urgency = next;
        assert!(
            !launched,
            "still armed above the threshold - not a new crossing"
        );
    }
}

#[test]
fn advance_attack_urgency_never_launches_while_command_points_are_not_low() {
    let values = urgency_values();
    let mut urgency = AttackUrgency::new(&values);
    for _ in 0..50 {
        let (next, launched) = advance_attack_urgency(&values, urgency, false);
        urgency = next;
        assert!(!launched, "urgency never rises without the trigger");
    }
    assert_eq!(urgency.attack, values.attack_initial);
}

#[test]
fn raise_attack_med_is_a_conserved_three_way_transfer() {
    let values = urgency_values();
    let (attack, defense, economy) = raise_attack_med(&values, 250, 250, 500).unwrap();
    assert_eq!(attack, 250 + values.change_med);
    assert_eq!(defense, 250 - values.change_med / 2);
    assert_eq!(economy, 500 - values.change_med / 2);
    assert_eq!(attack + defense + economy, 250 + 250 + 500);
}

#[test]
fn launch_reset_stops_early_once_defense_and_economy_run_out_of_headroom() {
    let values = urgency_values();
    let upper_defense = values.upper_bound_high_base - values.change_high;
    let (attack, defense, economy) =
        launch_reset(&values, values.launch, upper_defense, upper_defense);
    assert_eq!(attack, values.launch, "no headroom, no reset");
    assert_eq!(defense, upper_defense);
    assert_eq!(economy, upper_defense);
}

#[test]
fn command_points_low_is_at_or_below_the_threshold() {
    assert!(command_points_low(20, 20));
    assert!(command_points_low(5, 20));
    assert!(!command_points_low(21, 20));
}

fn marker(category: RouteCategory, x: f32) -> RouteMarker {
    RouteMarker {
        category,
        position: Vec2::new(x, 0.0),
    }
}

#[test]
fn pick_route_wraps_the_roll_onto_whichever_categories_are_actually_present() {
    let markers = [
        marker(RouteCategory::Flank, 10.0),
        marker(RouteCategory::Backdoor, 20.0),
    ];
    assert_eq!(
        pick_route(&markers, Vec2::ZERO, 0),
        Some(Vec2::new(10.0, 0.0))
    );
    assert_eq!(
        pick_route(&markers, Vec2::ZERO, 1),
        Some(Vec2::new(20.0, 0.0))
    );
    assert_eq!(
        pick_route(&markers, Vec2::ZERO, 2),
        Some(Vec2::new(10.0, 0.0))
    );
}

#[test]
fn pick_route_picks_the_nearest_marker_of_the_chosen_category() {
    let markers = [
        marker(RouteCategory::Center, 500.0),
        marker(RouteCategory::Center, 50.0),
    ];
    assert_eq!(
        pick_route(&markers, Vec2::ZERO, 0),
        Some(Vec2::new(50.0, 0.0)),
        "the nearer of the two Center markers wins"
    );
}

#[test]
fn pick_route_returns_none_with_no_markers_at_all() {
    assert_eq!(pick_route(&[], Vec2::ZERO, 0), None);
}

#[test]
fn route_markers_reads_the_category_from_the_template_name_case_insensitively() {
    use mint_sage_map::{Dictionary, Object};
    let object = |name: &str, x: f32| Object {
        x,
        y: 0.0,
        z: 0.0,
        angle: 0.0,
        flags: 0,
        template_name: name.to_owned(),
        properties: Dictionary::default(),
    };
    let objects = vec![
        object("center1", 1.0),
        object("Flank2", 2.0),
        object("BACKDOOR3", 3.0),
        object("NotAMarker", 4.0),
    ];
    let found = route_markers(&objects);
    assert_eq!(found.len(), 3, "only the three marker-shaped names count");
    assert!(found.iter().any(|m| m.category == RouteCategory::Center));
    assert!(found.iter().any(|m| m.category == RouteCategory::Flank));
    assert!(found.iter().any(|m| m.category == RouteCategory::Backdoor));
}
