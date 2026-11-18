use super::*;

fn ten_per_char(text: &str) -> f32 {
    text.chars().count() as f32 * 10.0
}

fn after(narration: &mut Narration, frames: u64) {
    narration.advance(Duration::from_secs_f32(frames as f32 / 30.0 + 0.001));
}

#[test]
fn a_line_breaks_at_a_newline_and_before_the_word_that_passes_the_wrap() {
    let wrapped = lines("aaa bbb ccc\nd", 80.0, &ten_per_char);
    assert_eq!(wrapped, ["aaa bbb", "ccc", "d"]);
    assert_eq!(lines("", 80.0, &ten_per_char), [""]);
}

#[test]
fn one_word_wider_than_the_wrap_stays_on_a_line_of_its_own() {
    assert_eq!(
        lines("abcdefghijkl x", 50.0, &ten_per_char),
        ["abcdefghijkl", "x"]
    );
}

#[test]
fn the_area_is_the_objects_default_until_the_tutorial_moves_it() {
    let mut narration = Narration::default();
    assert_eq!(narration.area(), DEFAULT_AREA);
    narration.show("a", 1_000_000.0);
    let at = narration.placed((1024.0, 768.0), 14.0, &ten_per_char);
    assert_eq!((at[0].x, at[0].y), (20.0, 125.0));
    narration.set_area(DIALOG_AREA);
    let moved = narration.placed((1024.0, 768.0), 14.0, &ten_per_char);
    assert!((moved[0].x - 390.0).abs() < 0.1 && (moved[0].y - 554.0).abs() < 0.1);
}

#[test]
fn the_wrap_width_is_a_fraction_of_the_window() {
    let mut narration = Narration::default();
    narration.show("aaaa bbbb cccc", 1_000_000.0);
    let wide = narration.placed((1000.0, 700.0), 14.0, &ten_per_char);
    assert_eq!(wide.len(), 1);
    narration.set_area(Area {
        wrap: 0.05,
        ..DEFAULT_AREA
    });
    let narrow = narration.placed((1000.0, 700.0), 14.0, &ten_per_char);
    assert_eq!(narrow.len(), 3);
}

#[test]
fn each_line_is_one_and_a_quarter_font_heights_below_the_one_before() {
    let mut narration = Narration::default();
    narration.show("a\nb\nc", 1_000_000.0);
    let placed = narration.placed((1024.0, 768.0), 16.0, &ten_per_char);
    let ys: Vec<f32> = placed.iter().map(|l| l.y).collect();
    assert_eq!(ys, [125.0, 145.0, 165.0]);
}

#[test]
fn each_lines_reveal_starts_thirty_frames_after_the_previous() {
    let mut narration = Narration::default();
    narration.show("a\nb\nc", 1_000_000.0);
    let alphas = |n: &Narration| -> Vec<f32> {
        n.placed((1024.0, 768.0), 14.0, &ten_per_char)
            .iter()
            .map(|l| l.alpha)
            .collect()
    };
    assert_eq!(alphas(&narration), [0.0, 0.0, 0.0]);
    narration.frame = 4;
    let early = alphas(&narration);
    assert!(early[0] > 0.0 && early[0] < 1.0, "{early:?}");
    assert_eq!((early[1], early[2]), (0.0, 0.0));
    narration.frame = 30 + REVEAL_FRAMES;
    assert_eq!(alphas(&narration), [1.0, 1.0, 0.0]);
    narration.frame = 60 + REVEAL_FRAMES;
    assert_eq!(alphas(&narration), [1.0, 1.0, 1.0]);
}

#[test]
fn a_timed_text_clears_after_its_seconds_and_fades_over_the_last_one() {
    let mut narration = Narration::default();
    narration.show("a", 3.0);
    narration.frame = 60;
    let at = |n: &Narration| n.placed((1024.0, 768.0), 14.0, &ten_per_char)[0].alpha;
    assert_eq!(at(&narration), 1.0);
    narration.frame = 75;
    assert_eq!(at(&narration), 0.5);
    narration.advance(Duration::from_secs_f32(1.0));
    assert!(!narration.is_showing());
}

#[test]
fn an_untimed_text_never_clears_by_time_and_starts_fading_fifteen_seconds_in() {
    let mut narration = Narration::default();
    narration.show("a", 0.0);
    narration.frame = 450;
    let at = |n: &Narration| n.placed((1024.0, 768.0), 14.0, &ten_per_char)[0].alpha;
    assert_eq!(at(&narration), 1.0);
    narration.frame = 465;
    assert_eq!(at(&narration), 0.5);
    after(&mut narration, 100_000);
    assert!(narration.is_showing());
}

#[test]
fn clearing_takes_the_text_down() {
    let mut narration = Narration::default();
    narration.show("a", 1_000_000.0);
    narration.clear();
    assert!(narration
        .placed((1024.0, 768.0), 14.0, &ten_per_char)
        .is_empty());
}

#[test]
fn frame_time_counts_in_thirty_hertz_frames() {
    let mut narration = Narration::default();
    narration.advance(Duration::from_millis(500));
    assert_eq!(narration.frame, 15);
    narration.advance(Duration::from_millis(17));
    narration.advance(Duration::from_millis(17));
    assert_eq!(narration.frame, 16);
}
