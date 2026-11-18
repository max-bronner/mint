use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_ini::{AnimationStateDef, AnimationStateKind};
use mint_sage_model::load_object_index;
use mint_sage_scene::{reachable_condition_sets, Conditions, StateMachine};
use mint_vfs::Vfs;

const RESEARCH_ORDERING_PERCENT: f64 = 97.8;

const ORDERING_TOLERANCE: f64 = 8.0;

const SUB_OBJECT_CALL_NAMES: [&str; 4] = [
    "CurDrawableHideSubObject",
    "CurDrawableShowSubObject",
    "CurDrawableHideSubObjectPermanently",
    "CurDrawableShowSubObjectPermanently",
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: animation_states <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last - see the module docs");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    let mut vfs = Vfs::new();
    let mut archives = 0usize;
    for root in &roots {
        for path in mint_sage_big::archives_under(root) {
            if let Ok(archive) = mint_sage_big::BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                archives += 1;
            }
        }
    }
    println!(
        "{archives} archive(s) mounted from {} install(s)",
        roots.len()
    );

    let (index, report) = load_object_index(&vfs);
    println!(
        "{} INI entr(ies), {} top-level document(s), {} parsed, {} failed; \
         {} object template(s)",
        report.entries,
        report.top_level,
        report.parsed,
        report.failures.len(),
        index.len(),
    );

    let probe = reachable_condition_sets();
    let mut totals = Totals::default();
    let mut unmodelled: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_flag: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut ordering = Ordering::default();

    for (name, states) in index.animation_state_declarations() {
        totals.modules += 1;
        let machine = StateMachine::compile(states);
        let found = machine.report();
        totals.states += found.states;
        totals.unreachable += found.unreachable;
        totals.transitions += found.transitions;
        totals.scripted += found.scripted;
        totals.clipless += found.clipless;
        if !found.has_fallback {
            totals.without_fallback += 1;
            if totals.no_fallback_examples.len() < 10 {
                totals.no_fallback_examples.push(name.to_owned());
            }
        }
        for state in states {
            let calls = state
                .script
                .as_ref()
                .and_then(|script| mint_sage_lua::Script::parse(&script.body).ok())
                .map(|script| {
                    script
                        .call_names()
                        .iter()
                        .filter(|name| SUB_OBJECT_CALL_NAMES.contains(name))
                        .count()
                })
                .unwrap_or(0);
            if calls > 0 {
                totals.sub_object_states += 1;
                totals.sub_object_calls += calls;
            }
            if state
                .conditions
                .iter()
                .any(|token| token == "BUILD_PLACEMENT_CURSOR")
            {
                totals.placement_states += 1;
                totals.placement_calls += calls;
            }
            if state.kind == AnimationStateKind::Transition {
                continue;
            }
            for token in &state.conditions {
                if Conditions::from_token(token).is_none() {
                    *unmodelled.entry(token.clone()).or_insert(0) += 1;
                }
            }
        }
        for live in &probe {
            totals.lookups += 1;
            match machine.select(*live) {
                None => totals.selected_nothing += 1,
                selected if selected == machine.fallback() => totals.fell_through += 1,
                _ => {}
            }
        }
        for (name, bit) in Conditions::NAMES {
            if machine.select(bit) != machine.fallback() {
                *per_flag.entry(name).or_insert(0) += 1;
            }
        }
        ordering.measure(states);
    }

    println!("\n== the machine ==");
    println!(
        "{} draw module(s) declare states, {} state(s) total",
        totals.modules, totals.states
    );
    println!(
        "{} unreachable (a header names a flag mint does not model), \
         {} transition(s) (entered only by a script's own queue, never by condition matching)",
        totals.unreachable, totals.transitions
    );
    println!(
        "{} state(s) carry a BeginScript region, parsed and run at the state-change edge \
         (`mint-sage-scene::anim_script`); {} carry no playable clip",
        totals.scripted, totals.clipless
    );
    println!(
        "{} state(s) make {} sub-object call(s) anywhere in their script; \
         {} BUILD_PLACEMENT_CURSOR state(s) make {}",
        totals.sub_object_states,
        totals.sub_object_calls,
        totals.placement_states,
        totals.placement_calls
    );
    println!(
        "{} module(s) have no unconditional state at all - an unmatched flag \
         set draws the bind pose there",
        totals.without_fallback
    );
    for name in &totals.no_fallback_examples {
        println!("  NO FALLBACK {name}");
    }

    println!("\n== selection ==");
    println!(
        "{} lookup(s) over {} reachable flag set(s) x {} module(s)",
        totals.lookups,
        probe.len(),
        totals.modules
    );
    println!(
        "{} fell through to the fallback ({:.1}%), {} selected nothing at all ({:.1}%)",
        totals.fell_through,
        percent(totals.fell_through, totals.lookups),
        totals.selected_nothing,
        percent(totals.selected_nothing, totals.lookups),
    );
    println!("modules where one flag on its own selects a state of its own:");
    for (name, count) in &per_flag {
        println!("  {name}: {count} of {}", totals.modules);
    }

    println!("\n== unmodelled condition flags ==");
    println!(
        "{} distinct token(s) across {} header use(s); the 20 most used:",
        unmodelled.len(),
        unmodelled.values().sum::<usize>()
    );
    let mut ranked: Vec<(&String, &usize)> = unmodelled.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (token, count) in ranked.into_iter().take(20) {
        println!("  {token}: {count}");
    }

    println!("\n== cross-check: the ordering discipline ==");
    let ordered_percent = percent(ordering.specific_first, ordering.pairs);
    println!(
        "{} ordered pair(s) where one header's flag set is a strict subset of \
         another's in the same module",
        ordering.pairs
    );
    println!(
        "{} put the more specific state first ({ordered_percent:.1}%), {} put the \
         subset first (which shadows the specific one)",
        ordering.specific_first, ordering.subset_first,
    );
    let drift = (ordered_percent - RESEARCH_ORDERING_PERCENT).abs();
    let agrees = ordering.pairs > 0 && drift <= ORDERING_TOLERANCE;
    println!(
        "research (animation-states.md 4.2) measured {RESEARCH_ORDERING_PERCENT:.1}% \
         with an independent scanner: {}",
        if agrees {
            "AGREES"
        } else {
            "DISAGREES - the reader or the rule is wrong"
        }
    );

    println!("\n== throughput ==");
    println!("{:.2} s total", started.elapsed().as_secs_f64());

    if agrees {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[allow(clippy::cast_precision_loss)]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 * 100.0 / whole as f64
}

