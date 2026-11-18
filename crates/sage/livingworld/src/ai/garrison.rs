use crate::ai::data::TYPES;
use crate::ai::{Ctx, MoveKind, MoveOrder};
use crate::model::{ArmyId, RegionId};

const GOALS: i32 = 6;

fn is_ally(ctx: &Ctx<'_>, owner: Option<crate::model::PlayerId>) -> bool {
    ctx.state.allied_owner(owner, ctx.me)
}

fn goal(ctx: &Ctx<'_>, from: RegionId, k: i32) -> Option<RegionId> {
    let snap = ctx.snap;
    let plots =
        |r: &crate::ai::snapshot::RegionRec| ctx.state.region(r.id).map_or(0, |s| s.plots.len());
    let allied = |r: &crate::ai::snapshot::RegionRec| is_ally(ctx, r.owner);
    match k {
        0 => snap
            .nearest(from, false, |a| a.hero && a.player == ctx.me)
            .map(|(a, _)| a.region),
        1 => snap.nearest_region(from, |r| r.owner.is_some() && !allied(r)),
        2 => snap.nearest_region(from, |r| allied(r) && plots(r) == 1),
        3 => snap.nearest_region(from, |r| {
            allied(r)
                && ctx
                    .state
                    .rules
                    .region_rules(r.id)
                    .is_some_and(|x| x.create_auto_fort)
        }),
        4 => snap.nearest_region(from, |r| r.owner == Some(ctx.me) && !r.upgrade.is_empty()),
        5 => snap
            .nearest(from, false, |a| a.hero && !is_ally(ctx, Some(a.player)))
            .map(|(a, _)| a.region),
        _ => snap.nearest_region(from, |r| allied(r) && plots(r) == 3),
    }
}

struct Feed {
    hero: ArmyId,
    members: usize,
    by_type: [usize; TYPES],
}

pub(crate) fn plan(ctx: &mut Ctx<'_>, moves: &mut Vec<MoveOrder>) {
    let Some(summary) = ctx.snap.summary(ctx.me) else {
        return;
    };
    let snap = ctx.snap;
    for id in &summary.heroless {
        let Some(rec) = snap.army(*id) else { continue };
        let mut feeds: Vec<Feed> = summary
            .hero_armies
            .iter()
            .filter_map(|h| snap.army(*h))
            .filter(|h| snap.dist(rec.region, h.region).is_some_and(|d| d <= 1))
            .map(|h| {
                let mut by_type = [0usize; TYPES];
                for (t, _) in &h.members {
                    if let Some(slot) = by_type.get_mut(*t) {
                        *slot = slot.saturating_add(1);
                    }
                }
                Feed {
                    hero: h.id,
                    members: h.members.len(),
                    by_type,
                }
            })
            .collect();
        if !feeds.is_empty() {
            let enemy_far = snap
                .nearest(rec.region, false, |a| {
                    a.hero && !is_ally(ctx, Some(a.player))
                })
                .is_none_or(|(_, d)| d > 3);
            for t in 0..TYPES {
                for (_, member) in rec.members.iter().filter(|(mt, _)| *mt == t) {
                    let index = if enemy_far {
                        (0..feeds.len())
                            .min_by_key(|i| {
                                feeds.get(*i).map_or((usize::MAX, usize::MAX), |f| {
                                    (f.members, f.by_type.get(t).copied().unwrap_or(0))
                                })
                            })
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let Some(feed) = feeds.get_mut(index) else {
                        continue;
                    };
                    feed.members = feed.members.saturating_add(1);
                    if let Some(slot) = feed.by_type.get_mut(t) {
                        *slot = slot.saturating_add(1);
                    }
                    moves.push(MoveOrder {
                        army: *id,
                        heroless: true,
                        kind: MoveKind::Transfer {
                            to: feed.hero,
                            member: *member,
                        },
                    });
                }
            }
            continue;
        }
        let k = ctx.rolls.int(0, GOALS);
        let Some(target) = goal(ctx, rec.region, k) else {
            continue;
        };
        let here = snap.dist_or_far(rec.region, target);
        let step = snap
            .ring(rec.region, 1)
            .iter()
            .copied()
            .find(|n| is_ally(ctx, ctx.state.owner(*n)) && snap.dist_or_far(*n, target) < here);
        if let Some(region) = step {
            moves.push(MoveOrder {
                army: *id,
                heroless: true,
                kind: MoveKind::Move { region },
            });
        }
    }
}
