use std::time::Duration;

use mint_sage_ini::multiplayer_color::rgb_field;
use mint_sage_ini::{BlockTable, Document, MemorySources, Reader};
use mint_sage_str::{format_placeholder, Placeholder, StringTable};
use mint_sim::Entity;
use mint_vfs::Vfs;

use crate::command_bar::{SlotAction, SlotHero};

const INGAMEUI_INI: &str = "data/ini/ingameui.ini";

const LANGUAGE_INI: &str = "language.ini";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineStyle {
    pub(crate) font: String,
    pub(crate) point_size: f32,
    pub(crate) bold: bool,
    pub(crate) color: [u8; 3],
}

const DEFAULT_FONT: &str = "Albertus MT";

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            font: DEFAULT_FONT.to_owned(),
            point_size: 14.0,
            bold: false,
            color: [255, 204, 0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HelpBoxConfig {
    pub(crate) delay: Duration,
    pub(crate) name: LineStyle,
    pub(crate) cost: LineStyle,
    pub(crate) shortcut: LineStyle,
    pub(crate) description: LineStyle,
}

impl Default for HelpBoxConfig {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(1000),
            name: LineStyle {
                font: DEFAULT_FONT.to_owned(),
                point_size: 16.0,
                bold: false,
                color: [255, 255, 255],
            },
            cost: LineStyle::default(),
            shortcut: LineStyle::default(),
            description: LineStyle::default(),
        }
    }
}

fn line_style(block: &mint_sage_ini::Block, prefix: &str, default: LineStyle) -> LineStyle {
    let field = |suffix: &str| block.field(&format!("HelpBox{prefix}{suffix}"));
    let font = field("Font")
        .and_then(mint_sage_ini::Field::value)
        .filter(|v| !mint_sage_ini::value::is_none(v))
        .map(str::to_owned)
        .unwrap_or(default.font);
    let point_size = field("PointSize")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .unwrap_or(default.point_size);
    let bold = field("Bold")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_bool)
        .unwrap_or(default.bold);
    let color = rgb_field(block, &format!("HelpBox{prefix}Color"))
        .map(|c| [c.r, c.g, c.b])
        .unwrap_or(default.color);
    LineStyle {
        font,
        point_size,
        bold,
        color,
    }
}

#[must_use]
pub(crate) fn read_help_box_config(documents: &[Document]) -> HelpBoxConfig {
    let mut config = HelpBoxConfig::default();
    for block in documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"))
    {
        if let Some(seconds) = block
            .field("UnitHelpTextDelay")
            .and_then(mint_sage_ini::Field::value)
            .and_then(mint_sage_ini::value::parse_float)
        {
            config.delay = Duration::from_secs_f32(seconds.max(0.0));
        }
        config.name = line_style(block, "Name", config.name);
        config.cost = line_style(block, "Cost", config.cost);
        config.shortcut = line_style(block, "Shortcut", config.shortcut);
        config.description = line_style(block, "Description", config.description);
    }
    config
}

fn font_desc(field: &mint_sage_ini::Field) -> Option<(String, f32, bool)> {
    let [family, points, bold, ..] = field.args.as_slice() else {
        return None;
    };
    Some((
        family.clone(),
        mint_sage_ini::value::parse_float(points)?,
        mint_sage_ini::value::parse_bool(bold)?,
    ))
}

#[must_use]
pub(crate) fn with_language_fonts(
    mut config: HelpBoxConfig,
    documents: &[Document],
) -> HelpBoxConfig {
    for block in documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("Language"))
    {
        for (prefix, style) in [
            ("Name", &mut config.name),
            ("Cost", &mut config.cost),
            ("Shortcut", &mut config.shortcut),
            ("Description", &mut config.description),
        ] {
            let desc = block
                .field(&format!("HelpBox{prefix}Font"))
                .and_then(font_desc);
            if let Some((font, point_size, bold)) = desc {
                style.font = font;
                style.point_size = point_size;
                style.bold = bold;
            }
        }
    }
    config
}

#[must_use]
pub(crate) fn help_box_table() -> BlockTable {
    let mut table = BlockTable::corpus_path();
    table.insert("", "Language", true);
    for line in LINES {
        table.insert("Language", &format!("HelpBox{line}Font"), false);
    }
    table
}

const LINES: [&str; 4] = ["Name", "Cost", "Shortcut", "Description"];

fn read_one(vfs: &Vfs, path: &str) -> Result<Option<Document>, String> {
    if !vfs.contains(path) {
        return Ok(None);
    }
    let bytes = vfs.read(path).map_err(|error| format!("{path}: {error}"))?;
    let mut sources = MemorySources::new();
    sources.insert(path, bytes);
    Reader::new(&sources)
        .read(path, &help_box_table())
        .map(Some)
        .map_err(|error| format!("{path}: {error}"))
}

