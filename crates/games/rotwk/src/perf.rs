use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub(crate) const PASS_NAMES: [&str; 70] = [
    "clock",
    "step_map_scripts",
    "step_team_builders",
    "step_castle_unpack",
    "end_ai_busy",
    "replay_deferred_orders",
    "ai",
    "path_budgets",
    "gates",
    "run_burning",
    "check_builder_arrivals",
    "movers",
    "coast_while_waiting",
    "hold_still",
    "run_enemy_contact",
    "run_cell_changes",
    "fly_giant_birds",
    "contain",
    "grid",
    "step_disabled",
    "members",
    "advance_knockback",
    "advance_floods",
    "player_overrides",
    "visibility",
    "advance_stealth",
    "advance_detectors",
    "advance_pick_ups",
    "advance_attach",
    "advance_guards",
    "projectiles",
    "weapons",
    "crush",
    "tick_rampage",
    "run_topple_pass",
    "run_aod_crush_pass",
    "modules",
    "run_crate_pickups",
    "step_terrain_fire",
    "step_crush_notices",
    "step_emotions",
    "step_animals",
    "step_repulsors",
    "step_repulsed",
    "step_portal_fades",
    "step_critter_emitters",
    "step_amphibious",
    "step_siege",
    "step_group_bonuses",
    "step_formation_lists",
    "settle_path_requests",
    "damage",
    "pending_impacts",
    "tick_fire",
    "note_attackers",
    "step_engaged",
    "step_hit_reactions",
    "deaths",
    "victory",
    "corpses",
    "hordes",
    "construction",
    "advance_harvesting",
    "advance_tree_motion",
    "cancel_unaffected_object_upgrades",
    "economy",
    "advance_match_stats",
    "modifiers",
    "stand_on_terrain",
    "wading",
];

pub(crate) const fn position(names: &[&str], name: &str) -> usize {
    const fn same(a: &[u8], b: &[u8]) -> bool {
        match (a, b) {
            ([x, a @ ..], [y, b @ ..]) => *x == *y && same(a, b),
            ([], []) => true,
            _ => false,
        }
    }
    match names {
        [first, rest @ ..] => {
            if same(first.as_bytes(), name.as_bytes()) {
                0
            } else {
                position(rest, name).saturating_add(1)
            }
        }
        [] => usize::MAX,
    }
}

macro_rules! row {
    ($name:literal) => {
        const {
            let index = crate::perf::position(&crate::perf::PASS_NAMES, $name);
            assert!(index < crate::perf::PASS_NAMES.len(), "no such perf row");
            index
        }
    };
}
pub(crate) use row;

pub(crate) const SUB_NAMES: [&str; 65] = [
    "advance_group_attacks",
    "horde_melee",
    "ranged_horde_spot",
    "armed_loop",
    "member_attack_victim",
    "unit_cells",
    "unit_cost_view",
    "body_cells",
    "path_plan",
    "los_search_goal",
    "guard_scan",
    "unit_index_sync",
    "unit_index_query",
    "amoeba",
    "decide_guards",
    "decide_group_attacks",
    "decide_armed",
    "fill_unit_cells",
    "melee_destination",
    "member_contact",
    "adjacent_enemy",
    "acquire_scan",
    "fire_shot",
    "area_victims",
    "projectile_collide",
    "living_walk",
    "continued_victim",
    "adjust_victim",
    "armed_choose",
    "armed_engage",
    "fill_bodies",
    "acquire_for",
    "friendly_fire",
    "choose_bird_ground",
    "choose_order",
    "choose_idle_scan",
    "choose_reacquire",
    "choose_sticky_common",
    "engage_no_target",
    "engage_uncohere",
    "engage_weapon_choice",
    "engage_after_firing",
    "engage_reach",
    "engage_close",
    "engage_settle",
    "engage_aim",
    "engage_fire",
    "path_expanded",
    "member_reach",
    "member_replan",
    "engage_aim_weapon",
    "engage_range_gate",
    "engage_approach",
    "path_site_attack",
    "path_site_move",
    "path_site_rescue",
    "path_site_contact",
    "unit_cost_limits",
    "death_phase_updates",
    "remove_corpses",
    "reap_portals",
    "reap_obstacles",
    "reap_walls",
    "reap_goal_cells",
    "settle_claims",
];

macro_rules! sub_row {
    ($name:literal) => {
        const {
            let index = crate::perf::position(&crate::perf::SUB_NAMES, $name);
            assert!(index < crate::perf::SUB_NAMES.len(), "no such perf sub-row");
            index
        }
    };
}
pub(crate) use sub_row;

pub(crate) struct SubTimer {
    row: usize,
    started: Option<Instant>,
}

