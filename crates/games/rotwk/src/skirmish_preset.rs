use crate::lobby::{
    Catalogue, Difficulty, HeroPick, Occupant, SkirmishSetup, SlotSetup, MAX_SLOTS,
};
use crate::user_options::UserOptions;

const USER_KEY: &str = "CurrentUserName";
const MAP_FIELD: &str = "Map";
const INFO_FIELD: &str = "GameInfo";
const HERO_FIELD: &str = "HeroIndexes";
const PROFILE: &str = "0";
const RULES_KEY: &str = "GR";
const SLOT_LIST_KEY: &str = "S";
const RULE_COMMAND_POINTS: usize = 3;
const RULE_MONEY: usize = 4;

fn key_of(options: &UserOptions, field: &str) -> Option<String> {
    let user = options.get(USER_KEY)?;
    Some(format!(":{PROFILE}:{user}:{field}"))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Preset {
    pub(crate) map: Option<String>,
    pub(crate) slots: Vec<Option<SlotSetup>>,
    pub(crate) starting_money: Option<i64>,
    pub(crate) command_point_factor: Option<u32>,
}

pub(crate) fn same_map_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn number(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

fn small(value: Option<i32>, limit: usize) -> Option<u8> {
    let value = usize::try_from(value?).ok()?;
    if value < limit {
        u8::try_from(value).ok()
    } else {
        None
    }
}

fn difficulty_of(letter: char) -> Option<Difficulty> {
    match letter {
        'E' => Some(Difficulty::Easy),
        'M' => Some(Difficulty::Medium),
        'H' => Some(Difficulty::Hard),
        _ => None,
    }
}

fn letter_of(difficulty: Difficulty) -> Option<char> {
    match difficulty {
        Difficulty::Easy => Some('E'),
        Difficulty::Medium => Some('M'),
        Difficulty::Hard => Some('H'),
        Difficulty::Brutal => None,
    }
}

fn segments(info: &str) -> Vec<&str> {
    info.split(';').collect()
}

fn segment_value<'a>(parts: &[&'a str], key: &str) -> Option<&'a str> {
    parts
        .iter()
        .find_map(|part| part.strip_prefix(key)?.strip_prefix('='))
}

fn slot_tokens(list: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = list.split(':').collect();
    if tokens.last() == Some(&"") {
        tokens.pop();
    }
    tokens
}

fn slot_of(token: &str, index: usize, templates: &[String]) -> Option<SlotSetup> {
    let kind = token.chars().next()?;
    let fields: Vec<&str> = token.split(',').collect();
    let occupant = match kind {
        'H' if index == 0 => Occupant::Human,
        'C' => Occupant::Ai(difficulty_of(token.chars().nth(1)?)?),
        _ => return Some(SlotSetup::closed()),
    };
    let first: usize = if kind == 'H' { 4 } else { 1 };
    let at = |n: usize| {
        fields
            .get(first.saturating_add(n))
            .copied()
            .and_then(number)
    };
    let (colour, template, start, team) = (at(0), at(1), at(2), at(3));
    let faction = usize::try_from(template?)
        .ok()
        .and_then(|i| templates.get(i))
        .cloned();
    Some(SlotSetup {
        occupant,
        faction,
        hero: HeroPick::Random,
        team: small(team, usize::from(u8::MAX)),
        colour: small(colour, usize::from(u8::MAX)),
        handicap: 0,
        start: small(start, usize::from(u8::MAX)),
    })
}

fn hero_values(text: &str) -> Vec<i32> {
    text.split(':')
        .filter(|part| !part.is_empty())
        .map(|part| number(part).unwrap_or(-1))
        .collect()
}

pub(crate) fn load(options: &UserOptions, templates: &[String], heroes: usize) -> Option<Preset> {
    let info = options.get(&key_of(options, INFO_FIELD)?)?;
    let parts = segments(&info);
    let list = segment_value(&parts, SLOT_LIST_KEY)?;
    let hero_at = key_of(options, HERO_FIELD)
        .and_then(|key| options.get(&key))
        .map(|text| hero_values(&text))
        .unwrap_or_default();
    let slots = slot_tokens(list)
        .iter()
        .take(MAX_SLOTS)
        .enumerate()
        .map(|(index, token)| {
            let mut slot = slot_of(token, index, templates)?;
            if slot.occupant.occupied() {
                let stored = hero_at.get(index).and_then(|h| usize::try_from(*h).ok());
                if let Some(h) = stored.filter(|h| *h < heroes) {
                    slot.hero = HeroPick::Stored(h);
                }
            }
            Some(slot)
        })
        .collect();
    let rules: Vec<&str> = segment_value(&parts, RULES_KEY)
        .map(|text| text.split(' ').collect())
        .unwrap_or_default();
    let rule = |n: usize| rules.get(n).copied().and_then(number);
    Some(Preset {
        map: key_of(options, MAP_FIELD).and_then(|key| options.get(&key)),
        slots,
        starting_money: rule(RULE_MONEY).filter(|v| *v >= 0).map(i64::from),
        command_point_factor: rule(RULE_COMMAND_POINTS).and_then(|v| u32::try_from(v).ok()),
    })
}

