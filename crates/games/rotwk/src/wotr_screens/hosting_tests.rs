use mint_sage_livingworld::ArmyId;

use super::forward::{Signal, Told};
use super::message::{fill, is_button_command, Message, MessageQueue, MessageSource};

fn message(source: MessageSource, title: &str) -> Message {
    Message {
        source,
        title: title.to_owned(),
        text: String::new(),
        args: Vec::new(),
    }
}

#[test]
fn message_pop_ups_show_one_at_a_time_in_order_and_closing_one_reports_its_source() {
    let mut queue = MessageQueue::default();
    queue.push(message(MessageSource::Territory, "first"));
    queue.push(message(MessageSource::Tutorial, "second"));
    assert_eq!(queue.start_next().map(|m| m.title.as_str()), Some("first"));
    assert!(
        queue.start_next().is_none(),
        "the second waits while the first is up"
    );
    assert!(queue.close());
    assert_eq!(queue.take_closed(), vec![MessageSource::Territory]);
    assert_eq!(queue.start_next().map(|m| m.title.as_str()), Some("second"));
    assert!(queue.close());
    assert!(!queue.close(), "nothing is up to close");
    assert_eq!(queue.take_closed(), vec![MessageSource::Tutorial]);
    assert!(queue.take_closed().is_empty());
}

#[test]
fn only_a_button_of_the_message_box_clip_closes_the_pop_up() {
    assert!(is_button_command("_level0.MessageBox", "_OnButtonOk"));
    assert!(is_button_command(
        "_level0.StrategicMessageBox",
        "_OnButtonOk"
    ));
    assert!(!is_button_command("_level0.MessageBox", "_OnShowing"));
    assert!(!is_button_command("_level0.Other", "_OnButtonOk"));
}

#[test]
fn a_new_selection_sends_the_old_armys_deselect_before_the_new_armys_select() {
    let (a, b) = (ArmyId(1), ArmyId(2));
    let mut told = Told::default();
    assert!(told.changes(None, None).is_empty());
    assert_eq!(told.changes(Some(a), None), vec![Signal::Select(a)]);
    assert!(
        told.changes(Some(a), None).is_empty(),
        "an unchanged selection says nothing"
    );
    assert_eq!(
        told.changes(Some(b), None),
        vec![Signal::Deselect(a), Signal::Select(b)]
    );
    assert_eq!(told.changes(None, None), vec![Signal::Deselect(b)]);
}

#[test]
fn a_refused_army_was_never_selected_so_dropping_it_sends_no_deselect() {
    let a = ArmyId(1);
    let mut told = Told::default();
    assert_eq!(told.changes(Some(a), None), vec![Signal::Select(a)]);
    told.refused();
    assert!(told.changes(None, None).is_empty());
}

#[test]
fn each_ls_place_of_a_message_takes_the_next_argument_and_a_missing_one_is_empty() {
    let args = vec!["Arnor".to_owned(), "+10 CP".to_owned()];
    assert_eq!(
        fill("region of %ls.\n\nGrants:\n%ls", &args),
        "region of Arnor.\n\nGrants:\n+10 CP"
    );
    assert_eq!(fill("%ls and %ls", &args[..1]), "Arnor and ");
    assert_eq!(fill("no places", &args), "no places");
}

#[test]
fn a_battle_the_prompt_opens_for_is_told_once() {
    let mut told = Told::default();
    assert_eq!(told.changes(None, Some(7)), vec![Signal::Battle(7)]);
    assert!(told.changes(None, Some(7)).is_empty());
    assert!(told.changes(None, None).is_empty());
    assert_eq!(told.changes(None, Some(7)), vec![Signal::Battle(7)]);
}
