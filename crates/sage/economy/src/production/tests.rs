use super::*;

fn request(cost: i64, frames: u32) -> QueueRequest {
    QueueRequest {
        item: ItemId::new(7),
        cost,
        build_time: LogicFrames::from_frames(frames),
        quantity: 1,
        command_points: 0,
        kind: EntryKind::Unit { builder: false },
    }
}

fn always_fits(_: ItemId) -> bool {
    true
}

#[test]
fn queueing_charges_the_full_cost_immediately() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(5);
    queue.queue(request(400, 30), &mut purse).unwrap();
    assert_eq!(purse.amount(), 600);
    for _ in 0..29 {
        queue.advance();
    }
    assert_eq!(purse.amount(), 600, "nothing further may be charged");
}

#[test]
fn a_player_who_cannot_pay_is_refused_and_charged_nothing() {
    let mut purse = Money::new(100);
    let mut queue = ProductionQueue::new(5);
    let refusal = queue.queue(request(400, 30), &mut purse);
    assert!(matches!(
        refusal,
        Err(Refusal::CannotAfford {
            cost: 400,
            held: 100
        })
    ));
    assert_eq!(
        purse.amount(),
        100,
        "the clamping withdraw must not part-pay"
    );
    assert!(queue.is_empty());
}

#[test]
fn a_full_queue_rejects_rather_than_dropping_and_charges_nothing() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(2);
    queue.queue(request(100, 30), &mut purse).unwrap();
    queue.queue(request(100, 30), &mut purse).unwrap();
    assert_eq!(purse.amount(), 9_800);
    assert!(matches!(
        queue.queue(request(100, 30), &mut purse),
        Err(Refusal::QueueFull { limit: 2 })
    ));
    assert_eq!(purse.amount(), 9_800);
    assert_eq!(queue.entries().len(), 2);
}

#[test]
fn only_the_front_entry_advances() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    queue.queue(request(10, 10), &mut purse).unwrap();
    queue.queue(request(10, 10), &mut purse).unwrap();
    for _ in 0..5 {
        queue.advance();
    }
    assert_eq!(queue.entries().first().map(|e| e.frames_done), Some(5));
    assert_eq!(queue.entries().get(1).map(|e| e.frames_done), Some(0));
}

#[test]
fn the_first_and_last_entry_of_an_item_are_found() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let other = QueueRequest {
        item: ItemId::new(9),
        ..request(10, 5)
    };
    let first = queue.queue(request(10, 5), &mut purse).unwrap();
    queue.queue(other, &mut purse).unwrap();
    let last = queue.queue(request(10, 5), &mut purse).unwrap();
    let objects = ItemTable::Objects;
    assert_eq!(
        queue.first_of(objects, ItemId::new(7)).map(|e| e.id),
        Some(first)
    );
    assert_eq!(
        queue.last_of(objects, ItemId::new(7)).map(|e| e.id),
        Some(last)
    );
    assert!(queue.last_of(objects, ItemId::new(3)).is_none());
}

#[test]
fn an_upgrade_is_not_found_as_the_unit_sharing_its_index() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let upgrade = QueueRequest {
        kind: EntryKind::Upgrade,
        ..request(10, 5)
    };
    let id = queue.queue(upgrade, &mut purse).unwrap();
    assert!(queue.first_of(ItemTable::Objects, ItemId::new(7)).is_none());
    assert!(queue.last_of(ItemTable::Objects, ItemId::new(7)).is_none());
    assert_eq!(
        queue
            .first_of(ItemTable::Upgrades, ItemId::new(7))
            .map(|e| e.id),
        Some(id)
    );
}

#[test]
fn an_entry_completes_on_its_declared_frame_and_not_before() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(2);
    let id = queue.queue(request(10, 4), &mut purse).unwrap();
    for frame in 1..4 {
        assert!(
            matches!(queue.advance(), Progress::Building { .. }),
            "finished early on frame {frame}"
        );
    }
    assert_eq!(
        queue.advance(),
        Progress::Ready {
            id,
            item: ItemId::new(7)
        }
    );
}

