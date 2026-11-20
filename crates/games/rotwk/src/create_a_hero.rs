use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_cah::{HeroRecord, POWER_SLOTS};
use mint_sage_ini::{
    read_create_a_hero_system, CommandButtonIndex, CommandSetDef, CommandSetIndex,
    CreateAHeroSystemDef, Document, MacroTable, SubClassDef, UpgradeIndex, ValueErrors,
    CREATE_A_HERO_SYSTEM_KEYWORD,
};
use mint_sage_modules::{PowerSlot, UpgradeBits};
use mint_sim::{Component, PlayerId, StateHasher, UpgradeBit};

use crate::game_data::GameData;
use crate::lobby::CarriedHero;
use crate::templates::KindTable;
use crate::unit::Kind;

pub(crate) const CREATE_A_HERO_KIND: &str = "CREATE_A_HERO";
const ATTACK_MOVE_BUTTON: &str = "Command_AttackMove";
const ATTACK_MOVE_SLOT: u32 = 16;
const COMMAND_SLOTS: u32 = 33;
const DEFAULT_REVIVAL_DISCOUNT: u32 = 75;
const DEFAULT_POWER_DISCOUNT: u32 = 10;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CahData {
    pub(crate) system: CreateAHeroSystemDef,
    group_orders: BTreeMap<String, i64>,
    upgrade_groups: Vec<(String, String)>,
}

pub(crate) fn upgrade_groups(upgrades: &UpgradeIndex) -> Vec<(String, String)> {
    upgrades
        .names()
        .filter_map(|name| Some((name.to_owned(), upgrades.get(name)?.group_name.clone()?)))
        .collect()
}

pub(crate) fn attribute_blings(
    system: &CreateAHeroSystemDef,
    sub: &SubClassDef,
    upgrade_groups: &[(String, String)],
) -> Vec<(String, String)> {
    let is_attribute = |group: &str| {
        sub.attributes.iter().any(|a| {
            a.group
                .as_deref()
                .is_some_and(|g| g.eq_ignore_ascii_case(group))
        })
    };
    upgrade_groups
        .iter()
        .filter(|(_, group)| is_attribute(group))
        .filter_map(|(name, _)| {
            let bling = system.blings.iter().find(|bling| {
                bling
                    .upgrade
                    .as_deref()
                    .is_some_and(|upgrade| upgrade.eq_ignore_ascii_case(name))
            })?;
            Some((bling.group.clone()?, bling.upgrade.clone()?))
        })
        .collect()
}

impl CahData {
    pub(crate) fn read(
        documents: &[Document],
        macros: &MacroTable,
        upgrades: &UpgradeIndex,
    ) -> Self {
        let errors = ValueErrors::new();
        let system = documents
            .iter()
            .flat_map(|document| document.roots_named(CREATE_A_HERO_SYSTEM_KEYWORD))
            .map(|block| read_create_a_hero_system(block, &errors, macros))
            .next()
            .unwrap_or_default();
        let group_orders = upgrades
            .names()
            .filter_map(|name| Some((name.to_owned(), upgrades.get(name)?.group_order?)))
            .collect();
        Self {
            system,
            group_orders,
            upgrade_groups: upgrade_groups(upgrades),
        }
    }

