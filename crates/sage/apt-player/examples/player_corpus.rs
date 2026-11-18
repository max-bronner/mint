#![allow(
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mint_sage_apt_player::{Library, Limits, Player, RenderList};
use mint_sage_big::BigArchive;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(install) = args.first().map(PathBuf::from) else {
        eprintln!("usage: corpus <install> [--ticks N] [--bench a,b,c]");
        std::process::exit(2);
    };
    let mut ticks: Option<u32> = None;
    let mut bench: Vec<String> = Vec::new();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--ticks" => ticks = rest.next().and_then(|t| t.parse().ok()),
            "--bench" => {
                bench = rest
                    .next()
                    .map(|b| b.split(',').map(str::to_owned).collect())
                    .unwrap_or_default();
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let started = Instant::now();
    let (library, failures) = load_install(&install);
    let library = Arc::new(library);
    println!(
        "loaded {} movies from {} in {:.2}s; {} failed to load",
        library.movies().count(),
        install.display(),
        started.elapsed().as_secs_f64(),
        failures.len()
    );
    for (name, error) in &failures {
        println!("  load failure: {name}: {error}");
    }
    let report = library.report();
    println!(
        "load report: {} broken sprite timelines, {} broken clip-action records, \
         {} broken button-action arrays, {} unresolved imports",
        report.broken_timelines.len(),
        report.broken_clip_actions.len(),
        report.broken_button_actions.len(),
        report.unresolved_imports.len()
    );
    for (movie, id, error) in &report.broken_timelines {
        println!("  broken timeline: {movie}#{id}: {error}");
    }
    for (movie, offset, error) in &report.broken_clip_actions {
        println!("  broken clip-action record: {movie} @{offset:#x}: {error}");
    }

    if bench.is_empty() {
        survey(&library, ticks);
    } else {
        for name in &bench {
            benchmark(&library, name);
        }
    }
}

fn load_install(install: &Path) -> (Library, Vec<(String, String)>) {
    let mut library = Library::new();
    let mut failures = Vec::new();
    let mut archives: Vec<PathBuf> = std::fs::read_dir(install.join("apt"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("big"))
        })
        .collect();
    archives.sort();
    for path in archives {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let archive = match BigArchive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                failures.push((name, e.to_string()));
                continue;
            }
        };
        let mut apt = None;
        let mut constant = None;
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if lower.ends_with(".apt") {
                apt = archive.read_entry(entry).ok();
            } else if lower.ends_with(".const") {
                constant = archive.read_entry(entry).ok();
            }
        }
        let (Some(apt), Some(constant)) = (apt, constant) else {
            failures.push((name, "no .apt/.const pair".to_owned()));
            continue;
        };
        if let Err(e) = library.add(&name, &constant, apt) {
            failures.push((name, e.to_string()));
        }
    }
    library.link();
    (library, failures)
}

