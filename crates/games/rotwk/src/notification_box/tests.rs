use super::*;

fn entry(message: &str, ms: u32, audio: Option<&str>) -> Entry {
    Entry {
        title: "T".to_owned(),
        icon: None,
        open_audio: audio.map(str::to_owned),
        message: message.to_owned(),
        font: Some(mint_sage_ini::MessageFont {
            name: "Face".to_owned(),
            size: 16.0,
            bold: false,
        }),
        colour: Some([255, 204, 0, 255]),
        ms,
        anchor: Anchor::Main,
    }
}

fn ready() -> Manager {
    let mut manager = Manager::default();
    manager.on_initialized();
    manager.set_message_width(100.0);
    manager
}

#[test]
fn two_entries_in_one_frame_present_only_the_newest_with_its_own_sound() {
    let mut manager = ready();
    manager.submit(entry("first", 1000, Some("SoundA")));
    manager.submit(entry("second", 1000, Some("SoundB")));
    let effects = manager.update(0, false);
    assert_eq!(
        effects,
        [Effect::Present(entry("second", 1000, Some("SoundB")))]
    );
    assert!(manager.update(1, false).is_empty());
}

#[test]
fn a_new_entry_presents_over_one_that_is_open() {
    let mut manager = ready();
    manager.submit(entry("first", 0, None));
    manager.update(0, false);
    manager.on_open(10);
    manager.submit(entry("second", 0, None));
    assert!(
        matches!(manager.update(20, false).as_slice(), [Effect::Present(e)] if e.message == "second")
    );
}

#[test]
fn the_box_closes_after_its_time_counted_from_opening_and_never_with_zero() {
    let mut manager = ready();
    manager.submit(entry("m", 5000, None));
    manager.update(100, false);
    manager.on_open(1000);
    assert!(manager.update(5999, false).is_empty());
    assert_eq!(manager.update(6000, false), [Effect::Close]);
    manager.on_closed();

    manager.submit(entry("stays", 0, None));
    manager.update(7000, false);
    manager.on_open(7000);
    assert!(manager.update(1_000_000, false).is_empty());
    assert_eq!(manager.on_close_button(), Some(Effect::Close));
}

#[test]
fn nothing_is_presented_before_the_movie_initialises_and_a_widthless_movie_hides_the_box() {
    let mut manager = Manager::default();
    manager.submit(entry("m", 0, None));
    assert!(manager.update(0, false).is_empty());
    manager.on_initialized();
    assert_eq!(manager.update(0, false), [Effect::Hide]);
    manager.submit(Entry {
        font: None,
        ..entry("m", 0, None)
    });
    manager.set_message_width(50.0);
    assert_eq!(manager.update(0, false), [Effect::Hide]);
}

#[test]
fn an_object_type_names_its_template_button_image_as_the_icon() {
    let templates = |name: &str| match name {
        "Hero" => Some(Some("HeroButton".to_owned())),
        "Bare" => Some(None),
        _ => None,
    };
    assert_eq!(
        icon_for(Some("Objective"), "Hero", templates).as_deref(),
        Some("HeroButton")
    );
    assert_eq!(icon_for(Some("Objective"), "Bare", templates), None);
    assert_eq!(
        icon_for(Some("Objective"), "Unknown", templates).as_deref(),
        Some("Objective")
    );
    assert_eq!(
        icon_for(Some("Objective"), "", templates).as_deref(),
        Some("Objective")
    );
}

#[test]
fn a_pending_entry_is_dropped_while_the_letterbox_is_up_and_not_shown_after() {
    let mut manager = ready();
    manager.submit(entry("m", 0, None));
    assert_eq!(manager.update(0, true), [Effect::Hide]);
    assert!(manager.update(1, false).is_empty());
}

#[test]
fn the_letterbox_alone_does_not_hide_a_box_that_is_open() {
    let mut manager = ready();
    manager.submit(entry("m", 5000, None));
    manager.update(0, false);
    manager.on_open(0);
    assert!(manager.update(10, true).is_empty());
    manager.submit(entry("later", 5000, None));
    assert_eq!(manager.update(20, true), [Effect::Hide]);
    assert!(manager.update(6000, false).is_empty());
}
