use super::group_tests::melee_pair;
use super::*;

#[test]
fn a_fleeing_victim_marks_the_whole_attacking_horde_as_running_it_down() {
    let (mut simulation, ids, sides) = melee_pair(AvoidanceMode::Off);
    let attacker = simulation
        .state()
        .get(ids.member, sides[0][0])
        .unwrap()
        .horde;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if sides[0]
            .iter()
            .any(|m| simulation.state().contains(ids.member_attack, *m))
        {
            break;
        }
    }
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: sides[1].clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 1000.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut marked = 0;
    for _ in 0..300 {
        simulation.tick().unwrap();
        let holders = sides[0]
            .iter()
            .filter(|m| simulation.state().contains(ids.running_down, **m))
            .count();
        if holders > 0 {
            marked += 1;
            assert_eq!(holders, sides[0].len(), "not every member was marked");
            assert!(simulation.state().contains(ids.running_down, attacker));
        }
    }
    assert!(marked > 0, "the attacking horde never ran the victim down");
}
