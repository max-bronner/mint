#![allow(
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mint_sage_apt_player::{
    ButtonVisual, Halt, InstanceId, InstanceKind, Library, Limits, Outward, Player, RenderList,
    ScriptStats,
};
use mint_sage_big::BigArchive;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(install), Some(mode)) = (args.first().map(PathBuf::from), args.get(1)) else {
        eprintln!("usage: vm <install> play|tree|press|soak|bench ...");
        std::process::exit(2);
    };
    let mut ticks: Option<u32> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut rest = args.iter().skip(2);
    while let Some(arg) = rest.next() {
        if arg == "--ticks" {
            ticks = rest.next().and_then(|t| t.parse().ok());
        } else {
            positional.push(arg.clone());
        }
    }
    let started = Instant::now();
    let library = Arc::new(load_install(&install));
    println!(
        "loaded {} movies in {:.2}s",
        library.movies().count(),
        started.elapsed().as_secs_f64()
    );
    match mode.as_str() {
        "play" => play(&library, ticks.unwrap_or(300)),
        "tree" => {
            if let Some(movie) = positional.first() {
                tree(&library, movie, ticks.unwrap_or(30));
            }
        }
        "press" => {
            if let Some((movie, paths)) = positional.split_first() {
                press_paths(&library, movie, paths, ticks.unwrap_or(30));
            }
        }
        "soak" => {
            if let Some(movie) = positional.first() {
                let hover = positional.get(1).is_some_and(|m| m == "hover");
                soak(&library, movie, ticks.unwrap_or(30_000), hover);
            }
        }
        "mouse" => {
            if let Some((movie, steps)) = positional.split_first() {
                mouse(&library, movie, steps, ticks.unwrap_or(30));
            }
        }
        "bench" => {
            for movie in positional.iter().flat_map(|p| p.split(',')) {
                bench(&library, movie);
            }
        }
        "call" => {
            if let [movie, after, path, args @ ..] = positional.as_slice() {
                host_call(
                    &library,
                    movie,
                    ticks.unwrap_or(1),
                    after.parse().unwrap_or(0),
                    path,
                    args,
                );
            } else {
                eprintln!("usage: vm <install> call <movie> <after> <path> [args...] [--ticks N]");
            }
        }
        "textcolours" => {
            let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
            for (_, m) in library.movies() {
                for character in &m.movie.characters {
                    if let mint_sage_apt::Character::EditText { color, .. } = character {
                        *counts.entry(*color).or_default() += 1;
                    }
                }
            }
            let total: u32 = counts.values().sum();
            let low_zero: u32 = counts
                .iter()
                .filter(|(c, _)| *c & 0xff == 0)
                .map(|(_, n)| n)
                .sum();
            let high_zero: u32 = counts
                .iter()
                .filter(|(c, _)| *c >> 24 == 0)
                .map(|(_, n)| n)
                .sum();
            println!(
                "{total} EditText colours, {} distinct; low byte 0: {low_zero}; top byte 0: {high_zero}",
                counts.len()
            );
            let mut rows: Vec<_> = counts.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            for (c, n) in rows.iter().take(25) {
                println!("  {n:5}  {c:#010x}");
            }
        }
        "exports" => {
            for movie in &positional {
                let Some(m) = library.movie_id(movie).and_then(|id| library.movie(id)) else {
                    println!("exports: no movie {movie}");
                    continue;
                };
                for e in &m.movie.exports {
                    println!("{movie} export {:?} = #{}", e.name, e.character);
                }
                for i in &m.movie.imports {
                    println!("{movie} import {:?} from {:?}", i.name, i.movie);
                }
            }
        }
        "labels" => {
            if let [movie, path] = positional.as_slice() {
                clip_labels(&library, movie, path, ticks.unwrap_or(1));
            }
        }
        "frameitems" => {
            let [movie, character, frame] = positional.as_slice() else {
                eprintln!("usage: vm <install> frameitems <movie> <character id> <frame, from 1>");
                return;
            };
            let found = (|| {
                let mid = library.movie_id(movie)?;
                let cid: u32 = character.parse().ok()?;
                let fnum: u16 = frame.parse().ok()?;
                let cref = library.resolve(mid, cid)?;
                let tid = library.sprite_timeline(cref)?;
                let timeline = library.timeline(tid)?;
                Some((timeline, fnum))
            })();
            let Some((timeline, fnum)) = found else {
                eprintln!("frameitems: movie/character/frame did not resolve");
                return;
            };
            println!("frame_count={}", timeline.frame_count());
            if let Some(f) = timeline.frame(fnum) {
                for item in &f.items {
                    println!("{item:?}");
                }
            } else {
                println!("no such frame");
            }
        }
        other => eprintln!("unknown mode {other}"),
    }
}

