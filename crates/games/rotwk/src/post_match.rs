use std::hash::{Hash, Hasher};

use crate::hud::{CellSpec, ListSpec};
use crate::match_stats::{EndStatus, PlayerFigures, Sample, SAMPLE_PERIOD};

pub(crate) const GRAPH_MODE: &str = "_graphMode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Series {
    FinalScore,
    Units,
    Structures,
    Resources,
}

impl Series {
    pub(crate) const ALL: [(&'static str, Series); 4] = [
        ("FinalScore", Series::FinalScore),
        ("Units", Series::Units),
        ("Structures", Series::Structures),
        ("Resources", Series::Resources),
    ];

    pub(crate) fn named(mode: Option<&str>) -> Self {
        Self::ALL
            .iter()
            .find(|(name, _)| Some(*name) == mode)
            .map_or(Series::FinalScore, |(_, series)| *series)
    }

    #[allow(clippy::cast_precision_loss)]
    fn value(self, sample: &Sample) -> f32 {
        match self {
            Series::FinalScore => sample.score,
            Series::Units => f32::from(sample.units),
            Series::Structures => f32::from(sample.structures),
            Series::Resources => sample.money as f32,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GraphSpec {
    pub(crate) plots: Vec<(String, PlotSpec)>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PlotSpec {
    pub(crate) lines: Vec<LineSpec>,
    pub(crate) markers: Vec<MarkerSpec>,
}

#[derive(Debug, Clone)]
pub(crate) struct LineSpec {
    pub(crate) points: Vec<[f32; 2]>,
    pub(crate) rgba: [f32; 4],
    pub(crate) width: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct MarkerSpec {
    pub(crate) at: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) image: String,
}

impl GraphSpec {
    fn key(&self) -> (Vec<u32>, Vec<&str>) {
        let mut bits = Vec::new();
        let mut names = Vec::new();
        for (name, plot) in &self.plots {
            names.push(name.as_str());
            for line in &plot.lines {
                bits.push(u32::MAX);
                bits.extend(line.points.iter().flatten().map(|v| v.to_bits()));
                bits.extend(line.rgba.iter().map(|v| v.to_bits()));
                bits.push(line.width.to_bits());
            }
            for marker in &plot.markers {
                names.push(marker.image.as_str());
                bits.extend(marker.at.iter().chain(&marker.size).map(|v| v.to_bits()));
            }
        }
        (bits, names)
    }
}

impl PartialEq for GraphSpec {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for GraphSpec {}

impl Hash for GraphSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SummaryPlayer {
    pub(crate) name: String,
    pub(crate) rgb: [u8; 3],
    pub(crate) figures: PlayerFigures,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MatchSummary {
    pub(crate) players: Vec<SummaryPlayer>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TimeLabels {
    pub(crate) days: String,
    pub(crate) day: String,
    pub(crate) hours: String,
    pub(crate) hour: String,
    pub(crate) minutes: String,
    pub(crate) minute: String,
    pub(crate) seconds: String,
    pub(crate) second: String,
}

#[derive(Clone, Copy)]
enum Stat {
    SessionLength,
    StructuresCreated,
    StructuresLost,
    StructuresDestroyed,
    FortressesBuilt,
    UnitsCreated,
    UnitsLost,
    KillDeathRatio,
    UnitsKilled,
    FavoriteUnit,
    MaxIncome,
    ResourcesGathered,
    MoneyGiven,
    MoneyReceived,
    SpentOnStructures,
    SpentOnUnits,
    SpentOnHeroes,
    StrategicSkill,
    TacticalSkill,
    FirstHero,
    HeroesBuilt,
    HeroesLost,
}

const STAT_ROWS: [(&str, Stat); 22] = [
    ("STAT:RTS_SESSION_LENGTH", Stat::SessionLength),
    ("STAT:RTS_STRUCTURES_CREATED", Stat::StructuresCreated),
    ("STAT:RTS_STRUCTURES_LOST", Stat::StructuresLost),
    ("STAT:RTS_STRUCTURES_DESTROYED", Stat::StructuresDestroyed),
    ("STAT:RTS_FORTRESSES_BUILT", Stat::FortressesBuilt),
    ("STAT:RTS_UNITS_CREATED", Stat::UnitsCreated),
    ("STAT:RTS_UNITS_LOST", Stat::UnitsLost),
    ("STAT:RTS_UNIT_KILL_DEATH_RATIO", Stat::KillDeathRatio),
    ("STAT:RTS_UNITS_KILLED", Stat::UnitsKilled),
    ("STAT:RTS_FAVORITE_UNIT", Stat::FavoriteUnit),
    ("STAT:RTS_MAXIMUM_INCOME_RATE_PER_MINUTE", Stat::MaxIncome),
    ("STAT:RTS_TOTAL_RESOURCES_GATHERED", Stat::ResourcesGathered),
    ("STAT:RTS_MONEY_GIVEN_TO_ALLIES", Stat::MoneyGiven),
    ("STAT:RTS_MONEY_RECEIVED_FROM_ALLIES", Stat::MoneyReceived),
    (
        "STAT:RTS_RESOUCES_SPENT_ON_STRUCTURES",
        Stat::SpentOnStructures,
    ),
    ("STAT:RTS_RESOUCES_SPENT_ON_UNITS", Stat::SpentOnUnits),
    ("STAT:RTS_RESOUCES_SPENT_ON_HEROES", Stat::SpentOnHeroes),
    ("STAT:RTS_STRATEGIC_SKILL", Stat::StrategicSkill),
    ("STAT:RTS_TACTICAL_SKILL", Stat::TacticalSkill),
    ("STAT:RTS_TIME_SPENT_TO_BUILD_FIRST_HERO", Stat::FirstHero),
    ("STAT:RTS_HEROES_BUILT", Stat::HeroesBuilt),
    ("STAT:RTS_HEROES_LOST", Stat::HeroesLost),
];

pub(crate) const FIRST_FOCUS: usize = 1;

pub(crate) fn color_extern(rgb: [u8; 3]) -> String {
    let value = u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]);
    if value == 0 {
        "0".to_owned()
    } else {
        format!("{value:#x}")
    }
}

#[allow(clippy::cast_precision_loss)]
fn ratio(top: f64, bottom: f64) -> String {
    if top.abs() < 0.0001 {
        return "-".to_owned();
    }
    let bottom = if bottom.abs() < 0.0001 { 1.0 } else { bottom };
    let text = format!("{:.2}", top / bottom);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn counted(count: u32, plural: &str, single: &str) -> String {
    if count == 1 {
        single.to_owned()
    } else {
        plural.replace("%d", &count.to_string())
    }
}

pub(crate) fn duration(total: u32, labels: &TimeLabels) -> String {
    let days = total / 86_400;
    let hours = total % 86_400 / 3_600;
    let minutes = total % 3_600 / 60;
    let seconds = total % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(counted(days, &labels.days, &labels.day));
    }
    if hours > 0 {
        parts.push(counted(hours, &labels.hours, &labels.hour));
    }
    if minutes > 0 {
        parts.push(counted(minutes, &labels.minutes, &labels.minute));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(counted(seconds, &labels.seconds, &labels.second));
    }
    parts.join(" ")
}

fn seconds_of(tick: u32) -> u32 {
    tick / mint_sim::SIMULATION_HZ
}

#[allow(clippy::cast_precision_loss)]
fn cell(
    stat: Stat,
    figures: &PlayerFigures,
    text: &dyn Fn(&str) -> String,
    time: &TimeLabels,
) -> String {
    let session = seconds_of(figures.end_tick);
    let money = |value: i64| value.to_string();
    match stat {
        Stat::SessionLength => duration(session, time),
        Stat::StructuresCreated => figures.structures_built.to_string(),
        Stat::StructuresLost => figures.structures_lost.to_string(),
        Stat::StructuresDestroyed => figures.structures_destroyed.to_string(),
        Stat::FortressesBuilt => figures.fortresses.to_string(),
        Stat::UnitsCreated => figures.units_built.to_string(),
        Stat::UnitsLost => figures.units_lost.to_string(),
        Stat::KillDeathRatio => ratio(
            f64::from(figures.units_killed),
            f64::from(figures.units_lost),
        ),
        Stat::UnitsKilled => figures.units_killed.to_string(),
        Stat::FavoriteUnit => figures
            .favourite
            .as_deref()
            .map_or_else(|| text("GUI:None"), text),
        Stat::MaxIncome => ratio(figures.earned as f64, f64::from(session / 60)),
        Stat::ResourcesGathered => money(figures.earned),
        Stat::MoneyGiven | Stat::MoneyReceived => "0".to_owned(),
        Stat::SpentOnStructures => money(figures.spent_on_structures),
        Stat::SpentOnUnits => money(figures.spent_on_units),
        Stat::SpentOnHeroes => money(figures.spent_on_heroes),
        Stat::StrategicSkill => ratio(
            f64::from(
                figures
                    .units_killed
                    .saturating_add(figures.structures_destroyed),
            ) * 100.0,
            figures.spent as f64,
        ),
        Stat::TacticalSkill => ratio(
            f64::from(figures.units_built),
            f64::from(figures.units_lost),
        ),
        Stat::FirstHero => figures
            .first_hero_tick
            .map_or_else(String::new, |tick| duration(seconds_of(tick), time)),
        Stat::HeroesBuilt => figures.heroes_built.to_string(),
        Stat::HeroesLost => figures.heroes_lost.to_string(),
    }
}

pub(crate) fn y_step(max: i64) -> i64 {
    let digits = max.max(0).to_string().len();
    let unit = 10_i64.saturating_pow(u32::try_from(digits.saturating_sub(2).max(1)).unwrap_or(1));
    ((max.max(0) / unit + 1).saturating_mul(unit) / 10).max(1)
}

fn clock(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

impl MatchSummary {
    fn frames(&self) -> u32 {
        self.players
            .iter()
            .map(|player| player.figures.end_tick / SAMPLE_PERIOD)
            .max()
            .unwrap_or(0)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn series_step(&self, series: Series) -> i64 {
        let peak = self
            .players
            .iter()
            .flat_map(|player| &player.figures.samples)
            .map(|sample| series.value(sample) as i64)
            .max()
            .unwrap_or(0);
        y_step(peak)
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn graph(&self, focus: &[f32], size: &dyn Fn(&str) -> [f32; 2]) -> GraphSpec {
        let count = self.frames().max(1) as f32;
        let plots = Series::ALL
            .iter()
            .map(|(name, series)| {
                let top = self.series_step(*series).saturating_mul(10) as f32;
                let y = |sample: &Sample| 1.0 - series.value(sample) / top;
                let mut plot = PlotSpec::default();
                for (index, player) in self.players.iter().enumerate() {
                    let samples = &player.figures.samples;
                    let Some(first) = samples.first() else {
                        continue;
                    };
                    let mut points = vec![[0.0, y(first)]];
                    for (i, pair) in samples.windows(2).enumerate() {
                        if let [before, after] = pair {
                            if series.value(before).to_bits() != series.value(after).to_bits() {
                                let x = (i + 1) as f32 / count;
                                points.push([x, y(before)]);
                                points.push([x, y(after)]);
                            }
                        }
                    }
                    let last = samples.len().saturating_sub(1);
                    let end = samples.get(last).map_or(1.0, y);
                    points.push([(samples.len() as f32 / count).min(1.0), end]);
                    let weight = focus.get(index).copied().unwrap_or(1.0).clamp(0.0, 1.0);
                    let alpha = |scale: f32| (weight * scale * 255.5).trunc() / 255.0;
                    let [r, g, b] = player.rgb.map(|c| f32::from(c) / 255.0);
                    for (width, scale) in [(3.0, 0.5), (1.0, 1.0)] {
                        plot.lines.push(LineSpec {
                            points: points.clone(),
                            rgba: [r, g, b, alpha(scale)],
                            width,
                        });
                    }
                    let at = |i: usize| {
                        let i = i.min(last);
                        let x = ((i + 1) as f32 / count).min(1.0);
                        [x, samples.get(i).map_or(1.0, y)]
                    };
                    let mut marker = |i: usize, image: &str| {
                        plot.markers.push(MarkerSpec {
                            at: at(i),
                            size: size(image),
                            image: image.to_owned(),
                        });
                    };
                    match player.figures.status {
                        EndStatus::Victorious => marker(last, "AptTimelineVictorious"),
                        EndStatus::Defeated => marker(last, "AptTimelineDefeated"),
                        EndStatus::Undecided => {}
                    }
                    for tick in &player.figures.fortress_ticks {
                        marker((tick / SAMPLE_PERIOD) as usize, "AptTimelineFortress");
                    }
                }
                ((*name).to_owned(), plot)
            })
            .collect();
        GraphSpec { plots }
    }

    pub(crate) fn axis_labels(&self, series: Series) -> (Vec<String>, Vec<String>) {
        let frames = self.frames();
        let retail_hz = crate::game_data::RETAIL_LOGIC_HZ;
        let x = (0..=10_u32)
            .map(|i| clock(frames.saturating_mul(i) / 10 / retail_hz))
            .collect();
        let step = self.series_step(series);
        let y = (0..=10_i64)
            .map(|i| step.saturating_mul(i).to_string())
            .collect();
        (x, y)
    }

    pub(crate) fn stat_list(
        &self,
        focus: usize,
        text: &dyn Fn(&str) -> String,
        time: &TimeLabels,
    ) -> ListSpec {
        let (columns, shown): (Vec<(u8, bool)>, Vec<usize>) = if self.players.len() < 2 {
            (vec![(50, false), (50, true)], vec![0])
        } else {
            (
                vec![(25, false), (25, true), (25, true), (25, true)],
                vec![0, focus, focus.saturating_add(1)],
            )
        };
        let rows = STAT_ROWS
            .iter()
            .map(|(label, stat)| {
                let mut row = vec![CellSpec::Text(text(label))];
                row.extend(shown.iter().map(|index| {
                    CellSpec::Text(self.players.get(*index).map_or_else(String::new, |player| {
                        cell(*stat, &player.figures, text, time)
                    }))
                }));
                row
            })
            .collect();
        ListSpec {
            columns,
            rows,
            selected: None,
            scroll_bar: None,
            text_inset_percent: 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
