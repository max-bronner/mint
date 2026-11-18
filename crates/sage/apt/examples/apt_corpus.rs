use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mint_sage_apt::{
    accumulate_timeline, parse_const, parse_dat, parse_frames, parse_ru, resolve_import,
    shared_atlas_path, standalone_texture_path, AptMovie, Character, Frame, FrameItem, MovieSet,
    TextureLocation,
};
use mint_sage_big::BigArchive;

struct RawMovie {
    name: String,
    const_bytes: Vec<u8>,
    apt_bytes: Vec<u8>,
    dat_text: Option<String>,
    ru_files: Vec<(String, String)>,
    tga_names: BTreeSet<String>,
}

fn load_raw(path: &Path) -> Result<RawMovie, String> {
    let archive = BigArchive::open(path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .ok_or_else(|| "no file stem".to_string())?;
    let mut apt_bytes = None;
    let mut const_bytes = None;
    let mut dat_text = None;
    let mut ru_files = Vec::new();
    let mut tga_names = BTreeSet::new();
    for entry in archive.entries() {
        let full = entry.path.as_str();
        let lower = full.to_ascii_lowercase();
        if lower.ends_with(".apt") {
            apt_bytes = Some(archive.read_entry(entry).map_err(|e| e.to_string())?);
        } else if lower.ends_with(".const") {
            const_bytes = Some(archive.read_entry(entry).map_err(|e| e.to_string())?);
        } else if lower.ends_with(".dat") {
            let bytes = archive.read_entry(entry).map_err(|e| e.to_string())?;
            dat_text = Some(String::from_utf8_lossy(&bytes).into_owned());
        } else if lower.ends_with(".ru") {
            let bytes = archive.read_entry(entry).map_err(|e| e.to_string())?;
            ru_files.push((
                entry.path.as_str().to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        } else if lower.ends_with(".tga") {
            let base = lower
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&lower)
                .to_string();
            tga_names.insert(base);
        }
    }
    Ok(RawMovie {
        name,
        const_bytes: const_bytes.ok_or("no .const entry")?,
        apt_bytes: apt_bytes.ok_or("no .apt entry")?,
        tga_names,
        dat_text,
        ru_files,
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

#[derive(Default)]
struct InstallStats {
    movies: Vec<AptMovie>,
    load_failures: Vec<String>,
    ru_ok: usize,
    ru_failures: Vec<String>,
    dat_ok: usize,
    dat_failures: Vec<String>,
    ru_entry_count: usize,
    ru_run_count: usize,
    atlas_present: usize,
    atlas_total: usize,
    atlas_missing: Vec<String>,
    apt_bytes: std::collections::BTreeMap<String, Vec<u8>>,
}

fn scan_install(label: &str, root: &str) -> InstallStats {
    let mut stats = InstallStats::default();
    if root == "-" {
        return stats;
    }
    for path in big_files(root) {
        let raw = match load_raw(&path) {
            Ok(r) => r,
            Err(e) => {
                stats
                    .load_failures
                    .push(format!("{label} {}: {e}", path.display()));
                continue;
            }
        };
        for (ru_name, ru_text) in &raw.ru_files {
            match parse_ru(ru_text) {
                Ok(file) => {
                    stats.ru_ok += 1;
                    stats.ru_entry_count += file.entries.len();
                    stats.ru_run_count += file.entries.iter().map(|e| e.runs.len()).sum::<usize>();
                }
                Err(e) => stats
                    .ru_failures
                    .push(format!("{label} {}/{ru_name}: {e}", raw.name)),
            }
        }
        if let Some(dat_text) = &raw.dat_text {
            match parse_dat(dat_text) {
                Ok(entries) => {
                    stats.dat_ok += 1;
                    for entry in &entries {
                        stats.atlas_total += 1;
                        let path = match entry.location {
                            TextureLocation::Shared { page } => shared_atlas_path(&raw.name, page),
                            TextureLocation::Standalone { .. } => {
                                standalone_texture_path(&raw.name, entry.id)
                            }
                        };
                        let base = path.rsplit('/').next().unwrap_or(&path).to_string();
                        if raw.tga_names.contains(&base) {
                            stats.atlas_present += 1;
                        } else {
                            stats
                                .atlas_missing
                                .push(format!("{label} {}: {base}", raw.name));
                        }
                    }
                }
                Err(e) => stats
                    .dat_failures
                    .push(format!("{label} {}.dat: {e}", raw.name)),
            }
        }
        match AptMovie::load(&raw.name, &raw.const_bytes, &raw.apt_bytes) {
            Ok(movie) => {
                stats
                    .apt_bytes
                    .insert(raw.name.clone(), raw.apt_bytes.clone());
                stats.movies.push(movie);
            }
            Err(e) => stats
                .load_failures
                .push(format!("{label} {}: {e}", raw.name)),
        }
    }
    stats
}

fn const_reaches_valid_movie(root: &str) -> (usize, usize) {
    if root == "-" {
        return (0, 0);
    }
    let mut ok = 0;
    let mut total = 0;
    for path in big_files(root) {
        let Ok(raw) = load_raw(&path) else { continue };
        total += 1;
        let reached = parse_const(&raw.const_bytes).ok().and_then(|cst| {
            mint_sage_apt::parse_movie(&raw.apt_bytes, cst.movie_offset as usize).ok()
        });
        if reached.is_some() {
            ok += 1;
        }
    }
    (ok, total)
}

fn build_set(movies: Vec<AptMovie>) -> MovieSet {
    let mut set = MovieSet::new();
    for movie in movies {
        set.insert(movie);
    }
    set
}

fn resolve_all(set: &MovieSet) -> (usize, usize) {
    let mut resolved = 0;
    let mut total = 0;
    for movie in set.movies() {
        for imp in &movie.imports {
            total += 1;
            if resolve_import(set, imp).is_ok() {
                resolved += 1;
            }
        }
    }
    (resolved, total)
}

fn negative_control_wrong_movie(set: &MovieSet) -> (usize, usize) {
    let names: Vec<&str> = set.movies().map(|m| m.name.as_str()).collect();
    let mut resolved = 0;
    let mut total = 0;
    for movie in set.movies() {
        for imp in &movie.imports {
            total += 1;
            let idx = names
                .iter()
                .position(|n| *n == imp.movie.to_ascii_lowercase())
                .unwrap_or(0);
            let wrong_name = names
                .get((idx + 1) % names.len().max(1))
                .copied()
                .unwrap_or("");
            let mut wrong_imp = imp.clone();
            wrong_imp.movie = wrong_name.to_string();
            if resolve_import(set, &wrong_imp).is_ok() {
                resolved += 1;
            }
        }
    }
    (resolved, total)
}

fn arbitrary_export_slot_control(set: &MovieSet) -> (usize, usize) {
    let mut resolved = 0;
    let mut total = 0;
    for movie in set.movies() {
        for (i, imp) in movie.imports.iter().enumerate() {
            total += 1;
            let Some(target) = set.get(&imp.movie) else {
                continue;
            };
            if target.exports.is_empty() {
                continue;
            }
            let Some(export) = target.exports.get(i % target.exports.len()) else {
                continue;
            };
            if let Some(Character::Placeholder) = target.characters.get(export.character as usize) {
            } else if target.characters.get(export.character as usize).is_some() {
                resolved += 1;
            }
        }
    }
    (resolved, total)
}

fn placeholder_invariant(set: &MovieSet) -> (usize, usize, Vec<String>) {
    let mut ok = 0;
    let mut total = 0;
    let mut mismatches = Vec::new();
    for movie in set.movies() {
        total += 1;
        let placeholders: BTreeSet<u32> = movie
            .characters
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, Character::Placeholder))
            .map(|(i, _)| u32::try_from(i).unwrap_or(u32::MAX))
            .collect();
        let import_ids: BTreeSet<u32> = movie.imports.iter().map(|i| i.local_character).collect();
        if placeholders == import_ids {
            ok += 1;
        } else {
            mismatches.push(format!(
                "{}: {} placeholder slots, {} import ids, symmetric difference {}",
                movie.name,
                placeholders.len(),
                import_ids.len(),
                placeholders.symmetric_difference(&import_ids).count()
            ));
        }
    }
    (ok, total, mismatches)
}

const CENSUS_MAX_DEPTH: u32 = 16;

#[derive(Default)]
struct TimelineCensus {
    frames: u64,
    place_object: u64,
    remove_object: u64,
    action: u64,
    frame_label: u64,
    background_color: u64,
    init_action: u64,
    unknown_items: u64,
    flag_move: u64,
    flag_has_character: u64,
    flag_has_matrix: u64,
    flag_has_color_transform: u64,
    flag_has_ratio: u64,
    flag_has_name: u64,
    flag_has_clip_depth: u64,
    flag_has_clip_actions: u64,
    cx_nonidentity: u64,
    cx_rgb_equal: u64,
    cx_alpha_hist: std::collections::BTreeMap<u8, u64>,
    cx_add_nonzero: u64,
    cx_add_top_byte_zero: u64,
    cx_add_distinct_nongrey: Vec<u32>,
    buttons: u64,
    button_records: u64,
    button_flag_hist: std::collections::BTreeMap<u32, u64>,
}

#[derive(Default)]
struct TextCensus {
    edittext: u64,
    edittext_with_text: u64,
    edittext_with_variable: u64,
    edittext_empty_without_variable_resolved: u64,
    text_characters: u64,
    text_records: u64,
    text_glyphs: u64,
    font_characters: u64,
    font_names: std::collections::BTreeMap<String, u64>,
    font_unnamed: u64,
    font_glyph_table_entries: u64,
}

fn census_text(movie: &AptMovie, census: &mut TextCensus) {
    for character in &movie.characters {
        match character {
            Character::EditText { text, variable, .. } => {
                census.edittext += 1;
                let has_text = text.as_deref().is_some_and(|t| !t.is_empty());
                if has_text {
                    census.edittext_with_text += 1;
                }
                if variable.is_some() {
                    census.edittext_with_variable += 1;
                }
                if !has_text && variable.is_some() {
                    census.edittext_empty_without_variable_resolved += 1;
                }
            }
            Character::Text { records, .. } => {
                census.text_characters += 1;
                census.text_records += records.len() as u64;
                census.text_glyphs += records.iter().map(|r| r.glyphs.len() as u64).sum::<u64>();
            }
            Character::Font { name, glyphs } => {
                census.font_characters += 1;
                census.font_glyph_table_entries += glyphs.len() as u64;
                match name {
                    Some(n) => *census.font_names.entry(n.clone()).or_insert(0) += 1,
                    None => census.font_unnamed += 1,
                }
            }
            _ => {}
        }
    }
}

fn census_movie(apt: &[u8], movie: &AptMovie, census: &mut TimelineCensus) {
    if let Ok(frames) = parse_frames(apt, movie.record.frame_count, movie.record.frames_offset) {
        census_frames(&frames, census);
    }
    for character in &movie.characters {
        match character {
            Character::Sprite(sprite) => {
                if let Ok(frames) = parse_frames(apt, sprite.frame_count, sprite.frames_offset) {
                    census_frames(&frames, census);
                }
            }
            Character::Button(button) => {
                census.buttons += 1;
                census.button_records += button.records.len() as u64;
                for r in &button.records {
                    *census.button_flag_hist.entry(r.flags).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
}

fn census_frames(frames: &[Frame], census: &mut TimelineCensus) {
    for frame in frames {
        census.frames += 1;
        for item in &frame.items {
            match item {
                FrameItem::PlaceObject(po) => {
                    census.place_object += 1;
                    if po.flags.move_existing() {
                        census.flag_move += 1;
                    }
                    if po.flags.has_character() {
                        census.flag_has_character += 1;
                    }
                    if po.flags.has_matrix() {
                        census.flag_has_matrix += 1;
                    }
                    if po.flags.has_color_transform() {
                        census.flag_has_color_transform += 1;
                    }
                    if po.flags.has_ratio() {
                        census.flag_has_ratio += 1;
                    }
                    if po.flags.has_name() {
                        census.flag_has_name += 1;
                    }
                    if po.flags.has_clip_depth() {
                        census.flag_has_clip_depth += 1;
                    }
                    if po.flags.has_clip_actions() {
                        census.flag_has_clip_actions += 1;
                    }
                    if let Some(cx) = po.color_transform {
                        if cx != 0xFFFF_FFFF {
                            census.cx_nonidentity += 1;
                            let bytes = cx.to_le_bytes();
                            if bytes[0] == bytes[1] && bytes[1] == bytes[2] {
                                census.cx_rgb_equal += 1;
                            }
                            *census.cx_alpha_hist.entry(bytes[3]).or_insert(0) += 1;
                        }
                    }
                    if let Some(add) = po.color_add {
                        if add != 0 {
                            census.cx_add_nonzero += 1;
                            let bytes = add.to_le_bytes();
                            if bytes[3] == 0 {
                                census.cx_add_top_byte_zero += 1;
                            }
                            let nongrey = !(bytes[0] == bytes[1] && bytes[1] == bytes[2]);
                            if nongrey
                                && census.cx_add_distinct_nongrey.len() < 8
                                && !census.cx_add_distinct_nongrey.contains(&add)
                            {
                                census.cx_add_distinct_nongrey.push(add);
                            }
                        }
                    }
                }
                FrameItem::RemoveObject(_) => census.remove_object += 1,
                FrameItem::Action(_) => census.action += 1,
                FrameItem::FrameLabel(_) => census.frame_label += 1,
                FrameItem::BackgroundColor(_) => census.background_color += 1,
                FrameItem::InitAction(_) => census.init_action += 1,
                FrameItem::Unknown { .. } => census.unknown_items += 1,
            }
        }
    }
}

#[derive(Default)]
struct NoImportSweep {
    movies: usize,
    frames: u64,
    place_object_records: u64,
    character_refs: u64,
    character_refs_resolved: u64,
    character_refs_failed: u64,
    orphan_moves: u64,
    failures: Vec<String>,
}

fn sweep_no_import(stats: &InstallStats, label: &str) -> NoImportSweep {
    let mut sweep = NoImportSweep::default();
    for movie in &stats.movies {
        if !movie.imports.is_empty() {
            continue;
        }
        let Some(apt) = stats.apt_bytes.get(&movie.name) else {
            continue;
        };
        let Ok(top_frames) =
            parse_frames(apt, movie.record.frame_count, movie.record.frames_offset)
        else {
            sweep.failures.push(format!(
                "{label} {}: top-level frames failed to parse",
                movie.name
            ));
            continue;
        };
        sweep.movies += 1;
        sweep_timeline(
            apt,
            &movie.characters,
            &top_frames,
            CENSUS_MAX_DEPTH,
            movie,
            label,
            &mut sweep,
        );
    }
    sweep
}

#[allow(clippy::too_many_arguments)]
fn sweep_timeline(
    apt: &[u8],
    characters: &[Character],
    frames: &[Frame],
    depth_budget: u32,
    movie: &AptMovie,
    label: &str,
    sweep: &mut NoImportSweep,
) {
    if depth_budget == 0 {
        return;
    }
    let timeline = accumulate_timeline(frames);
    sweep.frames += timeline.len() as u64;
    for tf in &timeline {
        sweep.orphan_moves += u64::from(tf.orphan_moves);
    }
    for frame in frames {
        for item in &frame.items {
            let FrameItem::PlaceObject(po) = item else {
                continue;
            };
            sweep.place_object_records += 1;
            let Some(id) = po.character else {
                continue;
            };
            sweep.character_refs += 1;
            match characters.get(id as usize) {
                Some(Character::Unknown { character_type }) => {
                    sweep.character_refs_failed += 1;
                    if sweep.failures.len() < 40 {
                        sweep.failures.push(format!(
                            "{label} {}: character {id} is Unknown(type={character_type})",
                            movie.name
                        ));
                    }
                }
                Some(Character::Placeholder) => {
                    sweep.character_refs_failed += 1;
                    if sweep.failures.len() < 40 {
                        sweep.failures.push(format!(
                            "{label} {}: character {id} is a Placeholder in a movie with no imports",
                            movie.name
                        ));
                    }
                }
                Some(Character::Sprite(sprite)) => {
                    sweep.character_refs_resolved += 1;
                    match parse_frames(apt, sprite.frame_count, sprite.frames_offset) {
                        Ok(sub) => sweep_timeline(
                            apt,
                            characters,
                            &sub,
                            depth_budget - 1,
                            movie,
                            label,
                            sweep,
                        ),
                        Err(e) if sweep.failures.len() < 40 => sweep.failures.push(format!(
                            "{label} {}: sprite {id} frames failed to parse: {e}",
                            movie.name
                        )),
                        Err(_) => {}
                    }
                }
                Some(_) => sweep.character_refs_resolved += 1,
                None => {
                    sweep.character_refs_failed += 1;
                    if sweep.failures.len() < 40 {
                        sweep.failures.push(format!(
                            "{label} {}: character {id} out of range ({} characters)",
                            movie.name,
                            characters.len()
                        ));
                    }
                }
            }
        }
    }
}

fn dedup_union(bfme2: Vec<AptMovie>, rotwk: Vec<AptMovie>) -> (MovieSet, usize) {
    let mut set = MovieSet::new();
    for movie in bfme2 {
        set.insert(movie);
    }
    let mut shadowed = 0;
    for movie in rotwk {
        if set.get(&movie.name).is_some() {
            shadowed += 1;
        }
        set.insert(movie);
    }
    (set, shadowed)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bfme2_root = args.first().cloned().unwrap_or_else(|| "-".to_string());
    let rotwk_root = args.get(1).cloned().unwrap_or_else(|| "-".to_string());
    if bfme2_root == "-" && rotwk_root == "-" {
        eprintln!("usage: corpus <bfme2 install> <rotwk install>  ('-' to skip either)");
        std::process::exit(1);
    }

    println!("== parsing ==");
    let bfme2 = scan_install("BFME2", &bfme2_root);
    let rotwk = scan_install("ROTWK", &rotwk_root);

    for (label, stats, root) in [
        ("BFME2", &bfme2, &bfme2_root),
        ("ROTWK", &rotwk, &rotwk_root),
    ] {
        if root == "-" {
            continue;
        }
        println!(
            "{label}: {} movies loaded, {} load failures",
            stats.movies.len(),
            stats.load_failures.len()
        );
        for f in stats.load_failures.iter().take(20) {
            println!("  LOAD-FAIL {f}");
        }
        println!(
            "  .ru: {} parsed, {} failures ({} entries, {} styled runs - {} more runs than entries)",
            stats.ru_ok,
            stats.ru_failures.len(),
            stats.ru_entry_count,
            stats.ru_run_count,
            stats.ru_run_count.saturating_sub(stats.ru_entry_count)
        );
        for f in stats.ru_failures.iter().take(20) {
            println!("  RU-FAIL {f}");
        }
        println!(
            "  .dat: {} parsed, {} failures",
            stats.dat_ok,
            stats.dat_failures.len()
        );
        for f in stats.dat_failures.iter().take(20) {
            println!("  DAT-FAIL {f}");
        }
        println!(
            "  atlas images referenced by .dat present in the same BIG: {} of {}",
            stats.atlas_present, stats.atlas_total
        );
        for f in stats.atlas_missing.iter().take(20) {
            println!("  ATLAS-MISSING {f}");
        }
        let (reached, total) = const_reaches_valid_movie(root);
        println!("  .const reaches a valid Movie record: {reached} of {total}");
    }

    if bfme2_root != "-" {
        let set = build_set(bfme2.movies.to_vec());
        let (resolved, total) = resolve_all(&set);
        println!("\n== BFME2 only: imports resolved: {resolved} of {total} ==");
    }
    if rotwk_root != "-" {
        let set = build_set(rotwk.movies.to_vec());
        let (resolved, total) = resolve_all(&set);
        println!("== ROTWK only: imports resolved: {resolved} of {total} ==");
    }

    if bfme2_root != "-" && rotwk_root != "-" {
        let (set, shadowed) = dedup_union(bfme2.movies.clone(), rotwk.movies.clone());
        println!(
            "\n== deduplicated union: {} movies, {shadowed} shadowed by ROTWK ==",
            set.len()
        );
        let (resolved, total) = resolve_all(&set);
        println!("  imports resolved: {resolved} of {total}");

        let (wrong_resolved, wrong_total) = negative_control_wrong_movie(&set);
        println!(
            "  NEGATIVE CONTROL - resolving against the wrong movie: {wrong_resolved} of {wrong_total}"
        );
        let (slot_resolved, slot_total) = arbitrary_export_slot_control(&set);
        println!(
            "  control - right movie, arbitrary export slot: {slot_resolved} of {slot_total} \
             (expected to also be high - this control alone does not validate the name lookup, \
             see docs/research/apt-format.md)"
        );

        let (ph_ok, ph_total, mismatches) = placeholder_invariant(&set);
        println!("  placeholder invariant: {ph_ok} of {ph_total} movies match exactly");
        for m in mismatches.iter().take(10) {
            println!("    MISMATCH {m}");
        }
    } else if rotwk_root == "-" {
        println!("\n(ROTWK skipped - dedup union and negative controls need both installs)");
    } else {
        println!("\n(BFME2 skipped - dedup union and negative controls need both installs)");
    }

    println!("\n== D2c/D2d timeline census (this crate's own frame/display-list/button code) ==");
    for (label, stats, root) in [
        ("BFME2", &bfme2, &bfme2_root),
        ("ROTWK", &rotwk, &rotwk_root),
    ] {
        if root == "-" {
            continue;
        }
        let mut census = TimelineCensus::default();
        for movie in &stats.movies {
            let Some(apt) = stats.apt_bytes.get(&movie.name) else {
                continue;
            };
            census_movie(apt, movie, &mut census);
        }
        println!(
            "{label}: {} timeline frames (every movie's own plus every Sprite character's, \
             each counted once)",
            census.frames
        );
        println!(
            "  items: PlaceObject {}, RemoveObject {}, Action {}, FrameLabel {}, \
             BackgroundColor {}, InitAction {}, Unknown {}",
            census.place_object,
            census.remove_object,
            census.action,
            census.frame_label,
            census.background_color,
            census.init_action,
            census.unknown_items
        );
        println!(
            "  PlaceObject flags: Move {}, HasCharacter {}, HasMatrix {}, HasColorTransform {}, \
             HasRatio {}, HasName {}, HasClipDepth {}, HasClipActions {}",
            census.flag_move,
            census.flag_has_character,
            census.flag_has_matrix,
            census.flag_has_color_transform,
            census.flag_has_ratio,
            census.flag_has_name,
            census.flag_has_clip_depth,
            census.flag_has_clip_actions
        );
        println!(
            "  colour transform: {} non-identity records; {} of those ({:.1}%) have \
             byte0==byte1==byte2 (R=G=B under the AptConverter-derived packing - the \
             ARGB-vs-RGBA byte-order ambiguity is moot for that fraction)",
            census.cx_nonidentity,
            census.cx_rgb_equal,
            if census.cx_nonidentity == 0 {
                0.0
            } else {
                100.0 * census.cx_rgb_equal as f64 / census.cx_nonidentity as f64
            }
        );
        let mut alpha_hist: Vec<(&u8, &u64)> = census.cx_alpha_hist.iter().collect();
        alpha_hist.sort_by(|a, b| b.1.cmp(a.1));
        print!("  alpha byte (packing byte 3) histogram, top 8:");
        for (v, c) in alpha_hist.iter().take(8) {
            print!(" {v}={c}");
        }
        println!();
        println!(
            "  colour add (+44): {} of {} PlaceObject records non-zero \
             (docs/research/apt-format.md measured 924 of 47,822 for ROTWK); \
             {} of those ({:.1}%) have a zero top byte, corroborating the 0x00RRGGBB packing",
            census.cx_add_nonzero,
            census.place_object,
            census.cx_add_top_byte_zero,
            if census.cx_add_nonzero == 0 {
                0.0
            } else {
                100.0 * census.cx_add_top_byte_zero as f64 / census.cx_add_nonzero as f64
            }
        );
        print!("  distinct non-grey +44 values (up to 8), as 0x00RRGGBB:");
        for v in &census.cx_add_distinct_nongrey {
            print!(
                " {v:#010x}=rgb({},{},{})",
                (v >> 16) & 0xFF,
                (v >> 8) & 0xFF,
                v & 0xFF
            );
        }
        println!();
        println!(
            "  buttons reached: {}, button records: {}",
            census.buttons, census.button_records
        );
        let mut button_hist: Vec<(&u32, &u64)> = census.button_flag_hist.iter().collect();
        button_hist.sort_by(|a, b| b.1.cmp(a.1));
        print!("  button record flag histogram (bit0 Up, bit1 Over, bit2 Down, bit3 HitTest):");
        for (flags, count) in button_hist.iter().take(10) {
            print!(" {flags:#04x}={count}");
        }
        println!();
        let hit_only = census.button_flag_hist.get(&0x08).copied().unwrap_or(0);
        println!(
            "  hit-test-only button records (flags==8, must never be drawn): {hit_only} of {}",
            census.button_records
        );
    }

    println!(
        "\n== Text/EditText/Font census (this crate's own decode - the text-decoding round) =="
    );
    for (label, stats, root) in [
        ("BFME2", &bfme2, &bfme2_root),
        ("ROTWK", &rotwk, &rotwk_root),
    ] {
        if root == "-" {
            continue;
        }
        let mut census = TextCensus::default();
        for movie in &stats.movies {
            census_text(movie, &mut census);
        }
        println!(
            "{label}: EditText {} (own literal text: {}, bound to a variable: {}, empty with \
             no resolvable text: {}), Text {} ({} records, {} shaped glyphs), Font {} ({} \
             unnamed, {} glyph-table entries)",
            census.edittext,
            census.edittext_with_text,
            census.edittext_with_variable,
            census.edittext_empty_without_variable_resolved,
            census.text_characters,
            census.text_records,
            census.text_glyphs,
            census.font_characters,
            census.font_unnamed,
            census.font_glyph_table_entries
        );
        print!("  Font names:");
        for (name, count) in &census.font_names {
            print!(" {name:?}={count}");
        }
        println!();
    }

    println!("\n== D2c corpus sweep: every no-import movie's full timeline ==");
    for (label, stats, root) in [
        ("BFME2", &bfme2, &bfme2_root),
        ("ROTWK", &rotwk, &rotwk_root),
    ] {
        if root == "-" {
            continue;
        }
        let sweep = sweep_no_import(stats, label);
        println!(
            "{label}: {} no-import movies walked, {} frames, {} PlaceObject records, \
             {} character references ({} resolve, {} do NOT), {} orphan Move records",
            sweep.movies,
            sweep.frames,
            sweep.place_object_records,
            sweep.character_refs,
            sweep.character_refs_resolved,
            sweep.character_refs_failed,
            sweep.orphan_moves
        );
        for f in sweep.failures.iter().take(20) {
            println!("  UNRESOLVED {f}");
        }
    }
}
