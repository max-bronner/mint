use super::*;

#[test]
fn presses_step_older_then_wrap_and_reset_after_the_window() {
    let mut list = JumpList::new(400.0, 2000);
    list.push([0.0, 0.0]);
    list.push([1000.0, 0.0]);
    list.push([2000.0, 0.0]);

    assert_eq!(list.press(Duration::from_secs(0)), Some([2000.0, 0.0]));
    assert_eq!(list.press(Duration::from_secs(1)), Some([1000.0, 0.0]));
    assert_eq!(list.press(Duration::from_secs(2)), Some([0.0, 0.0]));
    assert_eq!(list.press(Duration::from_millis(2500)), Some([2000.0, 0.0]));
    assert_eq!(list.press(Duration::from_millis(4600)), Some([2000.0, 0.0]));
}

#[test]
fn the_reset_window_is_inclusive() {
    let mut list = JumpList::new(400.0, 2000);
    list.push([0.0, 0.0]);
    list.push([1000.0, 0.0]);
    assert_eq!(list.press(Duration::from_millis(0)), Some([1000.0, 0.0]));
    assert_eq!(list.press(Duration::from_millis(2000)), Some([0.0, 0.0]));
}

#[test]
fn a_near_point_refreshes_the_newest_in_place() {
    let mut list = JumpList::new(400.0, 2000);
    list.push([0.0, 0.0]);
    list.push([399.0, 0.0]);
    assert_eq!(
        list.press(Duration::from_secs(0)),
        Some([399.0, 0.0]),
        "the refreshed point"
    );
    assert_eq!(list.press(Duration::from_millis(1)), Some([399.0, 0.0]));
}

#[test]
fn a_far_point_is_pushed_as_a_new_entry() {
    let mut list = JumpList::new(400.0, 2000);
    list.push([0.0, 0.0]);
    list.push([401.0, 0.0]);
    assert_eq!(list.press(Duration::from_secs(0)), Some([401.0, 0.0]));
    assert_eq!(list.press(Duration::from_millis(1)), Some([0.0, 0.0]));
}

#[test]
fn a_push_lands_at_the_front_not_the_back() {
    let mut list = JumpList::new(400.0, 2000);
    list.push([0.0, 0.0]);
    list.push([1000.0, 0.0]);
    assert_eq!(list.press(Duration::from_secs(0)), Some([1000.0, 0.0]));
}

#[test]
fn an_empty_list_presses_to_nothing() {
    let mut list = JumpList::new(400.0, 2000);
    assert_eq!(list.press(Duration::from_secs(0)), None);
}