pub(crate) fn sub(row: usize) -> SubTimer {
    SubTimer {
        row,
        started: enabled().then(Instant::now),
    }
}

pub(crate) fn count(row: usize, n: usize) {
    if enabled() {
        SUB_TICK.with(|cells| {
            if let Some((_, calls)) = cells.borrow_mut().get_mut(row) {
                *calls = calls.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
            }
        });
    }
}

impl Drop for SubTimer {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = started.elapsed();
            SUB_TICK.with(|cells| {
                if let Some((time, calls)) = cells.borrow_mut().get_mut(self.row) {
                    *time = time.saturating_add(elapsed);
                    *calls = calls.saturating_add(1);
                }
            });
        }
    }
}

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("MINT_ROTWK_PROFILE").is_ok())
}

fn clock_probe_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| enabled() && std::env::var("MINT_ROTWK_PROFILE_CLOCK").is_ok())
}

const CLOCK_PROBE_STEPS: u32 = 160_000;

fn clock_probe_us() -> f64 {
    let started = Instant::now();
    let mut x = std::hint::black_box(0x9E37_79B9_7F4A_7C15_u64);
    for _ in 0..CLOCK_PROBE_STEPS {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
    }
    std::hint::black_box(x);
    started.elapsed().as_secs_f64() * 1e6
}

fn spike_threshold_ms() -> f64 {
    static THRESHOLD: OnceLock<f64> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("MINT_ROTWK_PROFILE_SPIKE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15.0)
    })
}

thread_local! {
    static PLAN_CALLS: Cell<u32> = const { Cell::new(0) };
    static ARMED_COUNT: Cell<u32> = const { Cell::new(0) };
    static LIVING_COUNT: Cell<u32> = const { Cell::new(0) };
    static PROJECTILE_COUNT: Cell<u32> = const { Cell::new(0) };
    static STRAGGLER_COUNT: Cell<u32> = const { Cell::new(0) };
    static APPLY: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static STEP_END: Cell<Option<Instant>> = const { Cell::new(None) };
    static SAMPLES: RefCell<Samples> = RefCell::new(Samples::new());
    static SUB_TICK: RefCell<[(Duration, u32); SUB_NAMES.len()]> =
        const { RefCell::new([(Duration::ZERO, 0); SUB_NAMES.len()]) };
}

struct Samples {
    ticks: u64,
    per_pass_ms: [Vec<f64>; PASS_NAMES.len()],
    per_sub: [Vec<(f64, u32)>; SUB_NAMES.len()],
    total_ms: Vec<f64>,
    step_whole_ms: Vec<f64>,
    apply_ms: Vec<f64>,
    hash_ms: Vec<f64>,
    counts: Vec<(u32, u32, u32, u32, u32)>,
    clock_probe_us: Vec<f64>,
}

impl Samples {
    fn new() -> Self {
        Self {
            ticks: 0,
            per_pass_ms: std::array::from_fn(|_| Vec::new()),
            per_sub: std::array::from_fn(|_| Vec::new()),
            total_ms: Vec::new(),
            step_whole_ms: Vec::new(),
            apply_ms: Vec::new(),
            hash_ms: Vec::new(),
            counts: Vec::new(),
            clock_probe_us: Vec::new(),
        }
    }
}

pub(crate) fn note_plan_call() {
    if enabled() {
        PLAN_CALLS.with(|c| c.set(c.get() + 1));
    }
}

pub(crate) fn note_combat_counts(armed: usize, living: usize) {
    if enabled() {
        ARMED_COUNT.with(|c| c.set(u32::try_from(armed).unwrap_or(u32::MAX)));
        LIVING_COUNT.with(|c| c.set(u32::try_from(living).unwrap_or(u32::MAX)));
    }
}

pub(crate) fn note_projectile_spawned() {
    if enabled() {
        PROJECTILE_COUNT.with(|c| c.set(c.get() + 1));
    }
}

pub(crate) fn note_stragglers(count: u32) {
    if enabled() {
        STRAGGLER_COUNT.with(|c| c.set(count));
    }
}

fn take_counts() -> (u32, u32, u32, u32, u32) {
    let plan = PLAN_CALLS.with(|c| c.replace(0));
    let armed = ARMED_COUNT.with(Cell::get);
    let living = LIVING_COUNT.with(Cell::get);
    let projectiles = PROJECTILE_COUNT.with(|c| c.replace(0));
    let stragglers = STRAGGLER_COUNT.with(|c| c.replace(0));
    (plan, armed, living, projectiles, stragglers)
}

pub(crate) struct TickProfile {
    frame: u64,
    durations: [Duration; PASS_NAMES.len()],
    probe_us: Option<f64>,
}