fn load_install(install: &Path) -> Library {
    let mut library = Library::new();
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
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
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
        if let (Some(apt), Some(constant)) = (apt, constant) {
            if let Err(e) = library.add(&name, &constant, apt) {
                println!("  load failure: {name}: {e}");
            }
        }
    }
    library.link();
    library
}

fn start(library: &Arc<Library>, movie: &str) -> Option<Player> {
    let id = library.movie_id(movie)?;
    match Player::new(Arc::clone(library), id, Limits::default()) {
        Ok(p) => Some(p),
        Err(e) => {
            println!("{movie}: player failed: {e}");
            None
        }
    }
}

fn halt_label(h: &Halt) -> String {
    match h {
        Halt::Unimplemented { opcode, .. } => {
            format!(
                "unimplemented {opcode:#04x} {}",
                mint_sage_apt::opcode_name(*opcode).unwrap_or("?")
            )
        }
        Halt::Budget { offset } => format!("instruction budget (at {offset:#x})"),
        Halt::StackUnderflow { .. } => "stack underflow".to_owned(),
        Halt::BadJump { .. } => "bad jump".to_owned(),
        Halt::BadIndex { .. } => "bad index".to_owned(),
        Halt::Limit { what } => format!("limit: {what}"),
        Halt::Decode => "decode".to_owned(),
    }
}

#[derive(Default)]
struct Census {
    movies: usize,
    stats: ScriptStats,
    halts: BTreeMap<String, u64>,
    unresolved: BTreeMap<String, (u64, usize)>,
    bytecode: BTreeMap<String, u64>,
    native: BTreeMap<String, u64>,
    outward: BTreeMap<String, u64>,
    errors: Vec<String>,
}

fn play(library: &Arc<Library>, ticks: u32) {
    let mut census = Census::default();
    println!(
        "\n{:<32} {:>7} {:>7} {:>6} {:>8} {:>7} {:>7} {:>7}",
        "movie", "blocks", "ended", "halted", "instrs", "unres", "objects", "outward"
    );
    let names: Vec<(String, _)> = library
        .movies()
        .map(|(id, m)| (m.movie.name.clone(), id))
        .collect();
    for (name, _) in &names {
        let Some(mut player) = start(library, name) else {
            census.errors.push(name.clone());
            continue;
        };
        player.set_record_calls(true);
        let mut first_error = None;
        let mut outward = 0usize;
        for _ in 0..ticks {
            if let Err(e) = player.tick() {
                first_error.get_or_insert(e);
            }
            for o in player.take_outward() {
                outward += 1;
                let key = match o {
                    Outward::FsCommand { command, .. } => format!("FSCommand:{command}"),
                    Outward::GetUrl { url, .. } => format!("getURL {url}"),
                    Outward::GameCode { name, args } => format!("{name}({})", args.join(",")),
                };
                *census.outward.entry(key).or_default() += 1;
            }
        }
        let s = player.take_script_stats();
        let halted: u64 = s.halts.values().sum();
        let unresolved: u64 = s.unresolved_calls.values().sum();
        println!(
            "{:<32} {:>7} {:>7} {:>6} {:>8} {:>7} {:>7} {:>7}{}",
            name,
            s.blocks_run,
            s.blocks_completed,
            halted,
            s.instructions,
            unresolved,
            player.heap_stats().objects,
            outward,
            first_error
                .map(|e| format!("  ERROR {e}"))
                .unwrap_or_default()
        );
        census.movies += 1;
        census.stats.blocks_run += s.blocks_run;
        census.stats.blocks_completed += s.blocks_completed;
        census.stats.instructions += s.instructions;
        census.stats.this_reads += s.this_reads;
        census.stats.this_undefined += s.this_undefined;
        census.stats.residue_blocks += s.residue_blocks;
        for (h, n) in &s.halts {
            *census.halts.entry(halt_label(h)).or_default() += n;
        }
        for (k, n) in &s.unresolved_calls {
            let e = census.unresolved.entry(k.clone()).or_default();
            e.0 += n;
            e.1 += 1;
        }
        for (k, n) in &s.bytecode_calls {
            *census.bytecode.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &s.native_calls {
            *census.native.entry(k.clone()).or_default() += n;
        }
    }
    let t = &census.stats;
    println!(
        "\n== play census: {} movies x {ticks} ticks; {} blocks run, {} ended ({:.1}%), {} instructions; \
         `this` pushed {} times, undefined {} times; {} blocks ended with a non-empty stack; players failed: {:?}",
        census.movies,
        t.blocks_run,
        t.blocks_completed,
        100.0 * t.blocks_completed as f64 / t.blocks_run.max(1) as f64,
        t.instructions,
        t.this_reads,
        t.this_undefined,
        t.residue_blocks,
        census.errors
    );
    println!("halts:");
    for (h, n) in &census.halts {
        println!("  {n:7}  {h}");
    }
    let print_map = |title: &str, map: &BTreeMap<String, u64>| {
        let mut rows: Vec<_> = map.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        println!(
            "{title}: {} names, {} calls",
            rows.len(),
            rows.iter().map(|r| *r.1).sum::<u64>()
        );
        for (k, n) in rows.iter().take(60) {
            println!("  {n:7}  {k}");
        }
    };
    let unresolved: BTreeMap<String, u64> = census
        .unresolved
        .iter()
        .map(|(k, v)| (k.clone(), v.0))
        .collect();
    print_map("unresolved (nothing callable)", &unresolved);
    print_map("movie bytecode functions called", &census.bytecode);
    print_map("built-ins called", &census.native);
    print_map("outward", &census.outward);
}

fn tree(library: &Arc<Library>, movie: &str, ticks: u32) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    for _ in 0..ticks {
        let _ = player.tick();
    }
    let root = player.root();
    dump(&mut player, root, 0);
    for o in player.take_outward() {
        println!("outward: {o:?}");
    }
}

