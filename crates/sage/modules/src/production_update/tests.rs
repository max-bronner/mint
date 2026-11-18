use super::*;

struct Fields(&'static [(&'static str, &'static str)]);

impl ModuleFieldSource for Fields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
            .map(|(_, value)| *value)
    }

    fn field_args(&self, keyword: &str) -> Vec<&str> {
        self.field(keyword).into_iter().collect()
    }
}

fn read(fields: &'static [(&'static str, &'static str)]) -> Result<ProductionUpdate, FieldError> {
    production_update_from_fields(&Fields(fields), Frame::ZERO)
}

fn door() -> ProductionUpdate {
    read(&[
        ("NumDoorAnimations", "1"),
        ("DoorOpeningTime", "3000"),
        ("DoorWaitOpenTime", "3000"),
        ("DoorCloseTime", "3000"),
    ])
    .unwrap()
}

fn change(add: &[&str], remove: &[&str]) -> Effect {
    conditions(add, remove)
}

#[test]
fn a_door_value_of_the_wrong_type_is_refused() {
    assert!(read(&[("NumDoorAnimations", "1.5")]).is_err());
    assert!(read(&[("NumDoorAnimations", "Yes")]).is_err());
    assert!(read(&[("DoorOpeningTime", "-100")]).is_err());
    assert!(read(&[("DoorCloseTime", "soon")]).is_err());
    assert!(!read(&[("NumDoorAnimations", "0")]).unwrap().has_doors());
    assert!(read(&[("NumDoorAnimations", "1")]).unwrap().has_doors());
}

#[test]
fn the_cycle_raises_and_clears_each_condition_after_its_time() {
    let data = door();
    let mut doors = ProductionDoors::default();
    let at = |frame| Frame::new(frame);
    assert_eq!(
        doors.exit(0, at(100)),
        (false, Some(change(&["DOOR_1_OPENING"], &[])))
    );
    assert!(doors.step(&data, at(190)).is_empty());
    assert_eq!(
        doors.step(&data, at(191)),
        [change(&["DOOR_1_WAITING_OPEN"], &["DOOR_1_OPENING"])]
    );
    assert_eq!(doors.exit(0, at(191)), (true, None));
    assert!(doors.step(&data, at(281)).is_empty());
    assert_eq!(
        doors.step(&data, at(282)),
        [change(&["DOOR_1_CLOSING"], &["DOOR_1_WAITING_OPEN"])]
    );
    assert!(doors.step(&data, at(372)).is_empty());
    assert_eq!(
        doors.step(&data, at(373)),
        [change(&[], &["DOOR_1_CLOSING"])]
    );
    assert!(doors.is_closed());
}

#[test]
fn an_exit_while_the_door_moves_waits_or_reopens_it() {
    let data = door();
    let mut doors = ProductionDoors::default();
    let at = |frame| Frame::new(frame);
    let _ = doors.exit(0, at(0));
    assert_eq!(doors.exit(0, at(50)), (false, None));
    let _ = doors.step(&data, at(91));
    assert_eq!(doors.exit(0, at(150)), (true, None));
    assert!(doors.step(&data, at(182)).is_empty());
    assert_eq!(doors.step(&data, at(241)).len(), 1);
    assert_eq!(
        doors.exit(0, at(260)),
        (
            true,
            Some(change(
                &["DOOR_1_WAITING_OPEN"],
                &["DOOR_1_OPENING", "DOOR_1_CLOSING"]
            ))
        )
    );
    assert!(doors.step(&data, at(350)).is_empty());
    assert_eq!(
        doors.step(&data, at(351)),
        [change(&["DOOR_1_CLOSING"], &["DOOR_1_WAITING_OPEN"])]
    );
}