fn signed(value: Option<u8>) -> String {
    value.map_or_else(|| "-1".to_owned(), |v| v.to_string())
}

fn template_index(slot: &SlotSetup, templates: &[String]) -> String {
    slot.faction
        .as_ref()
        .and_then(|name| templates.iter().position(|t| t == name))
        .map_or_else(|| "-1".to_owned(), |i| i.to_string())
}

fn token_of(slot: &SlotSetup, old: &str, templates: &[String]) -> String {
    let fields: Vec<&str> = old.split(',').collect();
    let tail = |from: usize| -> String {
        fields
            .iter()
            .skip(from)
            .map(|f| format!(",{f}"))
            .collect::<String>()
    };
    let numbers = format!(
        "{},{},{},{}",
        signed(slot.colour),
        template_index(slot, templates),
        signed(slot.start),
        signed(slot.team)
    );
    match slot.occupant {
        Occupant::Human if old.starts_with('H') => {
            let head: Vec<&str> = fields.iter().take(4).copied().collect();
            format!("{},{numbers}{}", head.join(","), tail(8))
        }
        Occupant::Ai(difficulty) => {
            let letter = letter_of(difficulty).or_else(|| {
                let kept = old.strip_prefix('C')?.chars().next()?;
                Some(kept)
            });
            match letter {
                Some(letter) if old.starts_with('C') => {
                    format!("C{letter},{numbers}{}", tail(5))
                }
                Some(letter) => format!("C{letter},{numbers},0,0"),
                None => old.to_owned(),
            }
        }
        Occupant::Closed if old == "O" || old == "X" => old.to_owned(),
        Occupant::Closed => "X".to_owned(),
        Occupant::Human => old.to_owned(),
    }
}

pub(crate) fn store(
    options: &mut UserOptions,
    lobby: &SkirmishSetup,
    templates: &[String],
) -> bool {
    let mut changed = false;
    if let Some(key) = key_of(options, INFO_FIELD) {
        if let Some(info) = options.get(&key) {
            let patched = patch_info(&info, lobby, templates);
            if patched != info {
                changed |= options.set(&key, &patched);
            }
        }
    }
    if let Some(key) = key_of(options, HERO_FIELD) {
        if let Some(text) = options.get(&key) {
            let old = hero_values(&text);
            let new: Vec<String> = (0..old.len())
                .map(|index| {
                    match lobby
                        .slots
                        .get(index)
                        .map(|s| (s.occupant.occupied(), s.hero))
                    {
                        Some((true, HeroPick::Stored(h))) => h.to_string(),
                        _ => "-1".to_owned(),
                    }
                })
                .collect();
            let value = format!(":{}:", new.join(":"));
            if value != text {
                changed |= options.set(&key, &value);
            }
        }
    }
    if let Some(key) = key_of(options, MAP_FIELD) {
        if let Some(old) = options.get(&key) {
            let value = if old.contains('\\') {
                lobby.map_label.replace('/', "\\")
            } else {
                lobby.map_label.clone()
            };
            if !lobby.map_label.is_empty() && value != old {
                changed |= options.set(&key, &value);
            }
        }
    }
    changed
}

fn patch_info(info: &str, lobby: &SkirmishSetup, templates: &[String]) -> String {
    let mut parts: Vec<String> = segments(info).into_iter().map(str::to_owned).collect();
    for part in &mut parts {
        if let Some(list) = part.strip_prefix("S=") {
            let ends_open = list.ends_with(':');
            let tokens = slot_tokens(list);
            let mut out: Vec<String> = Vec::new();
            for (index, token) in tokens.iter().enumerate() {
                out.push(match lobby.slots.get(index) {
                    Some(slot) => token_of(slot, token, templates),
                    None => (*token).to_owned(),
                });
            }
            let mut joined = out.join(":");
            if ends_open {
                joined.push(':');
            }
            *part = format!("S={joined}");
        } else if let Some(rules) = part.strip_prefix("GR=") {
            let mut fields: Vec<String> = rules.split(' ').map(str::to_owned).collect();
            let money = lobby.rules.starting_money.to_string();
            let factor = lobby.rules.command_point_factor.to_string();
            for (at, value) in [(RULE_COMMAND_POINTS, factor), (RULE_MONEY, money)] {
                if let Some(field) = fields.get_mut(at).filter(|f| number(f).is_some()) {
                    *field = value;
                }
            }
            *part = format!("GR={}", fields.join(" "));
        }
    }
    parts.join(";")
}

