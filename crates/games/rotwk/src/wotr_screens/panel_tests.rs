use mint_sage_apt_player::Value;

use super::panels::{build_queue, retinue_icon, retinue_page, Bind, MemberSlot, QueueSlot, Text};

fn member(id: u32, quantity: u32, leader: bool) -> MemberSlot {
    MemberSlot {
        id,
        name: format!("unit{id}"),
        quantity,
        level: 3,
        upgraded: false,
        leader,
        command_points: 10,
    }
}

fn text<'a>(binds: &'a [Bind], wanted: &str) -> Option<&'a Text> {
    binds.iter().find_map(|b| match b {
        Bind::Text { name, text } if name == wanted => Some(text),
        _ => None,
    })
}

fn plain(binds: &[Bind], wanted: &str) -> Option<String> {
    match text(binds, wanted)? {
        Text::Plain(s) => Some(s.clone()),
        Text::Formatted(label, value) => Some(format!("{label}:{value}")),
    }
}

fn picture<'a>(binds: &'a [Bind], wanted: &str) -> Option<&'a Option<String>> {
    binds.iter().find_map(|b| match b {
        Bind::Picture { name, element } if name == wanted => Some(element),
        _ => None,
    })
}

fn call_label(binds: &[Bind], function: &str) -> Option<Vec<Value>> {
    binds.iter().find_map(|b| match b {
        Bind::Call {
            function: f, args, ..
        } if *f == function => Some(args.clone()),
        _ => None,
    })
}

#[test]
fn the_retinue_names_the_picked_member_and_the_leader_and_sums_the_armys_command_points() {
    let members = [member(1, 1, true), member(2, 5, false), member(3, 2, false)];
    let page = retinue_page(&members, &[3, 2]);
    assert_eq!(plain(&page, "MemberName").as_deref(), Some("unit3"));
    assert_eq!(
        plain(&page, "MemberRank").as_deref(),
        Some("APT:RankLabel:3")
    );
    assert_eq!(plain(&page, "CommandPoints").as_deref(), Some("30"));
    assert_eq!(plain(&page, "LeaderQuantity").as_deref(), Some("1"));
    assert_eq!(
        picture(&page, "LeaderTypeImage"),
        Some(&Some("UnitHero".to_owned()))
    );
    assert_eq!(
        call_label(&page, "SetMemberNameState"),
        Some(vec![Value::str("_show")])
    );
}

#[test]
fn a_retinue_with_no_member_picked_and_no_leader_shows_none_of_them() {
    let members = [member(2, 5, false)];
    let page = retinue_page(&members, &[]);
    assert_eq!(plain(&page, "MemberName").as_deref(), Some(""));
    assert_eq!(plain(&page, "LeaderQuantity").as_deref(), Some(""));
    assert_eq!(picture(&page, "LeaderTypeImage"), Some(&None));
    assert_eq!(
        call_label(&page, "SetMemberNameState"),
        Some(vec![Value::str("_hide")])
    );
}

#[test]
fn a_unit_icon_carries_its_quantity_its_selection_and_its_upgrade() {
    let mut upgraded = member(7, 4, false);
    upgraded.upgraded = true;
    let icon = retinue_icon(&upgraded, true);
    assert_eq!(plain(&icon, "Quantity").as_deref(), Some("4"));
    assert_eq!(
        call_label(&icon, "SetState"),
        Some(vec![Value::str("_selected")])
    );
    assert_eq!(
        call_label(&icon, "SetUpgradeOverlayState"),
        Some(vec![Value::str("_show")])
    );
    let plain_icon = retinue_icon(&member(8, 1, false), false);
    assert_eq!(
        call_label(&plain_icon, "SetState"),
        Some(vec![Value::str("_up")])
    );
    assert_eq!(
        call_label(&plain_icon, "SetUpgradeOverlayState"),
        Some(vec![Value::str("_hide")])
    );
}

fn queued(name: &str, quantity: u32, build_time: i32, turns_remaining: i32) -> QueueSlot {
    QueueSlot {
        name: name.to_owned(),
        quantity,
        build_time,
        turns_remaining,
        hero: false,
        command_points: 20,
    }
}

#[test]
fn the_queue_head_is_in_progress_and_the_entries_behind_it_fill_the_queued_slots_in_order() {
    let queue = [queued("first", 3, 2, 1), queued("second", 4, 1, 1)];
    let binds = build_queue(&queue);
    assert_eq!(
        plain(&binds, "InProgressIconSlotQuantity").as_deref(),
        Some("3")
    );
    assert_eq!(
        plain(&binds, "InProgressIconSlotTurnsRemaining").as_deref(),
        Some("1")
    );
    assert_eq!(plain(&binds, "UnitName").as_deref(), Some("first"));
    assert_eq!(
        plain(&binds, "BuildTime").as_deref(),
        Some("STRATEGICHUD:BuildTimePluralLabel:2")
    );
    assert_eq!(
        call_label(&binds, "SetInProgressIconSlotState"),
        Some(vec![Value::str("_up")])
    );
    assert_eq!(
        plain(&binds, "QueuedIconSlotQuantity0").as_deref(),
        Some("4")
    );
    assert_eq!(
        plain(&binds, "QueuedIconSlotQuantity1").as_deref(),
        Some("")
    );
    let states: Vec<Vec<Value>> = binds
        .iter()
        .filter_map(|b| match b {
            Bind::Call {
                function: "SetQueuedIconSlotState",
                args,
                ..
            } => Some(args.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(states.len(), 6, "the movie has six queued slots");
    assert_eq!(states[0], vec![Value::Number(0.0), Value::str("_up")]);
    assert_eq!(states[1], vec![Value::Number(1.0), Value::str("_empty")]);
}

#[test]
fn an_empty_queue_empties_every_slot_and_hides_the_head_s_name_and_build_time() {
    let binds = build_queue(&[]);
    assert_eq!(
        call_label(&binds, "SetInProgressIconSlotState"),
        Some(vec![Value::str("_empty")])
    );
    assert_eq!(
        call_label(&binds, "SetUnitNameState"),
        Some(vec![Value::str("_hide")])
    );
    assert_eq!(plain(&binds, "BuildTime").as_deref(), Some(""));
}

#[test]
fn a_one_turn_entry_takes_the_singular_build_time_label() {
    let binds = build_queue(&[queued("only", 1, 1, 1)]);
    assert_eq!(
        plain(&binds, "BuildTime").as_deref(),
        Some("STRATEGICHUD:BuildTimeSingularLabel:1")
    );
}