fn survey(library: &Arc<Library>, ticks: Option<u32>) {
    let mut totals: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut errors = 0usize;
    let mut peak_instances = 0usize;
    let mut peak_movie = String::new();
    let mut list = RenderList::default();
    println!(
        "\n{:<32} {:>6} {:>6} {:>8} {:>8} {:>8} {:>7} {:>7} {:>8}",
        "movie", "frames", "ticks", "peak", "slots", "cmds", "loops", "rewind", "actions"
    );
    for (id, movie) in library.movies() {
        let frames = movie.movie.record.frame_count;
        let n = ticks.unwrap_or_else(|| frames.saturating_add(1).max(2));
        let mut player =
            match Player::new_without_scripts(Arc::clone(library), id, Limits::default()) {
                Ok(p) => p,
                Err(e) => {
                    println!("{:<32} player failed: {e}", movie.movie.name);
                    errors += 1;
                    continue;
                }
            };
        let mut peak = 0usize;
        let mut peak_cmds = 0usize;
        let mut first_error = None;
        for _ in 0..n {
            if let Err(e) = player.tick() {
                first_error.get_or_insert(e);
            }
            player.clear_actions();
            player.render(&mut list);
            peak = peak.max(player.live_instances());
            peak_cmds = peak_cmds.max(list.commands.len());
        }
        let c = player.counters();
        println!(
            "{:<32} {:>6} {:>6} {:>8} {:>8} {:>8} {:>7} {:>7} {:>8}{}",
            movie.movie.name,
            frames,
            n,
            peak,
            player.instance_slots(),
            peak_cmds,
            c.loops,
            c.rewinds.saturating_sub(c.loops),
            c.actions_queued,
            first_error
                .map(|e| format!("  ERROR {e}"))
                .unwrap_or_default()
        );
        if peak > peak_instances {
            peak_instances = peak;
            peak_movie.clone_from(&movie.movie.name);
        }
        for (key, value) in [
            ("frames_run", c.frames_run),
            ("instantiated", c.instantiated),
            ("reclaimed", c.reclaimed),
            ("place_refused_occupied", c.place_refused_occupied),
            ("replace_swapped", c.replace_swapped),
            ("replace_kept", c.replace_kept),
            ("replace_on_empty", c.replace_on_empty),
            ("modify_on_empty", c.modify_on_empty),
            ("remove_on_empty", c.remove_on_empty),
            ("invalid_records", c.invalid_records),
            ("unresolved", c.unresolved),
            ("not_placeable", c.not_placeable),
            ("loops", c.loops),
            ("rewind_kept", c.rewind_kept),
            ("rewind_removed", c.rewind_removed),
            ("actions_queued", c.actions_queued),
            ("limit_refusals", c.limit_refusals),
        ] {
            let slot = totals.entry(key).or_insert(0);
            *slot = slot.saturating_add(value);
        }
    }
    println!("\ntotals over every movie:");
    for (key, value) in &totals {
        println!("  {key:<24} {value}");
    }
    println!("  player construction errors {errors}");
    println!("  peak live instances        {peak_instances} ({peak_movie})");
}

fn benchmark(library: &Arc<Library>, name: &str) {
    let Some(id) = library.movie_id(name) else {
        println!("bench {name}: not in this install");
        return;
    };
    let Ok(mut player) = Player::new_without_scripts(Arc::clone(library), id, Limits::default())
    else {
        println!("bench {name}: player failed");
        return;
    };
    let mut list = RenderList::default();
    const WARMUP: u32 = 300;
    const TICKS: usize = 5000;
    for _ in 0..WARMUP {
        let _ = player.tick();
        player.clear_actions();
        player.render(&mut list);
    }
    let mut frames: Vec<(f64, f64, bool)> = Vec::with_capacity(TICKS);
    let mut commands = 0usize;
    for _ in 0..TICKS {
        let rewinds = player.counters().rewinds;
        let t0 = Instant::now();
        let _ = player.tick();
        player.clear_actions();
        let t1 = Instant::now();
        player.render(&mut list);
        let t2 = Instant::now();
        commands = commands.max(std::hint::black_box(list.commands.len()));
        frames.push((
            (t1 - t0).as_secs_f64() * 1000.0,
            (t2 - t1).as_secs_f64() * 1000.0,
            player.counters().rewinds != rewinds,
        ));
    }
    let n = frames.len() as f64;
    let advance = frames.iter().map(|f| f.0).sum::<f64>() / n;
    let build = frames.iter().map(|f| f.1).sum::<f64>() / n;
    let mut totals: Vec<f64> = frames.iter().map(|f| f.0 + f.1).collect();
    totals.sort_by(f64::total_cmp);
    let pct = |q: f64| {
        let i = ((totals.len() as f64 - 1.0) * q).round() as usize;
        totals.get(i).copied().unwrap_or(0.0)
    };
    let rewind_frames: Vec<f64> = frames.iter().filter(|f| f.2).map(|f| f.0 + f.1).collect();
    let rewind_max = rewind_frames.iter().copied().fold(0.0, f64::max);
    let mut slowest: Vec<&(f64, f64, bool)> = frames.iter().collect();
    slowest.sort_by(|a, b| (b.0 + b.1).total_cmp(&(a.0 + a.1)));
    let slow_rewinds = slowest.iter().take(10).filter(|f| f.2).count();
    println!(
        "bench {name}: {} live, <= {commands} commands; mean advance {advance:.4} + build {build:.4} = {:.4} ms/frame over {TICKS}; p50 {:.4} p99 {:.4} p99.9 {:.4} max {:.4} ms; {} rewind/loop frames, slowest {rewind_max:.4} ms; {slow_rewinds} of the 10 slowest frames were rewinds",
        player.live_instances(),
        advance + build,
        pct(0.5),
        pct(0.99),
        pct(0.999),
        pct(1.0),
        rewind_frames.len(),
    );
}
