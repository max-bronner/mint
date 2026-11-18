use crate::hud::{CheckSpec, ComboItemSpec, ComboSpec};
use crate::lobby::MatchRules;

pub(crate) const INIT: &str = "AptMpGameRules::InitGadgets";
pub(crate) const COMBOS: usize = 2;
pub(crate) const CHECKS: usize = 2;

pub(crate) const INITIAL_RESOURCES: [i64; 11] = [
    500, 750, 1000, 1200, 1400, 1600, 1800, 2000, 2500, 3000, 4000,
];

pub(crate) const COMMAND_POINT_FACTORS: [(u32, &str); 7] = [
    (33, "VALUE:ThirdX"),
    (50, "VALUE:HalfX"),
    (100, "VALUE:1X"),
    (200, "VALUE:2X"),
    (400, "VALUE:4X"),
    (800, "VALUE:8X"),
    (10_000, "VALUE:100X"),
];

pub(crate) const DEFAULT_ENTRY: usize = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RuleChoices {
    pub(crate) show_random_template: bool,
    pub(crate) show_random_colour: bool,
}

impl RuleChoices {
    pub(crate) fn read(documents: &[mint_sage_ini::Document]) -> Self {
        let mut choices = Self::default();
        let yes = |block: &mint_sage_ini::Block, name: &str, into: &mut bool| {
            if let Some(value) = block.field(name).and_then(mint_sage_ini::Field::value) {
                *into = value.trim().eq_ignore_ascii_case("yes");
            }
        };
        for document in documents {
            for block in document.roots() {
                if block.keyword.eq_ignore_ascii_case("MultiplayerSettings") {
                    yes(
                        block,
                        "ShowRandomPlayerTemplate",
                        &mut choices.show_random_template,
                    );
                    yes(block, "ShowRandomColor", &mut choices.show_random_colour);
                }
            }
        }
        choices
    }

    pub(crate) fn defaults(&self) -> MatchRules {
        MatchRules {
            starting_money: INITIAL_RESOURCES[DEFAULT_ENTRY],
            command_point_factor: COMMAND_POINT_FACTORS[DEFAULT_ENTRY].0,
            ..MatchRules::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RuleLabels {
    pub(crate) combos: [String; COMBOS],
    pub(crate) checks: [String; CHECKS],
    pub(crate) factors: Vec<String>,
    pub(crate) default_suffix: String,
}

pub(crate) const COMBO_KEYS: [&str; COMBOS] = ["RULE:InitialResources", "RULE:CommandPointFactor"];
pub(crate) const CHECK_KEYS: [&str; CHECKS] = ["RULE:AllowCustomHeroes", "RULE:AllowRingHeroes"];

pub(crate) fn combo_id(row: usize) -> String {
    format!("RuleComboBox_{row}")
}

pub(crate) fn check_id(row: usize) -> String {
    format!("RuleCheckBox_{row}")
}

pub(crate) fn combo(rules: &MatchRules, labels: &RuleLabels, row: usize) -> Option<ComboSpec> {
    let (items, selected): (Vec<String>, Option<usize>) = match row {
        0 => (
            INITIAL_RESOURCES.iter().map(ToString::to_string).collect(),
            INITIAL_RESOURCES
                .iter()
                .position(|m| *m == rules.starting_money),
        ),
        1 => (
            COMMAND_POINT_FACTORS
                .iter()
                .enumerate()
                .map(|(i, (_, key))| {
                    labels
                        .factors
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| (*key).to_owned())
                })
                .collect(),
            COMMAND_POINT_FACTORS
                .iter()
                .position(|(p, _)| *p == rules.command_point_factor),
        ),
        _ => return None,
    };
    Some(ComboSpec {
        items: items
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                if i == DEFAULT_ENTRY {
                    ComboItemSpec::text(format!("{text}{}", labels.default_suffix))
                } else {
                    ComboItemSpec::text(text)
                }
            })
            .collect(),
        selected,
        enabled: true,
    })
}

pub(crate) fn check(rules: &MatchRules, row: usize) -> Option<CheckSpec> {
    let checked = match row {
        0 => rules.allow_custom_heroes,
        1 => rules.allow_ring_heroes,
        _ => return None,
    };
    Some(CheckSpec {
        checked,
        enabled: true,
    })
}

pub(crate) fn choose(rules: &mut MatchRules, row: usize, entry: usize) -> bool {
    let before = rules.clone();
    match row {
        0 => {
            if let Some(money) = INITIAL_RESOURCES.get(entry) {
                rules.starting_money = *money;
            }
        }
        1 => {
            if let Some((factor, _)) = COMMAND_POINT_FACTORS.get(entry) {
                rules.command_point_factor = *factor;
            }
        }
        _ => {}
    }
    *rules != before
}

pub(crate) fn tick(rules: &mut MatchRules, row: usize, checked: bool) -> bool {
    let flag = match row {
        0 => &mut rules.allow_custom_heroes,
        1 => &mut rules.allow_ring_heroes,
        _ => return false,
    };
    let changed = *flag != checked;
    *flag = checked;
    changed
}

pub(crate) fn heroes_offered(
    rules: &MatchRules,
    ring: &[String],
    heroes: &[String],
) -> Vec<String> {
    hero_list(rules.allow_ring_heroes, ring, heroes)
}

pub(crate) fn hero_list(
    allow_ring_heroes: bool,
    ring: &[String],
    heroes: &[String],
) -> Vec<String> {
    let ring = if allow_ring_heroes { ring } else { &[] };
    ring.iter().chain(heroes).cloned().collect()
}

#[cfg(test)]
mod tests;