#[test]
fn a_blocked_exit_keeps_overbuilding_rather_than_losing_the_unit() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(2);
    let id = queue.queue(request(10, 2), &mut purse).unwrap();
    queue.advance();
    assert!(matches!(queue.advance(), Progress::Ready { .. }));
    for _ in 0..10 {
        assert_eq!(
            queue.advance(),
            Progress::Ready {
                id,
                item: ItemId::new(7)
            },
            "the ready entry was lost while the exit was blocked"
        );
    }
    assert_eq!(queue.entries().len(), 1);
    let delivery = queue.deliver_one().unwrap();
    assert!(delivery.entry_finished);
    assert!(queue.is_empty());
}

#[test]
fn a_quantity_modifier_produces_n_units_for_one_payment() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(2);
    let batch = QueueRequest {
        quantity: 4,
        ..request(200, 2)
    };
    queue.queue(batch, &mut purse).unwrap();
    assert_eq!(purse.amount(), 800);
    queue.advance();
    queue.advance();
    for remaining in (0..4u32).rev() {
        assert!(matches!(queue.advance(), Progress::Ready { .. }));
        let delivery = queue.deliver_one().unwrap();
        assert_eq!(delivery.entry_finished, remaining == 0);
    }
    assert!(queue.is_empty());
    assert_eq!(purse.amount(), 800, "one payment, four units");
}

#[test]
fn a_zero_quantity_is_read_as_one() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(2);
    queue
        .queue(
            QueueRequest {
                quantity: 0,
                ..request(10, 1)
            },
            &mut purse,
        )
        .unwrap();
    assert_eq!(queue.entries().first().map(|e| e.quantity_total), Some(1));
}

#[test]
fn cancelling_refunds_the_full_amount_that_was_paid() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(3);
    let id = queue.queue(request(400, 60), &mut purse).unwrap();
    queue.advance();
    queue.advance();
    let cancelled = queue.cancel(id, &mut purse).unwrap();
    assert_eq!(cancelled.refunded, 400);
    assert_eq!(purse.amount(), 1_000);
    assert!(queue.is_empty());
}

#[test]
fn cancelling_an_id_that_is_not_there_changes_nothing() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(3);
    queue.queue(request(400, 60), &mut purse).unwrap();
    assert!(queue
        .cancel(ProductionId::from_raw(999), &mut purse)
        .is_none());
    assert_eq!(purse.amount(), 600);
    assert_eq!(queue.entries().len(), 1);
}

#[test]
fn cancelling_from_the_middle_keeps_the_others_in_order() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(5);
    let first = queue.queue(request(100, 60), &mut purse).unwrap();
    let second = queue.queue(request(200, 60), &mut purse).unwrap();
    let third = queue.queue(request(300, 60), &mut purse).unwrap();
    queue.cancel(second, &mut purse).unwrap();
    let ids: Vec<_> = queue.entries().iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![first, third]);
    assert_eq!(purse.amount(), 10_000 - 100 - 300);
}

#[test]
fn twenty_hordes_queue_whatever_their_command_points_and_the_twenty_first_is_refused() {
    let mut purse = Money::new(1_000_000);
    let mut queue = ProductionQueue::new(20);
    let horde = QueueRequest {
        command_points: 60,
        ..request(10, 30)
    };
    for _ in 0..20 {
        queue.queue(horde, &mut purse).unwrap();
    }
    assert_eq!(queue.reserved_command_points(), 1_200);
    assert!(matches!(
        queue.queue(horde, &mut purse),
        Err(Refusal::QueueFull { limit: 20 })
    ));
}

#[test]
fn a_destroyed_factory_gives_every_queued_payment_back() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(5);
    queue.queue(request(100, 60), &mut purse).unwrap();
    queue.queue(request(250, 60), &mut purse).unwrap();
    assert_eq!(queue.cancel_all(&mut purse), 350);
    assert_eq!(purse.amount(), 10_000);
    assert!(queue.is_empty());
}

