#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_apt::{decode_block, parse_frames, AptMovie, Character, DecodeLimits, FrameItem};
use mint_sage_apt_player::{
    Halt, InstanceId, InstanceKind, Library, Limits, MovieId, Player, ScriptStats,
};
use mint_sage_apt_viewer::MovieLibrary;
use mint_sage_big::BigArchive;

const MAX_IMPORT_HOPS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Timeline(Option<u32>),
    ClipAction(u32),
    Button(u32),
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(at..end)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Some(u32::from_le_bytes(buf))
}

fn blocks(apt: &[u8], movie: &AptMovie) -> Vec<(u32, Context)> {
    let mut out = Vec::new();
    for (id, character) in movie.characters.iter().enumerate() {
        if let Character::Button(button) = character {
            for i in 0..button.action_count as usize {
                let Some(at) = (button.actions_offset as usize).checked_add(i.wrapping_mul(8))
                else {
                    continue;
                };
                if let Some(ptr) = u32_at(apt, at.wrapping_add(4)) {
                    out.push((ptr, Context::Button(u32::try_from(id).unwrap_or(u32::MAX))));
                }
            }
        }
    }
    let mut timelines = vec![(None, movie.record.frame_count, movie.record.frames_offset)];
    for (id, character) in movie.characters.iter().enumerate() {
        if let Character::Sprite(sprite) = character {
            timelines.push((
                Some(u32::try_from(id).unwrap_or(u32::MAX)),
                sprite.frame_count,
                sprite.frames_offset,
            ));
        }
    }
    for (owner, count, offset) in timelines {
        let Ok(frames) = parse_frames(apt, count, offset) else {
            continue;
        };
        for frame in &frames {
            for item in &frame.items {
                match item {
                    FrameItem::Action(a) => out.push((a.block_offset, Context::Timeline(owner))),
                    FrameItem::InitAction(i) => {
                        out.push((i.block_offset, Context::Timeline(owner)))
                    }
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
                                out.push((ptr, Context::ClipAction(poa)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Side {
    blocks: u64,
    ended: u64,
    halts: BTreeMap<String, u64>,
    this_blocks: u64,
}

#[derive(Default)]
struct After {
    side: Side,
    fell_back_to_root: u64,
    bytecode: BTreeMap<String, u64>,
    native: BTreeMap<String, u64>,
    unresolved: BTreeMap<String, (u64, BTreeSet<String>)>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(install) = args.first().map(PathBuf::from) else {
        eprintln!("usage: census <install>");
        std::process::exit(2);
    };
    let started = Instant::now();
    let names = MovieLibrary::new(&install).list_available();

    let mut library = Library::new();
    for name in &names {
        let Ok(archive) = BigArchive::open(install.join("apt").join(format!("{name}.big"))) else {
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
            let _ = library.add(name, &constant, apt);
        }
    }
    library.link();
    let library = Arc::new(library);

    let mut after = After::default();
    for name in &names {
        let mut loader = MovieLibrary::new(&install);
        if loader
            .load_imports_transitively(name, MAX_IMPORT_HOPS)
            .is_err()
        {
            continue;
        }
        let set = loader.movie_set();
        let Some(movie) = set.get(name).cloned() else {
            continue;
        };
        let Some(assets) = loader.assets(name) else {
            continue;
        };
        let movie_id = library.movie_id(name);
        for (offset, context) in blocks(&assets.apt, &movie) {
            if decode_block(&assets.apt, offset, &DecodeLimits::default()).is_err() {
                after.side.blocks += 1;
                *after
                    .side
                    .halts
                    .entry("decode failed".to_owned())
                    .or_default() += 1;
                continue;
            }
            after.side.blocks += 1;
            let Some(id) = movie_id else {
                *after
                    .side
                    .halts
                    .entry("movie not in the player library".to_owned())
                    .or_default() += 1;
                continue;
            };
            run_after(&library, id, name, offset, context, &mut after);
        }
    }

    report(&after, started.elapsed().as_secs_f64());
}

fn after_halt(h: &Halt) -> String {
    match h {
        Halt::Unimplemented { opcode, .. } => format!(
            "unimplemented {opcode:#04x} {}",
            mint_sage_apt::opcode_name(*opcode).unwrap_or("?")
        ),
        Halt::Budget { .. } => "instruction budget".to_owned(),
        Halt::StackUnderflow { .. } => "stack underflow".to_owned(),
        Halt::BadJump { .. } => "bad jump".to_owned(),
        Halt::BadIndex { .. } => "bad index".to_owned(),
        Halt::Limit { what } => format!("limit: {what}"),
        Halt::Decode => "decode".to_owned(),
    }
}

fn clip_for(player: &Player, context: Context) -> Option<InstanceId> {
    let root = player.root();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let inst = player.instance(id)?;
        let hit = match (context, inst.kind()) {
            (Context::Timeline(None), _) => id == root,
            (Context::Timeline(Some(ch)), InstanceKind::Clip(_)) => {
                id != root && inst.character().id == ch
            }
            (Context::ClipAction(poa), InstanceKind::Clip(c)) => {
                c.clip_actions.is_some_and(|(_, o)| o == poa)
            }
            (Context::Button(ch), InstanceKind::Button(_)) => inst.character().id == ch,
            _ => false,
        };
        if hit {
            return match context {
                Context::Button(_) => inst.parent(),
                _ => Some(id),
            };
        }
        stack.extend(inst.children().map(|(_, c)| c));
    }
    None
}

fn run_after(
    library: &Arc<Library>,
    id: MovieId,
    movie: &str,
    offset: u32,
    context: Context,
    after: &mut After,
) {
    let Ok(mut player) = Player::new(Arc::clone(library), id, Limits::default()) else {
        *after
            .side
            .halts
            .entry("player failed".to_owned())
            .or_default() += 1;
        return;
    };
    let _ = player.tick();
    let _ = player.take_script_stats();
    player.set_record_calls(true);
    let clip = clip_for(&player, context).unwrap_or_else(|| {
        after.fell_back_to_root += 1;
        player.root()
    });
    let result = player.run_block(clip, id, offset);
    let s: ScriptStats = player.take_script_stats();
    match result {
        Ok(()) => after.side.ended += 1,
        Err(h) => *after.side.halts.entry(after_halt(&h)).or_default() += 1,
    }
    if s.this_undefined > 0 {
        after.side.this_blocks += 1;
    }
    for (k, n) in s.bytecode_calls {
        *after.bytecode.entry(k).or_default() += n;
    }
    for (k, n) in s.native_calls {
        *after.native.entry(k).or_default() += n;
    }
    for (k, n) in s.unresolved_calls {
        let e = after.unresolved.entry(k).or_default();
        e.0 += n;
        e.1.insert(movie.to_owned());
    }
}

fn pct(n: u64, of: u64) -> f64 {
    100.0 * n as f64 / of.max(1) as f64
}

fn report(after: &After, secs: f64) {
    let blocks = after.side.blocks;
    println!("census over {blocks} blocks ({secs:.1}s)\n");
    println!(
        "{:<44} {:>8} {:>5.1}%",
        "blocks run to their end",
        after.side.ended,
        pct(after.side.ended, blocks)
    );
    println!(
        "{:<44} {:>8}",
        "blocks where `this` is undefined when pushed", after.side.this_blocks
    );
    let unresolved: u64 = after.unresolved.values().map(|v| v.0).sum();
    println!(
        "{:<44} {:>8} ({} names)",
        "calls nothing callable answered",
        unresolved,
        after.unresolved.len()
    );
    println!(
        "{:<44} {:>8}",
        "calls into the movie's own functions",
        after.bytecode.values().sum::<u64>()
    );
    println!(
        "{:<44} {:>8}",
        "calls into built-ins",
        after.native.values().sum::<u64>()
    );
    println!(
        "{} blocks ran as the root because no instance of their clip existed after one tick",
        after.fell_back_to_root
    );
    println!("\nhalts:");
    for (h, n) in &after.side.halts {
        println!("  {n:6}  {h}");
    }
    let mut still: Vec<_> = after.unresolved.iter().collect();
    still.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    println!("\nevery name nothing callable answered ({}):", still.len());
    for (name, (calls, movies)) in still {
        println!("  {calls:6}  {name:<32} in {} movie(s)", movies.len());
    }
}