pub(crate) fn apply(preset: &Preset, lobby: &mut SkirmishSetup, catalogue: &Catalogue) {
    for (index, slot) in preset.slots.iter().enumerate() {
        if let (Some(slot), Some(target)) = (slot, lobby.slots.get_mut(index)) {
            let mut slot = slot.clone();
            if slot
                .faction
                .as_ref()
                .is_some_and(|f| !catalogue.factions.contains(f))
            {
                slot.faction = None;
            }
            if slot.colour.is_some_and(|c| c >= catalogue.colours) {
                slot.colour = None;
            }
            if slot.team.is_some_and(|t| t >= crate::lobby::TEAMS) {
                slot.team = None;
            }
            *target = slot;
        }
    }
    if let Some(money) = preset
        .starting_money
        .filter(|m| crate::setup_rules::INITIAL_RESOURCES.contains(m))
    {
        lobby.rules.starting_money = money;
    }
    if let Some(factor) = preset.command_point_factor.filter(|f| {
        crate::setup_rules::COMMAND_POINT_FACTORS
            .iter()
            .any(|(p, _)| p == f)
    }) {
        lobby.rules.command_point_factor = factor;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    const FILE: &[u8] = b"CurrentUserName = ab\r\n\
:0:ab:GameInfo = M=000maps/m;MC=1;GR=1 0 1 100 1000 -1 ;S=Hab,0,0,TT,2,1,-1,-1,0,9,9:CE,3,0,-1,-1,7,7:X:X:X:X:X:X:;\r\n\
:0:ab:HeroIndexes = :1:-1:-1:-1:-1:-1:-1:-1:\r\n\
:0:ab:Map = maps\\m\\m.map\r\n";

    fn templates() -> Vec<String> {
        ["Obs", "Men", "Mordor"].map(String::from).to_vec()
    }

    fn lobby() -> SkirmishSetup {
        let mut lobby = SkirmishSetup::two_player(
            "Mordor".to_owned(),
            "Men".to_owned(),
            "maps/n/n.map".to_owned(),
            [false, false],
            None,
        );
        lobby.slots[0] = SlotSetup {
            faction: Some("Mordor".to_owned()),
            colour: Some(5),
            start: Some(2),
            team: Some(1),
            hero: HeroPick::Stored(4),
            ..SlotSetup::new(Occupant::Human)
        };
        lobby.slots[1] = SlotSetup {
            faction: Some("Men".to_owned()),
            colour: Some(0),
            ..SlotSetup::new(Occupant::Ai(Difficulty::Hard))
        };
        lobby.rules.starting_money = 2000;
        lobby
    }

    #[test]
    fn a_stored_setup_loads_back_and_keeps_the_fields_mint_does_not_model() {
        let mut options = UserOptions::parse(FILE);
        assert!(store(&mut options, &lobby(), &templates()));
        let text = String::from_utf8(options.render().to_vec()).unwrap();
        assert!(
            text.contains("Hab,0,0,TT,5,2,2,1,0,9,9:CH,0,1,-1,-1,7,7:X:"),
            "{text}"
        );
        assert!(text.contains("GR=1 0 1 100 2000 -1 ;"), "{text}");
        assert!(text.contains(":0:ab:Map = maps\\n\\n.map\r\n"), "{text}");
        let loaded = load(&options, &templates(), 8).unwrap();
        assert_eq!(loaded.map.as_deref(), Some("maps\\n\\n.map"));
        assert_eq!(loaded.starting_money, Some(2000));
        let human = loaded.slots[0].as_ref().unwrap();
        assert_eq!(
            (
                human.faction.as_deref(),
                human.colour,
                human.start,
                human.team,
                human.hero
            ),
            (
                Some("Mordor"),
                Some(5),
                Some(2),
                Some(1),
                HeroPick::Stored(4)
            )
        );
        assert_eq!(
            loaded.slots[1].as_ref().unwrap().occupant,
            Occupant::Ai(Difficulty::Hard)
        );
    }

    #[test]
    fn nothing_is_created_when_the_file_has_no_game_info() {
        let mut options = UserOptions::parse(b"CurrentUserName = ab\r\n");
        assert!(!store(&mut options, &lobby(), &templates()));
        assert_eq!(options.render(), b"CurrentUserName = ab\r\n");
        assert!(load(&options, &templates(), 8).is_none());
    }
}
