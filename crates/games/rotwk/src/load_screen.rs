use crate::hud::ImageSource;
use crate::lobby::{ResolvedPlayer, SkirmishSetup};
use crate::setup_grid::GridLabels;
use mint_sage_apt_player::{Exposure, ImageKey, Value};

pub(crate) const ROWS: usize = 8;

const HIDDEN_COLOUR: [u8; 3] = [255; 3];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) name: String,
    pub(crate) team: String,
    pub(crate) army: String,
    pub(crate) colour: [u8; 3],
    pub(crate) local: bool,
    pub(crate) level: Option<String>,
}

const RANK_SIDES: [&str; 9] = [
    "Men", "Elves", "Dwarves", "Isengard", "Mordor", "Wild", "Angmar", "Arnor", "Neutral",
];

const UNRANKED_LEVEL: u8 = 1;

fn ai_level(difficulty: crate::lobby::Difficulty) -> u8 {
    match difficulty {
        crate::lobby::Difficulty::Easy => 1,
        crate::lobby::Difficulty::Medium => 4,
        crate::lobby::Difficulty::Hard => 6,
        crate::lobby::Difficulty::Brutal => 9,
    }
}

fn rank_icon(army: &str, level: u8, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    let side = if army.eq_ignore_ascii_case("Goblins") {
        Some("Wild")
    } else {
        RANK_SIDES
            .iter()
            .copied()
            .find(|side| side.eq_ignore_ascii_case(army))
    };
    side.map(|side| format!("AptRankIcon{side}{level}"))
        .filter(|name| exists(name))
        .or_else(|| Some(format!("AptRankIcon{level}")).filter(|name| exists(name)))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShowRandom {
    pub(crate) template: bool,
    pub(crate) colour: bool,
}

pub(crate) fn rows(
    setup: &SkirmishSetup,
    players: &[ResolvedPlayer],
    labels: &GridLabels,
    show: ShowRandom,
    army_name: &dyn Fn(&str) -> String,
    image_exists: &dyn Fn(&str) -> bool,
) -> Vec<Row> {
    let local = players.iter().find(|player| player.ai.is_none());
    let local_slot = local.map(|player| player.slot);
    let local_team = local.and_then(|player| player.team);
    players
        .iter()
        .map(|player| {
            let chosen = setup.slots.get(player.slot);
            let ally = Some(player.slot) == local_slot
                || (player.team.is_some() && player.team == local_team);
            let hidden = |chose_random: bool, shown: bool| chose_random && shown && !ally;
            let name = match player.ai {
                Some(difficulty) => crate::lobby::Difficulty::ALL
                    .iter()
                    .position(|d| *d == difficulty)
                    .and_then(|i| labels.difficulties.get(i))
                    .cloned()
                    .unwrap_or_default(),
                None => labels.human.clone(),
            };
            let army = if hidden(chosen.is_some_and(|s| s.faction.is_none()), show.template) {
                labels.random.clone()
            } else {
                army_name(&player.faction)
            };
            let colour = if hidden(chosen.is_some_and(|s| s.colour.is_none()), show.colour) {
                HIDDEN_COLOUR
            } else {
                labels
                    .colours
                    .get(usize::from(player.colour))
                    .copied()
                    .unwrap_or(HIDDEN_COLOUR)
            };
            let level = player.ai.map_or(UNRANKED_LEVEL, ai_level);
            let level = rank_icon(&army, level, image_exists);
            Row {
                name,
                team: player
                    .team
                    .and_then(|t| labels.teams.get(usize::from(t)))
                    .unwrap_or(&labels.none)
                    .clone(),
                army,
                colour,
                local: Some(player.slot) == local_slot,
                level,
            }
        })
        .collect()
}

pub(crate) fn map_title(map: &crate::mapcache::SkirmishMap) -> String {
    if map.players >= 2 {
        format!("{} ({})", map.display_name, map.players)
    } else {
        map.display_name.clone()
    }
}

pub(crate) fn declare_rows(expose: &mut Exposure<ImageSource>, rows: &[Row]) {
    for i in 0..ROWS {
        let row = rows.get(i);
        let text = |value: Option<&String>| Some(value.cloned().unwrap_or_else(|| " ".to_owned()));
        expose.string(&format!("LoadingScreen::Rank{i}"), Some(" ".to_owned()));
        expose.string(
            &format!("LoadingScreen::PlayerName{i}"),
            text(row.map(|r| &r.name)),
        );
        expose.string(
            &format!("LoadingScreen::TeamNumber{i}"),
            text(row.map(|r| &r.team)),
        );
        expose.string(
            &format!("LoadingScreen::ArmyName{i}"),
            text(row.map(|r| &r.army)),
        );
        expose.image(
            ImageKey::Path(format!("UIClip.Level.{i}")),
            Some(
                row.and_then(|r| r.level.clone())
                    .map_or(ImageSource::Hidden, ImageSource::MappedImage),
            ),
        );
        if let Some(row) = row {
            let [r, g, b] = row.colour;
            let argb = 0xFF00_0000_u32 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            expose.set_extern(
                &format!("GameLoading:PlayerColor:{i}"),
                Value::str(&argb.to_string()),
            );
        }
        set_bar(expose, i, 0);
    }
}

pub(crate) fn set_bar(expose: &mut Exposure<ImageSource>, row: usize, percent: u8) {
    expose.fire(
        "SetBarTo",
        vec![Value::Number(row as f64), Value::Number(f64::from(percent))],
    );
}

#[cfg(test)]
mod tests;
