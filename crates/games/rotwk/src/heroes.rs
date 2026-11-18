use mint_sage_modules::{ProductionModifierDef, RespawnEntry, RespawnRule};
use mint_sim::{Component, Entity, LogicFrames, StateHasher};

use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeroState {
    Available,
    Queued {
        structure: Entity,
        revive: Option<u32>,
    },
    Alive(Entity),
    Dead {
        points: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeroSlot {
    pub(crate) kind: Option<Kind>,
    pub(crate) state: HeroState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct HeroRoster(pub(crate) Vec<HeroSlot>);

impl Component for HeroRoster {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for slot in &self.0 {
            hasher.write_bool(slot.kind.is_some());
            hasher.write_u16(slot.kind.map_or(0, |kind| kind.0));
            match slot.state {
                HeroState::Available => hasher.write_u8(0),
                HeroState::Queued { structure, revive } => {
                    hasher.write_u8(1);
                    structure.hash_into(hasher);
                    hasher.write_bool(revive.is_some());
                    hasher.write_u32(revive.unwrap_or(0));
                }
                HeroState::Alive(entity) => {
                    hasher.write_u8(2);
                    entity.hash_into(hasher);
                }
                HeroState::Dead { points } => {
                    hasher.write_u8(3);
                    hasher.write_u32(points);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeroRefusal {
    NotOwned,
    NoButton,
    NoHero,
    NeedsUpgrade,
    Fielded,
    InProduction,
    NoProducer,
    NoPrice,
    CantAfford { cost: i64 },
    NotAllowed,
    QueueRefused,
    QueueFull,
    CommandPoints { needed: i32, free: i32 },
}

impl HeroRefusal {
    pub(crate) fn message(self) -> String {
        match self {
            Self::NotOwned => "not the player's structure".to_owned(),
            Self::NoButton => "no such hero button here".to_owned(),
            Self::NoHero => "no hero on that button".to_owned(),
            Self::NeedsUpgrade => "its NeededUpgrade is not held".to_owned(),
            Self::Fielded => "the hero is on the field".to_owned(),
            Self::InProduction => "the hero is already in production".to_owned(),
            Self::NoProducer => "the structure produces nothing".to_owned(),
            Self::NoPrice => "the data gives no price".to_owned(),
            Self::CantAfford { cost } => format!("{cost} is more than the purse holds"),
            Self::NotAllowed => "the player is not allowed to build it".to_owned(),
            Self::QueueRefused => "the queue refused it".to_owned(),
            Self::QueueFull => "the structure's queue is full".to_owned(),
            Self::CommandPoints { needed, free } => {
                format!("needs {needed} command point(s), {free} free")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeroOrder {
    pub(crate) slot: usize,
    pub(crate) kind: Kind,
    pub(crate) revive: Option<u32>,
    pub(crate) cost: i64,
    pub(crate) build_time: LogicFrames,
}

pub(crate) fn respawn_rule(
    rules: &[RespawnRule],
    entries: &[RespawnEntry],
    rank: u16,
) -> Option<RespawnRule> {
    let base = *rules
        .iter()
        .find(|rule| rule.level == Some(rank))
        .or_else(|| rules.iter().find(|rule| rule.level.is_none()))?;
    let mut applicable: Vec<&RespawnEntry> = entries
        .iter()
        .filter(|entry| entry.level.is_some_and(|level| level <= rank))
        .collect();
    applicable.sort_by_key(|entry| entry.level);
    let cost = applicable
        .iter()
        .rev()
        .find_map(|entry| entry.cost)
        .or(base.cost);
    let time_ms = applicable
        .iter()
        .rev()
        .find_map(|entry| entry.time_ms)
        .or(base.time_ms);
    Some(RespawnRule {
        cost,
        time_ms,
        ..base
    })
}

pub(crate) fn hero_discount(
    modifiers: &[ProductionModifierDef],
    revive: bool,
    passes: impl Fn(&[String]) -> bool,
    held: impl Fn(&str) -> bool,
) -> (f32, f32) {
    modifiers
        .iter()
        .filter(|m| {
            if revive {
                m.hero_revive
            } else {
                m.hero_purchase
            }
        })
        .filter(|m| passes(&m.filter))
        .filter(|m| m.required_upgrade.as_deref().is_none_or(&held))
        .fold((1.0, 1.0), |(cost, time), m| {
            (cost * m.cost_multiplier, time * m.time_multiplier)
        })
}

#[must_use]
pub(crate) fn command_points_fit(needed: i32, free: i32, army_of_dead: bool) -> bool {
    needed <= 0 || army_of_dead || needed <= free
}

pub(crate) fn discounted(
    cost: i64,
    seconds: f32,
    (cost_mult, time_mult): (f32, f32),
) -> (i64, LogicFrames) {
    (
        mint_sage_economy::cost_to_build(
            cost,
            &mint_sage_economy::CostModifiers {
                kind_of_modifier: cost_mult,
                ..mint_sage_economy::CostModifiers::default()
            },
        ),
        mint_sage_economy::time_to_build(
            seconds,
            &mint_sage_economy::TimeModifiers {
                production_time_change_percent: time_mult - 1.0,
                ..mint_sage_economy::TimeModifiers::default()
            },
        ),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeroAvailability {
    pub(crate) fielded: bool,
    pub(crate) affordable: bool,
    pub(crate) allowed: bool,
    pub(crate) cost: Option<i64>,
}

const ENGINE_HERO_PAGE_IMAGES: [(&str, &str, &str); 1] = [(
    "GondorGandalf",
    "SCIENCE_GandalftheWhite",
    "HIGandalTheGrey",
)];

pub(crate) fn page_image(
    name: &str,
    button_image: Option<&str>,
    holds_science: impl Fn(&str) -> bool,
    image_defined: impl Fn(&str) -> bool,
) -> Option<String> {
    ENGINE_HERO_PAGE_IMAGES
        .iter()
        .find(|(template, science, image)| {
            template.eq_ignore_ascii_case(name) && !holds_science(science) && image_defined(image)
        })
        .map(|(_, _, image)| (*image).to_owned())
        .or_else(|| button_image.map(str::to_owned))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_named_hero_shows_its_own_picture_until_the_science_is_held() {
        let defined = |_: &str| true;
        let own = Some("TemplateOwnPicture");
        assert_eq!(
            page_image("GondorGandalf", own, |_| false, defined).as_deref(),
            Some("HIGandalTheGrey")
        );
        assert_eq!(
            page_image(
                "GondorGandalf",
                own,
                |s| s == "SCIENCE_GandalftheWhite",
                defined
            )
            .as_deref(),
            Some("TemplateOwnPicture")
        );
        assert_eq!(
            page_image("GondorGandalf", own, |_| false, |_| false).as_deref(),
            Some("TemplateOwnPicture"),
            "an undefined override image falls back to the template's own"
        );
        assert_eq!(
            page_image("SomeOtherHero", own, |_| false, defined).as_deref(),
            Some("TemplateOwnPicture")
        );
    }

    fn rule(level: Option<u16>, cost: f32) -> RespawnRule {
        RespawnRule {
            level,
            auto_spawn: false,
            cost: Some(cost),
            time_ms: Some(60_000.0),
            health: Some(1.0),
        }
    }

    fn entry(level: u16, cost: Option<f32>, time_ms: Option<f32>) -> RespawnEntry {
        RespawnEntry {
            level: Some(level),
            cost,
            time_ms,
        }
    }

    #[test]
    fn command_points_fit_cases() {
        assert!(command_points_fit(0, 0, false), "zero cost always fits");
        assert!(
            command_points_fit(50, 10, true),
            "ARMY_OF_DEAD ignores the limit"
        );
        assert!(
            !command_points_fit(50, 10, false),
            "over the limit, not exempt"
        );
        assert!(
            command_points_fit(50, 50, false),
            "exactly at the limit fits"
        );
    }

    #[test]
    fn a_level_specific_rule_wins_over_any_level() {
        let rules = [rule(None, 625.0), rule(Some(3), 900.0)];
        assert_eq!(respawn_rule(&rules, &[], 3).unwrap().cost, Some(900.0));
        assert_eq!(respawn_rule(&rules, &[], 2).unwrap().cost, Some(625.0));
        assert!(respawn_rule(&[rule(Some(3), 1.0)], &[], 2).is_none());
    }

    #[test]
    fn a_respawn_entry_overrides_only_its_own_field_until_the_next_entry() {
        let rules = [rule(None, 625.0)];
        let entries = [entry(3, Some(900.0), None), entry(5, None, Some(50_000.0))];
        let at = |rank| respawn_rule(&rules, &entries, rank).unwrap();
        assert_eq!((at(2).cost, at(2).time_ms), (Some(625.0), Some(60_000.0)));
        assert_eq!((at(3).cost, at(3).time_ms), (Some(900.0), Some(60_000.0)));
        assert_eq!((at(4).cost, at(4).time_ms), (Some(900.0), Some(60_000.0)));
        assert_eq!((at(5).cost, at(5).time_ms), (Some(900.0), Some(50_000.0)));
        assert_eq!((at(6).cost, at(6).time_ms), (Some(900.0), Some(50_000.0)));
    }

    fn modifier(
        revive: bool,
        purchase: bool,
        upgrade: &str,
        cost: f32,
        time: f32,
    ) -> ProductionModifierDef {
        ProductionModifierDef {
            required_upgrade: Some(upgrade.to_owned()),
            cost_multiplier: cost,
            time_multiplier: time,
            hero_purchase: purchase,
            hero_revive: revive,
            filter: vec!["NONE".to_owned(), "+HERO".to_owned()],
        }
    }

    #[test]
    fn a_discount_applies_to_its_own_kind_of_press_behind_its_upgrade() {
        let mods = [
            modifier(false, false, "Banners", 0.8, 1.0),
            modifier(false, true, "Banners", 0.9, 0.9),
            modifier(true, false, "Healing", 0.8, 0.8),
        ];
        let hero = |filter: &[String]| filter.iter().any(|t| t == "+HERO");
        let both = |_: &str| true;
        assert_eq!(hero_discount(&mods, false, hero, both), (0.9, 0.9));
        assert_eq!(hero_discount(&mods, true, hero, both), (0.8, 0.8));
        let banners_only = |name: &str| name == "Banners";
        assert_eq!(hero_discount(&mods, true, hero, banners_only), (1.0, 1.0));
        let nothing = |_: &[String]| false;
        assert_eq!(hero_discount(&mods, false, nothing, both), (1.0, 1.0));
    }

    #[test]
    fn a_discounted_price_truncates_as_ea_does() {
        let (cost, time) = discounted(625, 60.0, (0.8, 0.8));
        assert_eq!(cost, 500);
        assert_eq!(time.get(), 1_440, "60 s at 30 Hz is 1800, less 20%");
        let (cost, time) = discounted(1500, 60.0, (1.0, 1.0));
        assert_eq!((cost, time.get()), (1500, 1800));
    }
}
