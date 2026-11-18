use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_ui::{Font, FontId, GlyphAtlas, GlyphAtlasConfig};
use mint_vfs::{DirMount, Vfs};

const BUNDLED_FONTS: &[&str] = &["albertusmt.otf", "omnialtstd.ttf", "sachwt__.ttf"];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: text_corpus <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        mount_install(&mut vfs, root);
    }
    println!("{} visible paths after shadowing", vfs.len());

    let mut fonts: Vec<(String, Font)> = Vec::new();
    for name in BUNDLED_FONTS {
        let Some(path) = find_by_suffix(&vfs, name) else {
            eprintln!("FAILED: {name} not found in the mounted namespace");
            return ExitCode::FAILURE;
        };
        let bytes = match vfs.read(path.as_str()) {
            Ok(b) => b,
            Err(error) => {
                eprintln!("FAILED: reading {path}: {error}");
                return ExitCode::FAILURE;
            }
        };
        let signature = bytes
            .get(0..4)
            .map(|s| String::from_utf8_lossy(s).into_owned());
        match Font::from_bytes(bytes) {
            Ok(font) => {
                println!(
                    "  loaded {name:<16} from {path} (signature {:?})",
                    signature.unwrap_or_default()
                );
                fonts.push(((*name).to_string(), font));
            }
            Err(error) => {
                eprintln!("FAILED: {name} at {path} did not parse: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    if let Some((_, albertus)) = fonts.iter().find(|(name, _)| name == "albertusmt.otf") {
        match albertus
            .glyph_for_char('A')
            .and_then(|g| albertus.outline(g, 24).ok())
        {
            Some(outline) if !outline.contours.is_empty() => {
                println!(
                    "  CFF check: 'A' in albertusmt.otf drew {} contour(s)",
                    outline.contours.len()
                );
            }
            other => {
                eprintln!("FAILED: CFF outline check on albertusmt.otf: {other:?}");
                return ExitCode::FAILURE;
            }
        }
    }

    let str_paths: Vec<String> = vfs
        .paths()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("str")))
        .map(|p| p.as_str().to_string())
        .collect();
    println!("{} .str files found", str_paths.len());

    let mut codepoints: BTreeSet<char> = BTreeSet::new();
    let mut records = 0usize;
    for path in &str_paths {
        let Ok(bytes) = vfs.read(path) else { continue };
        records += bytes.iter().filter(|&&b| b == b':').count();
        for byte in quoted_bytes(&bytes) {
            codepoints.insert(char::from(byte));
        }
    }
    println!(
        "{} distinct codepoints across {} .str files (~{} ':' occurrences, an upper bound on record count)",
        codepoints.len(),
        str_paths.len(),
        records
    );

    let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap_or_else(|_| {
        eprintln!("FAILED: default GlyphAtlasConfig rejected its own defaults");
        std::process::exit(1);
    });
    let sizes = [12u32, 16, 24];
    let mut mapped = 0usize;
    let mut rasterized = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let started = Instant::now();

    for &c in &codepoints {
        for (font_id, (name, font)) in fonts.iter().enumerate() {
            let Some(glyph) = font.glyph_for_char(c) else {
                continue;
            };
            mapped += 1;
            for &size in &sizes {
                atlas.begin_frame();
                match atlas.glyph(
                    FontId(u32::try_from(font_id).unwrap_or(0)),
                    font,
                    size,
                    glyph,
                ) {
                    Ok(slot) if !slot.placeholder => rasterized += 1,
                    Ok(_) => {}
                    Err(error) => errors.push(format!(
                        "{name} U+{:04X} ({c:?}) @ {size}px: {error}",
                        c as u32
                    )),
                }
            }
        }
    }
    let elapsed = started.elapsed();

    println!(
        "{mapped} (font, codepoint) pairs mapped to a glyph; {rasterized} of {} rasterisation \
         attempts succeeded, in {:.3}s",
        mapped * sizes.len(),
        elapsed.as_secs_f64()
    );
    println!(
        "zero panics across {} distinct codepoints x {} fonts x {} sizes",
        codepoints.len(),
        fonts.len(),
        sizes.len()
    );

    if errors.is_empty() {
        println!("0 rasterisation errors");
        ExitCode::SUCCESS
    } else {
        println!("{} rasterisation errors:", errors.len());
        for e in errors.iter().take(50) {
            println!("  {e}");
        }
        ExitCode::FAILURE
    }
}

fn quoted_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut inside = false;
    for &b in data {
        if b == b'"' {
            inside = !inside;
            continue;
        }
        if inside {
            out.push(b);
        }
    }
    out
}

fn find_by_suffix(vfs: &Vfs, suffix: &str) -> Option<mint_vfs::VfsPath> {
    let suffix_lower = suffix.to_ascii_lowercase();
    vfs.paths()
        .find(|p| p.as_str().to_ascii_lowercase().ends_with(&suffix_lower))
        .cloned()
}

fn mount_install(vfs: &mut Vfs, root: &Path) {
    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        vfs.mount(Arc::new(archive));
    }
    let data = root.join("data");
    if data.is_dir() {
        if let Ok(mount) = DirMount::with_prefix(&data, "data") {
            vfs.mount(Arc::new(mount));
        }
    }
}

fn archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    let is_big = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"));
                    if is_big {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}
