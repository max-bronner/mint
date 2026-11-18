use super::*;

fn type_all(caption: &mut Caption) -> u32 {
    (0..200).map(|_| caption.update(33, false).clicks).sum()
}

#[test]
fn the_origin_rounds_the_fraction_of_the_display_to_a_pixel() {
    let style = CaptionStyle {
        position: [0.1, 0.2],
        ..CaptionStyle::default()
    };
    assert_eq!(origin(&style, 1024.0, 768.0), [102.0, 154.0]);
}

#[test]
fn one_non_space_character_is_typed_per_update_and_spaces_ride_along() {
    let mut caption = Caption::new("ab c", 1000, 16);
    caption.update(33, false);
    assert_eq!(
        (caption.lines()[0].as_str(), caption.staged().map(|s| s.0)),
        ("", Some('a'))
    );
    caption.update(33, false);
    assert_eq!(
        (caption.lines()[0].as_str(), caption.staged().map(|s| s.0)),
        ("a", Some('b'))
    );
    let step = caption.update(33, false);
    assert_eq!(caption.lines()[0], "ab ");
    assert_eq!(step.clicks, 1);
    assert!(caption.duration_ms >= caption.elapsed_ms + 2000);
    for _ in 0..5 {
        caption.update(33, false);
    }
    assert_eq!(
        caption.lines()[0],
        "ab ",
        "the last character never reaches the line"
    );
}

#[test]
fn every_typed_character_clicks_and_spaces_do_not() {
    let mut caption = Caption::new("ab c", 1000, 16);
    assert_eq!(type_all(&mut caption), 3);
}

#[test]
fn a_shell_menu_adds_no_time_and_a_stall_counts_at_most_a_hundred_ms() {
    let mut caption = Caption::new("abc", 1000, 16);
    caption.update(33, true);
    assert_eq!(caption.elapsed_ms, 0);
    caption.update(5000, false);
    assert_eq!(caption.elapsed_ms, 100);
}

#[test]
fn a_fifth_line_is_dropped_and_the_caption_fades_out_after_its_hold() {
    let mut caption = Caption::new("a\nb\nc\nd\ne\nf", 1000, 16);
    for _ in 0..60 {
        caption.update(33, false);
    }
    assert_eq!(caption.lines().len(), 4);
    let mut removed_at = None;
    for frame in 0..400 {
        if caption.update(33, false).removed {
            removed_at = Some(frame);
            break;
        }
    }
    assert!(removed_at.is_some());
    assert!(caption.elapsed_ms > caption.duration_ms);
}