impl TickProfile {
    pub(crate) fn new(frame: u64) -> Self {
        Self {
            frame,
            durations: [Duration::ZERO; PASS_NAMES.len()],
            probe_us: clock_probe_enabled().then(clock_probe_us),
        }
    }

    pub(crate) fn record(&mut self, pass: usize, elapsed: Duration) {
        if let Some(slot) = self.durations.get_mut(pass) {
            *slot = slot.saturating_add(elapsed);
        }
    }

    pub(crate) fn finish(self, step_whole: Duration) {
        if let Some(before) = self.probe_us {
            let probe = (before + clock_probe_us()) / 2.0;
            SAMPLES.with(|samples| samples.borrow_mut().clock_probe_us.push(probe));
        }
        let (plan_calls, armed, living, projectiles, stragglers) = take_counts();
        let total: Duration = self.durations.iter().sum();
        let total_ms = total.as_secs_f64() * 1000.0;
        let step_whole_ms = step_whole.as_secs_f64() * 1000.0;
        let apply_ms = APPLY
            .with(|apply| apply.replace(Duration::ZERO))
            .as_secs_f64()
            * 1000.0;
        SAMPLES.with(|samples| {
            let mut samples = samples.borrow_mut();
            samples.ticks += 1;
            samples.total_ms.push(total_ms);
            samples.step_whole_ms.push(step_whole_ms);
            samples.apply_ms.push(apply_ms);
            samples
                .counts
                .push((plan_calls, armed, living, projectiles, stragglers));
            for (index, duration) in self.durations.iter().enumerate() {
                if let Some(bucket) = samples.per_pass_ms.get_mut(index) {
                    bucket.push(duration.as_secs_f64() * 1000.0);
                }
            }
            let subs = SUB_TICK.with(|cells| {
                std::mem::replace(
                    &mut *cells.borrow_mut(),
                    [(Duration::ZERO, 0); SUB_NAMES.len()],
                )
            });
            for (bucket, (time, calls)) in samples.per_sub.iter_mut().zip(subs) {
                bucket.push((time.as_secs_f64() * 1000.0, calls));
            }
        });
        if total_ms > spike_threshold_ms() {
            let worst = self
                .durations
                .iter()
                .enumerate()
                .max_by_key(|(_, duration)| **duration)
                .map_or((0, Duration::ZERO), |(index, duration)| (index, *duration));
            println!(
                "perf: spike frame={} total_ms={total_ms:.2} step_whole_ms={step_whole_ms:.2} worst={}={:.2}ms plan_calls={plan_calls} armed={armed} living={living} projectiles={projectiles} stragglers={stragglers}",
                self.frame,
                PASS_NAMES.get(worst.0).copied().unwrap_or("?"),
                worst.1.as_secs_f64() * 1000.0,
            );
        }
        STEP_END.with(|end| end.set(Some(Instant::now())));
    }
}

pub(crate) fn note_apply(elapsed: Duration) {
    APPLY.with(|apply| apply.set(apply.get().saturating_add(elapsed)));
}

pub(crate) fn note_tick_returned() {
    if !enabled() {
        return;
    }
    if let Some(end) = STEP_END.with(Cell::take) {
        let ms = end.elapsed().as_secs_f64() * 1000.0;
        SAMPLES.with(|samples| samples.borrow_mut().hash_ms.push(ms));
    }
}

pub(crate) fn stats(values: &[f64]) -> (f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    #[allow(clippy::cast_precision_loss)]
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
    let p99_index = ((sorted.len() as f64) * 0.99).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p99_index = if p99_index.is_finite() {
        p99_index as usize
    } else {
        sorted.len()
    };
    let last = sorted.len().saturating_sub(1);
    let p99_index = p99_index.saturating_sub(1).min(last);
    let p99 = sorted.get(p99_index).copied().unwrap_or(0.0);
    let max = sorted.get(last).copied().unwrap_or(0.0);
    (mean, p99, max)
}

const NAME_WIDTH: usize = 34;