fn clip_labels(library: &Arc<Library>, movie: &str, path: &str, ticks: u32) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    for _ in 0..ticks {
        let _ = player.tick();
    }
    let mut at = player.root();
    for segment in path.split('.') {
        let Some(next) = player.child_by_name(at, segment) else {
            println!(
                "labels: {segment:?} not found under {}",
                player.target_path(at)
            );
            return;
        };
        at = next;
    }
    let Some(InstanceKind::Clip(clip)) = player.instance(at).map(|i| i.kind().clone()) else {
        println!("labels: {path} is not a clip");
        return;
    };
    let Some(timeline) = library.timeline(clip.timeline) else {
        return;
    };
    println!(
        "{path}: {} frame(s), now at {}",
        timeline.frame_count(),
        clip.current_frame
    );
    for frame in 1..=timeline.frame_count() {
        let labels: Vec<&str> = timeline.labels_at(frame).collect();
        if !labels.is_empty() {
            println!("  frame {frame}: {labels:?}");
        }
    }
}

fn host_call(
    library: &Arc<Library>,
    movie: &str,
    ticks: u32,
    after: u32,
    path: &str,
    args: &[String],
) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    for _ in 0..ticks {
        let _ = player.tick();
    }
    let _ = player.take_outward();
    let values = args
        .iter()
        .map(|a| {
            a.parse::<f64>().map_or_else(
                |_| mint_sage_apt_player::Value::str(a),
                mint_sage_apt_player::Value::Number,
            )
        })
        .collect();
    let root = player.root();
    let result = player.call_defined(root, path, values);
    println!("call {path}{args:?} after {ticks} tick(s): {result:?}");
    for _ in 0..after {
        let _ = player.tick();
    }
    dump(&mut player, root, 0);
    for o in player.take_outward() {
        println!("outward: {o:?}");
    }
}

