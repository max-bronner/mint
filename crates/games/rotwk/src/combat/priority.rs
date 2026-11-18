use std::collections::BTreeMap;

use mint_movement::Vec2;
use mint_sage_combat::{surface_distance_squared_2d, within_attack_range, Point3};
use mint_sage_modules::AiKindOf;
use mint_sim::{Entity, PlayerId};

use super::{CombatChainTable, Foe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrioritySet {
    pub(crate) default: i32,
    pub(crate) templates: BTreeMap<String, i32>,
}

impl Default for PrioritySet {
    fn default() -> Self {
        Self {
            default: 1,
            templates: BTreeMap::new(),
        }
    }
}

pub(crate) static DEFAULT_SET: PrioritySet = PrioritySet {
    default: 1,
    templates: BTreeMap::new(),
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AttitudeRanges {
    pub(crate) alert: f32,
    pub(crate) aggressive: f32,
}

impl Default for AttitudeRanges {
    fn default() -> Self {
        Self {
            alert: 1.1,
            aggressive: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttackPriorityData {
    pub(crate) sets: BTreeMap<String, PrioritySet>,
    pub(crate) distance_modifier: f32,
    pub(crate) ranges: AttitudeRanges,
    pub(crate) max_friendly_fire: f32,
    pub(crate) defined_templates: std::collections::BTreeSet<String>,
}

impl Default for AttackPriorityData {
    fn default() -> Self {
        Self {
            sets: BTreeMap::new(),
            distance_modifier: 80.0,
            ranges: AttitudeRanges::default(),
            max_friendly_fire: 1.0,
            defined_templates: std::collections::BTreeSet::new(),
        }
    }
}

impl AttackPriorityData {
    pub(crate) fn read(
        ai_data: &mint_sage_ini::Block,
        sets: &BTreeMap<String, &mint_sage_ini::Block>,
        errors: &mint_sage_ini::ValueErrors,
    ) -> Self {
        let defaults = Self::default();
        let number = |name: &str, default: f32, positive: bool| {
            let Some(field) = ai_data.field(name) else {
                return default;
            };
            let read = field
                .value()
                .and_then(|text| text.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && if positive { *v > 0.0 } else { *v >= 0.0 });
            if read.is_none() {
                errors.field("AIData", field, "a positive number");
            }
            read.unwrap_or(default)
        };
        let mut out = Self {
            sets: BTreeMap::new(),
            distance_modifier: number(
                "AttackPriorityDistanceModifier",
                defaults.distance_modifier,
                true,
            ),
            ranges: AttitudeRanges {
                alert: number("AlertRangeModifier", defaults.ranges.alert, false),
                aggressive: number("AggressiveRangeModifier", defaults.ranges.aggressive, false),
            },
            max_friendly_fire: percent(ai_data, errors).unwrap_or(defaults.max_friendly_fire),
            defined_templates: std::collections::BTreeSet::new(),
        };
        for (name, block) in sets {
            let mut set = PrioritySet::default();
            for field in block.fields() {
                let int = |text: Option<&str>| text.and_then(|t| t.trim().parse::<i32>().ok());
                if field.keyword == "Default" {
                    match int(field.value()) {
                        Some(value) => set.default = value,
                        None => errors.field("AIData AttackPriority", field, "an integer"),
                    }
                } else if field.keyword == "Target" {
                    let (thing, value) = (
                        field.args.first(),
                        int(field.args.get(1).map(String::as_str)),
                    );
                    match (thing, value) {
                        (Some(thing), Some(value)) => {
                            set.templates.insert(thing.to_ascii_lowercase(), value);
                        }
                        _ => errors.field("AIData AttackPriority", field, "a name and an integer"),
                    }
                }
            }
            out.sets.insert(name.clone(), set);
        }
        out
    }
}

fn percent(ai_data: &mint_sage_ini::Block, errors: &mint_sage_ini::ValueErrors) -> Option<f32> {
    let field = ai_data.field("ArtilleryMaxFriendlyFire")?;
    let read = field
        .value()
        .and_then(|text| {
            let text = text.trim();
            mint_sage_ini::value::parse_float(text.strip_suffix('%').unwrap_or(text))
        })
        .filter(|value| value.is_finite());
    if read.is_none() {
        errors.field("AIData", field, "a percentage");
    }
    read.map(|value| value * 0.01)
}

pub(crate) fn friendly_fire_skips(allied: f32, other: f32, max_share: f32) -> bool {
    let total = other + allied;
    total < 1.0 || allied / total >= max_share
}

const AI_TARGET_PLAYER_BONUS: f32 = 100.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PriorityRanking<'a> {
    pub(crate) set: &'a PrioritySet,
    pub(crate) distance_modifier: f32,
    pub(crate) chain: Option<(AiKindOf, &'a CombatChainTable)>,
    pub(crate) target_player: Option<PlayerId>,
    pub(crate) computer: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TargetPriority<'a> {
    pub(crate) template: Option<&'a str>,
    pub(crate) owner: Option<PlayerId>,
    pub(crate) horde: Option<(Option<&'a str>, Option<AiKindOf>)>,
    pub(crate) horde_contained: bool,
    pub(crate) contents: Vec<(Option<&'a str>, Option<AiKindOf>, Option<PlayerId>)>,
    pub(crate) creep: bool,
    pub(crate) unpickable: bool,
}

impl PriorityRanking<'_> {
    fn priority_of(
        &self,
        template: Option<&str>,
        kind: Option<AiKindOf>,
        owner: Option<PlayerId>,
    ) -> f32 {
        let base = template
            .and_then(|name| {
                if self.set.templates.is_empty() {
                    None
                } else {
                    self.set.templates.get(&name.to_ascii_lowercase())
                }
            })
            .copied()
            .unwrap_or(self.set.default);
        #[allow(clippy::cast_precision_loss)]
        let base = base as f32;
        let Some((attacker, table)) = self.chain else {
            return base;
        };
        let Some(modifier) = kind.map_or(Some(0.0), |target| table.modifier(attacker, target))
        else {
            return 0.0;
        };
        let modifier = if self.target_player.is_some() && owner == self.target_player {
            modifier + AI_TARGET_PLAYER_BONUS
        } else {
            modifier
        };
        base + modifier / self.distance_modifier
    }

    fn total(&self, target: &TargetPriority<'_>, kind: Option<AiKindOf>) -> Option<f32> {
        let own = self.priority_of(target.template, kind, target.owner);
        let priority = match target.horde {
            Some(_) if target.horde_contained => 0.0,
            Some((template, kind)) => {
                let horde = self.priority_of(template, kind, target.owner);
                if own > horde {
                    own
                } else {
                    horde
                }
            }
            None => own,
        };
        (priority != 0.0).then_some(priority)
    }

    fn lent(&self, priority: f32, target: &TargetPriority<'_>) -> f32 {
        target
            .contents
            .iter()
            .fold(priority, |best, &(template, kind, owner)| {
                let held = self.priority_of(template, kind, owner);
                if held > best {
                    held
                } else {
                    best
                }
            })
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn acquire_ranked<'a>(
    shooter: Vec2,
    shooter_radius: f32,
    foes: &[Foe],
    min_range: f32,
    max_range: f32,
    ranking: &PriorityRanking<'_>,
    info: impl Fn(Entity) -> TargetPriority<'a>,
    reachable: impl Fn(&Foe) -> bool,
    friendly_fire: impl Fn(&Foe) -> bool,
) -> Option<Entity> {
    let here = Point3::new(shooter.x, shooter.y, 0.0);
    let mut best: Option<Entity> = None;
    let (mut best_score, mut best_priority, mut best_reach) = (0.0_f32, 0.0_f32, false);
    for foe in foes {
        let there = Point3::new(foe.position.x, foe.position.y, 0.0);
        let distance_squared = surface_distance_squared_2d(here, shooter_radius, there, foe.radius);
        if !within_attack_range(distance_squared, min_range, max_range) {
            continue;
        }
        let target = info(foe.entity);
        if target.unpickable {
            continue;
        }
        let Some(priority) = ranking.total(&target, foe.ai_kind_of) else {
            continue;
        };
        let priority = ranking.lent(priority, &target);
        let score = if ranking.computer && target.creep {
            1.0
        } else {
            priority + 100.0 - distance_squared.sqrt() / ranking.distance_modifier
        };
        let better = score > best_score
            || (score == best_score
                && (priority > best_priority
                    || (priority == best_priority
                        && best.is_some_and(|best| foe.entity.index() < best.index()))));
        if !better && best_reach {
            continue;
        }
        let can_reach = reachable(foe);
        if !can_reach && (best_reach || !better) {
            continue;
        }
        if friendly_fire(foe) {
            continue;
        }
        best = Some(foe.entity);
        (best_score, best_priority, best_reach) = (score, priority, can_reach);
    }
    best
}
