use super::*;
use mint_sage_ini::eva::EvaEventDef;

fn event(name: &str, priority: u32) -> EvaEventDef {
    EvaEventDef {
        name: name.to_string(),
        priority,
        time_between_events_ms: 10_000,
        expiration_time_ms: 1_000,
        quiet_time_ms: 0,
        wait_before_playing_ms: 0,
        always_play_from_home_base: false,
        count_as_jump_to_location: false,
        side_sounds: vec![("Blue".to_string(), Some(format!("{name}Line")))],
        blocks: Vec::new(),
    }
}

fn eva(events: Vec<EvaEventDef>) -> Eva {
    Eva::new(
        &EvaDefs {
            events,
            announcers: Vec::new(),
            malformed: Vec::new(),
        },
        5,
    )
}

const FRAME_MS: f32 = 1000.0 / 30.0;

fn step(eva: &mut Eva, frame: u32) -> Option<EvaLine> {
    eva.update(frame, FRAME_MS, "Blue", Some([1.0, 2.0, 0.0]), &mut |_| {
        Some(500)
    })
}

#[test]
fn the_highest_priority_waiting_event_speaks() {
    let mut eva = eva(vec![event("Low", 2), event("High", 7)]);
    eva.report(0, 10, Some([5.0, 5.0, 0.0]));
    eva.report(1, 10, None);
    let line = step(&mut eva, 10).unwrap();
    assert_eq!(line.sound, "HighLine");
    assert_eq!(line.at, [1.0, 2.0, 0.0], "no place: the home base");
    assert_eq!(eva.last_played(1).map(|p| p.0), Some(10));
}

#[test]
fn a_played_event_refuses_reports_until_its_time_between_passes() {
    let mut eva = eva(vec![event("Only", 3)]);
    eva.report(0, 10, Some([5.0, 5.0, 0.0]));
    assert!(step(&mut eva, 10).is_some());
    assert!(eva.is_blocked(0));
    eva.report(0, 11, None);
    assert!(!eva.is_about_to_play(0));
    for frame in 11..(11 + 300) {
        step(&mut eva, frame);
    }
    assert!(!eva.is_blocked(0));
    eva.report(0, 400, None);
    assert!(step(&mut eva, 400).is_some());
}

#[test]
fn a_report_that_cannot_speak_expires() {
    let mut eva = eva(vec![event("A", 5), event("B", 1)]);
    eva.report(0, 10, None);
    eva.report(1, 10, None);
    assert!(eva
        .update(10, FRAME_MS, "Blue", None, &mut |_| Some(1500))
        .is_some());
    let mut later = Vec::new();
    for frame in 11..100 {
        later.extend(step(&mut eva, frame));
    }
    assert!(later.is_empty(), "{later:?}");
}

#[test]
fn playing_an_event_blocks_the_events_it_names() {
    let mut first = event("First", 5);
    first.blocks = vec!["Second".to_string()];
    let mut eva = eva(vec![first, event("Second", 4)]);
    eva.report(0, 10, None);
    eva.report(1, 10, None);
    assert_eq!(step(&mut eva, 10).unwrap().event, "First");
    assert!(eva.is_blocked(1));
    assert!(!eva.is_about_to_play(1));
    assert_eq!(eva.last_played(1), None);
}

#[test]
fn a_wait_holds_the_report_in_logic_frames() {
    let mut held = event("Held", 5);
    held.wait_before_playing_ms = 300;
    let mut eva = eva(vec![held]);
    eva.report(0, 10, None);
    assert!(step(&mut eva, 10).is_none());
    assert!(step(&mut eva, 11).is_none());
    assert!(!eva.is_about_to_play(0));
    assert!(
        step(&mut eva, 12).is_none(),
        "released after this frame's choice"
    );
    assert!(step(&mut eva, 13).is_some());
}

#[test]
fn equal_priority_breaks_by_the_most_recently_reported() {
    let mut eva = eva(vec![event("Distractor", 100), event("A", 5), event("B", 5)]);
    eva.report(0, 10, None);
    let spoken = eva.update(10, FRAME_MS, "Blue", None, &mut |_| Some(40));
    assert_eq!(spoken.unwrap().event, "Distractor");

    eva.report(1, 10, None);
    assert!(eva
        .update(11, FRAME_MS, "Blue", None, &mut |_| Some(500))
        .is_none());

    eva.report(2, 11, None);
    let winner = eva
        .update(12, FRAME_MS, "Blue", None, &mut |_| Some(500))
        .unwrap();
    assert_eq!(winner.event, "B");
}

#[test]
fn a_silent_event_still_counts_as_played() {
    let mut silent = event("Silent", 9);
    silent.side_sounds.clear();
    let mut eva = eva(vec![silent, event("Spoken", 1)]);
    eva.report(0, 20, Some([3.0, 4.0, 0.0]));
    eva.report(1, 20, None);
    assert_eq!(step(&mut eva, 20).unwrap().event, "Spoken");
    assert_eq!(eva.last_played(0), Some((20, [3.0, 4.0, 0.0])));
}
