#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod command_points;
pub mod cost;
pub mod income;
pub mod money;
pub mod production;

pub use command_points::{
    Alignment, CommandPointPool, EVIL_COMMAND_POINT_LIMIT, GOOD_COMMAND_POINT_LIMIT,
};
pub use cost::{
    cost_to_build, kind_of_modifier_for, reduce_frames, time_to_build, upgrade_time_to_build,
    CostModifierStacking, CostModifiers, TimeModifiers,
};
pub use income::AutoDeposit;
pub use money::{Money, DEFAULT_STARTING_CASH, REFUND_PERCENT};
pub use production::{
    Cancellation, Delivery, EntryKind, ItemId, ItemTable, ProductionEntry, ProductionId,
    ProductionQueue, Progress, QueueRequest, Refusal, MAX_QUEUE_ENTRIES_CAP,
};

pub type Result<T> = core::result::Result<T, Refusal>;

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::{LogicFrames, StateHasher};

    #[test]
    fn a_structure_earns_queues_and_produces_and_does_it_identically_twice() {
        fn run() -> (u64, Vec<u32>) {
            let mut purse = Money::new(300);
            let mut income = AutoDeposit::new(LogicFrames::from_engine_milliseconds(1_000), 40);
            let mut queue = ProductionQueue::new(5);
            let pool = CommandPointPool::for_alignment(Alignment::Good);
            let cost = cost_to_build(500, &CostModifiers::default());
            let build = time_to_build(6.0, &TimeModifiers::default());
            let mut produced = Vec::new();

            for frame in 0..600u32 {
                income.tick(&mut purse);
                let request = QueueRequest {
                    item: ItemId::new(3),
                    cost,
                    build_time: build,
                    quantity: 1,
                    command_points: 60,
                    kind: EntryKind::Unit { builder: false },
                };
                let _ = queue.queue(request, &mut purse);
                if let Progress::Ready { .. } = queue.advance() {
                    if let Some(delivery) = queue.deliver_one() {
                        assert_eq!(delivery.item, ItemId::new(3));
                        produced.push(frame);
                    }
                }
            }

            let mut hasher = StateHasher::new();
            purse.hash_into(&mut hasher);
            income.hash_into(&mut hasher);
            queue.hash_into(&mut hasher);
            pool.hash_into(&mut hasher);
            (hasher.finish(), produced)
        }

        let (hash_a, produced_a) = run();
        let (hash_b, produced_b) = run();
        assert_eq!(
            hash_a, hash_b,
            "the same sequence produced a different state"
        );
        assert_eq!(produced_a, produced_b);
        assert!(
            !produced_a.is_empty(),
            "600 frames of income should have produced something"
        );
    }

    #[test]
    fn a_player_can_never_spend_more_than_it_has_over_a_long_run() {
        let mut purse = Money::new(1_000);
        let mut queue = ProductionQueue::new(6);
        let mut ids = Vec::new();
        let mut spent = 0i64;

        for step in 0..500u32 {
            let request = QueueRequest {
                item: ItemId::new(step % 4),
                cost: i64::from(step % 7) * 90,
                build_time: LogicFrames::from_frames(3),
                quantity: 1,
                command_points: 0,
                kind: EntryKind::Unit { builder: false },
            };
            let before = purse.amount();
            if let Ok(id) = queue.queue(request, &mut purse) {
                spent = spent.saturating_add(before.saturating_sub(purse.amount()));
                ids.push(id);
            }
            assert!(purse.amount() >= 0);
            if step % 5 == 0 {
                if let Some(id) = ids.pop() {
                    if let Some(cancelled) = queue.cancel(id, &mut purse) {
                        spent = spent.saturating_sub(cancelled.refunded);
                    }
                }
            }
            if let Progress::Ready { .. } = queue.advance() {
                queue.deliver_one();
            }
        }

        let outstanding: i64 = queue
            .entries()
            .iter()
            .fold(0i64, |sum, entry| sum.saturating_add(entry.paid));
        assert!(purse.amount() >= 0);
        assert!(
            spent >= outstanding,
            "the books do not balance: spent {spent}, still queued {outstanding}"
        );
    }
}
