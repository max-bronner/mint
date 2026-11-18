use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mint_sage_apt::{
    decode_block, opcode_name, parse_const, parse_frames, preload_register_base, AptMovie, Block,
    Character, ConstItem, DecodeLimits, Frame, FrameItem, Function, Instruction, Op,
};
use mint_sage_big::BigArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = bytes.get(offset..end)?;
    let array: [u8; 4] = slice.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

struct RawMovie {
    name: String,
    const_bytes: Vec<u8>,
    apt_bytes: Vec<u8>,
}

fn load_raw(path: &Path) -> Result<RawMovie, String> {
    let archive = BigArchive::open(path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .ok_or_else(|| "no file stem".to_string())?;
    let mut apt_bytes = None;
    let mut const_bytes = None;
    for entry in archive.entries() {
        let lower = entry.path.as_str().to_ascii_lowercase();
        if lower.ends_with(".apt") {
            apt_bytes = Some(archive.read_entry(entry).map_err(|e| e.to_string())?);
        } else if lower.ends_with(".const") {
            const_bytes = Some(archive.read_entry(entry).map_err(|e| e.to_string())?);
        }
    }
    Ok(RawMovie {
        name,
        const_bytes: const_bytes.ok_or("no .const entry")?,
        apt_bytes: apt_bytes.ok_or("no .apt entry")?,
    })
}

fn big_files(install_root: &str) -> Vec<PathBuf> {
    let dir = PathBuf::from(install_root).join("apt");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("big"))
        })
        .collect();
    files.sort();
    files
}

