use std::collections::BTreeMap;

use mint_movement::Vec2;
use mint_sage_economy::{
    Alignment, AutoDeposit, CommandPointPool, CostModifierStacking, CostModifiers, EntryKind,
    ItemId, ItemTable, Money, ProductionId, ProductionQueue, Progress, QueueRequest,
};
use mint_sim::{
    Component, ComponentId, Entity, PlayerId, Result, StateHasher, Tick, UpgradeBit, UpgradeMask,
    World,
};

use crate::diag::diag;
use crate::templates::KindTable;
use crate::unit::{Kind, Owner};

#[derive(Debug, Clone, Copy)]
pub(crate) struct EconomyIds {
    pub(crate) slot: ComponentId<PlayerSlot>,
    pub(crate) purse: ComponentId<Purse>,
    pub(crate) points: ComponentId<Points>,
    pub(crate) producer: ComponentId<Producer>,
    pub(crate) income: ComponentId<Income>,
    pub(crate) upkeep: ComponentId<Upkeep>,
    pub(crate) owner: ComponentId<Owner>,
    pub(crate) kind: ComponentId<Kind>,
    pub(crate) construction: ComponentId<crate::construction::Construction>,
    pub(crate) player_upgrades: ComponentId<PlayerUpgrades>,
    pub(crate) player_upgrades_in_progress: ComponentId<PlayerUpgradesInProgress>,
    pub(crate) object_upgrades: ComponentId<ObjectUpgrades>,
    pub(crate) bonuses: ComponentId<crate::experience::Bonuses>,
    pub(crate) doomed: ComponentId<crate::combat::Doomed>,
    pub(crate) claimant: ComponentId<crate::claims::Claimant>,
    pub(crate) match_stats: ComponentId<crate::match_stats::MatchStats>,
    pub(crate) object_value: ComponentId<ObjectValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ObjectValue {
    pub(crate) paid_cost: i32,
    pub(crate) upgrade_spend: i32,
}

impl Component for ObjectValue {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.paid_cost);
        hasher.write_i32(self.upgrade_spend);
    }
}

