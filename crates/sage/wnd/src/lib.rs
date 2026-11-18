#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("line {line}: `{key}` has no closing `;`")]
    Unterminated { line: usize, key: String },
    #[error("line {line}: `{text}` is neither a keyword nor `KEY = value;`")]
    NotAStatement { line: usize, text: String },
    #[error("line {line}: a WINDOW opened here is never closed by END")]
    UnclosedWindow { line: usize },
    #[error("line {line}: `{keyword}` out of place")]
    Misplaced { line: usize, keyword: &'static str },
    #[error("line {line}: windows nested more than {MAX_DEPTH} deep")]
    TooDeep { line: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    pub version: Option<u32>,
    pub fields: Vec<Field>,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Window {
    pub fields: Vec<Field>,
    pub children: Vec<Window>,
    pub line: usize,
}

pub type Rgba = [u8; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub upper_left: [i32; 2],
    pub bottom_right: [i32; 2],
    pub creation_resolution: [i32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    pub name: String,
    pub size: i32,
    pub bold: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextColors {
    pub enabled: Rgba,
    pub enabled_border: Rgba,
    pub disabled: Rgba,
    pub disabled_border: Rgba,
    pub hilite: Rgba,
    pub hilite_border: Rgba,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrawItem {
    pub image: Option<String>,
    pub color: Rgba,
    pub border: Rgba,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListBoxData {
    pub length: i32,
    pub auto_scroll: bool,
    pub scroll_if_at_end: bool,
    pub auto_purge: bool,
    pub scroll_bar: bool,
    pub multi_select: bool,
    pub columns: i32,
    pub column_widths: Vec<i32>,
    pub force_select: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComboBoxData {
    pub is_editable: bool,
    pub max_chars: i32,
    pub max_display: i32,
    pub ascii_only: bool,
    pub letters_and_numbers: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SliderData {
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextEntryData {
    pub max_len: i32,
    pub secret: bool,
    pub numerical_only: bool,
    pub alphanumerical_only: bool,
    pub ascii_only: bool,
}

impl Layout {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let statements = statements(text)?;
        let mut out = Self::default();
        let mut stream = statements.into_iter();
        let mut in_layout_block = false;
        while let Some(statement) = stream.next() {
            match statement {
                Statement::Field(field) => {
                    if field.key.eq_ignore_ascii_case("FILE_VERSION") {
                        out.version = field.value.trim().parse().ok();
                    }
                    out.fields.push(field);
                }
                Statement::Keyword(Keyword::StartLayoutBlock, line) => {
                    if in_layout_block {
                        return Err(Error::Misplaced {
                            line,
                            keyword: "STARTLAYOUTBLOCK",
                        });
                    }
                    in_layout_block = true;
                }
                Statement::Keyword(Keyword::EndLayoutBlock, line) => {
                    if !in_layout_block {
                        return Err(Error::Misplaced {
                            line,
                            keyword: "ENDLAYOUTBLOCK",
                        });
                    }
                    in_layout_block = false;
                }
                Statement::Keyword(Keyword::Window, line) => {
                    out.windows.push(window(&mut stream, line, 1)?);
                }
                Statement::Keyword(keyword, line) => {
                    return Err(Error::Misplaced {
                        line,
                        keyword: keyword.name(),
                    });
                }
            }
        }
        Ok(out)
    }

    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        field_of(&self.fields, key)
    }

    #[must_use]
    pub fn all_windows(&self) -> Vec<&Window> {
        let mut out = Vec::new();
        for window in &self.windows {
            window.walk(&mut out);
        }
        out
    }
}

impl Window {
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        field_of(&self.fields, key)
    }

    fn walk<'a>(&'a self, out: &mut Vec<&'a Window>) {
        out.push(self);
        for child in &self.children {
            child.walk(out);
        }
    }

    #[must_use]
    pub fn window_type(&self) -> Option<&str> {
        self.field("WINDOWTYPE").map(str::trim)
    }

    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.field("NAME").map(unquote)
    }

    #[must_use]
    pub fn text(&self) -> Option<String> {
        self.field("TEXT").map(unquote)
    }

    #[must_use]
    pub fn status(&self) -> Vec<&str> {
        flags(self.field("STATUS"))
    }

    #[must_use]
    pub fn style(&self) -> Vec<&str> {
        flags(self.field("STYLE"))
    }

    #[must_use]
    pub fn screen_rect(&self) -> Option<ScreenRect> {
        let items = labelled(self.field("SCREENRECT")?);
        let pair = |label: &str| -> Option<[i32; 2]> {
            let values = ints(value_of(&items, label)?);
            match values.as_slice() {
                [x, y, ..] => Some([*x, *y]),
                _ => None,
            }
        };
        Some(ScreenRect {
            upper_left: pair("UPPERLEFT")?,
            bottom_right: pair("BOTTOMRIGHT")?,
            creation_resolution: pair("CREATIONRESOLUTION")?,
        })
    }

    #[must_use]
    pub fn font(&self) -> Option<Font> {
        let items = labelled(self.field("FONT")?);
        Some(Font {
            name: unquote(value_of(&items, "NAME")?),
            size: value_of(&items, "SIZE")
                .and_then(|v| ints(v).first().copied())
                .unwrap_or(0),
            bold: value_of(&items, "BOLD").is_some_and(truthy),
        })
    }

    #[must_use]
    pub fn text_colors(&self) -> Option<TextColors> {
        let items = labelled(self.field("TEXTCOLOR")?);
        let colour = |label: &str| value_of(&items, label).map_or([0; 4], rgba);
        Some(TextColors {
            enabled: colour("ENABLED"),
            enabled_border: colour("ENABLEDBORDER"),
            disabled: colour("DISABLED"),
            disabled_border: colour("DISABLEDBORDER"),
            hilite: colour("HILITE"),
            hilite_border: colour("HILITEBORDER"),
        })
    }

    #[must_use]
    pub fn draw_data(&self, key: &str) -> Option<Vec<DrawItem>> {
        Some(draw_items(self.field(key)?))
    }

    #[must_use]
    pub fn draw_keys(&self) -> Vec<&str> {
        self.fields
            .iter()
            .map(|f| f.key.as_str())
            .filter(|k| k.to_ascii_uppercase().ends_with("DRAWDATA"))
            .collect()
    }

    #[must_use]
    pub fn list_box(&self) -> Option<ListBoxData> {
        let items = labelled(self.field("LISTBOXDATA")?);
        let int = |label: &str| value_of(&items, label).and_then(|v| ints(v).first().copied());
        let flag = |label: &str| value_of(&items, label).is_some_and(truthy);
        Some(ListBoxData {
            length: int("LENGTH").unwrap_or(0),
            auto_scroll: flag("AUTOSCROLL"),
            scroll_if_at_end: flag("SCROLLIFATEND"),
            auto_purge: flag("AUTOPURGE"),
            scroll_bar: flag("SCROLLBAR"),
            multi_select: flag("MULTISELECT"),
            columns: int("COLUMNS").unwrap_or(1),
            column_widths: items
                .iter()
                .filter(|(label, _)| label == "COLUMNSWIDTH")
                .filter_map(|(_, v)| ints(v).first().copied())
                .collect(),
            force_select: flag("FORCESELECT"),
        })
    }

    #[must_use]
    pub fn combo_box(&self) -> Option<ComboBoxData> {
        let items = labelled(self.field("COMBOBOXDATA")?);
        let int = |label: &str| value_of(&items, label).and_then(|v| ints(v).first().copied());
        let flag = |label: &str| value_of(&items, label).is_some_and(truthy);
        Some(ComboBoxData {
            is_editable: flag("ISEDITABLE"),
            max_chars: int("MAXCHARS").unwrap_or(0),
            max_display: int("MAXDISPLAY").unwrap_or(0),
            ascii_only: flag("ASCIIONLY"),
            letters_and_numbers: flag("LETTERSANDNUMBERS"),
        })
    }

    #[must_use]
    pub fn slider(&self) -> Option<SliderData> {
        let items = labelled(self.field("SLIDERDATA")?);
        let int = |label: &str| value_of(&items, label).and_then(|v| ints(v).first().copied());
        Some(SliderData {
            min: int("MINVALUE").unwrap_or(0),
            max: int("MAXVALUE").unwrap_or(0),
        })
    }

    #[must_use]
    pub fn text_entry(&self) -> Option<TextEntryData> {
        let items = labelled(self.field("TEXTENTRYDATA")?);
        let int = |label: &str| value_of(&items, label).and_then(|v| ints(v).first().copied());
        let flag = |label: &str| value_of(&items, label).is_some_and(truthy);
        Some(TextEntryData {
            max_len: int("MAXLEN").unwrap_or(0),
            secret: flag("SECRETTEXT"),
            numerical_only: flag("NUMERICALONLY"),
            alphanumerical_only: flag("ALPHANUMERICALONLY"),
            ascii_only: flag("ASCIIONLY"),
        })
    }
}

#[must_use]
pub fn labelled(value: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut chars = value.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let Some(&first) = chars.peek() else {
            break;
        };
        let mut word = String::new();
        if first == '"' {
            word.push('"');
            chars.next();
            for c in chars.by_ref() {
                word.push(c);
                if c == '"' {
                    break;
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == ',' || c == ':' {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() == Some(&':') {
            chars.next();
            out.push((word.to_ascii_uppercase(), Vec::new()));
        } else if word.is_empty() {
            chars.next();
        } else {
            match out.last_mut() {
                Some((_, values)) => values.push(word),
                None => out.push((String::new(), vec![word])),
            }
        }
    }
    out.into_iter()
        .map(|(label, values)| (label, values.join(" ")))
        .collect()
}

fn draw_items(value: &str) -> Vec<DrawItem> {
    let mut out: Vec<DrawItem> = Vec::new();
    for (label, item) in labelled(value) {
        match label.as_str() {
            "IMAGE" => {
                let name = unquote(&item);
                out.push(DrawItem {
                    image: (!name.is_empty() && !name.eq_ignore_ascii_case("NoImage"))
                        .then_some(name),
                    ..DrawItem::default()
                });
            }
            "COLOR" => {
                if let Some(last) = out.last_mut() {
                    last.color = rgba(&item);
                }
            }
            "BORDERCOLOR" => {
                if let Some(last) = out.last_mut() {
                    last.border = rgba(&item);
                }
            }
            _ => {}
        }
    }
    out
}

fn value_of<'a>(items: &'a [(String, String)], label: &str) -> Option<&'a str> {
    items
        .iter()
        .rev()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v.as_str())
}

fn field_of<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .rev()
        .find(|f| f.key.eq_ignore_ascii_case(key))
        .map(|f| f.value.as_str())
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    match value.split_once('"') {
        Some((_, rest)) => rest.split('"').next().unwrap_or_default().to_owned(),
        None => value.to_owned(),
    }
}

fn flags(value: Option<&str>) -> Vec<&str> {
    value
        .map(|v| {
            v.split('+')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn ints(value: &str) -> Vec<i32> {
    value
        .split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok())
        .map(|v| i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0))
        .collect()
}

fn truthy(value: &str) -> bool {
    ints(value).first().is_some_and(|v| *v != 0)
}

fn rgba(value: &str) -> Rgba {
    let mut out = [0u8; 4];
    for (slot, v) in out.iter_mut().zip(ints(value)) {
        *slot = u8::try_from(v.clamp(0, 255)).unwrap_or(0);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keyword {
    Window,
    End,
    Child,
    EndAllChildren,
    StartLayoutBlock,
    EndLayoutBlock,
}

impl Keyword {
    fn of(word: &str) -> Option<Self> {
        match word {
            "WINDOW" => Some(Self::Window),
            "END" => Some(Self::End),
            "CHILD" => Some(Self::Child),
            "ENDALLCHILDREN" => Some(Self::EndAllChildren),
            "STARTLAYOUTBLOCK" => Some(Self::StartLayoutBlock),
            "ENDLAYOUTBLOCK" => Some(Self::EndLayoutBlock),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Window => "WINDOW",
            Self::End => "END",
            Self::Child => "CHILD",
            Self::EndAllChildren => "ENDALLCHILDREN",
            Self::StartLayoutBlock => "STARTLAYOUTBLOCK",
            Self::EndLayoutBlock => "ENDLAYOUTBLOCK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Statement {
    Keyword(Keyword, usize),
    Field(Field),
}

fn statements(text: &str) -> Result<Vec<Statement>, Error> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut line = 1usize;
    loop {
        let trimmed = rest.trim_start();
        let skipped = rest
            .get(..rest.len().saturating_sub(trimmed.len()))
            .unwrap_or_default();
        line = line.saturating_add(skipped.matches('\n').count());
        rest = trimmed;
        if rest.is_empty() {
            return Ok(out);
        }
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = rest.get(..word_end).unwrap_or_default();
        if let Some(keyword) = Keyword::of(word) {
            out.push(Statement::Keyword(keyword, line));
            rest = rest.get(word_end..).unwrap_or_default();
            continue;
        }
        let Some(end) = rest.find(';') else {
            return Err(Error::Unterminated {
                line,
                key: word.chars().take(40).collect(),
            });
        };
        let body = rest.get(..end).unwrap_or_default();
        let Some((key, value)) = body.split_once('=') else {
            return Err(Error::NotAStatement {
                line,
                text: body.chars().take(40).collect(),
            });
        };
        out.push(Statement::Field(Field {
            key: key.trim().to_owned(),
            value: value.split_whitespace().collect::<Vec<_>>().join(" "),
            line,
        }));
        line = line.saturating_add(body.matches('\n').count());
        rest = rest.get(end.saturating_add(1)..).unwrap_or_default();
    }
}

fn window(
    stream: &mut impl Iterator<Item = Statement>,
    line: usize,
    depth: usize,
) -> Result<Window, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::TooDeep { line });
    }
    let mut out = Window {
        line,
        ..Window::default()
    };
    let mut children = false;
    while let Some(statement) = stream.next() {
        match statement {
            Statement::Field(field) => out.fields.push(field),
            Statement::Keyword(Keyword::End, _) => return Ok(out),
            Statement::Keyword(Keyword::Child, _) => children = true,
            Statement::Keyword(Keyword::EndAllChildren, at) => {
                if !children {
                    return Err(Error::Misplaced {
                        line: at,
                        keyword: "ENDALLCHILDREN",
                    });
                }
                children = false;
            }
            Statement::Keyword(Keyword::Window, at) => {
                if !children {
                    return Err(Error::Misplaced {
                        line: at,
                        keyword: "WINDOW",
                    });
                }
                out.children
                    .push(window(stream, at, depth.saturating_add(1))?);
            }
            Statement::Keyword(keyword, at) => {
                return Err(Error::Misplaced {
                    line: at,
                    keyword: keyword.name(),
                });
            }
        }
    }
    Err(Error::UnclosedWindow { line })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests;
