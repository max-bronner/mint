use super::*;

#[test]
fn selecting_a_horde_by_its_handle_selects_its_members() {
    let (mut simulation, ids) = horde_sim(Vec2::ZERO, square_formation());
    let horde = horde_entity(&simulation, ids).unwrap();
    let mut members = members_of(&simulation, ids);
    assert!(!members.is_empty());

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation.tick().unwrap();

    let mut selected = selection(&simulation, ids, PLAYER);
    selected.sort_unstable_by_key(|entity| entity.index());
    members.sort_unstable_by_key(|entity| entity.index());
    assert_eq!(selected, members);
}
