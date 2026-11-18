use std::borrow::Cow;

use mint_sage_wnd::Layout;

use super::check::State;
use super::list::unit;

pub const TEXT_INSET: f32 = 5.0;

pub const PEN_LEAD: f32 = 2.0;

pub const CARET_TOP: f32 = 2.0;

pub const CARET_WIDTH: f32 = 3.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryContent {
    pub text: String,
    pub focused: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryLook {
    pub font: String,
    pub points: f32,
    pub text: [[f32; 4]; 3],
    pub border: [[f32; 4]; 3],
    pub one_line: bool,
    pub masked: bool,
}

impl EntryLook {
    #[must_use]
    pub fn plain() -> Self {
        Self {
            font: "Albertus MT".to_owned(),
            points: super::combo::TEXT_POINTS,
            text: [[1.0; 4], [1.0; 4], [0.75, 0.75, 0.75, 1.0]],
            border: [[0.0, 0.0, 0.0, 1.0]; 3],
            one_line: false,
            masked: false,
        }
    }

    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        let mut look = Self::plain();
        let Some(window) = layout.windows.first() else {
            return look;
        };
        if let Some(font) = window.font() {
            look.font = font.name;
        }
        if let Some(c) = window.text_colors() {
            look.text = [unit(c.enabled), unit(c.hilite), unit(c.disabled)];
            look.border = [
                unit(c.enabled_border),
                unit(c.hilite_border),
                unit(c.disabled_border),
            ];
        }
        look.one_line = window
            .status()
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("ONE_LINE"));
        look.masked = window.text_entry().is_some_and(|data| data.secret);
        look
    }

    #[must_use]
    pub fn colours(&self, state: State) -> ([f32; 4], [f32; 4]) {
        let i = state as usize;
        (
            self.text.get(i).copied().unwrap_or([1.0; 4]),
            self.border.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]),
        )
    }
}

#[must_use]
pub fn shown_text(text: &str, masked: bool) -> Cow<'_, str> {
    if masked {
        Cow::Owned("*".repeat(text.chars().count()))
    } else {
        Cow::Borrowed(text)
    }
}

#[must_use]
pub fn text_origin(bounds: [f32; 4], line: f32, one_line: bool) -> [f32; 2] {
    let [left, top, _, bottom] = bounds;
    let y = if one_line {
        (top + bottom - line) * 0.5
    } else {
        top + TEXT_INSET
    };
    [left + TEXT_INSET + PEN_LEAD, y]
}

#[must_use]
pub fn caret_rect(bounds: [f32; 4], x: f32, line: f32) -> [f32; 4] {
    let [_, top, _, bottom] = bounds;
    let y = top + CARET_TOP;
    let height = (bottom - top - 2.0 * CARET_TOP).max(line);
    [x, y, x + CARET_WIDTH, y + height]
}

#[must_use]
pub fn caret_lit(focused: bool, millis: u128) -> bool {
    focused && millis & 0x200 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_sits_inset_from_the_top_left_and_one_line_centres() {
        let bounds = [100.0, 40.0, 400.0, 80.0];
        assert_eq!(text_origin(bounds, 20.0, false), [107.0, 45.0]);
        assert_eq!(text_origin(bounds, 20.0, true), [107.0, 50.0]);
    }

    #[test]
    fn the_caret_spans_the_box_less_its_insets_or_the_line() {
        let bounds = [0.0, 10.0, 200.0, 40.0];
        assert_eq!(caret_rect(bounds, 50.0, 12.0), [50.0, 12.0, 53.0, 38.0]);
        assert_eq!(caret_rect(bounds, 50.0, 30.0), [50.0, 12.0, 53.0, 42.0]);
    }

    #[test]
    fn a_secret_entry_shows_stars_and_the_caret_blinks_on_bit_nine() {
        assert_eq!(shown_text("Elrond", true), "******");
        assert_eq!(shown_text("Elrond", false), "Elrond");
        assert!(!caret_lit(true, 0x1FF));
        assert!(caret_lit(true, 0x200));
        assert!(!caret_lit(false, 0x200));
    }

    #[test]
    fn reads_font_colours_and_flags_from_the_template() {
        let text = "WINDOW\n WINDOWTYPE = ENTRYFIELD;\n STATUS = ENABLED+ONE_LINE;\n \
                    FONT = NAME: \"Albertus MT\", SIZE: 16, BOLD: 0;\n \
                    TEXTCOLOR = ENABLED: 255 255 255 255, ENABLEDBORDER: 0 0 0 255, \
                    DISABLED: 192 192 192 255, DISABLEDBORDER: 64 64 64 255, \
                    HILITE: 255 255 255 255, HILITEBORDER: 0 0 0 255;\n \
                    TEXTENTRYDATA = MAXLEN: 256, SECRETTEXT: 1;\nEND\n";
        let layout = Layout::parse(text).unwrap();
        let look = EntryLook::read(&layout);
        assert_eq!(look.font, "Albertus MT");
        assert!(look.one_line);
        assert!(look.masked);
        let (fg, shadow) = look.colours(State::Disabled);
        assert!((fg[0] - 192.0 / 255.0).abs() < 1e-6);
        assert!((shadow[0] - 64.0 / 255.0).abs() < 1e-6);
    }
}