#[test]
fn a_declared_max_of_zero_becomes_one_rather_than_an_unusable_structure() {
    assert_eq!(ProductionQueue::new(0).max_entries(), 1);
    assert_eq!(
        ProductionQueue::new(u32::MAX).max_entries(),
        MAX_QUEUE_ENTRIES_CAP
    );
}

#[test]
fn the_hash_moves_with_the_queues_order_and_its_progress() {
    let mut purse = Money::new(10_000);
    let mut a = ProductionQueue::new(4);
    let mut b = ProductionQueue::new(4);
    let one = QueueRequest {
        item: ItemId::new(1),
        ..request(100, 60)
    };
    let two = QueueRequest {
        item: ItemId::new(2),
        ..request(100, 60)
    };
    a.queue(one, &mut purse).unwrap();
    a.queue(two, &mut purse).unwrap();
    b.queue(two, &mut purse).unwrap();
    b.queue(one, &mut purse).unwrap();

    let mut hasher_a = StateHasher::new();
    let mut hasher_b = StateHasher::new();
    a.hash_into(&mut hasher_a);
    b.hash_into(&mut hasher_b);
    assert_ne!(hasher_a, hasher_b, "the queue's order must reach the hash");

    let mut before = StateHasher::new();
    a.hash_into(&mut before);
    a.advance();
    let mut after = StateHasher::new();
    a.hash_into(&mut after);
    assert_ne!(before, after, "one frame of progress must reach the hash");
}

#[test]
fn the_progress_fraction_is_derived_and_never_leaves_zero_to_one() {
    let mut purse = Money::new(1_000);
    let mut queue = ProductionQueue::new(2);
    queue.queue(request(10, 4), &mut purse).unwrap();
    assert_eq!(
        queue.entries().first().map(ProductionEntry::fraction),
        Some(0.0)
    );
    for _ in 0..10 {
        queue.advance();
    }
    let fraction = queue.entries().first().map(ProductionEntry::fraction);
    assert_eq!(fraction, Some(1.0), "overbuilding must not exceed 100%");

    let mut instant = ProductionQueue::new(2);
    instant.queue(request(0, 0), &mut purse).unwrap();
    assert!(matches!(instant.advance(), Progress::Ready { .. }));
}

#[test]
fn builder_preempts_upgrade_and_upgrade_keeps_progress() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let upgrade = QueueRequest {
        kind: EntryKind::Upgrade,
        ..request(10, 100)
    };
    let upgrade_id = queue.queue(upgrade, &mut purse).unwrap();
    for _ in 0..40 {
        queue.tick(1, true, always_fits);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == upgrade_id)
            .map(|e| e.frames_done),
        Some(40)
    );
    let porter = QueueRequest {
        item: ItemId::new(2),
        kind: EntryKind::Unit { builder: true },
        ..request(10, 10)
    };
    let porter_id = queue.queue(porter, &mut purse).unwrap();
    for _ in 0..5 {
        queue.tick(1, true, always_fits);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == upgrade_id)
            .map(|e| e.frames_done),
        Some(40),
        "the upgrade's frames must stay frozen while the porter builds"
    );
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == porter_id)
            .map(|e| e.frames_done),
        Some(5)
    );
}

#[test]
fn revival_behind_unit_finishes_in_parallel() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let unit = request(10, 100);
    queue.queue(unit, &mut purse).unwrap();
    let hero = QueueRequest {
        item: ItemId::new(2),
        kind: EntryKind::Revival { builder: false },
        ..request(0, 30)
    };
    let hero_id = queue.queue(hero, &mut purse).unwrap();
    for _ in 0..30 {
        queue.tick(1, true, always_fits);
    }
    let hero_entry = queue.entries().iter().find(|e| e.id == hero_id).unwrap();
    assert!(
        hero_entry.is_ready(),
        "the hero's clock must have advanced every frame regardless of selection"
    );
}

