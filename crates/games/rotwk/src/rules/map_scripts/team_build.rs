use std::collections::BTreeMap;

use mint_sage_map::UnitSlot;
use mint_sim::{Entity, PlayerId, StateHasher};

use super::team::TeamId;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::unit::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkOrder {
    pub(crate) kind: Kind,
    pub(crate) factory: Option<Entity>,
    pub(crate) completed: i32,
    pub(crate) required: i32,
    pub(crate) required_flag: bool,
    pub(crate) optional: bool,
    pub(crate) xp_level: i32,
    pub(crate) upgrades: Vec<String>,
}

impl WorkOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.kind.0);
        hasher.write_bool(self.factory.is_some());
        if let Some(factory) = self.factory {
            factory.hash_into(hasher);
        }
        hasher.write_i32(self.completed);
        hasher.write_i32(self.required);
        hasher.write_bool(self.required_flag);
        hasher.write_bool(self.optional);
        hasher.write_i32(self.xp_level);
        hasher.write_usize(self.upgrades.len());
        for upgrade in &self.upgrades {
            hasher.write_bytes(upgrade.as_bytes());
            hasher.write_u8(0);
        }
    }
}

pub(crate) fn orders_for(
    slots: &[UnitSlot],
    resolve: impl Fn(&str) -> Option<Kind>,
) -> Vec<WorkOrder> {
    let order = |kind, count, required, slot: &UnitSlot| WorkOrder {
        kind,
        factory: None,
        completed: 0,
        required: count,
        required_flag: required,
        optional: !required,
        xp_level: slot.experience_level,
        upgrades: slot.upgrades.clone(),
    };
    let mut orders = Vec::new();
    for slot in slots {
        let spare = slot.max.saturating_sub(slot.min);
        if let (Some(kind), true) = (resolve(&slot.template), spare > 0) {
            orders.insert(0, order(kind, spare, false, slot));
        }
    }
    for slot in slots {
        if let (Some(kind), true) = (resolve(&slot.template), slot.min > 0) {
            orders.insert(0, order(kind, slot.min, true, slot));
        }
    }
    orders
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueuedTeam {
    pub(crate) team: TeamId,
    pub(crate) orders: Vec<WorkOrder>,
    pub(crate) priority: bool,
    pub(crate) started: u64,
    pub(crate) sent_to_start: bool,
    pub(crate) reinforce: Option<Entity>,
}

impl QueuedTeam {
    pub(crate) fn build_time_expired(&self, idle_seconds: i32, frame: u64) -> bool {
        if idle_seconds < 1 {
            return false;
        }
        let idle = u64::from(idle_seconds.unsigned_abs()) * u64::from(RETAIL_LOGIC_HZ);
        frame > self.started.saturating_add(idle)
    }

    pub(crate) fn minimum_built(&self) -> bool {
        self.orders
            .iter()
            .filter(|order| order.required_flag)
            .all(|order| {
                let running = i32::from(order.factory.is_some());
                order.completed.saturating_add(running) >= order.required
            })
    }

    pub(crate) fn builds_complete(&self) -> bool {
        self.orders.iter().all(|order| order.factory.is_none())
    }

    pub(crate) fn is_all_built(&self) -> bool {
        let required_done = self
            .orders
            .iter()
            .filter(|order| order.required_flag)
            .all(|order| order.completed >= order.required);
        let any = self.orders.iter().any(|order| order.completed > 0);
        required_done && any
    }

    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.team.get());
        hasher.write_bool(self.priority);
        hasher.write_u64(self.started);
        hasher.write_bool(self.sent_to_start);
        hasher.write_bool(self.reinforce.is_some());
        if let Some(unit) = self.reinforce {
            unit.hash_into(hasher);
        }
        hasher.write_usize(self.orders.len());
        for order in &self.orders {
            order.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeamBuilder {
    pub(crate) player: PlayerId,
    pub(crate) ready: bool,
    pub(crate) team_timer: i32,
    pub(crate) team_seconds: i32,
    pub(crate) team_delay: i32,
    pub(crate) build: Vec<QueuedTeam>,
    pub(crate) ready_queue: Vec<QueuedTeam>,
}

impl TeamBuilder {
    pub(crate) fn new(player: PlayerId, team_seconds: i32) -> Self {
        Self {
            player,
            ready: false,
            team_timer: 2,
            team_seconds,
            team_delay: 0,
            build: Vec::new(),
            ready_queue: Vec::new(),
        }
    }

    pub(crate) fn enqueue(&mut self, entry: QueuedTeam) {
        if entry.priority {
            self.build.insert(0, entry);
        } else {
            self.build.push(entry);
        }
    }

    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.player.get());
        hasher.write_bool(self.ready);
        hasher.write_i32(self.team_timer);
        hasher.write_i32(self.team_seconds);
        hasher.write_i32(self.team_delay);
        for queue in [&self.build, &self.ready_queue] {
            hasher.write_usize(queue.len());
            for entry in queue {
                entry.hash_into(hasher);
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProtoRuntime {
    pub(crate) always_false: bool,
    pub(crate) script: Option<usize>,
    pub(crate) next_frame: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotBonus {
    pub(crate) level: i32,
    pub(crate) upgrades: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Builders {
    list: Vec<TeamBuilder>,
    protos: BTreeMap<usize, ProtoRuntime>,
    bonuses: BTreeMap<Entity, SlotBonus>,
}

impl Builders {
    pub(crate) fn seeded(players: impl Iterator<Item = PlayerId>, team_seconds: i32) -> Self {
        Self {
            list: players
                .map(|player| TeamBuilder::new(player, team_seconds))
                .collect(),
            protos: BTreeMap::new(),
            bonuses: BTreeMap::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub(crate) fn players(&self) -> Vec<PlayerId> {
        self.list.iter().map(|builder| builder.player).collect()
    }

    pub(crate) fn get(&self, player: PlayerId) -> Option<&TeamBuilder> {
        self.list.iter().find(|builder| builder.player == player)
    }

    pub(crate) fn get_mut(&mut self, player: PlayerId) -> Option<&mut TeamBuilder> {
        self.list
            .iter_mut()
            .find(|builder| builder.player == player)
    }

    pub(crate) fn proto(&self, proto: usize) -> ProtoRuntime {
        self.protos.get(&proto).cloned().unwrap_or_default()
    }

    pub(crate) fn proto_mut(&mut self, proto: usize) -> &mut ProtoRuntime {
        self.protos.entry(proto).or_default()
    }

    pub(crate) fn remember_slot(&mut self, unit: Entity, order: &WorkOrder) {
        if order.xp_level > 1 || !order.upgrades.is_empty() {
            self.bonuses.insert(
                unit,
                SlotBonus {
                    level: order.xp_level,
                    upgrades: order.upgrades.clone(),
                },
            );
        }
    }

    pub(crate) fn bonus(&self, unit: Entity) -> Option<&SlotBonus> {
        self.bonuses.get(&unit)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.bonuses.retain(|&unit, _| is_live(unit));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.list.is_empty() {
            return;
        }
        hasher.write_usize(self.list.len());
        for builder in &self.list {
            builder.hash_into(hasher);
        }
        hasher.write_usize(self.protos.len());
        for (proto, runtime) in &self.protos {
            hasher.write_usize(*proto);
            hasher.write_bool(runtime.always_false);
            hasher.write_bool(runtime.script.is_some());
            hasher.write_usize(runtime.script.unwrap_or(0));
            hasher.write_u64(runtime.next_frame);
        }
        if !self.bonuses.is_empty() {
            hasher.write_usize(self.bonuses.len());
            for (unit, bonus) in &self.bonuses {
                unit.hash_into(hasher);
                hasher.write_i32(bonus.level);
                hasher.write_usize(bonus.upgrades.len());
                for upgrade in &bonus.upgrades {
                    hasher.write_bytes(upgrade.as_bytes());
                    hasher.write_u8(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(min: i32, max: i32, template: &str) -> UnitSlot {
        UnitSlot {
            min,
            max,
            experience_level: 1,
            upgrades: Vec::new(),
            template: template.to_owned(),
        }
    }

    fn resolve(name: &str) -> Option<Kind> {
        match name {
            "A" => Some(Kind(1)),
            "B" => Some(Kind(2)),
            _ => None,
        }
    }

    fn order(required: bool, completed: i32, required_count: i32, running: bool) -> WorkOrder {
        WorkOrder {
            kind: Kind(1),
            factory: running.then(|| Entity::from_parts(0, 0)),
            completed,
            required: required_count,
            required_flag: required,
            optional: !required,
            xp_level: 0,
            upgrades: Vec::new(),
        }
    }

    fn queued(orders: Vec<WorkOrder>) -> QueuedTeam {
        QueuedTeam {
            team: TeamId::new(1),
            orders,
            priority: false,
            started: 10,
            sent_to_start: false,
            reinforce: None,
        }
    }

    #[test]
    fn orders_list_required_slots_then_optional_slots_each_in_reverse_slot_order() {
        let slots = [slot(2, 3, "A"), slot(1, 1, "B"), slot(0, 2, "A")];
        let orders = orders_for(&slots, resolve);
        let shape: Vec<(u16, i32, bool)> = orders
            .iter()
            .map(|o| (o.kind.0, o.required, o.required_flag))
            .collect();
        assert_eq!(
            shape,
            vec![(2, 1, true), (1, 2, true), (1, 2, false), (1, 1, false)]
        );
    }

    #[test]
    fn a_slot_with_no_minimum_makes_no_required_order_and_an_unknown_template_none() {
        let orders = orders_for(&[slot(0, 1, "A"), slot(1, 1, "Nope")], resolve);
        assert_eq!(orders.len(), 1);
        assert!(orders.iter().all(|o| o.optional));
    }

    #[test]
    fn a_team_expires_only_after_its_idle_time_has_passed() {
        let team = queued(Vec::new());
        assert!(!team.build_time_expired(0, 10_000));
        assert!(!team.build_time_expired(2, 20));
        assert!(team.build_time_expired(2, 21));
    }

    #[test]
    fn a_running_factory_counts_toward_the_minimum_but_not_toward_complete() {
        let team = queued(vec![order(true, 1, 2, true)]);
        assert!(team.minimum_built());
        assert!(!team.builds_complete());
        let short = queued(vec![order(true, 0, 2, true)]);
        assert!(!short.minimum_built());
    }

    #[test]
    fn all_built_needs_one_unit_in_even_when_nothing_is_required() {
        assert!(!queued(vec![order(false, 0, 2, false)]).is_all_built());
        assert!(queued(vec![order(false, 1, 2, false)]).is_all_built());
        assert!(!queued(vec![order(true, 1, 2, false), order(false, 1, 1, false)]).is_all_built());
    }

    #[test]
    fn a_priority_entry_goes_to_the_head_and_the_rest_to_the_tail() {
        let mut builder = TeamBuilder::new(PlayerId::new(2), 10);
        let mut first = queued(Vec::new());
        first.team = TeamId::new(1);
        let mut second = queued(Vec::new());
        second.team = TeamId::new(2);
        let mut urgent = queued(Vec::new());
        urgent.team = TeamId::new(3);
        urgent.priority = true;
        builder.enqueue(first);
        builder.enqueue(second);
        builder.enqueue(urgent);
        let ids: Vec<u32> = builder.build.iter().map(|e| e.team.get()).collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }
}
