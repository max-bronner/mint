use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use std::sync::Arc;

use mint_sage_apt::{
    decode_block, opcode_name, parse_frames, AptMovie, Block, Character, DecodeLimits, Frame,
    FrameItem,
};
use mint_sage_apt_player::{
    HitArea, Library, Limits, Player, RenderCommand, RenderList, RenderOptions,
};
use mint_sage_apt_viewer::MovieLibrary;
use mint_sage_big::BigArchive;

const MAX_IMPORT_HOPS: u32 = 4;
const SETTLE_TICKS: u32 = 60;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: coverage <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    for root in &roots {
        println!("======== {} ========", root.display());
        sweep(root);
        println!();
    }
    ExitCode::SUCCESS
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Frame,
    ClipAction,
    ButtonAction,
    InitAction,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Frame => "frame",
            Source::ClipAction => "clip action",
            Source::ButtonAction => "button action",
            Source::InitAction => "init action",
        }
    }
}

#[derive(Default)]
struct Totals {
    movies_found: usize,
    movies_loaded: usize,
    load_failures: Vec<String>,

    characters: BTreeMap<&'static str, u64>,
    frame_items: BTreeMap<&'static str, u64>,
    place_flags: [u64; 8],
    place_objects: u64,
    clip_depth_spans: Vec<(String, i32, i32)>,
    clip_action_flags: BTreeMap<u32, u64>,
    button_state_flags: BTreeMap<u32, u64>,
    buttons_without_over: u64,
    buttons_without_down: u64,
    buttons_with_up: u64,
    edittext_flags: [u64; 3],
    edittext_total: u64,

    blocks: u64,
    blocks_decoded: u64,
    blocks_by_source: BTreeMap<Source, u64>,
    opcodes: BTreeMap<u8, u64>,

    screens: Vec<Screen>,
}

#[derive(Default)]
struct Screen {
    name: String,
    characters: usize,
    frames: usize,
    imports: usize,
    instances: usize,
    shapes: u64,
    text: u64,
    buttons: u64,
    engine_surfaces: u64,
}

impl Screen {
    fn draws_something(&self) -> bool {
        self.shapes > 0 || self.text > 0
    }
}

fn sweep(root: &Path) {
    let started = Instant::now();
    let mut library = MovieLibrary::new(root);
    let names = library.list_available();
    let mut totals = Totals {
        movies_found: names.len(),
        ..Totals::default()
    };
    let player_library = load_player_library(root, &names);

    for name in &names {
        if library
            .load_imports_transitively(name, MAX_IMPORT_HOPS)
            .is_err()
        {
            totals.load_failures.push(name.clone());
            continue;
        }
        totals.movies_loaded += 1;
        let movie_set = library.movie_set();
        let Some(movie) = movie_set.get(name).cloned() else {
            totals.load_failures.push(name.clone());
            continue;
        };
        let Some(assets) = library.assets(name) else {
            continue;
        };
        let apt = assets.apt.clone();

        census_characters(&apt, &movie, &mut totals);
        census_frames(&apt, &movie, &mut totals);
        decode_blocks(&apt, &movie, &mut totals);
        totals
            .screens
            .push(play_screen(&player_library, &movie, name));
    }

    report(&totals, started.elapsed().as_secs_f64());
}

fn character_kind(character: &Character) -> &'static str {
    match character {
        Character::Shape { .. } => "Shape",
        Character::EditText { .. } => "EditText",
        Character::Font { .. } => "Font",
        Character::Button(_) => "Button",
        Character::Sprite(_) => "Sprite",
        Character::Image { .. } => "Image",
        Character::Morph { .. } => "Morph",
        Character::Movie(_) => "Movie",
        Character::Text { .. } => "Text",
        Character::Placeholder => "Placeholder (import)",
        Character::Unknown { .. } => "Unknown",
    }
}