fn report_range(samples: &Samples, prefix: &str, range: std::ops::Range<usize>) {
    let cut = |values: &[f64]| -> Vec<f64> {
        let end = range.end.min(values.len());
        values
            .get(range.start.min(end)..end)
            .unwrap_or(&[])
            .to_vec()
    };
    let line = |name: &str, values: &[f64]| {
        let (mean, p99, max) = stats(&cut(values));
        println!("perf: {prefix}{name:<NAME_WIDTH$} {mean:>9.4} {p99:>11.4} {max:>11.3}");
        mean
    };
    println!(
        "perf: {prefix}{:<NAME_WIDTH$}   mean_ms      p99_ms      max_ms",
        "row"
    );
    let mut means = Vec::with_capacity(PASS_NAMES.len());
    for (name, bucket) in PASS_NAMES.iter().zip(&samples.per_pass_ms) {
        means.push((line(name, bucket), *name));
    }
    let total = line("TOTAL", &samples.total_ms);
    line("STEP_WHOLE", &samples.step_whole_ms);
    let untimed: Vec<f64> = samples
        .step_whole_ms
        .iter()
        .zip(&samples.total_ms)
        .map(|(whole, total)| whole - total)
        .collect();
    line("STEP_WHOLE-TOTAL", &untimed);
    line("apply", &samples.apply_ms);
    if !samples.hash_ms.is_empty() {
        line("state_hash", &samples.hash_ms);
    }
    if !samples.clock_probe_us.is_empty() {
        let mut probes = cut(&samples.clock_probe_us);
        probes.sort_by(f64::total_cmp);
        let at = |share: usize| {
            probes
                .get(probes.len().saturating_mul(share) / 100)
                .copied()
                .unwrap_or(0.0)
        };
        let median = at(50);
        println!(
            "perf: {prefix}clock_probe_us p1 {:.2} p50 {median:.2} p90 {:.2} p99 {:.2}",
            at(1),
            at(90),
            at(99)
        );
        let (mut fast, mut slow) = (Vec::new(), Vec::new());
        for (probe, total) in cut(&samples.clock_probe_us)
            .iter()
            .zip(cut(&samples.total_ms))
        {
            if *probe <= median {
                fast.push(total);
            } else {
                slow.push(total);
            }
        }
        line("TOTAL, probe <= p50", &fast);
        line("TOTAL, probe > p50", &slow);
    }
    for (name, bucket) in SUB_NAMES.iter().zip(&samples.per_sub) {
        let times: Vec<f64> = bucket.iter().map(|(time, _)| *time).collect();
        let calls: Vec<f64> = bucket.iter().map(|(_, calls)| f64::from(*calls)).collect();
        let (calls_mean, _, _) = stats(&cut(&calls));
        line(&format!("sub {name} ({calls_mean:.1}/tick)"), &times);
    }
    means.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    println!("perf: {prefix}top 10 by mean                    mean_ms  share_of_TOTAL");
    for (mean, name) in means.iter().take(10) {
        let share = if total > 0.0 {
            mean / total * 100.0
        } else {
            0.0
        };
        println!("perf: {prefix}top {name:<NAME_WIDTH$} {mean:>9.4} {share:>8.1}%");
    }
}

pub(crate) fn report() {
    if !enabled() {
        return;
    }
    SAMPLES.with(|samples| {
        let samples = samples.borrow();
        if samples.ticks == 0 {
            return;
        }
        println!("perf: {} ticks profiled", samples.ticks);
        report_range(&samples, "", 0..usize::MAX);
        if let Some(window) = std::env::var("MINT_ROTWK_PROFILE_WINDOW")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            println!("perf: window: first {window} ticks");
            report_range(&samples, "window ", 0..window);
        }
        let plan_calls: Vec<f64> = samples.counts.iter().map(|c| f64::from(c.0)).collect();
        let armed: Vec<f64> = samples.counts.iter().map(|c| f64::from(c.1)).collect();
        let living: Vec<f64> = samples.counts.iter().map(|c| f64::from(c.2)).collect();
        let projectiles: Vec<f64> = samples.counts.iter().map(|c| f64::from(c.3)).collect();
        let stragglers: Vec<f64> = samples.counts.iter().map(|c| f64::from(c.4)).collect();
        let (pc_mean, pc_p99, pc_max) = stats(&plan_calls);
        let (ar_mean, ar_p99, ar_max) = stats(&armed);
        let (li_mean, li_p99, li_max) = stats(&living);
        let (pr_mean, pr_p99, pr_max) = stats(&projectiles);
        let (st_mean, st_p99, st_max) = stats(&stragglers);
        println!("perf: counts             mean         p99         max");
        println!("perf: path_requests  {pc_mean:>9.2} {pc_p99:>11.2} {pc_max:>11.0}");
        println!("perf: armed          {ar_mean:>9.2} {ar_p99:>11.2} {ar_max:>11.0}");
        println!("perf: living         {li_mean:>9.2} {li_p99:>11.2} {li_max:>11.0}");
        println!("perf: projectiles    {pr_mean:>9.2} {pr_p99:>11.2} {pr_max:>11.0}");
        println!("perf: stragglers     {st_mean:>9.2} {st_p99:>11.2} {st_max:>11.0}");
    });
}
