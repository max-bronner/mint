use mint_sage_apt_viewer::host::{PlacedRow, PlacedText};
use mint_vfs::Vfs;

pub(crate) const RENDER_CREDITS: &str = "AptMainMenu::RenderCredits";
pub(crate) const HIDE_CREDITS: &str = "HideCredits";
pub(crate) const UPDATES_PER_SECOND: f32 = 100.0;

const CREDITS_INI: &str = "data/ini/credits.ini";
const SPACE_OFFSET: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Title,
    MinorTitle,
    Normal,
    Column,
    Blank,
}

impl Style {
    fn parse(token: &str) -> Option<Self> {
        [
            ("TITLE", Self::Title),
            ("MINORTITLE", Self::MinorTitle),
            ("NORMAL", Self::Normal),
            ("COLUMN", Self::Column),
        ]
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, style)| style)
    }
}

#[derive(Debug, Clone)]
struct Line {
    style: Style,
    text: String,
    second: String,
    paired: bool,
}

#[derive(Debug, Clone)]
struct Shown {
    line: usize,
    x: i32,
    y: i32,
    height: i32,
    widths: [i32; 2],
}

pub(crate) type Font = (String, f32);

#[derive(Debug, Clone)]
pub(crate) struct Fonts {
    pub(crate) title: Font,
    pub(crate) minor: Font,
    pub(crate) normal: Font,
}

impl Fonts {
    pub(crate) fn load(vfs: &Vfs) -> Result<Self, String> {
        let one = |field: &str| -> Result<Font, String> {
            crate::help_box::language_font(vfs, field)?
                .map(|(family, points, _bold)| (family, points))
                .ok_or_else(|| format!("language.ini names no {field}"))
        };
        Ok(Self {
            title: one("CreditsTitleFont")?,
            minor: one("CreditsMinorTitleFont")?,
            normal: one("CreditsNormalFont")?,
        })
    }

