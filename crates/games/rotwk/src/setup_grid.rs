use std::sync::Arc;

use crate::hud::{ComboItemSpec, ComboSpec, ImageSource, MapWindowSpec};
use crate::lobby::{
    Catalogue, Difficulty, HeroPick, Occupant, SkirmishSetup, SlotSetup, HANDICAP_MAX,
    HANDICAP_STEP, MAX_SLOTS, TEAMS,
};

const UNUSABLE_HERO: [u8; 4] = [128, 128, 128, 255];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Column {
    Player,
    Army,
    Hero,
    Team,
    Colour,
    Handicap,
}

impl Column {
    pub(crate) const ALL: [Self; 6] = [
        Self::Player,
        Self::Army,
        Self::Hero,
        Self::Team,
        Self::Colour,
        Self::Handicap,
    ];

    pub(crate) const fn surface(self) -> &'static str {
        match self {
            Self::Player => "Player",
            Self::Army => "PlayerTemplate",
            Self::Hero => "Hero",
            Self::Team => "Team",
            Self::Colour => "Color",
            Self::Handicap => "Handicap",
        }
    }
}

pub(crate) fn list_path(setup_clip: &str) -> String {
    format!("{setup_clip}.#1.PlayerList")
}

pub(crate) fn cell_path(list: &str, row: usize, column: Column) -> String {
    format!("{list}.~{row}.{}", column.surface())
}