    fn group_order(&self, upgrade: &str) -> i64 {
        self.group_orders
            .get(&upgrade.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayerHero(pub(crate) Arc<HeroRecord>);

impl Component for PlayerHero {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(&mint_sage_cah::write(&self.0));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RankSet {
    pub(crate) rank: u16,
    pub(crate) name: String,
    pub(crate) def: CommandSetDef,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HeroPlan {
    pub(crate) player: PlayerId,
    pub(crate) record: Arc<HeroRecord>,
    pub(crate) grants: Vec<UpgradeBit>,
    pub(crate) removes: Vec<UpgradeBit>,
    pub(crate) ladders: Vec<(Kind, Vec<Option<UpgradeBit>>)>,
    pub(crate) sets: Vec<RankSet>,
    pub(crate) power_cost: u32,
    pub(crate) name: String,
    pub(crate) button_image: Option<String>,
}

impl HeroPlan {
    pub(crate) fn set_at(&self, rank: u16) -> Option<&RankSet> {
        self.sets
            .iter()
            .rev()
            .find(|set| set.rank <= rank)
            .or_else(|| self.sets.first())
    }
}

fn bling_options(
    data: &CahData,
    class: usize,
    subclass: usize,
    binder: &str,
    earned: &[(String, String)],
) -> Vec<String> {
    let Some(sub) = data
        .system
        .classes
        .get(class)
        .and_then(|class| class.subclasses.get(subclass))
    else {
        return Vec::new();
    };
    let mut options: Vec<String> = Vec::new();
    for choices in &sub.bling_upgrades {
        for upgrade in &choices.upgrades {
            let in_binder = data.system.blings.iter().any(|bling| {
                bling
                    .upgrade
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(upgrade))
                    && bling
                        .group
                        .as_deref()
                        .is_some_and(|group| group.eq_ignore_ascii_case(binder))
            });
            if in_binder && !options.iter().any(|o| o.eq_ignore_ascii_case(upgrade)) {
                options.push(upgrade.clone());
            }
        }
    }
    for (group, upgrade) in attribute_blings(&data.system, sub, &data.upgrade_groups) {
        if group.eq_ignore_ascii_case(binder)
            && !options.iter().any(|o| o.eq_ignore_ascii_case(&upgrade))
        {
            options.push(upgrade);
        }
    }
    options.sort_by_key(|upgrade| data.group_order(upgrade));
    if !options.is_empty() {
        for (group, upgrade) in earned {
            if group.eq_ignore_ascii_case(binder)
                && !options.iter().any(|o| o.eq_ignore_ascii_case(upgrade))
            {
                options.push(upgrade.clone());
            }
        }
    }
    options
}

pub(crate) fn upgrade_names(
    data: &CahData,
    record: &HeroRecord,
    earned: &[(String, String)],
    resolves: &dyn Fn(&str) -> bool,
) -> (Vec<String>, Vec<String>) {
    let (mut grants, mut removes) = (Vec::new(), Vec::new());
    let system = &data.system;
    if let (Some(map), Some(game)) = (&system.map_mode_upgrade, &system.game_mode_upgrade) {
        if resolves(map) && resolves(game) {
            grants.push(game.clone());
            removes.push(map.clone());
        }
    }
    let class = usize::try_from(record.class_index).unwrap_or(usize::MAX);
    let subclass = usize::try_from(record.subclass_index).unwrap_or(usize::MAX);
    for (group, index) in &record.blings {
        let group = String::from_utf8_lossy(group);
        let options = bling_options(data, class, subclass, &group, earned);
        let chosen = usize::try_from(*index)
            .ok()
            .and_then(|index| options.get(index));
        let bling = chosen.and_then(|upgrade| {
            system.blings.iter().find(|bling| {
                bling
                    .upgrade
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(upgrade))
            })
        });
        if let Some(name) = bling.and_then(|bling| bling.upgrade.clone()) {
            grants.push(name);
        }
    }
    let class_def = system.classes.get(class);
    grants.extend(class_def.and_then(|class| class.upgrade.clone()));
    grants.extend(
        class_def
            .and_then(|class| class.subclasses.get(subclass))
            .and_then(|sub| sub.upgrade.clone()),
    );
    (grants, removes)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_u32(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        value as u32
    } else {
        0
    }
}

pub(crate) fn button_cost(cost: u32, level: u32, min_level: u32, discount: u32) -> u32 {
    if cost == 0 || level < min_level {
        return cost;
    }
    let steps = u64::from(level.saturating_sub(min_level)).saturating_mul(u64::from(discount));
    let off = u64::from(cost).saturating_mul(steps) / 100;
    u32::try_from(off).map_or(0, |off| cost.saturating_sub(off))
}

pub(crate) fn power_cost(record: &HeroRecord, buttons: &CommandButtonIndex, discount: u32) -> u32 {
    let mut total = 0u32;
    for (at, entry) in record.powers.iter().enumerate() {
        let name = String::from_utf8_lossy(&entry.button);
        let Some(button) = buttons.get(&name) else {
            continue;
        };
        let cost = button
            .create_a_hero_ui_cost_if_selected
            .map_or(0, whole_u32);
        let min_level = button
            .create_a_hero_ui_minimum_level
            .and_then(|level| u32::try_from(level).ok())
            .unwrap_or(0);
        let level = u32::try_from(at).map_or(u32::MAX, |at| at.saturating_add(1));
        total = total.saturating_add(button_cost(cost, level, min_level, discount));
    }
    total
}

pub(crate) fn level_unlocks(
    record: &HeroRecord,
    buttons: &CommandButtonIndex,
    powers: &[PowerSlot],
    bits: &UpgradeBits,
    levels: usize,
) -> Vec<Option<UpgradeBit>> {
    (0..levels)
        .map(|at| {
            let entry = record.powers.get(at)?;
            let button = buttons.get(&String::from_utf8_lossy(&entry.button))?;
            let is_power = button.command.as_deref().is_some_and(|command| {
                command.eq_ignore_ascii_case("SPECIAL_POWER")
                    || command.eq_ignore_ascii_case("SPECIAL_POWER_TOGGLE")
            });
            let name = if is_power {
                let power = button.special_power.as_deref()?;
                powers
                    .iter()
                    .find(|slot| slot.power.eq_ignore_ascii_case(power))?
                    .unlock_upgrade
                    .as_deref()?
            } else if button
                .options
                .iter()
                .any(|option| option.eq_ignore_ascii_case("NEED_UPGRADE"))
                && button.needed_upgrade.len() == 1
            {
                button.needed_upgrade.first()?
            } else {
                return None;
            };
            bits.bit(name)
        })
        .collect()
}

pub(crate) fn command_set_at(
    template: &CommandSetDef,
    record: &HeroRecord,
    rank: u32,
) -> CommandSetDef {
    let mut def = CommandSetDef {
        slots: template
            .slots
            .iter()
            .filter(|(number, _)| **number >= 1 && **number <= COMMAND_SLOTS)
            .map(|(number, name)| (*number, name.clone()))
            .collect(),
        initial_visible: template.initial_visible,
    };
    for index in 0..COMMAND_SLOTS {
        let number = index.saturating_add(1);
        let mut has_power = false;
        for entry in record.powers.iter().take(POWER_SLOTS) {
            if entry.button.is_empty() || entry.button_index != index {
                continue;
            }
            if !has_power || entry.exp_level < rank {
                def.slots
                    .insert(number, String::from_utf8_lossy(&entry.button).into_owned());
                has_power = true;
            }
        }
    }
    def.slots.insert(
        ATTACK_MOVE_SLOT.saturating_add(1),
        ATTACK_MOVE_BUTTON.to_owned(),
    );
    def
}

pub(crate) fn plan_heroes(
    data: &GameData,
    kinds: &mut KindTable,
    heroes: &[(PlayerId, CarriedHero)],
) -> (Vec<HeroPlan>, Vec<String>) {
    let cah = &data.create_a_hero;
    let system = &cah.system;
    let mut lines = Vec::new();
    let bits = &data.upgrade_bits;
    let cah_kinds: Vec<Kind> = (0..kinds.len())
        .filter_map(|at| u16::try_from(at).ok().map(Kind))
        .filter(|kind| {
            kinds
                .get(*kind)
                .is_some_and(|entry| entry.record.has_kind(CREATE_A_HERO_KIND))
        })
        .collect();
    let mut ranks: Vec<u16> = cah_kinds
        .iter()
        .filter_map(|kind| kinds.get(*kind))
        .filter_map(|entry| data.experience.table(&entry.record.name))
        .flat_map(|levels| levels.iter().map(|level| level.rank))
        .collect();
    ranks.sort_unstable();
    ranks.dedup();
    if ranks.is_empty() {
        ranks.push(1);
    }
    let template = system
        .command_set_template
        .as_deref()
        .and_then(|name| data.command_sets.get(name))
        .cloned()
        .unwrap_or_default();
    let discount = system
        .special_power_discount_per_level
        .map_or(DEFAULT_POWER_DISCOUNT, whole_u32);
    let mut plans = Vec::new();
    for (player, carried) in heroes {
        let record = &carried.record;
        let class = usize::try_from(record.class_index).unwrap_or(usize::MAX);
        let subclass = usize::try_from(record.subclass_index).unwrap_or(usize::MAX);
        let earned = data.cah_system.as_deref().map_or_else(Vec::new, |sys| {
            sys.award_blings(class, subclass, &carried.stats)
        });
        let (grants, removes) =
            upgrade_names(cah, record, &earned, &|name| bits.bit(name).is_some());
        let button_image = system
            .classes
            .get(class)
            .and_then(|class| class.subclasses.get(subclass))
            .and_then(|sub| sub.button_image.clone());
        let id = record
            .id_str()
            .unwrap_or_else(|| format!("player{}", player.get()));
        let mut scratch = CommandSetIndex::new();
        let sets: Vec<RankSet> = ranks
            .iter()
            .map(|rank| {
                let name = format!("CommandSet_{id}_rank_{rank}");
                let def = command_set_at(&template, record, u32::from(*rank));
                scratch.insert(&name, def.clone());
                RankSet {
                    rank: *rank,
                    name,
                    def,
                }
            })
            .collect();
        let mut ladders = Vec::new();
        for kind in &cah_kinds {
            let Some(entry) = kinds.get(*kind) else {
                continue;
            };
            let levels = data
                .experience
                .table(&entry.record.name)
                .map_or(0, <[_]>::len);
            ladders.push((
                *kind,
                level_unlocks(
                    record,
                    &data.command_buttons,
                    &entry.record.powers,
                    bits,
                    levels,
                ),
            ));
        }
        for kind in &cah_kinds {
            for set in &sets {
                kinds.add_command_set_variant(*kind, &set.name, &scratch, &data.command_buttons);
            }
        }
        let plan = HeroPlan {
            player: *player,
            record: Arc::clone(record),
            grants: grants.iter().filter_map(|name| bits.bit(name)).collect(),
            removes: removes.iter().filter_map(|name| bits.bit(name)).collect(),
            ladders,
            sets,
            power_cost: power_cost(record, &data.command_buttons, discount),
            name: record.name_string(),
            button_image,
        };
        lines.push(format!(
            "cah: player {} holds {:?}: {} upgrade(s) at bind, {} rank set(s), powers add {}",
            player.get(),
            plan.name,
            plan.grants.len(),
            plan.sets.len(),
            plan.power_cost
        ));
        plans.push(plan);
    }
    (plans, lines)
}

pub(crate) fn revival_discount(system: &CreateAHeroSystemDef) -> u32 {
    system
        .hero_revival_discount
        .map_or(DEFAULT_REVIVAL_DISCOUNT, whole_u32)
}

#[cfg(test)]
mod tests;