fn dump(player: &mut Player, id: InstanceId, depth: usize) {
    let shown = player.text(id);
    let surface = match player.variable(id, "_type") {
        mint_sage_apt_player::Value::Undefined => String::new(),
        v => format!(
            " _type={} world alpha {:.2}",
            player.display_string(&v),
            player.world_color(id).map_or(0.0, |c| c.mult[3])
        ),
    };
    let Some(inst) = player.instance(id) else {
        return;
    };
    let what = match inst.kind() {
        InstanceKind::Clip(c) => {
            let timeline = player.library().timeline(c.timeline);
            let total = timeline.map_or(0, |t| t.frame_count());
            let labels: Vec<&str> = timeline
                .map(|t| t.labels_at(c.current_frame).collect())
                .unwrap_or_default();
            format!(
                "clip frame {}/{} {:?} {}",
                c.current_frame,
                total,
                labels,
                if c.playing { "playing" } else { "stopped" }
            )
        }
        InstanceKind::Button(v) => format!(
            "button {v:?} actions {:?}",
            player
                .library()
                .button_actions(inst.character())
                .iter()
                .map(|a| format!("{:#x}", a.condition))
                .collect::<Vec<_>>()
        ),
        InstanceKind::EditText { text } => {
            let library = player.library();
            let (font, height, colour) = match library.character(inst.character()) {
                Some(mint_sage_apt::Character::EditText {
                    font,
                    font_height,
                    color,
                    ..
                }) => {
                    let name = library
                        .resolve(inst.character().movie, *font)
                        .and_then(|f| match library.character(f) {
                            Some(mint_sage_apt::Character::Font { name, .. }) => name.clone(),
                            _ => None,
                        });
                    (name, *font_height, *color)
                }
                _ => (None, 0.0, 0),
            };
            let alpha = player.world_color(id).map_or(0.0, |c| c.mult[3]);
            format!(
                "text {text:?} shows {shown:?} font {font:?} {height}u colour {colour:#010x} \
                 world alpha {alpha:.2}"
            )
        }
        InstanceKind::Shape => "shape".to_owned(),
        InstanceKind::Morph => "morph".to_owned(),
        InstanceKind::StaticText => "static text".to_owned(),
    };
    println!(
        "{:indent$}{} [{}] #{} ({:.0},{:.0}) {what}{surface}{}",
        "",
        inst.name(),
        inst.depth(),
        inst.character().id,
        inst.x(),
        inst.y(),
        if inst.visible() { "" } else { " (invisible)" },
        indent = depth * 2
    );
    if matches!(inst.kind(), InstanceKind::Shape | InstanceKind::StaticText) {
        return;
    }
    let children: Vec<_> = inst.children().collect();
    for (_, child) in children {
        dump(player, child, depth + 1);
    }
}

fn find_path(player: &Player, path: &str) -> Option<InstanceId> {
    let mut at = player.root();
    for name in path.split('.').filter(|n| !n.is_empty()) {
        at = player.child_by_name(at, name)?;
    }
    Some(at)
}

fn buttons_under(player: &Player, id: InstanceId, out: &mut Vec<InstanceId>) {
    let Some(inst) = player.instance(id) else {
        return;
    };
    if matches!(inst.kind(), InstanceKind::Button(_)) {
        out.push(id);
    }
    for (_, child) in inst.children() {
        buttons_under(player, child, out);
    }
}

fn press(player: &mut Player, button: InstanceId) -> usize {
    let mut queued = 0;
    for (visual, condition) in [
        (ButtonVisual::Over, 0x1u32),
        (ButtonVisual::Down, 0x4),
        (ButtonVisual::Over, 0x8),
    ] {
        let _ = player.set_button_state(button, visual);
        queued += player.queue_button_actions(button, condition).unwrap_or(0);
        let _ = player.run_actions();
    }
    queued
}

fn press_paths(library: &Arc<Library>, movie: &str, paths: &[String], ticks: u32) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    player.set_trace_log(true);
    player.set_record_calls(true);
    for _ in 0..ticks {
        let _ = player.tick();
    }
    report_step(&mut player, "settled");
    for path in paths {
        let Some(target) = find_path(&player, path) else {
            println!("\n== {path}: no such instance");
            continue;
        };
        let mut buttons = Vec::new();
        buttons_under(&player, target, &mut buttons);
        let Some(button) = buttons.first().copied() else {
            println!("\n== {path}: no button under it");
            continue;
        };
        let described = format!(
            "button #{} at {}",
            player.instance(button).map_or(0, |i| i.character().id),
            player.target_path(button)
        );
        let queued = press(&mut player, button);
        println!("\n== pressed {path} ({described}): {queued} records queued");
        report_step(&mut player, "after the press");
        for _ in 0..ticks {
            let _ = player.tick();
        }
        report_step(&mut player, &format!("after {ticks} more ticks"));
    }
    println!("\n== final tree");
    let root = player.root();
    dump(&mut player, root, 0);
}