fn block_addresses(apt: &[u8], movie: &AptMovie) -> Vec<(u32, Source)> {
    let mut out = Vec::new();
    let mut timelines: Vec<(u32, u32)> =
        vec![(movie.record.frame_count, movie.record.frames_offset)];
    for character in &movie.characters {
        match character {
            Character::Sprite(sprite) => timelines.push((sprite.frame_count, sprite.frames_offset)),
            Character::Button(button) => {
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
            _ => {}
        }
    }
    for (count, offset) in timelines {
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

#[derive(Default)]
struct Census {
    movies: usize,
    blocks_attempted: u64,
    blocks_decoded: u64,
    distinct_block_addresses: u64,
    instructions: u64,
    by_source: BTreeMap<&'static str, (u64, u64)>,
    opcodes: BTreeMap<u8, u64>,
    failures: Vec<String>,

    functions: u64,
    functions_exact: u64,
    functions2: u64,
    functions_interior_end: u64,
    functions_magic_ok: u64,
    functions_ending_in_end: u64,
    functions_containing_a_function: u64,
    functions_top_level: u64,
    functions_global_dedup: u64,
    global_addresses: BTreeSet<u32>,
    fn2_flag_hist: BTreeMap<u32, u64>,
    fn2_with_params: u64,
    fn2_first_param_matches_preload: u64,
    fn2_first_param_mismatch: Vec<String>,

    const_items: u64,
    const_referenced: u64,
    const_out_of_range: u64,
    const_by_type: BTreeMap<&'static str, u64>,
    pushdata_instructions: u64,
    pushdata_elements: u64,
    pushdata_count_not_one: u64,
    constantpool_instructions: u64,
    constantpool_entries: u64,

    branches: u64,
    branches_on_boundary: u64,
    branch_misses: Vec<String>,
    branch_alt_base_hits: u64,

    ea56_sites: Vec<String>,
    ea58_sites: Vec<String>,
}

fn census_movie(census: &mut Census, label: &str, raw: &RawMovie, movie: &AptMovie) {
    let limits = DecodeLimits::default();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut referenced: BTreeSet<u32> = BTreeSet::new();
    let const_items = parse_const(&raw.const_bytes)
        .map(|c| c.items)
        .unwrap_or_default();
    census.const_items = census.const_items.saturating_add(const_items.len() as u64);

    for (address, source) in block_addresses(&raw.apt_bytes, movie) {
        census.blocks_attempted += 1;
        let entry = census.by_source.entry(source.label()).or_insert((0, 0));
        entry.0 += 1;
        let globally_new = census.global_addresses.insert(address);
        let is_new = seen.insert(address);
        if is_new {
            census.distinct_block_addresses += 1;
        }
        match decode_block(&raw.apt_bytes, address, &limits) {
            Ok(block) => {
                census.blocks_decoded += 1;
                let mut instrs = 0u64;
                walk(
                    &block,
                    &block.code,
                    block.end,
                    false,
                    census,
                    &mut instrs,
                    &mut referenced,
                    &const_items,
                    label,
                    &raw.name,
                );
                let nested: BTreeSet<usize> = block
                    .functions
                    .iter()
                    .flat_map(|outer| {
                        block
                            .functions
                            .iter()
                            .enumerate()
                            .filter_map(move |(i, f)| {
                                let inside = f.body_start > outer.body_start
                                    && f.body_start
                                        < outer.body_start.saturating_add(outer.body_size);
                                inside.then_some(i)
                            })
                    })
                    .collect();
                for (index, f) in block.functions.iter().enumerate() {
                    census.functions += 1;
                    if globally_new {
                        census.functions_global_dedup += 1;
                    }
                    if !nested.contains(&index) {
                        census.functions_top_level += 1;
                    }
                    if f.body_end_exact {
                        census.functions_exact += 1;
                    }
                    if f.is_function2 {
                        census.functions2 += 1;
                        *census.fn2_flag_hist.entry(f.flags).or_insert(0) += 1;
                        check_preload(census, f, label, &raw.name);
                    }
                    if f.interior_end {
                        census.functions_interior_end += 1;
                    }
                    if f.trailing_magic == (0x9876_5432, 0x1234_5678) {
                        census.functions_magic_ok += 1;
                    }
                    let mut body_instrs = 0u64;
                    let body_end = f.body_start.saturating_add(f.body_size);
                    walk(
                        &block,
                        &f.body,
                        body_end,
                        true,
                        census,
                        &mut body_instrs,
                        &mut referenced,
                        &const_items,
                        label,
                        &raw.name,
                    );
                    instrs = instrs.saturating_add(body_instrs);
                    if f.body.last().is_some_and(|i| matches!(i.op, Op::End)) {
                        census.functions_ending_in_end += 1;
                    }
                    if f.body
                        .iter()
                        .any(|i| matches!(i.op, Op::DefineFunction(_) | Op::DefineFunction2(_)))
                    {
                        census.functions_containing_a_function += 1;
                    }
                }
                let entry = census.by_source.entry(source.label()).or_insert((0, 0));
                entry.1 += instrs;
                census.instructions += instrs;
            }
            Err(e) => {
                if census.failures.len() < 40 {
                    census.failures.push(format!(
                        "{label} {} {} block at {address:#x}: {e}",
                        raw.name,
                        source.label()
                    ));
                }
            }
        }
    }
    census.const_referenced = census
        .const_referenced
        .saturating_add(referenced.len() as u64);
    for index in &referenced {
        match const_items.get(*index as usize) {
            Some(ConstItem::Text(_)) => *census.const_by_type.entry("string").or_insert(0) += 1,
            Some(ConstItem::Number(_)) => *census.const_by_type.entry("number").or_insert(0) += 1,
            None => census.const_out_of_range += 1,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    block: &Block,
    code: &[Instruction],
    region_end: u32,
    in_function_body: bool,
    census: &mut Census,
    instrs: &mut u64,
    referenced: &mut BTreeSet<u32>,
    const_items: &[ConstItem],
    label: &str,
    movie: &str,
) {
    let boundaries: BTreeSet<u32> = code.iter().map(|i| i.offset).collect();
    let _ = in_function_body;
    for instruction in code.iter() {
        *instrs += 1;
        *census.opcodes.entry(instruction.op.opcode()).or_insert(0) += 1;
        match &instruction.op {
            Op::ConstantPool(list) => {
                census.constantpool_instructions += 1;
                census.constantpool_entries += list.len() as u64;
                referenced.extend(list.iter().copied());
            }
            Op::PushData(list) => {
                census.pushdata_instructions += 1;
                census.pushdata_elements += list.len() as u64;
                if list.len() != 1 {
                    census.pushdata_count_not_one += 1;
                }
                referenced.extend(list.iter().copied());
            }
            Op::EaAction56 => {
                if census.ea56_sites.len() < 20 {
                    census.ea56_sites.push(format!(
                        "{label} {movie} block {:#x}: EA_ACTION56 at {:#x} (in a function body: {})",
                        block.start,
                        instruction.offset,
                        !std::ptr::eq(code.as_ptr(), block.code.as_ptr())
                    ));
                }
            }
            Op::EaAction58 => {
                if census.ea58_sites.len() < 20 {
                    census.ea58_sites.push(format!(
                        "{label} {movie} block {:#x}: EA_ACTION58 at {:#x}",
                        block.start, instruction.offset
                    ));
                }
            }
            _ => {}
        }
        let _ = const_items;
        if let Some(delta) = branch_delta(&instruction.op) {
            census.branches += 1;
            let after = Some(
                (instruction.offset.saturating_add(1).saturating_add(3) & !3u32).saturating_add(4),
            );
            let target = after.and_then(|a| add_signed(a, delta));
            let hit = target.is_some_and(|t| boundaries.contains(&t) || t == region_end);
            let alt =
                add_signed(instruction.offset, delta).is_some_and(|t| boundaries.contains(&t));
            if hit {
                census.branches_on_boundary += 1;
            } else if census.branch_misses.len() < 20 {
                census.branch_misses.push(format!(
                    "{label} {movie} block {:#x}: branch at {:#x} delta {delta} -> {target:?} not an instruction boundary (region ends {region_end:#x})",
                    block.start, instruction.offset
                ));
            }
            if alt {
                census.branch_alt_base_hits += 1;
            }
        }
    }
}

fn branch_delta(op: &Op) -> Option<i32> {
    match *op {
        Op::BranchAlways(d) | Op::BranchIfTrue(d) | Op::EaBranchIfFalse(d) => Some(d),
        _ => None,
    }
}

fn add_signed(base: u32, delta: i32) -> Option<u32> {
    if delta >= 0 {
        base.checked_add(delta.unsigned_abs())
    } else {
        base.checked_sub(delta.unsigned_abs())
    }
}

fn check_preload(census: &mut Census, f: &Function, label: &str, movie: &str) {
    let registers: Vec<u32> = f
        .params
        .iter()
        .map(|p| p.register)
        .filter(|r| *r > 0)
        .collect();
    let Some(lowest) = registers.iter().min().copied() else {
        return;
    };
    census.fn2_with_params += 1;
    let base = preload_register_base(f.swf_flags());
    let within = registers.iter().all(|r| *r < f.register_count());
    if lowest >= base && within {
        census.fn2_first_param_matches_preload += 1;
    } else if census.fn2_first_param_mismatch.len() < 20 {
        census.fn2_first_param_mismatch.push(format!(
            "{label} {movie} fn `{}` flags {:#x}: argument registers {registers:?}, preload base {base}, count {}",
            f.name,
            f.flags,
            f.register_count()
        ));
    }
}

fn run_install(label: &str, root: &str) {
    if root == "-" {
        return;
    }
    let mut census = Census::default();
    for path in big_files(root) {
        let raw = match load_raw(&path) {
            Ok(r) => r,
            Err(e) => {
                println!("  LOAD-FAIL {label} {}: {e}", path.display());
                continue;
            }
        };
        let Ok(movie) = AptMovie::load(&raw.name, &raw.const_bytes, &raw.apt_bytes) else {
            println!("  LOAD-FAIL {label} {}", raw.name);
            continue;
        };
        census.movies += 1;
        census_movie(&mut census, label, &raw, &movie);
    }

    println!("\n== {label}: decode ==");
    println!(
        "  {} movies, {} action blocks attempted, {} decoded, {} failed ({} distinct block addresses)",
        census.movies,
        census.blocks_attempted,
        census.blocks_decoded,
        census.blocks_attempted.saturating_sub(census.blocks_decoded),
        census.distinct_block_addresses
    );
    println!(
        "  the same addresses pooled across the whole install, ignoring which .apt they belong to: {}",
        census.global_addresses.len()
    );
    for f in census.failures.iter().take(40) {
        println!("  DECODE-FAIL {f}");
    }
    println!(
        "  {} instructions, {} distinct opcodes",
        census.instructions,
        census.opcodes.len()
    );
    print!("  by source:");
    for (source, (blocks, instrs)) in &census.by_source {
        print!(" {source} {blocks}/{instrs}");
    }
    println!();

    println!("\n== {label}: function-size invariant ==");
    println!(
        "  {} function bodies close exactly on their declared size, of {} decoded ({} DefineFunction2, {} DefineFunction)",
        census.functions_exact,
        census.functions,
        census.functions2,
        census.functions.saturating_sub(census.functions2)
    );
    println!(
        "  {} of {} function headers carry the trailing word pair (0x98765432, 0x12345678)",
        census.functions_magic_ok, census.functions
    );
    println!(
        "  {} of {} bodies end in an End opcode; {} contain a nested function definition; {} functions are not themselves nested inside another",
        census.functions_ending_in_end,
        census.functions,
        census.functions_containing_a_function,
        census.functions_top_level
    );
    println!(
        "  counted only over blocks whose address is new across the whole install: {}",
        census.functions_global_dedup
    );
    println!(
        "  {} bodies contain an End before their declared end (decoding deliberately continues past it)",
        census.functions_interior_end
    );
    println!(
        "  DefineFunction2 flags word (count | flags << 16) vs register arguments: {} of {} functions-with-register-arguments agree",
        census.fn2_first_param_matches_preload, census.fn2_with_params
    );
    for m in census.fn2_first_param_mismatch.iter().take(10) {
        println!("    PRELOAD-MISMATCH {m}");
    }
    let mut flags: Vec<(&u32, &u64)> = census.fn2_flag_hist.iter().collect();
    flags.sort_by(|a, b| b.1.cmp(a.1));
    print!("  DefineFunction2 flags histogram:");
    for (value, count) in flags.iter().take(12) {
        print!(" {value:#x}={count}");
    }
    println!();

    println!("\n== {label}: .const cross-check ==");
    println!(
        "  {} items, {} referenced, {} out of range; PushData {} instructions / {} elements ({} with a count != 1); ConstantPool {} instructions / {} entries",
        census.const_items,
        census.const_referenced,
        census.const_out_of_range,
        census.pushdata_instructions,
        census.pushdata_elements,
        census.pushdata_count_not_one,
        census.constantpool_instructions,
        census.constantpool_entries
    );
    println!("  referenced items by type: {:?}", census.const_by_type);
    println!(
        "  items never referenced: {}",
        census.const_items.saturating_sub(census.const_referenced)
    );

    println!("\n== {label}: branch targets ==");
    println!(
        "  {} of {} branches land on an instruction boundary when taken relative to the end of the branch instruction",
        census.branches_on_boundary, census.branches
    );
    println!(
        "  control - the same branches relative to the branch's own opcode byte: {} land on a boundary",
        census.branch_alt_base_hits
    );
    for m in census.branch_misses.iter().take(10) {
        println!("    BRANCH-MISS {m}");
    }

    println!("\n== {label}: the two unnamed EA opcodes ==");
    println!(
        "  EA_ACTION56 (0x56): {} occurrences; EA_ACTION58 (0x58): {} occurrences",
        census.opcodes.get(&0x56).copied().unwrap_or(0),
        census.opcodes.get(&0x58).copied().unwrap_or(0)
    );
    for s in census.ea56_sites.iter().take(10) {
        println!("    {s}");
    }
    for s in census.ea58_sites.iter().take(10) {
        println!("    {s}");
    }

    println!(
        "\n== {label}: opcode histogram (all {} distinct) ==",
        census.opcodes.len()
    );
    let mut hist: Vec<(&u8, &u64)> = census.opcodes.iter().collect();
    hist.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (opcode, count) in &hist {
        println!(
            "  {:>7}  {:#04x} {}",
            count,
            opcode,
            opcode_name(**opcode).unwrap_or("<unnamed>")
        );
    }
}

fn dump(install: &str, movie: &str, address: u32) {
    let path = PathBuf::from(install)
        .join("apt")
        .join(format!("{movie}.big"));
    let raw = match load_raw(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return;
        }
    };
    let apt = &raw.apt_bytes;
    println!("{} .apt is {} bytes", raw.name, apt.len());
    let start = address as usize;
    let end = (start + 256).min(apt.len());
    let window = apt.get(start..end).unwrap_or(&[]);
    for (i, chunk) in window.chunks(16).enumerate() {
        print!("{:#08x} ", start + i * 16);
        for b in chunk {
            print!("{b:02x} ");
        }
        println!();
    }
    let consts = parse_const(&raw.const_bytes)
        .map(|c| c.items)
        .unwrap_or_default();
    match decode_block(apt, address, &DecodeLimits::default()) {
        Ok(block) => {
            print_code(&block.code, &consts, "  ");
            for f in &block.functions {
                println!(
                    "  fn `{}` flags {:#x} size {} params {:?}",
                    f.name, f.flags, f.body_size, f.params
                );
                print_code(&f.body, &consts, "      ");
            }
        }
        Err(e) => println!("  DECODE FAILED: {e}"),
    }
}

fn print_code(code: &[Instruction], consts: &[ConstItem], indent: &str) {
    let mut pool: Vec<String> = Vec::new();
    for i in code {
        let mut note = String::new();
        if let Op::ConstantPool(list) = &i.op {
            pool = list
                .iter()
                .map(|index| match consts.get(*index as usize) {
                    Some(ConstItem::Text(t)) => t.clone(),
                    Some(ConstItem::Number(n)) => n.to_string(),
                    None => "<out of range>".to_string(),
                })
                .collect();
            note = format!("  pool = {pool:?}");
        }
        let pooled = |index: u8| -> String {
            pool.get(index as usize)
                .map_or_else(|| " -> <no pool>".to_string(), |v| format!(" -> {v:?}"))
        };
        match i.op {
            Op::EaPushConstant(v)
            | Op::EaPushValueOfVar(v)
            | Op::EaGetNamedMember(v)
            | Op::EaCallNamedFunction(v)
            | Op::EaCallNamedFunctionPop(v)
            | Op::EaCallNamedMethod(v)
            | Op::EaCallNamedMethodPop(v) => note = pooled(v),
            Op::PushData(ref list) => {
                let values: Vec<String> = list
                    .iter()
                    .map(|index| match consts.get(*index as usize) {
                        Some(ConstItem::Text(t)) => format!("{t:?}"),
                        Some(ConstItem::Number(n)) => n.to_string(),
                        None => "<out of range>".to_string(),
                    })
                    .collect();
                note = format!(" -> {values:?}");
            }
            _ => {}
        }
        println!("{indent}{:#08x}  {:?}{note}", i.offset, i.op);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--dump") {
        let (Some(install), Some(movie), Some(addr)) = (args.get(1), args.get(2), args.get(3))
        else {
            eprintln!("usage: bytecode --dump <install> <movie> <hex address>");
            std::process::exit(1);
        };
        let address = u32::from_str_radix(addr.trim_start_matches("0x"), 16).unwrap_or(0);
        dump(install, movie, address);
        return;
    }
    let bfme2 = args.first().cloned().unwrap_or_else(|| "-".to_string());
    let rotwk = args.get(1).cloned().unwrap_or_else(|| "-".to_string());
    if bfme2 == "-" && rotwk == "-" {
        eprintln!("usage: bytecode <bfme2 install> <rotwk install>  ('-' to skip either)");
        std::process::exit(1);
    }
    run_install("BFME2", &bfme2);
    run_install("ROTWK", &rotwk);
}