pub(crate) fn connection_icon(row: usize) -> String {
    format!("ConnectionIcon~{row}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapType {
    OpenPlay,
    Strategic,
}

const STRATEGIC_ROWS: usize = 6;

pub(crate) fn cell_source(map_type: MapType, row: usize, spec: Arc<ComboSpec>) -> ImageSource {
    if map_type == MapType::Strategic && row >= STRATEGIC_ROWS {
        ImageSource::Hidden
    } else {
        ImageSource::Combo(spec)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GridLabels {
    pub(crate) human: String,
    pub(crate) closed: String,
    pub(crate) difficulties: [String; 4],
    pub(crate) random: String,
    pub(crate) none: String,
    pub(crate) teams: [String; 4],
    pub(crate) factions: Vec<String>,
    pub(crate) colours: Vec<[u8; 3]>,
    pub(crate) random_colour: String,
    pub(crate) hero_default_suffix: String,
    pub(crate) numbers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Choice {
    Occupant(Occupant),
    Faction(Option<String>),
    Hero(HeroPick),
    Team(Option<u8>),
    Colour(Option<u8>),
    Handicap(u8),
    Nothing,
}

pub(crate) fn cell(
    setup: &SkirmishSetup,
    catalogue: &Catalogue,
    labels: &GridLabels,
    row: usize,
    column: Column,
) -> (ComboSpec, Vec<Choice>) {
    let closed = SlotSetup::closed();
    let slot = setup.slots.get(row).unwrap_or(&closed);
    let occupied = slot.occupant.occupied();
    let mut entries: Vec<(ComboItemSpec, Choice)> = Vec::new();
    let selected;
    let text = |s: &str| ComboItemSpec::text(s);
    let white = |s: &str| ComboItemSpec {
        colour: Some([255, 255, 255, 255]),
        ..ComboItemSpec::text(s)
    };
    let enabled;
    match column {
        Column::Player => {
            if slot.occupant == Occupant::Human {
                entries.push((white(&labels.human), Choice::Occupant(Occupant::Human)));
                enabled = false;
            } else {
                entries.push((white(&labels.closed), Choice::Occupant(Occupant::Closed)));
                let room = occupied || setup.players() < usize::from(catalogue.starts);
                if room {
                    for (d, label) in Difficulty::ALL.iter().zip(&labels.difficulties) {
                        entries.push((white(label), Choice::Occupant(Occupant::Ai(*d))));
                    }
                }
                enabled = true;
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Occupant(slot.occupant));
        }
        _ if !occupied => {
            let placeholder = match column {
                Column::Colour => ComboItemSpec {
                    image: Some(labels.random_colour.clone()),
                    ..ComboItemSpec::default()
                },
                Column::Handicap => text("0%"),
                _ => text(&labels.none),
            };
            entries.push((placeholder, Choice::Nothing));
            selected = Some(0);
            enabled = false;
        }
        Column::Army => {
            entries.push((white(&labels.random), Choice::Faction(None)));
            for (name, label) in catalogue.factions.iter().zip(&labels.factions) {
                entries.push((white(label), Choice::Faction(Some(name.clone()))));
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Faction(slot.faction.clone()));
            enabled = true;
        }
        Column::Hero if !setup.rules.allow_custom_heroes => {
            entries.push((white(&labels.none), Choice::Hero(HeroPick::None)));
            selected = Some(0);
            enabled = false;
        }
        Column::Hero => {
            let faction = slot
                .faction
                .as_deref()
                .and_then(|name| catalogue.faction_bit(name));
            entries.push((white(&labels.none), Choice::Hero(HeroPick::None)));
            entries.push((white(&labels.random), Choice::Hero(HeroPick::Random)));
            for (index, hero) in catalogue.heroes.iter().enumerate().rev() {
                let label = if hero.system {
                    format!("{}{}", hero.name, labels.hero_default_suffix)
                } else {
                    hero.name.clone()
                };
                let item = if hero.usable_by(faction) {
                    white(&label)
                } else {
                    ComboItemSpec {
                        colour: Some(UNUSABLE_HERO),
                        ..ComboItemSpec::text(&label)
                    }
                };
                entries.push((item, Choice::Hero(HeroPick::Stored(index))));
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Hero(slot.hero));
            enabled = true;
        }
        Column::Team => {
            entries.push((white(&labels.none), Choice::Team(None)));
            for (t, label) in (0..TEAMS).zip(&labels.teams) {
                entries.push((white(label), Choice::Team(Some(t))));
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Team(slot.team));
            enabled = true;
        }
        Column::Colour => {
            entries.push((
                ComboItemSpec {
                    image: Some(labels.random_colour.clone()),
                    ..ComboItemSpec::default()
                },
                Choice::Colour(None),
            ));
            for (index, rgb) in labels.colours.iter().enumerate() {
                let Ok(c) = u8::try_from(index) else {
                    break;
                };
                if setup.colour_taken(c, row) {
                    continue;
                }
                entries.push((
                    ComboItemSpec {
                        swatch: Some([rgb[0], rgb[1], rgb[2], 255]),
                        ..ComboItemSpec::default()
                    },
                    Choice::Colour(Some(c)),
                ));
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Colour(slot.colour));
            enabled = true;
        }
        Column::Handicap => {
            for h in (0..=HANDICAP_MAX).step_by(usize::from(HANDICAP_STEP)) {
                let label = if h == 0 {
                    "0%".to_owned()
                } else {
                    format!("-{h}%")
                };
                entries.push((white(&label), Choice::Handicap(h)));
            }
            selected = entries
                .iter()
                .position(|(_, c)| *c == Choice::Handicap(slot.handicap));
            enabled = true;
        }
    }
    let (items, choices) = entries.into_iter().unzip();
    (
        ComboSpec {
            items,
            selected,
            enabled,
        },
        choices,
    )
}

pub(crate) fn apply(
    setup: &mut SkirmishSetup,
    catalogue: &Catalogue,
    row: usize,
    choice: &Choice,
) -> bool {
    while setup.slots.len() < MAX_SLOTS.min(row.saturating_add(1)) {
        setup.slots.push(SlotSetup::closed());
    }
    setup.try_edit(row, catalogue, |slot| match choice {
        Choice::Occupant(o) => slot.occupant = *o,
        Choice::Faction(f) => slot.faction.clone_from(f),
        Choice::Hero(h) => slot.hero = *h,
        Choice::Team(t) => slot.team = *t,
        Choice::Colour(c) => slot.colour = *c,
        Choice::Handicap(h) => slot.handicap = *h,
        Choice::Nothing => {}
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;

pub(crate) fn preview(
    map: &crate::mapcache::SkirmishMap,
    lobby: &SkirmishSetup,
    labels: &GridLabels,
    backdrop: &str,
) -> MapWindowSpec {
    #[allow(clippy::cast_possible_truncation)]
    let whole = |v: f32| v.round() as i32;
    let offered = usize::try_from(map.players).unwrap_or(usize::MAX);
    let spots = map
        .starts
        .iter()
        .take(offered)
        .enumerate()
        .map(|(i, [x, y])| {
            let holder = lobby.slots.iter().position(|s| {
                s.occupant.occupied() && s.start.is_some_and(|start| usize::from(start) == i)
            });
            let label = holder.map(|slot| {
                labels
                    .numbers
                    .get(slot)
                    .cloned()
                    .unwrap_or_else(|| slot.saturating_add(1).to_string())
            });
            ([whole(*x), whole(*y)], label)
        })
        .collect();
    MapWindowSpec {
        backdrop: Some(backdrop.to_owned()),
        art: crate::hud::minimap_image_path(&map.map_path),
        extent: map.extent.map_or([0, 0, 1, 1], |e| e.map(whole)),
        spots,
        enabled: true,
    }
}