fn mouse(library: &Arc<Library>, movie: &str, steps: &[String], ticks: u32) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    player.set_trace_log(true);
    player.set_record_calls(true);
    for _ in 0..ticks {
        let _ = player.tick();
    }
    report_step(&mut player, "settled");
    let point = |s: &str| -> Option<(f32, f32)> {
        let (x, y) = s.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    };
    for step in steps {
        let hovered_before = player.hovered();
        match step.split_once(':') {
            Some(("m", at)) => {
                if let Some((x, y)) = point(at) {
                    let _ = player.pointer_move(x, y);
                }
            }
            Some(("c", at)) => {
                if let Some((x, y)) = point(at) {
                    let _ = player.pointer_move(x, y);
                    let _ = player.pointer_button(true);
                    let _ = player.pointer_button(false);
                }
            }
            Some(("p", at)) => {
                if let Some((x, y)) = point(at) {
                    let _ = player.pointer_move(x, y);
                    let _ = player.pointer_button(true);
                }
            }
            Some(("u", _)) => {
                let _ = player.pointer_button(false);
            }
            Some(("t", n)) => {
                for _ in 0..n.parse::<u32>().unwrap_or(0) {
                    let _ = player.tick();
                }
            }
            Some(("f", call)) => {
                let mut parts = call.split(',');
                let name = parts.next().unwrap_or_default().to_owned();
                let values = parts
                    .map(|a| {
                        a.parse::<f64>().map_or_else(
                            |_| mint_sage_apt_player::Value::str(a),
                            mint_sage_apt_player::Value::Number,
                        )
                    })
                    .collect();
                let root = player.root();
                println!(
                    "call {name}: {:?}",
                    player.call_defined(root, &name, values)
                );
            }
            _ => println!("bad step {step}"),
        }
        let describe = |p: &Player, h: Option<InstanceId>| {
            h.map_or_else(
                || "-".to_owned(),
                |h| {
                    format!(
                        "{} #{}",
                        p.target_path(h),
                        p.instance(h).map_or(0, |i| i.character().id)
                    )
                },
            )
        };
        println!(
            "\n== {step}: hovered {} -> {}",
            describe(&player, hovered_before),
            describe(&player, player.hovered())
        );
        report_step(&mut player, "after");
        for _ in 0..ticks {
            let _ = player.tick();
        }
        report_step(&mut player, &format!("after {ticks} ticks"));
    }
    println!("\n== final tree");
    let root = player.root();
    dump(&mut player, root, 0);
}

fn report_step(player: &mut Player, when: &str) {
    let s = player.take_script_stats();
    println!(
        "-- {when}: {} blocks, {} ended; halts {:?}",
        s.blocks_run,
        s.blocks_completed,
        s.halts
            .iter()
            .map(|(h, n)| format!("{} x{n}", halt_label(h)))
            .collect::<Vec<_>>()
    );
    if !s.bytecode_calls.is_empty() {
        println!("   movie functions: {:?}", s.bytecode_calls);
    }
    if !s.unresolved_calls.is_empty() {
        println!("   unresolved: {:?}", s.unresolved_calls);
    }
    for o in player.take_outward() {
        println!("   outward: {o:?}");
    }
    for t in player.take_traces() {
        println!("   trace: {t}");
    }
}

fn soak(library: &Arc<Library>, movie: &str, ticks: u32, hover: bool) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    let mut list = RenderList::default();
    let started = Instant::now();
    let mut presses = 0u64;
    let mut cursor = 0usize;
    let mut outward = 0usize;
    println!(
        "{:>7} {:>8} {:>8} {:>8} {:>8} {:>6} {:>9} {:>8} {:>8}",
        "tick", "objects", "scopes", "slots", "live", "colls", "reclaimed", "presses", "blocks"
    );
    for tick in 1..=ticks {
        let _ = player.tick();
        player.render(&mut list);
        outward += player.take_outward().len();
        if tick % 5 == 0 {
            let mut buttons = Vec::new();
            buttons_under(&player, player.root(), &mut buttons);
            if !buttons.is_empty() {
                cursor = (cursor + 1) % buttons.len();
                if let Some(b) = buttons.get(cursor) {
                    if hover {
                        let _ = player.set_button_state(*b, ButtonVisual::Over);
                        let _ = player.queue_button_actions(*b, 0x1);
                        let _ = player.run_actions();
                    } else {
                        press(&mut player, *b);
                    }
                    let _ = player.set_button_state(*b, ButtonVisual::Up);
                    let _ = player.queue_button_actions(*b, 0x2);
                    let _ = player.run_actions();
                    presses += 1;
                }
            }
        }
        if tick % (ticks / 20).max(1) == 0 {
            let h = player.heap_stats();
            println!(
                "{tick:>7} {:>8} {:>8} {:>8} {:>8} {:>6} {:>9} {presses:>8} {:>8}",
                h.objects,
                h.scopes,
                h.object_slots,
                player.live_instances(),
                h.collections,
                h.reclaimed,
                player.script_stats().blocks_run
            );
        }
    }
    let s = player.script_stats();
    println!(
        "soak {movie}: {ticks} ticks in {:.1}s, {presses} presses, {outward} outward values, \
         {} blocks run, {} ended, halts {:?}",
        started.elapsed().as_secs_f64(),
        s.blocks_run,
        s.blocks_completed,
        s.halts
            .iter()
            .map(|(h, n)| format!("{} x{n}", halt_label(h)))
            .collect::<Vec<_>>()
    );
}

