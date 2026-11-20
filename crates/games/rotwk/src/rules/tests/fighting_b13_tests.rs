use super::*;

#[test]
fn a_ranged_pick_under_a_range_bonus_reaches_the_range_factor_squared() {
    use super::fighting_b11_tests::{line_of_members, scene_with_banner};
    let (_, last, along) = line_of_members();
    let at = last.plus(along.scale(110.0));
    let banner = at.plus(Vec2::new(0.0, 12.0));
    let far = (1..=24)
        .filter_map(|seed| {
            let (mut simulation, _, members, who) =
                scene_with_banner(seed, "Archer", Some(at), Some(banner));
            let who = who.unwrap();
            let ids = simulation.rules().ids().unwrap();
            for _ in 0..60 {
                simulation.tick().unwrap();
                if let Some(target) = simulation
                    .state()
                    .get(ids.armed, who)
                    .and_then(|armed| armed.target)
                {
                    return members.iter().position(|member| *member == target);
                }
            }
            None
        })
        .collect::<Vec<_>>();
    assert!(far.len() >= 20, "the archer never acquired: {far:?}");
    assert!(far.contains(&0), "the far member was never picked: {far:?}");
}