    fn of(&self, style: Style) -> &Font {
        match style {
            Style::Title => &self.title,
            Style::MinorTitle => &self.minor,
            Style::Normal | Style::Column | Style::Blank => &self.normal,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CreditsRoll {
    scroll_rate: i32,
    every: i32,
    scroll_down: bool,
    colours: [[u8; 4]; 3],
    lines: Vec<Line>,
    next: usize,
    shown: Vec<Shown>,
    frames: i32,
    finished: bool,
    fonts: Fonts,
}

impl CreditsRoll {
    pub(crate) fn load(
        vfs: &Vfs,
        fonts: Fonts,
        fetch: impl Fn(&str) -> String,
    ) -> Result<Self, String> {
        let bytes = vfs
            .read(CREDITS_INI)
            .map_err(|e| format!("{CREDITS_INI}: {e}"))?;
        Self::parse(&mint_sage_ini::lex::decode_latin1(&bytes), fonts, fetch)
    }

    fn parse(text: &str, fonts: Fonts, fetch: impl Fn(&str) -> String) -> Result<Self, String> {
        let mut roll = Self {
            scroll_rate: 1,
            every: 1,
            scroll_down: true,
            colours: [[255; 4]; 3],
            lines: Vec::new(),
            next: 0,
            shown: Vec::new(),
            frames: 0,
            finished: false,
            fonts,
        };
        let mut style = Style::Normal;
        for (number, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            let at = || format!("{CREDITS_INI}:{}", number.saturating_add(1));
            let (key, value) = match line.split_once('=') {
                Some((key, value)) => (key.trim(), value.trim()),
                None => (line, ""),
            };
            match key.to_ascii_lowercase().as_str() {
                "end" => break,
                "blank" => roll.add(style_line(Style::Blank, String::new())),
                "scrollrate" => roll.scroll_rate = int(value).ok_or_else(at)?,
                "scrollrateeveryframes" => roll.every = int(value).ok_or_else(at)?,
                "scrolldown" => roll.scroll_down = boolean(value).ok_or_else(at)?,
                "titlecolor" => roll.colours[0] = colour(value).ok_or_else(at)?,
                "minortitlecolor" => roll.colours[1] = colour(value).ok_or_else(at)?,
                "normalcolor" => roll.colours[2] = colour(value).ok_or_else(at)?,
                "style" => style = Style::parse(value).ok_or_else(at)?,
                "text" => {
                    let raw = value.trim_matches('"');
                    let shown = if raw == "<BLANK>" {
                        String::new()
                    } else if raw.contains(':') {
                        fetch(raw)
                    } else {
                        raw.to_string()
                    };
                    roll.add_text(style, shown);
                }
                _ => return Err(format!("{}: unknown statement {key}", at())),
            }
        }
        roll.every = roll.every.max(1);
        roll.scroll_rate = roll.scroll_rate.max(1);
        Ok(roll)
    }

    fn add(&mut self, line: Line) {
        self.lines.push(line);
    }

    fn add_text(&mut self, style: Style, text: String) {
        if style == Style::Column {
            if let Some(last) = self
                .lines
                .last_mut()
                .filter(|l| l.style == Style::Column && !l.paired)
            {
                last.second = text;
                last.paired = true;
                return;
            }
        }
        self.add(style_line(style, text));
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    pub(crate) fn update(
        &mut self,
        width: i32,
        height: i32,
        mut measure: impl FnMut(&str, &Font) -> [i32; 2],
    ) {
        if self.finished || width <= 0 || height <= 0 {
            return;
        }
        self.frames = self.frames.saturating_add(1);
        if self.frames % self.every != 0 {
            return;
        }
        let down = self.scroll_down;
        let (start, end) = if down { (0, height) } else { (height, 0) };
        let off_start = if down { -1 } else { 0 };
        let off_end = i32::from(!down);
        let step = if down {
            self.scroll_rate
        } else {
            -self.scroll_rate
        };
        let mut y_test = 0;
        self.shown.retain_mut(|row| {
            row.y = row.y.saturating_add(step);
            y_test = row
                .y
                .saturating_add(row.height.saturating_add(SPACE_OFFSET) * off_end);
            !((down && y_test > end) || (!down && y_test < end))
        });
        if !((down && y_test >= start) || (!down && y_test <= start)) {
            return;
        }
        if self.shown.is_empty() && self.next >= self.lines.len() {
            self.finished = true;
        }
        let Some(line) = self.lines.get(self.next).cloned() else {
            return;
        };
        let font = self.fonts.of(line.style).clone();
        let mut row = Shown {
            line: self.next,
            x: 0,
            y: start,
            height: 0,
            widths: [0; 2],
        };
        let mut place = |text: &str, slot: usize, row: &mut Shown| {
            if text.is_empty() {
                return;
            }
            let [w, h] = measure(text, &font);
            row.height = h;
            row.x = width / 2 - w / 2;
            row.y = start.saturating_add(h * off_start);
            if let Some(width) = row.widths.get_mut(slot) {
                *width = w;
            }
        };
        match line.style {
            Style::Blank => {
                let [_, h] = measure("", &font);
                row.height = h;
                row.y = start.saturating_add(h * off_start);
            }
            _ => {
                place(&line.text, 0, &mut row);
                place(&line.second, 1, &mut row);
            }
        }
        self.shown.push(row);
        self.next = self.next.saturating_add(1);
    }

    pub(crate) fn rows(&self, width: i32, height: i32) -> PlacedText {
        let chunk = height / 3;
        let mut rows = Vec::new();
        for row in &self.shown {
            let Some(line) = self.lines.get(row.line) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let perc = if row.y >= chunk && row.y <= chunk * 2 {
                1.0
            } else if row.y < 0 || row.y > height {
                0.0
            } else if row.y < chunk {
                row.y as f32 / chunk as f32
            } else {
                1.0 - (row.y - 2 * chunk) as f32 / chunk as f32
            };
            let colour = match line.style {
                Style::Title => self.colours[0],
                Style::MinorTitle => self.colours[1],
                _ => self.colours[2],
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let alpha = (f32::from(colour[3]) * perc).clamp(0.0, 255.0) as u8;
            let rgba = [
                f32::from(colour[0]) / 255.0,
                f32::from(colour[1]) / 255.0,
                f32::from(colour[2]) / 255.0,
                f32::from(alpha) / 255.0,
            ];
            let font = self.fonts.of(line.style);
            let mut push = |text: &str, x: i32| {
                if text.is_empty() {
                    return;
                }
                #[allow(clippy::cast_precision_loss)]
                rows.push(PlacedRow {
                    text: text.to_string(),
                    font: font.0.clone(),
                    point_size: font.1,
                    x: x as f32,
                    y: row.y as f32,
                    color: rgba,
                });
            };
            match line.style {
                Style::Blank => {}
                Style::Column => {
                    let column = width / 3;
                    push(&line.text, column - row.widths[0] / 2);
                    push(&line.second, 2 * column - row.widths[1] / 2);
                }
                _ => push(&line.text, row.x),
            }
        }
        PlacedText { rows }
    }

    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.lines.len(), self.shown.len())
    }
}

fn style_line(style: Style, text: String) -> Line {
    Line {
        style,
        text,
        second: String::new(),
        paired: false,
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b';' if !quoted => return line.get(..i).unwrap_or(line),
            b'/' if !quoted && bytes.get(i.saturating_add(1)) == Some(&b'/') => {
                return line.get(..i).unwrap_or(line);
            }
            _ => {}
        }
    }
    line
}

fn int(value: &str) -> Option<i32> {
    value.split_whitespace().next()?.parse().ok()
}

fn boolean(value: &str) -> Option<bool> {
    match value
        .split_whitespace()
        .next()?
        .to_ascii_lowercase()
        .as_str()
    {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

fn colour(value: &str) -> Option<[u8; 4]> {
    let mut out = [0, 0, 0, 255];
    let mut seen = 0;
    for token in value.split_whitespace() {
        let (channel, number) = token.split_once(':')?;
        let index = match channel.to_ascii_uppercase().as_str() {
            "R" => 0,
            "G" => 1,
            "B" => 2,
            "A" => 3,
            _ => return None,
        };
        *out.get_mut(index)? = number.parse().ok()?;
        seen += 1;
    }
    (seen >= 3).then_some(out)
}

#[cfg(test)]
mod tests;
