use super::*;

fn fonts() -> Fonts {
    Fonts {
        title: ("T".into(), 24.0),
        minor: ("M".into(), 16.0),
        normal: ("N".into(), 14.0),
    }
}

fn measure(text: &str, _: &Font) -> [i32; 2] {
    [i32::try_from(text.len()).unwrap() * 10, 20]
}

const INI: &str = "ScrollRate = 1\nScrollRateEveryFrames = 1\nScrollDown = No\n\
    TitleColor = R:192 G:238 B:255\nStyle = TITLE\nText = \"GUI:Title\"\n\
    Style = COLUMN\nText = \"Left\"\nText = \"Right\"\nEnd\n";

#[test]
fn lines_enter_at_the_bottom_and_rise_one_pixel_an_update() {
    let mut roll = CreditsRoll::parse(INI, fonts(), |label| format!("<{label}>")).unwrap();
    assert_eq!(roll.counts(), (2, 0));
    roll.update(300, 300, measure);
    let rows = roll.rows(300, 300);
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0].text, "<GUI:Title>");
    assert_eq!((rows.rows[0].x, rows.rows[0].y), (150.0 - 55.0, 300.0));
    for _ in 0..21 {
        roll.update(300, 300, measure);
    }
    assert_eq!(
        roll.counts().1,
        1,
        "the next line waits for height + 2 pixels"
    );
    roll.update(300, 300, measure);
    let rows = roll.rows(300, 300);
    assert_eq!(roll.counts().1, 2);
    let column: Vec<_> = rows
        .rows
        .iter()
        .skip(1)
        .map(|r| (r.text.as_str(), r.x))
        .collect();
    assert_eq!(column, [("Left", 100.0 - 20.0), ("Right", 200.0 - 25.0)]);
}

#[test]
fn the_roll_finishes_once_every_line_is_off_screen() {
    let mut roll = CreditsRoll::parse(INI, fonts(), str::to_string).unwrap();
    let mut updates = 0;
    while !roll.is_finished() && updates < 10_000 {
        roll.update(300, 300, measure);
        updates += 1;
    }
    assert!(roll.is_finished());
    assert!(updates > 300, "{updates}");
}

#[test]
fn an_unknown_statement_is_an_error() {
    assert!(CreditsRoll::parse("Speed = 3\n", fonts(), str::to_string).is_err());
}