#[derive(Default)]
struct Totals {
    modules: usize,
    states: usize,
    unreachable: usize,
    transitions: usize,
    scripted: usize,
    sub_object_states: usize,
    sub_object_calls: usize,
    placement_states: usize,
    placement_calls: usize,
    clipless: usize,
    without_fallback: usize,
    no_fallback_examples: Vec<String>,
    lookups: usize,
    fell_through: usize,
    selected_nothing: usize,
}

#[derive(Default)]
struct Ordering {
    pairs: usize,
    specific_first: usize,
    subset_first: usize,
}

impl Ordering {
    fn measure(&mut self, states: &[AnimationStateDef]) {
        let sets: Vec<Vec<String>> = states
            .iter()
            .filter(|state| state.kind == AnimationStateKind::Conditioned)
            .map(|state| {
                let mut tokens = state.conditions.clone();
                tokens.sort();
                tokens.dedup();
                tokens
            })
            .collect();
        for (i, earlier) in sets.iter().enumerate() {
            for later in sets.iter().skip(i + 1) {
                if earlier.is_empty() || later.is_empty() || earlier == later {
                    continue;
                }
                if strict_subset(later, earlier) {
                    self.pairs += 1;
                    self.specific_first += 1;
                } else if strict_subset(earlier, later) {
                    self.pairs += 1;
                    self.subset_first += 1;
                }
            }
        }
    }
}

fn strict_subset(small: &[String], big: &[String]) -> bool {
    small.len() < big.len() && small.iter().all(|token| big.contains(token))
}