fn census_characters(apt: &[u8], movie: &AptMovie, totals: &mut Totals) {
    for character in &movie.characters {
        *totals
            .characters
            .entry(character_kind(character))
            .or_default() += 1;
        match character {
            Character::Button(button) => {
                for record in &button.records {
                    *totals.button_state_flags.entry(record.flags).or_default() += 1;
                }
                let has = |bit: u32| button.records.iter().any(|r| r.flags & bit != 0);
                if has(0x1) {
                    totals.buttons_with_up += 1;
                    totals.buttons_without_over += u64::from(!has(0x2));
                    totals.buttons_without_down += u64::from(!has(0x4));
                }
            }
            Character::EditText {
                readonly,
                multiline,
                wordwrap,
                ..
            } => {
                totals.edittext_total += 1;
                for (i, value) in [readonly, multiline, wordwrap].into_iter().enumerate() {
                    if *value != 0 {
                        if let Some(slot) = totals.edittext_flags.get_mut(i) {
                            *slot += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let _ = apt;
}

fn timelines(movie: &AptMovie) -> Vec<(u32, u32)> {
    let mut out = vec![(movie.record.frame_count, movie.record.frames_offset)];
    for character in &movie.characters {
        if let Character::Sprite(sprite) = character {
            out.push((sprite.frame_count, sprite.frames_offset));
        }
    }
    out
}

fn census_frames(apt: &[u8], movie: &AptMovie, totals: &mut Totals) {
    for (count, offset) in timelines(movie) {
        let Ok(frames) = parse_frames(apt, count, offset) else {
            continue;
        };
        for frame in &frames {
            for item in &frame.items {
                let kind = match item {
                    FrameItem::Action(_) => "Action",
                    FrameItem::FrameLabel(_) => "FrameLabel",
                    FrameItem::PlaceObject(_) => "PlaceObject",
                    FrameItem::RemoveObject(_) => "RemoveObject",
                    FrameItem::BackgroundColor(_) => "BackgroundColor",
                    FrameItem::InitAction(_) => "InitAction",
                    FrameItem::Unknown { .. } => "Unknown",
                };
                *totals.frame_items.entry(kind).or_default() += 1;
                if let FrameItem::PlaceObject(po) = item {
                    totals.place_objects += 1;
                    let flags = po.flags;
                    let bits = [
                        flags.move_existing(),
                        flags.has_character(),
                        flags.has_matrix(),
                        flags.has_color_transform(),
                        flags.has_ratio(),
                        flags.has_name(),
                        flags.has_clip_depth(),
                        flags.has_clip_actions(),
                    ];
                    for (i, set) in bits.iter().enumerate() {
                        if *set {
                            if let Some(slot) = totals.place_flags.get_mut(i) {
                                *slot += 1;
                            }
                        }
                    }
                    if let Some(clip) = po.clip_depth {
                        totals
                            .clip_depth_spans
                            .push((movie.name.clone(), po.depth, clip));
                    }
                    if let Some(poa) = po.clip_actions_offset {
                        let base = poa as usize;
                        let (Some(count), Some(array)) =
                            (u32_at(apt, base), u32_at(apt, base.wrapping_add(4)))
                        else {
                            continue;
                        };
                        for i in 0..count as usize {
                            let Some(at) = (array as usize).checked_add(i.wrapping_mul(12)) else {
                                continue;
                            };
                            if let Some(flags) = u32_at(apt, at) {
                                *totals.clip_action_flags.entry(flags).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn block_addresses(apt: &[u8], movie: &AptMovie) -> Vec<(u32, Source)> {
    let mut out = Vec::new();
    for character in &movie.characters {
        if let Character::Button(button) = character {
            for i in 0..button.action_count as usize {
                let Some(at) = (button.actions_offset as usize).checked_add(i.wrapping_mul(8))
                else {
                    continue;
                };
                if let Some(ptr) = u32_at(apt, at.wrapping_add(4)) {
                    out.push((ptr, Source::ButtonAction));
                }
            }
        }
    }
    for (count, offset) in timelines(movie) {
        let Ok(frames) = parse_frames(apt, count, offset) else {
            continue;
        };
        collect_frame_blocks(apt, &frames, &mut out);
    }
    out
}

fn collect_frame_blocks(apt: &[u8], frames: &[Frame], out: &mut Vec<(u32, Source)>) {
    for frame in frames {
        for item in &frame.items {
            match item {
                FrameItem::Action(a) => out.push((a.block_offset, Source::Frame)),
                FrameItem::InitAction(i) => out.push((i.block_offset, Source::InitAction)),
                FrameItem::PlaceObject(po) => {
                    let Some(poa) = po.clip_actions_offset else {
                        continue;
                    };
                    let base = poa as usize;
                    let (Some(count), Some(array)) =
                        (u32_at(apt, base), u32_at(apt, base.wrapping_add(4)))
                    else {
                        continue;
                    };
                    for i in 0..count as usize {
                        let Some(at) = (array as usize).checked_add(i.wrapping_mul(12)) else {
                            continue;
                        };
                        if let Some(ptr) = u32_at(apt, at.wrapping_add(8)) {
                            out.push((ptr, Source::ClipAction));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(at..end)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Some(u32::from_le_bytes(buf))
}

fn record_opcodes(block: &Block, into: &mut BTreeMap<u8, u64>) {
    for instruction in &block.code {
        *into.entry(instruction.op.opcode()).or_default() += 1;
    }
    for function in &block.functions {
        for instruction in &function.body {
            *into.entry(instruction.op.opcode()).or_default() += 1;
        }
    }
}

fn decode_blocks(apt: &[u8], movie: &AptMovie, totals: &mut Totals) {
    for (offset, source) in block_addresses(apt, movie) {
        totals.blocks += 1;
        *totals.blocks_by_source.entry(source).or_default() += 1;
        let Ok(block) = decode_block(apt, offset, &DecodeLimits::default()) else {
            continue;
        };
        totals.blocks_decoded += 1;
        record_opcodes(&block, &mut totals.opcodes);
    }
}

fn load_player_library(root: &Path, names: &[String]) -> Arc<Library> {
    let mut library = Library::new();
    for name in names {
        let Ok(archive) = BigArchive::open(root.join("apt").join(format!("{name}.big"))) else {
            continue;
        };
        let mut apt = None;
        let mut constant = None;
        for entry in archive.entries() {
            let lower = entry.path.as_str().to_ascii_lowercase();
            if lower.ends_with(".apt") {
                apt = archive.read_entry(entry).ok();
            } else if lower.ends_with(".const") {
                constant = archive.read_entry(entry).ok();
            }
        }
        if let (Some(apt), Some(constant)) = (apt, constant) {
            let _ = library.add(name, &constant, apt);
        }
    }
    library.link();
    Arc::new(library)
}

fn play_screen(library: &Arc<Library>, movie: &AptMovie, name: &str) -> Screen {
    let mut screen = Screen {
        name: name.to_owned(),
        characters: movie.characters.len(),
        frames: usize::try_from(movie.record.frame_count).unwrap_or(0),
        imports: movie.imports.len(),
        ..Screen::default()
    };
    let Some(id) = library.movie_id(name) else {
        return screen;
    };
    let Ok(mut player) = Player::new(Arc::clone(library), id, Limits::default()) else {
        return screen;
    };
    for _ in 0..SETTLE_TICKS {
        let _ = player.tick();
    }
    let mut list = RenderList::default();
    player.render_with(&mut list, RenderOptions { surfaces: true });
    for command in &list.commands {
        match command {
            RenderCommand::Shape { .. } => screen.shapes += 1,
            RenderCommand::EditText { .. } => screen.text += 1,
            RenderCommand::Surface { .. } => screen.engine_surfaces += 1,
            _ => {}
        }
    }
    let mut hits: Vec<HitArea> = Vec::new();
    player.hit_areas(&mut hits);
    screen.buttons = hits.len() as u64;
    screen.instances = player.live_instances();
    screen
}

fn character_support(kind: &str) -> &'static str {
    match kind {
        "Shape" => "drawn (solid/line/textured .ru runs)",
        "Sprite" => "drawn, own playhead per instance (player)",
        "Image" => "drawn only as a Shape's texture style",
        "EditText" => "drawn: literal, binding (pulled), alignment, font_height",
        "Button" => "hit-tested by hit records in their own space; state records drawn",
        "Font" => "resolved by name to a bundled font file",
        "Text" => "IGNORED - placed, not drawn; glyph indices map to nothing known",
        "Morph" => "IGNORED - placed, not drawn; never interpolated",
        "Movie" => "the movie record itself",
        "Placeholder (import)" => "resolved through the import table",
        _ => "?",
    }
}

fn frame_item_support(kind: &str) -> &'static str {
    match kind {
        "PlaceObject" => "handled (retained tree, Ruffle place/replace/modify)",
        "RemoveObject" => "handled",
        "Action" => "queued and run by the player's VM",
        "FrameLabel" => "handled (goto by label, first wins)",
        "BackgroundColor" => "parsed; painted by the viewer on `b`",
        "InitAction" => "run by the player's VM (see apt-playback.md P2)",
        _ => "?",
    }
}

fn clip_event_name(flags: u32) -> &'static str {
    match flags {
        0x0000_0001 => "Load",
        0x0000_0002 => "EnterFrame",
        0x0000_0004 => "Unload",
        0x0000_0008 => "MouseMove",
        0x0000_0010 => "MouseDown",
        0x0000_0020 => "MouseUp",
        0x0000_0040 => "KeyDown",
        0x0000_0080 => "KeyUp",
        0x0000_0100 => "Data",
        0x0000_0200 => "Initialize",
        0x0000_0400 => "Press",
        0x0000_0800 => "Release",
        0x0000_1000 => "ReleaseOutside",
        0x0000_2000 => "RollOver",
        0x0000_4000 => "RollOut",
        0x0000_8000 => "DragOver",
        0x0001_0000 => "DragOut",
        0x0002_0000 => "KeyPress",
        0x0004_0000 => "Construct",
        _ => "<combination or unknown>",
    }
}

fn button_state_name(flags: u32) -> &'static str {
    match flags {
        0x8 => "HitTest only - never drawn",
        0xF => "Up|Over|Down|HitTest",
        0x7 => "Up|Over|Down",
        0x1 => "Up",
        0x2 => "Over",
        0x4 => "Down",
        0xE => "Over|Down|HitTest",
        _ => "<other combination>",
    }
}

fn flag_support(bit: usize) -> (&'static str, &'static str) {
    match bit {
        0 => ("Move", "acted on: modifies the occupied depth"),
        1 => (
            "HasCharacter",
            "acted on: replaces the object at that depth",
        ),
        2 => ("HasMatrix", "acted on"),
        3 => ("HasColorTransform", "acted on (multiply and add)"),
        4 => ("HasRatio", "rewind identity only - no morph interpolation"),
        5 => ("HasName", "acted on: names the clip for script binding"),
        6 => (
            "HasClipDepth",
            "acted on: stencil mask, a sprite mask as its whole subtree",
        ),
        7 => (
            "HasClipActions",
            "acted on: clip events dispatched by their flags",
        ),
        _ => ("?", "?"),
    }
}

fn report(totals: &Totals, elapsed: f64) {
    println!("-- 0. corpus --");
    println!(
        "  movies: {} found, {} loaded, {} failed  ({elapsed:.1}s)",
        totals.movies_found,
        totals.movies_loaded,
        totals.load_failures.len()
    );
    for name in &totals.load_failures {
        println!("    FAILED: {name}");
    }

    println!();
    println!("-- 1. character types (every movie's own dictionary) --");
    println!(
        "  {:<22} {:>7}  what mint does [hand-checked]",
        "type", "count"
    );
    let mut chars: Vec<(&&str, &u64)> = totals.characters.iter().collect();
    chars.sort_by(|a, b| b.1.cmp(a.1));
    for (kind, count) in chars {
        println!("  {:<22} {count:>7}  {}", kind, character_support(kind));
    }

    println!();
    println!(
        "-- 2. frame item types (every timeline: movie's own plus every Sprite character's) --"
    );
    println!(
        "  {:<18} {:>7}  what mint does [hand-checked]",
        "type", "count"
    );
    let mut items: Vec<(&&str, &u64)> = totals.frame_items.iter().collect();
    items.sort_by(|a, b| b.1.cmp(a.1));
    for (kind, count) in items {
        println!("  {:<18} {count:>7}  {}", kind, frame_item_support(kind));
    }

    println!();
    println!(
        "-- 3. PlaceObject flag bits, over {} records --",
        totals.place_objects
    );
    println!(
        "  {:<3} {:<20} {:>7} {:>6}  what mint does [hand-checked]",
        "bit", "name", "set", "%"
    );
    for (bit, count) in totals.place_flags.iter().enumerate() {
        let (name, support) = flag_support(bit);
        let pct = if totals.place_objects == 0 {
            0.0
        } else {
            100.0 * (*count as f64) / (totals.place_objects as f64)
        };
        println!("  {bit:<3} {name:<20} {count:>7} {pct:>5.1}%  {support}");
    }
    println!(
        "  clip-depth placements: {} records; the spans they would mask:",
        totals.clip_depth_spans.len()
    );
    let mut per_movie: BTreeMap<&str, u64> = BTreeMap::new();
    for (movie, _, _) in &totals.clip_depth_spans {
        *per_movie.entry(movie.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&&str, &u64)> = per_movie.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    for (movie, count) in ranked.iter().take(12) {
        println!("    {count:4}  {movie}");
    }

    println!();
    println!("-- 3b. the other per-record flag words --");
    println!("  clip-action flags, one per PlaceObject clip-action record:");
    println!(
        "    (SWF ClipEventFlags, INFERRED from Flash; the player dispatches each event \
         to the records whose flags carry it.)"
    );
    let mut clip_flags: Vec<(&u32, &u64)> = totals.clip_action_flags.iter().collect();
    clip_flags.sort_by(|a, b| b.1.cmp(a.1));
    for (flags, count) in clip_flags {
        println!("    {count:6}  {flags:#010x}  {}", clip_event_name(*flags));
    }
    println!("  Button record state bits (bit 0 Up, 1 Over, 2 Down, 3 HitTest):");
    println!("    (mint acts on all four: a hit-only record is never drawn, by construction.)");
    let mut states: Vec<(&u32, &u64)> = totals.button_state_flags.iter().collect();
    states.sort_by(|a, b| b.1.cmp(a.1));
    for (flags, count) in states {
        println!("    {count:6}  {flags:#04x}  {}", button_state_name(*flags));
    }
    println!(
        "    of {} buttons with Up records, {} have no Over and {} no Down record \
         (the player draws nothing for a missing state, as Ruffle's set_state does)",
        totals.buttons_with_up, totals.buttons_without_over, totals.buttons_without_down
    );
    println!(
        "  EditText booleans, over {} characters: readonly {} / multiline {} / wordwrap {} \
         set non-zero",
        totals.edittext_total,
        totals.edittext_flags.first().copied().unwrap_or(0),
        totals.edittext_flags.get(1).copied().unwrap_or(0),
        totals.edittext_flags.get(2).copied().unwrap_or(0)
    );
    println!("    (all three are decoded and IGNORED: no editing, no wrapping, no line breaking.)");

    println!();
    println!("-- 4. action blocks: decode --");
    println!(
        "  {} blocks found, {} decoded ({} failed)",
        totals.blocks,
        totals.blocks_decoded,
        totals.blocks.saturating_sub(totals.blocks_decoded)
    );
    for (source, count) in &totals.blocks_by_source {
        println!("    {count:6}  {}", source.label());
    }
    println!("  (whether they run under the player's VM: examples/census.rs)");

    println!();
    println!("-- 5. opcodes: static histogram --");
    println!("  {} distinct opcodes appear", totals.opcodes.len());
    let mut ranked: Vec<(&u8, &u64)> = totals.opcodes.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    for (opcode, count) in ranked {
        println!(
            "    {count:7}  {:#04x} {}",
            opcode,
            opcode_name(*opcode).unwrap_or("<unnamed>")
        );
    }

    println!();
    println!("-- 6. screens: what each movie draws after {SETTLE_TICKS} ticks with no input --");
    println!(
        "  {:<32} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>4} {:>4}",
        "movie", "chars", "frm", "imp", "insts", "shapes", "text", "btn", "surf"
    );
    let mut screens: Vec<&Screen> = totals.screens.iter().collect();
    screens.sort_by_key(|s| s.name.clone());
    for screen in &screens {
        println!(
            "  {:<32} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>4} {:>4}",
            screen.name,
            screen.characters,
            screen.frames,
            screen.imports,
            screen.instances,
            screen.shapes,
            screen.text,
            screen.buttons,
            screen.engine_surfaces
        );
    }
    let drawing = screens.iter().filter(|s| s.draws_something()).count();
    let blank = screens.len().saturating_sub(drawing);
    println!(
        "  {drawing} of {} movies draw at least one shape or text field; {blank} draw nothing",
        screens.len()
    );
    let mut ranked: Vec<&&Screen> = screens.iter().collect();
    ranked.sort_by(|a, b| b.shapes.cmp(&a.shapes));
    println!("  richest screens by shapes drawn:");
    for screen in ranked.iter().take(10) {
        println!("    {:>5}  {}", screen.shapes, screen.name);
    }
    println!("  blank ones:");
    for screen in screens.iter().filter(|s| !s.draws_something()) {
        println!(
            "    {} ({} chars, {} frames, {} live instances)",
            screen.name, screen.characters, screen.frames, screen.instances
        );
    }
}