fn bench(library: &Arc<Library>, movie: &str) {
    let Some(mut player) = start(library, movie) else {
        return;
    };
    let mut list = RenderList::default();
    let mut cursor = 0usize;
    for (phase, ticks, hover) in [
        ("startup", 300usize, false),
        ("idle", 5000, false),
        ("hover", 5000, true),
    ] {
        let before = player.script_stats().clone();
        let mut frames: Vec<[f64; 4]> = Vec::with_capacity(ticks);
        for tick in 0..ticks {
            let t0 = Instant::now();
            player.advance();
            let t1 = Instant::now();
            let _ = player.run_actions();
            player.run_timers();
            if hover && tick % 5 == 0 {
                let mut buttons = Vec::new();
                buttons_under(&player, player.root(), &mut buttons);
                if !buttons.is_empty() {
                    cursor = (cursor + 1) % buttons.len();
                    if let Some(b) = buttons.get(cursor).copied() {
                        let _ = player.set_button_state(b, ButtonVisual::Over);
                        let _ = player.queue_button_actions(b, 0x1);
                        let _ = player.run_actions();
                        let _ = player.set_button_state(b, ButtonVisual::Up);
                        let _ = player.queue_button_actions(b, 0x2);
                        let _ = player.run_actions();
                    }
                }
            }
            let t2 = Instant::now();
            player.collect_if_due();
            let t3 = Instant::now();
            player.render(&mut list);
            let t4 = Instant::now();
            let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
            frames.push([ms(t0, t1), ms(t1, t2), ms(t2, t3), ms(t3, t4)]);
        }
        let after = player.script_stats();
        let n = frames.len() as f64;
        let mean = |i: usize| frames.iter().map(|f| f[i]).sum::<f64>() / n;
        let worst = frames
            .iter()
            .enumerate()
            .max_by(|a, b| (a.1[1] + a.1[2]).total_cmp(&(b.1[1] + b.1[2])))
            .map_or(0, |(i, _)| i + 1);
        let mut vm: Vec<f64> = frames.iter().map(|f| f[1] + f[2]).collect();
        vm.sort_by(f64::total_cmp);
        let pct = |v: &[f64], q: f64| {
            let i = ((v.len() as f64 - 1.0) * q).round() as usize;
            v.get(i).copied().unwrap_or(0.0)
        };
        let mut total: Vec<f64> = frames.iter().map(|f| f.iter().sum()).collect();
        total.sort_by(f64::total_cmp);
        println!(
            "bench {movie} {phase}: {} live, {} heap objects; mean advance {:.4} + VM {:.4} + GC {:.4} + build {:.4} ms; \
             VM (with GC) p50 {:.4} p99 {:.4} max {:.4} ms (frame {worst}); frame total p99 {:.4} max {:.4} ms; {:.2} blocks and {:.0} instructions per frame",
            player.live_instances(),
            player.heap_stats().objects,
            mean(0),
            mean(1),
            mean(2),
            mean(3),
            pct(&vm, 0.5),
            pct(&vm, 0.99),
            pct(&vm, 1.0),
            pct(&total, 0.99),
            pct(&total, 1.0),
            (after.blocks_run - before.blocks_run) as f64 / n,
            (after.instructions - before.instructions) as f64 / n,
        );
    }
}