fn stores_upgrade_price(
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    kind: Kind,
) -> bool {
    kinds.get(kind).is_some_and(|entry| {
        entry
            .record
            .declared_modules
            .iter()
            .filter(|module| module.name.eq_ignore_ascii_case("CastleMemberBehavior"))
            .any(|module| {
                mint_sage_modules::castle_member_behavior_from_fields(
                    &module.source(upgrade_bits),
                    mint_sim::Frame::new(0),
                )
                .is_ok_and(|behavior| behavior.store_upgrade_price())
            })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerSlot(pub(crate) PlayerId);

impl Component for PlayerSlot {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Purse(pub(crate) Money);

impl Component for Purse {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Points(pub(crate) CommandPointPool);

impl Component for Points {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Upkeep(pub(crate) i32);

impl Component for Upkeep {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DisallowedBuildings(pub(crate) Vec<Kind>);

impl Component for DisallowedBuildings {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for kind in &self.0 {
            hasher.write_u16(kind.0);
        }
    }
}

impl DisallowedBuildings {
    pub(crate) fn set(&mut self, kind: Kind, disallowed: bool) {
        self.0.retain(|&held| held != kind);
        if disallowed {
            self.0.push(kind);
            self.0.sort_unstable_by_key(|k| k.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildSwitches {
    pub(crate) can_build_units: bool,
    pub(crate) can_build_base: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputerSide {
    pub(crate) player: PlayerId,
    pub(crate) evil: bool,
    pub(crate) cash: i64,
}

impl Component for BuildSwitches {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.can_build_units);
        hasher.write_bool(self.can_build_base);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PlayerUpgrades(pub(crate) UpgradeMask);

impl Component for PlayerUpgrades {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PlayerUpgradesInProgress(pub(crate) Vec<UpgradeBit>);

impl Component for PlayerUpgradesInProgress {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for bit in &self.0 {
            hasher.write_u32(bit.index());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ObjectUpgrades(pub(crate) UpgradeMask);

impl Component for ObjectUpgrades {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SubUpgrade {
    pub(crate) bit: UpgradeBit,
    pub(crate) cost: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UpgradeBuildable {
    pub(crate) name: String,
    pub(crate) cost_discount_template: Option<String>,
    pub(crate) bit: UpgradeBit,
    pub(crate) is_player: bool,
    pub(crate) cost: i64,
    pub(crate) subs: Vec<SubUpgrade>,
    pub(crate) build_time: mint_sim::LogicFrames,
    pub(crate) command_button: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Producer {
    pub(crate) queue: ProductionQueue,
    pub(crate) menu: Option<u16>,
    pub(crate) object_menu: Option<u16>,
    pub(crate) player_menu: Option<u16>,
    pub(crate) rally: Vec2,
    pub(crate) player_rally: Option<Vec2>,
}

impl Component for Producer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.queue.hash_into(hasher);
        hasher.write_bool(self.menu.is_some());
        hasher.write_u16(self.menu.unwrap_or(0));
        hasher.write_bool(self.object_menu.is_some());
        hasher.write_u16(self.object_menu.unwrap_or(0));
        hasher.write_bool(self.player_menu.is_some());
        hasher.write_u16(self.player_menu.unwrap_or(0));
        hasher.write_f32(self.rally.x);
        hasher.write_f32(self.rally.y);
        hasher.write_bool(self.player_rally.is_some());
        if let Some(point) = self.player_rally {
            hasher.write_f32(point.x);
            hasher.write_f32(point.y);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Income(pub(crate) AutoDeposit);

impl Component for Income {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Output {
    Unit(crate::unit::Spawn),
    Horde {
        formation: u16,
        member: crate::unit::Spawn,
        count: u32,
        vision: f32,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Buildable {
    pub(crate) output: Output,
    pub(crate) kind: Kind,
    pub(crate) cost: i64,
    pub(crate) build_seconds: f32,
    pub(crate) command_points: i32,
    pub(crate) label: String,
    pub(crate) builder: bool,
}

pub(crate) fn player_entity(world: &World, ids: EconomyIds, player: PlayerId) -> Option<Entity> {
    world.store(ids.slot).and_then(|store| {
        store
            .iter()
            .find(|(_, slot)| slot.0 == player)
            .map(|(entity, _)| entity)
    })
}

pub(crate) fn purse_of(world: &World, ids: EconomyIds, player: PlayerId) -> Money {
    player_entity(world, ids, player)
        .and_then(|entity| world.get(ids.purse, entity))
        .map_or(Money::new(0), |purse| purse.0)
}

pub(crate) fn points_of(world: &World, ids: EconomyIds, player: PlayerId) -> CommandPointPool {
    player_entity(world, ids, player)
        .and_then(|entity| world.get(ids.points, entity))
        .map_or(CommandPointPool::new(0), |points| points.0)
}

pub(crate) enum PriceTarget<'a> {
    Object {
        kind_of: &'a [String],
        name: &'a str,
    },
    Upgrade {
        name: &'a str,
        cost_discount_template: Option<(&'a [String], &'a str)>,
    },
}

pub(crate) fn cost_modifiers_for(
    world: &World,
    ids: EconomyIds,
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    player: PlayerId,
    target: &PriceTarget<'_>,
    production_cost_change_percent: f32,
) -> CostModifiers {
    cost_modifiers_from_counts(
        &player_cost_modifier_counts(world, ids, player),
        kinds,
        upgrade_bits,
        target,
        production_cost_change_percent,
    )
}

pub(crate) struct PlayerCostModifierCounts {
    counts: BTreeMap<u16, u32>,
    held: UpgradeMask,
}

pub(crate) fn player_cost_modifier_counts(
    world: &World,
    ids: EconomyIds,
    player: PlayerId,
) -> PlayerCostModifierCounts {
    let mut counts: BTreeMap<u16, u32> = BTreeMap::new();
    if let Some(store) = world.store(ids.owner) {
        for (entity, owner) in store.iter() {
            if owner.0 != player
                || !world.is_live(entity)
                || world.contains(ids.construction, entity)
            {
                continue;
            }
            if let Some(kind) = world.get(ids.kind, entity) {
                let slot = counts.entry(kind.0).or_insert(0);
                *slot = slot.saturating_add(1);
            }
        }
    }
    let held = player_entity(world, ids, player)
        .and_then(|entity| world.get(ids.player_upgrades, entity))
        .map_or_else(UpgradeMask::default, |upgrades| upgrades.0.clone());
    PlayerCostModifierCounts { counts, held }
}

pub(crate) fn cost_modifiers_from_counts(
    state: &PlayerCostModifierCounts,
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    target: &PriceTarget<'_>,
    production_cost_change_percent: f32,
) -> CostModifiers {
    let mut kind_of_modifier = 1.0f32;
    for (&kind, &count) in &state.counts {
        let Some(entry) = kinds.get(Kind(kind)) else {
            continue;
        };
        for modifier in &entry.record.cost_modifiers {
            let matches = match target {
                PriceTarget::Object { kind_of, name } => {
                    !modifier.upgrade_discount && modifier.object_filter.passes(kind_of, name)
                }
                PriceTarget::Upgrade {
                    name,
                    cost_discount_template,
                } => {
                    if modifier.upgrade_discount {
                        modifier
                            .apply_to_these_upgrades
                            .iter()
                            .any(|upgrade| upgrade.eq_ignore_ascii_case(name))
                    } else {
                        cost_discount_template.is_some_and(|(kind_of, template)| {
                            modifier.object_filter.passes(kind_of, template)
                        })
                    }
                }
            };
            if !matches
                || !(modifier.starts_active
                    || cost_modifier_triggered(modifier, &state.held, upgrade_bits))
            {
                continue;
            }
            kind_of_modifier *= mint_sage_economy::kind_of_modifier_for(
                &modifier.percentages,
                count,
                CostModifierStacking::Replace,
            );
        }
    }
    CostModifiers {
        production_cost_change_percent,
        kind_of_modifier,
        handicap: 1.0,
        brutal_reduction: 0.0,
    }
}

fn cost_modifier_triggered(
    modifier: &mint_sage_modules::CostModifierRecord,
    held: &UpgradeMask,
    bits: &mint_sage_modules::UpgradeBits,
) -> bool {
    if modifier.triggered_by.is_empty() {
        return false;
    }
    let triggered_by = bits.mask(modifier.triggered_by.iter().map(String::as_str));
    let conflicts_with = bits.mask(modifier.conflicts_with.iter().map(String::as_str));
    if held.intersects(&conflicts_with) {
        return false;
    }
    if modifier.requires_all_triggers {
        held.contains_all(&triggered_by)
    } else {
        held.intersects(&triggered_by)
    }
}

pub(crate) fn advance_income(
    tick: &mut Tick<'_>,
    ids: EconomyIds,
    money_mult: f32,
) -> Result<Vec<(Entity, i64)>> {
    let mut deposits = Vec::new();
    let structures: Vec<Entity> = tick
        .state()
        .store(ids.income)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    for structure in structures {
        if tick.state().contains(ids.doomed, structure) {
            continue;
        }
        let Some(owner) = tick.state().get(ids.owner, structure).map(|owner| owner.0) else {
            continue;
        };
        let Some(player) = player_entity(tick.state(), ids, owner) else {
            continue;
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player)
            .map_or(Money::new(0), |held| held.0);
        let production = tick
            .state()
            .get(ids.bonuses, structure)
            .map_or(1.0, |bonuses| bonuses.0.production);
        let share = tick
            .state()
            .get(ids.claimant, structure)
            .filter(|claim| claim.scales_income)
            .and_then(|claim| claim.share);
        let Some(income) = tick.state_mut().get_mut(ids.income, structure) else {
            continue;
        };
        if let Some(share) = share {
            income.0.set_claim(share);
        }
        let paid = income.0.tick(&mut purse);
        if paid > 0 {
            let deposited = deposited(paid, money_mult, production);
            if deposited > paid {
                purse.deposit(deposited.saturating_sub(paid));
            } else if deposited < paid {
                purse.withdraw(paid.saturating_sub(deposited));
            }
            tick.state_mut().insert(ids.purse, player, Purse(purse))?;
            crate::match_stats::record(tick, ids.match_stats, player, |stats| {
                stats.earned = stats.earned.saturating_add(deposited);
            });
            deposits.push((structure, deposited));
        }
    }
    Ok(deposits)
}

pub(crate) fn deposited(paid: i64, money_mult: f32, production: f32) -> i64 {
    crate::experience::scaled_amount(paid, money_mult * production).unwrap_or(paid)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_production(
    tick: &mut Tick<'_>,
    ids: EconomyIds,
    frames_for: impl Fn(&World, PlayerId, u32) -> u32,
    time_for: impl Fn(&World, Entity, u32) -> Option<u32>,
    allowed: impl Fn(&World, PlayerId, u32) -> bool,
    fits: impl Fn(&World, PlayerId, u32) -> bool,
    mut spawn: impl FnMut(&mut Tick<'_>, u16, Vec2, u32, PlayerId, Entity) -> Result<Option<Entity>>,
    upgrade_time_for: impl Fn(&World, Entity, u32) -> Option<u32>,
    mut on_upgrade_complete: impl FnMut(&mut Tick<'_>, Entity, PlayerId, u32, i64) -> Result<()>,
) -> Result<()> {
    let structures: Vec<Entity> = tick
        .state()
        .store(ids.producer)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    for structure in structures {
        if tick.state().contains(ids.doomed, structure) {
            continue;
        }
        let Some(owner) = tick.state().get(ids.owner, structure).map(|owner| owner.0) else {
            continue;
        };
        let Some((id, item, kind, starting, revival_items)) = tick
            .state()
            .get(ids.producer, structure)
            .and_then(|producer| {
                let id = producer.queue.entry_to_process()?;
                let entry = producer
                    .queue
                    .entries()
                    .iter()
                    .find(|entry| entry.id == id)?;
                let revival_items: Vec<ItemId> = producer
                    .queue
                    .entries()
                    .iter()
                    .filter(|entry| matches!(entry.kind, EntryKind::Revival { .. }))
                    .map(|entry| entry.item)
                    .collect();
                Some((
                    entry.id,
                    entry.item,
                    entry.kind,
                    entry.frames_done == 0,
                    revival_items,
                ))
            })
        else {
            if let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) {
                producer.queue.decay_revival_cooldown();
            }
            continue;
        };
        let is_upgrade = matches!(kind, EntryKind::Upgrade);
        let frames = if is_upgrade {
            1
        } else {
            frames_for(tick.state(), owner, item.index())
        };
        if starting {
            let needed = if is_upgrade {
                upgrade_time_for(tick.state(), structure, item.index())
            } else {
                time_for(tick.state(), structure, item.index())
            };
            if let Some(needed) = needed {
                if let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) {
                    producer.queue.set_frames_needed(id, needed);
                }
            }
        }
        let allowed_now = allowed(tick.state(), owner, item.index());
        let mut fits_table: Vec<(ItemId, bool)> = revival_items
            .into_iter()
            .map(|revival_item| {
                (
                    revival_item,
                    fits(tick.state(), owner, revival_item.index()),
                )
            })
            .collect();
        if !fits_table.iter().any(|(candidate, _)| *candidate == item) {
            fits_table.push((item, fits(tick.state(), owner, item.index())));
        }
        let (progress, rally) = {
            let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) else {
                continue;
            };
            let progress = producer.queue.tick(frames, allowed_now, |candidate| {
                fits_table
                    .iter()
                    .find(|(candidate_item, _)| *candidate_item == candidate)
                    .is_none_or(|(_, fits)| *fits)
            });
            (progress, producer.rally)
        };
        if let Some(fraction) = tick
            .state()
            .get(ids.producer, structure)
            .and_then(|producer| producer.queue.entries().iter().find(|entry| entry.id == id))
            .map(|entry| entry.fraction())
        {
            diag!(
                "production: entity {} processing #{} ({}) at {:.1}%",
                structure.index(),
                id.get(),
                item.index(),
                fraction * 100.0
            );
        }
        let Progress::Ready { id, item } = progress else {
            continue;
        };
        if is_upgrade {
            let paid = tick
                .state()
                .get(ids.producer, structure)
                .and_then(|producer| producer.queue.entries().iter().find(|entry| entry.id == id))
                .map_or(0, |entry| entry.paid);
            if let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) {
                producer.queue.deliver(id);
            }
            on_upgrade_complete(tick, structure, owner, item.index(), paid)?;
            continue;
        }
        let Ok(buildable) = u16::try_from(item.index()) else {
            continue;
        };
        let paid = tick
            .state()
            .get(ids.producer, structure)
            .and_then(|producer| producer.queue.entries().iter().find(|entry| entry.id == id))
            .map_or(0, |entry| entry.paid);
        if let Some(made) = spawn(tick, buildable, rally, id.get(), owner, structure)? {
            tick.state_mut().insert(
                ids.object_value,
                made,
                ObjectValue {
                    paid_cost: i32::try_from(paid).unwrap_or(i32::MAX),
                    upgrade_spend: 0,
                },
            )?;
            if let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) {
                producer.queue.deliver(id);
            }
        }
    }
    Ok(())
}

pub(crate) fn recount_command_points(tick: &mut Tick<'_>, ids: EconomyIds) -> Result<()> {
    let players: Vec<(Entity, PlayerId)> = tick
        .state()
        .store(ids.slot)
        .map(|store| {
            store
                .iter()
                .map(|(entity, slot)| (entity, slot.0))
                .collect()
        })
        .unwrap_or_default();
    for (entity, player) in players {
        let mut used = 0i32;
        if let Some(store) = tick.state().store(ids.upkeep) {
            for (object, upkeep) in store.iter() {
                if tick.state().contains(ids.doomed, object) {
                    continue;
                }
                if tick
                    .state()
                    .get(ids.owner, object)
                    .is_some_and(|owner| owner.0 == player)
                {
                    used = used.saturating_add(upkeep.0);
                }
            }
        }
        if let Some(points) = tick.state_mut().get_mut(ids.points, entity) {
            points.0.set_used(used);
        }
    }
    Ok(())
}

pub(crate) fn exit_point(rally: Vec2, production_id: u32) -> Vec2 {
    let column = production_id % 6;
    let row = production_id / 6 % 6;
    #[allow(clippy::cast_precision_loss)]
    let (column, row) = (column as f32, row as f32);
    Vec2::new(
        rally.x + column * EXIT_SPACING,
        rally.y + row * EXIT_SPACING,
    )
}

const EXIT_SPACING: f32 = 25.0;

pub(crate) fn unit_frames_to_produce(
    world: &World,
    ids: EconomyIds,
    structure: Entity,
    seconds: f32,
    time: &mint_sage_economy::TimeModifiers,
) -> mint_sim::LogicFrames {
    frames_to_produce(
        world,
        ids,
        structure,
        mint_sage_economy::time_to_build(seconds, time),
    )
}

pub(crate) fn frames_to_produce(
    world: &World,
    ids: EconomyIds,
    structure: Entity,
    base: mint_sim::LogicFrames,
) -> mint_sim::LogicFrames {
    world.get(ids.bonuses, structure).map_or(base, |bonuses| {
        crate::experience::production_time(base, bonuses.0.production)
    })
}

fn simultaneous_count(
    world: &World,
    ids: EconomyIds,
    player: PlayerId,
    kind: Kind,
    item: ItemId,
) -> u32 {
    let owned = |entity| {
        world.is_live(entity)
            && !world.contains(ids.doomed, entity)
            && world
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player)
    };
    let mut count = 0u32;
    if let Some(store) = world.store(ids.kind) {
        for (entity, held) in store.iter() {
            if *held == kind && owned(entity) {
                count = count.saturating_add(1);
            }
        }
    }
    if let Some(store) = world.store(ids.producer) {
        for (entity, producer) in store.iter() {
            if !owned(entity) {
                continue;
            }
            let queued = producer
                .queue
                .entries()
                .iter()
                .filter(|entry| entry.makes(ItemTable::Objects, item))
                .count();
            count = count.saturating_add(u32::try_from(queued).unwrap_or(u32::MAX));
        }
    }
    count
}

pub(crate) type Gate<'a> = &'a dyn Fn(&World, Entity, u16, u8) -> Option<&'static str>;

pub(crate) struct QueueRules<'a> {
    pub(crate) menus: &'a [Vec<u16>],
    pub(crate) buildables: &'a [Buildable],
    pub(crate) gate: Gate<'a>,
    pub(crate) kinds: &'a KindTable,
    pub(crate) upgrade_bits: &'a mint_sage_modules::UpgradeBits,
    pub(crate) brutal_cost_reduction: f32,
}

pub(crate) fn queue_at(
    tick: &mut Tick<'_>,
    ids: EconomyIds,
    player: PlayerId,
    selected: &[Entity],
    button: u8,
    rules: QueueRules<'_>,
    unit_time: &mint_sage_economy::TimeModifiers,
    production_cost_change_percent: f32,
) -> Result<Vec<String>> {
    let QueueRules {
        menus,
        buildables,
        gate,
        kinds,
        upgrade_bits,
        brutal_cost_reduction,
    } = rules;
    let mut lines = Vec::new();
    let Some(player_entity) = player_entity(tick.state(), ids, player) else {
        return Ok(lines);
    };
    for structure in selected {
        let owned = tick
            .state()
            .get(ids.owner, *structure)
            .is_some_and(|owner| owner.0 == player);
        if !owned {
            continue;
        }
        let Some(menu) = tick
            .state()
            .get(ids.producer, *structure)
            .and_then(|producer| producer.menu)
        else {
            continue;
        };
        let offered = menus.get(usize::from(menu)).map_or(0, Vec::len);
        let Some(index) = menus
            .get(usize::from(menu))
            .and_then(|menu| menu.get(usize::from(button)))
            .copied()
        else {
            lines.push(format!(
                "production: entity {} has no button {} - it offers {offered}",
                structure.index(),
                u16::from(button).saturating_add(1)
            ));
            continue;
        };
        let Some(buildable) = buildables.get(usize::from(index)) else {
            continue;
        };
        if let Some(reason) = gate(tick.state(), *structure, menu, button) {
            lines.push(format!(
                "production: {} refused at entity {} - {reason}",
                buildable.label,
                structure.index()
            ));
            continue;
        }
        if let Some(limit) = kinds
            .get(buildable.kind)
            .and_then(|entry| entry.record.max_simultaneous_of_type)
            .and_then(crate::templates::whole)
            .and_then(|limit| u32::try_from(limit).ok())
            .filter(|limit| *limit > 0)
        {
            let standing = simultaneous_count(
                tick.state(),
                ids,
                player,
                buildable.kind,
                ItemId::new(u32::from(index)),
            );
            if standing >= limit {
                lines.push(format!(
                    "production: {} refused at entity {} - {standing} of {limit} already, \
                     MaxSimultaneousOfType",
                    buildable.label,
                    structure.index()
                ));
                continue;
            }
        }
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(Money::new(0), |held| held.0);
        let build_time = unit_frames_to_produce(
            tick.state(),
            ids,
            *structure,
            buildable.build_seconds,
            unit_time,
        );
        let cost = kinds.get(buildable.kind).map_or(buildable.cost, |entry| {
            if entry.record.has_kind("BUILD_FOR_FREE") {
                return 0;
            }
            mint_sage_economy::cost_to_build(
                buildable.cost,
                &CostModifiers {
                    brutal_reduction: brutal_cost_reduction,
                    ..cost_modifiers_for(
                        tick.state(),
                        ids,
                        kinds,
                        upgrade_bits,
                        player,
                        &PriceTarget::Object {
                            kind_of: &entry.record.kind_of,
                            name: &entry.record.name,
                        },
                        production_cost_change_percent,
                    )
                },
            )
        });
        if purse.can_afford(cost) {
            let free = points_of(tick.state(), ids, player).free();
            let army_of_dead = kinds
                .get(buildable.kind)
                .is_some_and(|entry| entry.record.has_kind("ARMY_OF_DEAD"));
            if !crate::heroes::command_points_fit(buildable.command_points, free, army_of_dead) {
                lines.push(format!(
                    "production: {} refused at entity {} - GUI:ErrorNoMoreCommandPoints \
                     (needs {}, {free} free)",
                    buildable.label,
                    structure.index(),
                    buildable.command_points
                ));
                continue;
            }
        }
        let request = QueueRequest {
            item: ItemId::new(u32::from(index)),
            cost,
            build_time,
            quantity: 1,
            command_points: buildable.command_points,
            kind: EntryKind::Unit {
                builder: buildable.builder,
            },
        };
        let Some(producer) = tick.state_mut().get_mut(ids.producer, *structure) else {
            continue;
        };
        match producer.queue.queue(request, &mut purse) {
            Ok(id) => {
                let depth = producer.queue.entries().len();
                tick.state_mut()
                    .insert(ids.purse, player_entity, Purse(purse))?;
                crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
                    stats.spend(crate::match_stats::SpendKind::Units, cost);
                });
                lines.push(format!(
                    "production: queued {} (#{}) at entity {} for {cost} - {depth} in the queue, {} left",
                    buildable.label,
                    id.get(),
                    structure.index(),
                    purse.amount()
                ));
            }
            Err(refusal) => lines.push(format!(
                "production: {} refused at entity {} - {refusal}",
                buildable.label,
                structure.index()
            )),
        }
    }
    Ok(lines)
}

pub(crate) fn cancel_at(
    tick: &mut Tick<'_>,
    ids: EconomyIds,
    player: PlayerId,
    selected: &[Entity],
    pick: &dyn Fn(&Producer) -> Option<ProductionId>,
    label: &dyn Fn(u32) -> String,
) -> Result<(Vec<String>, Vec<(Entity, u32)>)> {
    let mut lines = Vec::new();
    let mut cancelled_items = Vec::new();
    let Some(player_entity) = player_entity(tick.state(), ids, player) else {
        return Ok((lines, cancelled_items));
    };
    for structure in selected {
        let owned = tick
            .state()
            .get(ids.owner, *structure)
            .is_some_and(|owner| owner.0 == player);
        if !owned {
            continue;
        }
        let Some(entry) = tick.state().get(ids.producer, *structure).and_then(pick) else {
            continue;
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(Money::new(0), |held| held.0);
        let Some(producer) = tick.state_mut().get_mut(ids.producer, *structure) else {
            continue;
        };
        let hero = producer
            .queue
            .entries()
            .iter()
            .any(|held| held.id == entry && matches!(held.kind, EntryKind::Revival { .. }));
        let Some(cancelled) = producer.queue.cancel(entry, &mut purse) else {
            continue;
        };
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        let category = if hero {
            crate::match_stats::SpendKind::Heroes
        } else {
            crate::match_stats::SpendKind::Units
        };
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.refund(category, cancelled.refunded);
        });
        cancelled_items.push((*structure, cancelled.item.index()));
        lines.push(format!(
            "production: cancelled {} (#{}) at entity {} - {} refunded, {} held",
            label(cancelled.item.index()),
            entry.get(),
            structure.index(),
            cancelled.refunded,
            purse.amount()
        ));
    }
    Ok((lines, cancelled_items))
}

pub(crate) type SubCounted<'a> = &'a dyn Fn(&World, Option<Entity>, UpgradeBit) -> bool;

#[allow(clippy::too_many_arguments)]
pub(crate) fn upgrade_check<'u>(
    world: &World,
    ids: EconomyIds,
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    player: PlayerId,
    structure: Entity,
    button: u8,
    is_player_upgrade: bool,
    menus: &[Vec<u16>],
    upgrades: &'u [UpgradeBuildable],
    gate: Gate<'_>,
    production_cost_change_percent: f32,
    brutal_cost_reduction: f32,
    sub_counted: SubCounted<'_>,
) -> core::result::Result<(u16, i64, &'u UpgradeBuildable), String> {
    let player_entity = player_entity(world, ids, player).ok_or("no player")?;
    let owned = world
        .get(ids.owner, structure)
        .is_some_and(|owner| owner.0 == player);
    if !owned {
        return Err("not the player's".to_owned());
    }
    let menu = world
        .get(ids.producer, structure)
        .and_then(|producer| {
            if is_player_upgrade {
                producer.player_menu
            } else {
                producer.object_menu
            }
        })
        .ok_or("offers no upgrade menu")?;
    let offered = menus.get(usize::from(menu)).map_or(0, Vec::len);
    let index = menus
        .get(usize::from(menu))
        .and_then(|menu| menu.get(usize::from(button)))
        .copied()
        .ok_or_else(|| {
            format!(
                "has no button {} - it offers {offered}",
                u16::from(button).saturating_add(1)
            )
        })?;
    let upgrade = upgrades.get(usize::from(index)).ok_or("no such upgrade")?;
    if is_player_upgrade {
        if world
            .get(ids.player_upgrades, player_entity)
            .is_some_and(|held| held.0.contains(upgrade.bit))
        {
            return Err(format!("already researched (player {})", player.get()));
        }
        if world
            .get(ids.player_upgrades_in_progress, player_entity)
            .is_some_and(|queued| queued.0.contains(&upgrade.bit))
        {
            return Err(format!(
                "already in production for player {} at another structure",
                player.get()
            ));
        }
    } else {
        if world
            .get(ids.object_upgrades, structure)
            .is_some_and(|held| held.0.contains(upgrade.bit))
        {
            return Err("already holds this upgrade".to_owned());
        }
        if world.get(ids.producer, structure).is_some_and(|producer| {
            producer.queue.entries().iter().any(|entry| {
                matches!(entry.kind, EntryKind::Upgrade)
                    && entry.item == ItemId::new(u32::from(index))
            })
        }) {
            return Err("is already researching this upgrade".to_owned());
        }
    }
    if let Some(reason) = gate(world, structure, menu, button) {
        return Err(reason.to_owned());
    }
    let cost = upgrade_cost(
        world,
        ids,
        kinds,
        upgrade_bits,
        player,
        upgrade,
        Some(structure),
        production_cost_change_percent,
        brutal_cost_reduction,
        sub_counted,
    );
    let purse = world
        .get(ids.purse, player_entity)
        .map_or(Money::new(0), |held| held.0);
    if !purse.can_afford(cost) {
        return Err(format!("{cost} is more than the purse holds"));
    }
    Ok((index, cost, upgrade))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn upgrade_cost(
    world: &World,
    ids: EconomyIds,
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    player: PlayerId,
    upgrade: &UpgradeBuildable,
    builder: Option<Entity>,
    production_cost_change_percent: f32,
    brutal_cost_reduction: f32,
    sub_counted: SubCounted<'_>,
) -> i64 {
    let cost_discount_template = upgrade
        .cost_discount_template
        .as_deref()
        .and_then(|name| kinds.kind(name))
        .and_then(|kind| kinds.get(kind))
        .map(|entry| (entry.record.kind_of.as_slice(), entry.record.name.as_str()));
    let own_cost = if upgrade.subs.is_empty() {
        upgrade.cost
    } else {
        upgrade
            .subs
            .iter()
            .filter(|sub| sub_counted(world, builder, sub.bit))
            .fold(0_i64, |sum, sub| sum.saturating_add(sub.cost))
    };
    mint_sage_economy::cost_to_build(
        own_cost,
        &CostModifiers {
            brutal_reduction: brutal_cost_reduction,
            ..cost_modifiers_for(
                world,
                ids,
                kinds,
                upgrade_bits,
                player,
                &PriceTarget::Upgrade {
                    name: &upgrade.name,
                    cost_discount_template,
                },
                production_cost_change_percent,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_upgrade_at(
    tick: &mut Tick<'_>,
    ids: EconomyIds,
    kinds: &KindTable,
    upgrade_bits: &mint_sage_modules::UpgradeBits,
    player: PlayerId,
    selected: &[Entity],
    button: u8,
    is_player_upgrade: bool,
    menus: &[Vec<u16>],
    upgrades: &[UpgradeBuildable],
    gate: Gate<'_>,
    production_cost_change_percent: f32,
    brutal_cost_reduction: f32,
    sub_counted: SubCounted<'_>,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let Some(player_entity) = player_entity(tick.state(), ids, player) else {
        return Ok(lines);
    };
    for structure in selected {
        let has_menu = tick
            .state()
            .get(ids.producer, *structure)
            .is_some_and(|producer| {
                if is_player_upgrade {
                    producer.player_menu.is_some()
                } else {
                    producer.object_menu.is_some()
                }
            });
        let owned = tick
            .state()
            .get(ids.owner, *structure)
            .is_some_and(|owner| owner.0 == player);
        if !has_menu || !owned {
            continue;
        }
        let (index, cost, upgrade) = match upgrade_check(
            tick.state(),
            ids,
            kinds,
            upgrade_bits,
            player,
            *structure,
            button,
            is_player_upgrade,
            menus,
            upgrades,
            gate,
            production_cost_change_percent,
            brutal_cost_reduction,
            sub_counted,
        ) {
            Ok((index, cost, upgrade)) => (index, cost, upgrade.clone()),
            Err(reason) => {
                lines.push(format!(
                    "upgrade: refused at entity {} - {reason}",
                    structure.index()
                ));
                continue;
            }
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(Money::new(0), |held| held.0);
        let request = QueueRequest {
            item: ItemId::new(u32::from(index)),
            cost,
            build_time: upgrade.build_time,
            quantity: 1,
            command_points: 0,
            kind: EntryKind::Upgrade,
        };
        let Some(producer) = tick.state_mut().get_mut(ids.producer, *structure) else {
            continue;
        };
        match producer.queue.queue(request, &mut purse) {
            Ok(id) => {
                tick.state_mut()
                    .insert(ids.purse, player_entity, Purse(purse))?;
                if !is_player_upgrade {
                    let spent = i32::try_from(cost).unwrap_or(i32::MAX);
                    let mut value = tick
                        .state()
                        .get(ids.object_value, *structure)
                        .copied()
                        .unwrap_or_default();
                    value.upgrade_spend = value.upgrade_spend.saturating_add(spent);
                    if tick
                        .state()
                        .get(ids.kind, *structure)
                        .is_some_and(|kind| stores_upgrade_price(kinds, upgrade_bits, *kind))
                    {
                        value.paid_cost = spent;
                    }
                    tick.state_mut()
                        .insert(ids.object_value, *structure, value)?;
                }
                if is_player_upgrade {
                    let mut in_progress = tick
                        .state()
                        .get(ids.player_upgrades_in_progress, player_entity)
                        .cloned()
                        .unwrap_or_default();
                    in_progress.0.push(upgrade.bit);
                    tick.state_mut().insert(
                        ids.player_upgrades_in_progress,
                        player_entity,
                        in_progress,
                    )?;
                }
                lines.push(format!(
                    "upgrade: queued (#{}) at entity {} for {cost} - {} left",
                    id.get(),
                    structure.index(),
                    purse.amount()
                ));
            }
            Err(refusal) => lines.push(format!(
                "upgrade: refused at entity {} - {refusal}",
                structure.index()
            )),
        }
    }
    Ok(lines)
}

pub(crate) fn census(world: &World, ids: EconomyIds) -> Vec<String> {
    let Some(store) = world.store(ids.slot) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for (_, slot) in store.iter() {
        let purse = purse_of(world, ids, slot.0);
        let points = points_of(world, ids, slot.0);
        let mut queued = 0usize;
        let mut building = String::from("nothing");
        if let Some(producers) = world.store(ids.producer) {
            for (structure, producer) in producers.iter() {
                if world
                    .get(ids.owner, structure)
                    .is_some_and(|owner| owner.0 == slot.0)
                {
                    queued = queued.saturating_add(producer.queue.entries().len());
                    if let Some(front) = producer.queue.entries().first() {
                        building = format!(
                            "item {} at {:.0}%",
                            front.item.index(),
                            front.fraction() * 100.0
                        );
                    }
                }
            }
        }
        lines.push(format!(
            "economy: player {} holds {purse} resource(s), {points} command point(s), \
             {queued} queued, building {building}",
            slot.0.get()
        ));
    }
    lines
}

pub(crate) const fn alignment_of(player: PlayerId) -> Alignment {
    if player.get() == 0 {
        Alignment::Good
    } else {
        Alignment::Evil
    }
}