#[test]
fn revival_clock_freezes_without_command_points() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let hero = QueueRequest {
        item: ItemId::new(2),
        kind: EntryKind::Revival { builder: false },
        ..request(0, 30)
    };
    let hero_id = queue.queue(hero, &mut purse).unwrap();
    for _ in 0..10 {
        queue.tick(1, true, |_| false);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == hero_id)
            .map(|e| e.frames_done),
        Some(0),
        "no command-point room must freeze the clock"
    );
    for _ in 0..40 {
        queue.tick(1, true, always_fits);
    }
    assert!(matches!(
        queue.tick(1, true, always_fits),
        Progress::Ready { id, .. } if id == hero_id
    ));
}

#[test]
fn unready_revival_at_head_blocks_upgrade() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let hero = QueueRequest {
        item: ItemId::new(2),
        kind: EntryKind::Revival { builder: false },
        ..request(0, 30)
    };
    queue.queue(hero, &mut purse).unwrap();
    let upgrade = QueueRequest {
        kind: EntryKind::Upgrade,
        ..request(10, 50)
    };
    let upgrade_id = queue.queue(upgrade, &mut purse).unwrap();
    for _ in 0..29 {
        queue.tick(1, true, always_fits);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == upgrade_id)
            .map(|e| e.frames_done),
        Some(0),
        "the unready hero at the head must block the upgrade behind it"
    );
}

#[test]
fn delivering_batch_beats_builder() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let batch = QueueRequest {
        quantity: 3,
        ..request(300, 10)
    };
    let batch_id = queue.queue(batch, &mut purse).unwrap();
    for _ in 0..10 {
        queue.tick(1, true, always_fits);
    }
    queue.deliver(batch_id);
    let porter = QueueRequest {
        item: ItemId::new(2),
        kind: EntryKind::Unit { builder: true },
        ..request(10, 10)
    };
    queue.queue(porter, &mut purse).unwrap();
    assert_eq!(
        queue.entry_to_process(),
        Some(batch_id),
        "a batch already delivering is chosen over a porter"
    );
}

#[test]
fn one_cap_counts_units_upgrades_and_revivals() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(3);
    queue.queue(request(10, 10), &mut purse).unwrap();
    let upgrade = QueueRequest {
        kind: EntryKind::Upgrade,
        ..request(10, 10)
    };
    queue.queue(upgrade, &mut purse).unwrap();
    let hero = QueueRequest {
        kind: EntryKind::Revival { builder: false },
        ..request(10, 10)
    };
    queue.queue(hero, &mut purse).unwrap();
    assert!(matches!(
        queue.queue(request(10, 10), &mut purse),
        Err(Refusal::QueueFull { limit: 3 })
    ));
}

#[test]
fn misfit_unit_stalls_in_place() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(2);
    let id = queue.queue(request(10, 100), &mut purse).unwrap();
    for _ in 0..50 {
        queue.tick(1, true, always_fits);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.frames_done),
        Some(50)
    );
    for _ in 0..10 {
        queue.tick(1, true, |_| false);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.frames_done),
        Some(50),
        "a misfit entry must not be cancelled or lose progress"
    );
    for _ in 0..50 {
        queue.tick(1, true, always_fits);
    }
    assert_eq!(
        queue
            .entries()
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.frames_done),
        Some(100)
    );
}

#[test]
fn second_ready_revival_waits_for_cooldown() {
    let mut purse = Money::new(10_000);
    let mut queue = ProductionQueue::new(4);
    let hero = QueueRequest {
        kind: EntryKind::Revival { builder: false },
        ..request(0, 1)
    };
    let first = queue.queue(hero, &mut purse).unwrap();
    let second = queue.queue(hero, &mut purse).unwrap();
    queue.tick_revivals(always_fits);
    assert!(matches!(
        queue.tick(1, true, always_fits),
        Progress::Ready { id, .. } if id == first
    ));
    queue.deliver(first);
    queue.set_revival_cooldown(20);
    for call in 0..19 {
        assert!(
            matches!(
                queue.tick(5, true, always_fits),
                Progress::Building { id, .. } if id == second
            ),
            "the second hero completed after only {call} of 20 cooldown ticks"
        );
    }
    assert!(matches!(
        queue.tick(5, true, always_fits),
        Progress::Ready { id, .. } if id == second
    ));
}
