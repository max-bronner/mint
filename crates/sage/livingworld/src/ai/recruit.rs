use crate::ai::catalogue::Spawner;
use crate::ai::data::{SpawnEntry, HERO, RATIOS, TYPES};
use crate::ai::snapshot::{ArmyRec, Built, RegionRec, FAR};
use crate::ai::Ctx;
use crate::model::{BuildingId, RegionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecruitOrder {
    pub plot: u8,
    pub region: RegionId,
    pub building: BuildingId,
    pub entry: usize,
}

struct Recruiter {
    deficit: [i32; TYPES],
    budget: i32,
    hero_hired: bool,
    hero_count: i32,
    orders: Vec<RecruitOrder>,
}

fn get(a: &[i32; TYPES], t: usize) -> i32 {
    a.get(t).copied().unwrap_or(0)
}

fn bump(a: &mut [i32; TYPES], t: usize, by: i32) {
    if let Some(slot) = a.get_mut(t) {
        *slot = slot.saturating_add(by);
    }
}

pub(crate) fn weights_within(
    ctx: &Ctx<'_>,
    from: RegionId,
    lo: u16,
    hi: u16,
    keep: impl Fn(&ArmyRec) -> bool,
) -> [i32; TYPES] {
    let mut out = [0; TYPES];
    for a in ctx.snap.armies().filter(|a| keep(a)) {
        if ctx
            .snap
            .dist(from, a.region)
            .is_some_and(|d| d >= lo && d <= hi)
        {
            for t in 0..TYPES {
                bump(&mut out, t, a.weight(t));
            }
        }
    }
    out
}

fn entries<'a>(ctx: &'a Ctx<'_>, building: BuildingId) -> &'a [SpawnEntry] {
    ctx.state
        .rules
        .ai
        .building(building)
        .and_then(|b| b.spawn.as_deref())
        .unwrap_or(&[])
}

fn entry_cost(ctx: &Ctx<'_>, building: BuildingId, entry: usize) -> i32 {
    let Some(first) = entries(ctx, building)
        .get(entry)
        .and_then(|e| e.members.first())
    else {
        return 0;
    };
    ctx.state.unit_price(ctx.me, first.unit)
}

fn first_type(ctx: &Ctx<'_>, building: BuildingId, entry: usize) -> usize {
    entries(ctx, building)
        .get(entry)
        .and_then(|e| e.members.first())
        .and_then(|m| ctx.state.rules.unit(m.unit).unit_type)
        .map_or(TYPES - 1, usize::from)
}

pub(crate) fn recruitable(ctx: &Ctx<'_>, building: BuildingId, entry: usize) -> bool {
    let Some(spawn) = entries(ctx, building).get(entry) else {
        return false;
    };
    let first = spawn.members.first().map(|m| m.unit);
    let hero = spawn
        .hero
        .or_else(|| first.filter(|u| ctx.state.rules.unit(*u).hero));
    hero.is_none_or(|h| ctx.state.hero_recruitable(ctx.me, h))
}

pub(crate) fn pick(
    ctx: &mut Ctx<'_>,
    building: BuildingId,
    spawner: &Spawner,
    t: usize,
) -> Option<usize> {
    let list = spawner.by_type.get(t)?;
    if t != HERO {
        if list.is_empty() {
            return None;
        }
        return list.get(ctx.rolls.pick(list.len())).copied();
    }
    let mut candidates: Vec<usize> = Vec::new();
    for &e in list {
        let hero = entries(ctx, building).get(e).and_then(|x| x.hero);
        if !recruitable(ctx, building, e) {
            continue;
        }
        let weight = if hero.is_some() { 10 } else { 1 };
        for _ in 0..weight {
            candidates.push(e);
        }
    }
    if candidates.is_empty() {
        return spawner.by_type.iter().find_map(|l| l.first()).copied();
    }
    candidates.get(ctx.rolls.pick(candidates.len())).copied()
}

fn pick_counter(ctx: &mut Ctx<'_>, spawner: &Spawner, t: usize) -> Option<usize> {
    let list = spawner.counters.get(t)?;
    if list.is_empty() {
        return None;
    }
    ctx.rolls.int(0, 0);
    list.get(ctx.rolls.pick(list.len())).copied()
}

fn buildings_of(rec: &RegionRec) -> impl Iterator<Item = &Built> {
    rec.spawn_only.iter().chain(&rec.spawn_other)
}

impl Recruiter {
    fn queue(&mut self, rec: &RegionRec, b: &Built, entry: usize, cost: i32) {
        self.budget = self.budget.saturating_sub(cost);
        self.orders.push(RecruitOrder {
            region: rec.id,
            building: b.building,
            plot: b.plot,
            entry,
        });
    }

    fn fill(&mut self, ctx: &mut Ctx<'_>, rec: &RegionRec) {
        for b in buildings_of(rec) {
            let Some(spawner) = ctx.catalogue.spawners.get(&b.building) else {
                continue;
            };
            let mut best = -100;
            let mut kind = None;
            for t in 0..TYPES {
                let can = spawner.by_type.get(t).is_some_and(|l| !l.is_empty());
                if can && get(&self.deficit, t) > best {
                    best = get(&self.deficit, t);
                    kind = Some(t);
                }
            }
            let Some(t) = kind.filter(|_| best < 10) else {
                continue;
            };
            let Some(entry) = pick(ctx, b.building, spawner, t) else {
                continue;
            };
            let cost = entry_cost(ctx, b.building, entry);
            if cost >= self.budget {
                continue;
            }
            if !recruitable(ctx, b.building, entry) {
                if t == HERO {
                    return;
                }
                continue;
            }
            if t == HERO {
                if self.hero_hired {
                    return;
                }
                self.hero_hired = true;
            }
            self.queue(rec, b, entry, cost);
            bump(&mut self.deficit, t, -1);
        }
    }

    fn hire_hero(&mut self, ctx: &mut Ctx<'_>, rec: &RegionRec) -> bool {
        let found = buildings_of(rec).find_map(|b| {
            let s = ctx.catalogue.spawners.get(&b.building)?;
            s.by_type
                .get(HERO)
                .is_some_and(|l| !l.is_empty())
                .then_some((b, s))
        });
        let Some((b, spawner)) = found else {
            return false;
        };
        let Some(entry) = pick(ctx, b.building, spawner, HERO) else {
            return false;
        };
        if self.hero_hired || !recruitable(ctx, b.building, entry) {
            return false;
        }
        let cost = entry_cost(ctx, b.building, entry);
        if cost < self.budget {
            self.hero_hired = true;
            self.queue(rec, b, entry, cost);
            self.hero_count = self.hero_count.saturating_add(1);
            bump(&mut self.deficit, HERO, -1);
            return true;
        }
        let top = ctx.difficulty.saturating_sub(2);
        if ctx.rolls.int(0, i32::from(top)) != 0 {
            self.budget = 0;
        }
        false
    }

    fn counter(&mut self, ctx: &mut Ctx<'_>, rec: &RegionRec, threat: &[i32; TYPES]) -> bool {
        let own = weights_within(ctx, rec.id, 1, 3, |a| a.player == ctx.me);
        let mut order: Vec<usize> = (0..TYPES).filter(|t| get(threat, *t) > 0).collect();
        order.sort_by_key(|t| std::cmp::Reverse(get(threat, *t)));
        let mut queued = false;
        for b in buildings_of(rec) {
            let Some(spawner) = ctx.catalogue.spawners.get(&b.building) else {
                continue;
            };
            for &t in &order {
                let Some(entry) = pick_counter(ctx, spawner, t) else {
                    continue;
                };
                let u = first_type(ctx, b.building, entry);
                let cost = entry_cost(ctx, b.building, entry);
                if get(&own, u) > get(threat, t)
                    || cost >= self.budget
                    || !recruitable(ctx, b.building, entry)
                {
                    continue;
                }
                if u == HERO {
                    if self.hero_hired {
                        continue;
                    }
                    self.hero_hired = true;
                }
                self.queue(rec, b, entry, cost);
                bump(&mut self.deficit, u, -1);
                queued = true;
                break;
            }
        }
        queued
    }
}

fn region_score(ctx: &Ctx<'_>, rec: &RegionRec, own_heroes: usize) -> i32 {
    let near = |keep: &dyn Fn(&ArmyRec) -> bool| {
        ctx.snap
            .nearest(rec.id, false, |a| a.hero && keep(a))
            .map_or(FAR, |(_, d)| d)
    };
    let allied = near(&|a| ctx.state.allied(ctx.me, a.player));
    let enemy = near(&|a| !ctx.state.allied(ctx.me, a.player));
    let mut score = 0i32;
    if allied < 3 {
        score = score.saturating_add(10);
    }
    if enemy < 3 {
        score = score.saturating_add(10);
    }
    score = score.saturating_add(
        i32::try_from(rec.spawn_only.len())
            .unwrap_or(0)
            .saturating_mul(5),
    );
    if !rec.strengthen.is_empty() && own_heroes < 4 {
        let near = 3i32.saturating_sub(i32::from(enemy)).max(0);
        score = score.saturating_add(20i32.saturating_sub(near.saturating_mul(5)));
    }
    score
}

pub(crate) fn plan(ctx: &mut Ctx<'_>, budget: i32) -> (Vec<RecruitOrder>, i32) {
    let sum: i32 = ctx.template.ratios.iter().sum();
    let mut ratio = [0f32; TYPES];
    if sum != 0 {
        for (slot, r) in ratio
            .iter_mut()
            .zip(ctx.template.ratios.iter().take(RATIOS))
        {
            *slot = *r as f32 / sum as f32;
        }
    }
    let mut count = [0i32; TYPES];
    for a in ctx.snap.armies().filter(|a| a.player == ctx.me) {
        for t in 0..TYPES {
            bump(&mut count, t, a.weight(t));
        }
    }
    let total = count.iter().fold(0i32, |s, v| s.saturating_add(*v));
    let mut deficit = [0i32; TYPES];
    for t in 0..TYPES {
        let want = trunc(total as f32 * ratio.get(t).copied().unwrap_or(0.0)).max(1);
        if let Some(slot) = deficit.get_mut(t) {
            *slot = want.saturating_sub(get(&count, t)).max(0);
        }
    }
    let summary = ctx.snap.summary(ctx.me).cloned().unwrap_or_default();
    let own_heroes = i32::try_from(summary.hero_armies.len()).unwrap_or(i32::MAX);
    let mut regions: Vec<&RegionRec> = summary
        .regions
        .iter()
        .filter_map(|r| ctx.snap.region(*r))
        .collect();
    if ctx.difficulty > 3 {
        let scores: Vec<i32> = regions
            .iter()
            .map(|r| region_score(ctx, r, summary.hero_armies.len()))
            .collect();
        let mut scores = scores;
        for i in 0..regions.len() {
            for j in i.saturating_add(1)..regions.len() {
                if scores.get(j) > scores.get(i) {
                    regions.swap(i, j);
                    scores.swap(i, j);
                }
            }
        }
    }
    let mut rec = Recruiter {
        deficit,
        budget,
        hero_hired: false,
        hero_count: own_heroes,
        orders: Vec::new(),
    };
    for region in regions {
        if region.spawn_only.is_empty() && region.spawn_other.is_empty() {
            continue;
        }
        let threat = weights_within(ctx, region.id, 1, 2, |a| a.player != ctx.me);
        if ctx.difficulty > 2
            && !region.strengthen.is_empty()
            && rec.hero_count < 4
            && rec.hire_hero(ctx, region)
        {
            break;
        }
        let total_threat = threat.iter().fold(0i32, |s, v| s.saturating_add(*v));
        if ctx.difficulty > 2 && total_threat > 0 && rec.counter(ctx, region, &threat) {
            continue;
        }
        rec.fill(ctx, region);
    }
    (rec.orders, rec.budget)
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn trunc(v: f32) -> i32 {
    v as i32
}