pub(crate) fn load_help_box_config(vfs: &Vfs) -> Result<HelpBoxConfig, String> {
    let config = read_one(vfs, INGAMEUI_INI)?
        .map(|document| read_help_box_config(std::slice::from_ref(&document)))
        .unwrap_or_default();
    match read_one(vfs, LANGUAGE_INI)? {
        Some(document) => Ok(with_language_fonts(config, std::slice::from_ref(&document))),
        None => {
            println!(
                "help box: {LANGUAGE_INI} is not in the install - the help box keeps                  ingameui.ini's fonts"
            );
            Ok(config)
        }
    }
}

pub(crate) fn language_font(vfs: &Vfs, field: &str) -> Result<Option<(String, f32, bool)>, String> {
    Ok(read_one(vfs, LANGUAGE_INI)?.and_then(|document| {
        document
            .roots()
            .iter()
            .filter(|block| block.keyword.eq_ignore_ascii_case("Language"))
            .filter_map(|block| block.field(field).and_then(font_desc))
            .next_back()
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HoverKey {
    Ring(u8),
    Bar(u8),
    Radial(u8),
    Flyout(u8),
    Hero(u8),
    Spell(u8),
    Native(&'static str),
    World(Entity),
    Portrait,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HelpBoxTimer {
    hovered: Option<HoverKey>,
    since: Option<Duration>,
}

impl HelpBoxTimer {
    pub(crate) fn update(
        &mut self,
        hover: Option<HoverKey>,
        now: Duration,
        delay: Duration,
    ) -> Option<HoverKey> {
        if hover != self.hovered {
            self.hovered = hover;
            self.since = self.hovered.is_some().then_some(now);
        }
        let key = self.hovered?;
        let start = self.since?;
        (now.saturating_sub(start) >= delay).then_some(key)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HelpText {
    pub(crate) title: String,
    pub(crate) stat: Option<String>,
    pub(crate) stat_kind: StatKind,
    pub(crate) shortcut: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum StatKind {
    #[default]
    Cost,
    Health,
}

fn build_shortcut(action: &SlotAction) -> Option<u8> {
    match action {
        SlotAction::Build(index) => Some(index.saturating_add(1)),
        _ => None,
    }
}

#[must_use]
pub(crate) fn command_button_help(
    text_label: Option<&str>,
    descript_label: Option<&str>,
    action: &SlotAction,
    cost: Option<i64>,
    strings: &StringTable,
) -> HelpText {
    button_help(
        text_label,
        descript_label,
        build_shortcut(action),
        cost,
        strings,
    )
}

#[must_use]
pub(crate) fn button_help(
    text_label: Option<&str>,
    descript_label: Option<&str>,
    shortcut: Option<u8>,
    cost: Option<i64>,
    strings: &StringTable,
) -> HelpText {
    let title = text_label
        .and_then(|k| strings.get(k))
        .map_or_else(String::new, strip_mnemonic);
    let stat = cost
        .filter(|&c| c > 0)
        .map(|c| format_placeholder(&strings.fetch("TOOLTIP:Cost"), Placeholder::Int(c)));
    let shortcut = shortcut.map(|n| {
        format_placeholder(
            &strings.fetch("TOOLTIP:Shortcut"),
            Placeholder::Int(i64::from(n)),
        )
    });
    let description = descript_label
        .and_then(|k| strings.get(k))
        .map(str::to_owned);
    HelpText {
        title,
        stat,
        stat_kind: StatKind::Cost,
        shortcut,
        description,
    }
}

#[must_use]
pub(crate) fn hero_help(hero: &SlotHero, cost: Option<i64>, strings: &StringTable) -> HelpText {
    let (title_key, desc_key, own_text) = if hero.revive {
        (
            "CONTROLBAR:GenericReviveHero",
            "CONTROLBAR:ToolTipGenericReviveHero",
            &hero.revive_text,
        )
    } else {
        (
            "CONTROLBAR:GenericRecruitHero",
            "CONTROLBAR:ToolTipGenericRecruitHero",
            &hero.recruit_text,
        )
    };
    let text_of = |key: &Option<String>| key.as_deref().and_then(|k| strings.get(k));
    let name = hero.literal_name.clone().unwrap_or_else(|| {
        text_of(&hero.hotkey)
            .or_else(|| text_of(&hero.display_name))
            .unwrap_or_default()
            .to_owned()
    });
    let description = text_of(own_text)
        .or_else(|| strings.get(desc_key))
        .map(|text| {
            let level = format_placeholder(
                &strings.fetch("APT:RankLabel"),
                Placeholder::Int(i64::from(hero.level)),
            );
            format!("{text}\n{level}")
        });
    let mut stat: Vec<String> = Vec::new();
    stat.extend(
        cost.filter(|&c| c > 0)
            .map(|c| format_placeholder(&strings.fetch("TOOLTIP:Cost"), Placeholder::Int(c))),
    );
    if hero.command_points > 0 {
        stat.push(format_placeholder(
            &strings.fetch("TOOLTIP:CommandPoints"),
            Placeholder::Int(hero.command_points),
        ));
    }
    HelpText {
        title: strip_mnemonic(&format_placeholder(
            &strings.fetch(title_key),
            Placeholder::Str(&name),
        )),
        stat: (!stat.is_empty()).then(|| stat.join("\n")),
        stat_kind: StatKind::Cost,
        shortcut: None,
        description,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ObjectHelp {
    pub(crate) display_name: Option<String>,
    pub(crate) literal_name: Option<String>,
    pub(crate) description: Option<String>,
}

#[must_use]
#[allow(dead_code)]
pub(crate) fn read_object_help(
    documents: &[Document],
) -> std::collections::BTreeMap<String, ObjectHelp> {
    let mut out = std::collections::BTreeMap::new();
    for block in documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("Object"))
    {
        let Some(name) = block.name() else {
            continue;
        };
        let display_name = block
            .field("DisplayName")
            .and_then(mint_sage_ini::Field::value)
            .filter(|v| !mint_sage_ini::value::is_none(v))
            .map(str::to_owned);
        let description = block
            .field("Description")
            .and_then(mint_sage_ini::Field::value)
            .filter(|v| !mint_sage_ini::value::is_none(v))
            .map(str::to_owned);
        if display_name.is_some() || description.is_some() {
            out.insert(
                name.to_ascii_lowercase(),
                ObjectHelp {
                    display_name,
                    literal_name: None,
                    description,
                },
            );
        }
    }
    out
}

#[must_use]
pub(crate) fn world_help(
    help: &ObjectHelp,
    max_health: Option<f32>,
    strings: &StringTable,
) -> HelpText {
    let title = help.literal_name.clone().unwrap_or_else(|| {
        help.display_name
            .as_deref()
            .and_then(|k| strings.get(k))
            .unwrap_or_default()
            .to_owned()
    });
    let stat = max_health.filter(|h| *h > 0.0).map(|h| {
        format_placeholder(
            &strings.fetch("TOOLTIP:MaxHealth"),
            Placeholder::Float(f64::from(h)),
        )
    });
    let description = help
        .description
        .as_deref()
        .and_then(|k| strings.get(k))
        .map(str::to_owned);
    HelpText {
        title,
        stat,
        stat_kind: StatKind::Health,
        shortcut: None,
        description,
    }
}

#[must_use]
pub(crate) fn native_help(name: &str, strings: &StringTable) -> Option<HelpText> {
    let (title_key, desc_key, shortcut): (&str, &str, Option<&str>) =
        if name.eq_ignore_ascii_case("SelectNearestBuilder") {
            (
                "CONTROLBAR:SelectNearestBuilder",
                "CONTROLBAR:SelectNearestBuilderDescription",
                Some("B"),
            )
        } else if name.eq_ignore_ascii_case("Players") {
            ("APT:Players", "CONTROLBAR:PlayerStatusDescription", None)
        } else if name.eq_ignore_ascii_case("Objectives") {
            (
                "CONTROLBAR:Objectives",
                "CONTROLBAR:ObjectivesDescription",
                None,
            )
        } else {
            return None;
        };
    let title = strip_mnemonic(&strings.fetch(title_key));
    let shortcut = shortcut
        .map(|k| format_placeholder(&strings.fetch("TOOLTIP:Shortcut"), Placeholder::Str(k)));
    let description = strings.get(desc_key).map(str::to_owned);
    Some(HelpText {
        title,
        stat: None,
        stat_kind: StatKind::Cost,
        shortcut,
        description,
    })
}

#[must_use]
pub(crate) fn strip_mnemonic(text: &str) -> String {
    text.replace('&', "")
}

#[must_use]
fn unit_color(color: [u8; 3]) -> [f32; 4] {
    [
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
        1.0,
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct HelpLineSpec {
    pub(crate) text: String,
    pub(crate) font: String,
    pub(crate) point_size_bits: u32,
    pub(crate) bold: bool,
    pub(crate) color: [u8; 3],
}

impl HelpLineSpec {
    #[must_use]
    pub(crate) fn point_size(&self) -> f32 {
        f32::from_bits(self.point_size_bits)
    }

    #[must_use]
    pub(crate) fn to_text_line(&self) -> mint_sage_apt_viewer::host::TextLine {
        mint_sage_apt_viewer::host::TextLine {
            text: self.text.clone(),
            font: self.font.clone(),
            point_size: self.point_size(),
            bold: self.bold,
            color: unit_color(self.color),
        }
    }
}

#[must_use]
pub(crate) fn help_text_lines(text: &HelpText, config: &HelpBoxConfig) -> Vec<HelpLineSpec> {
    let mut lines = Vec::with_capacity(4);
    let mut push = |value: Option<&str>, style: &LineStyle| {
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            return;
        };
        for row in value.split('\n').filter(|row| !row.is_empty()) {
            lines.push(HelpLineSpec {
                text: row.to_owned(),
                font: style.font.clone(),
                point_size_bits: style.point_size.to_bits(),
                bold: style.bold,
                color: style.color,
            });
        }
    };
    push(Some(text.title.as_str()), &config.name);
    let stat_style = match text.stat_kind {
        StatKind::Cost => &config.cost,
        StatKind::Health => &config.description,
    };
    push(text.stat.as_deref(), stat_style);
    push(text.shortcut.as_deref(), &config.shortcut);
    push(text.description.as_deref(), &config.description);
    lines
}

#[cfg(test)]
mod tests;
