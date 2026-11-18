use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::{
    BlockTable, CommandButtonIndex, MappedImageIndex, MemorySources, Reader, Sources,
};

const INI_ROOT: &str = "data/ini/";

const MAX_LISTED: usize = 40;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: mapped_images <BFME II dir> <ROTWK dir>");
        eprintln!("       ROTWK last - see the module doc on shadowing");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();

    let archives: Vec<PathBuf> = roots.iter().flat_map(|r| big_archives(r)).collect();
    println!(
        "{} .big archive(s) found under {} root(s)",
        archives.len(),
        roots.len()
    );

    let sources = ini_sources(&archives);
    let tops = top_level_paths(&sources);
    println!(
        "{} distinct entries under {INI_ROOT}, {} top-level documents",
        sources.len(),
        tops.len()
    );

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let mut buttons = CommandButtonIndex::new();
    let mut images = MappedImageIndex::new();
    let mut read_ok = 0usize;
    for path in &tops {
        let Ok(document) = reader.read(path, &table) else {
            continue;
        };
        read_ok = read_ok.saturating_add(1);
        buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
        images.insert_document(&document);
    }
    println!(
        "{read_ok} of {} top-level documents read; {} CommandButton definition(s), {} \
         MappedImage definition(s) ({} / {} distinct names)",
        tops.len(),
        buttons.definitions(),
        images.definitions(),
        buttons.len(),
        images.len()
    );

    let textures = texture_stems(&archives);
    println!(
        "{} distinct image-file stem(s) (any extension) across every archive entry",
        textures.len()
    );

    let mut with_image = 0usize;
    let mut resolved_to_mapped_image = 0usize;
    let mut texture_exists = 0usize;
    let mut no_mapped_image: Vec<String> = Vec::new();
    let mut mapped_image_no_texture_field: Vec<String> = Vec::new();
    let mut texture_missing: Vec<(String, String)> = Vec::new();

    let names: Vec<String> = buttons.names().map(str::to_owned).collect();
    for name in names {
        let Some(def) = buttons.get(&name) else {
            continue;
        };
        let Some(icon) = def.button_image.as_deref() else {
            continue;
        };
        with_image = with_image.saturating_add(1);
        let Some(image) = images.get(icon) else {
            if no_mapped_image.len() < MAX_LISTED {
                no_mapped_image.push(format!("{name}: ButtonImage={icon}"));
            }
            continue;
        };
        resolved_to_mapped_image = resolved_to_mapped_image.saturating_add(1);
        let Some(texture) = image.texture.as_deref() else {
            if mapped_image_no_texture_field.len() < MAX_LISTED {
                mapped_image_no_texture_field.push(format!("{icon} (button {name})"));
            }
            continue;
        };
        let stem = stem_lower(texture);
        if textures.contains(&stem) {
            texture_exists = texture_exists.saturating_add(1);
        } else if texture_missing.len() < MAX_LISTED {
            texture_missing.push((name.clone(), format!("{icon} -> {texture}")));
        }
    }

    println!("\n== CommandButton -> MappedImage -> texture ==");
    println!(
        "{} CommandButton definition(s) carry a ButtonImage",
        with_image
    );
    println!(
        "{resolved_to_mapped_image} of {with_image} ButtonImage name(s) resolve to a MappedImage \
         definition ({:.1}%)",
        share(resolved_to_mapped_image, with_image)
    );
    println!(
        "{texture_exists} of {resolved_to_mapped_image} resolved MappedImage(s) name a texture \
         found (by stem, case-insensitive, extension dropped) in the archives ({:.1}%)",
        share(texture_exists, resolved_to_mapped_image)
    );

    if !no_mapped_image.is_empty() {
        println!(
            "\n{} ButtonImage name(s) with no MappedImage definition (first {MAX_LISTED}):",
            no_mapped_image.len()
        );
        for line in &no_mapped_image {
            println!("  {line}");
        }
    }
    if !mapped_image_no_texture_field.is_empty() {
        println!(
            "\n{} MappedImage(s) resolved with no Texture field at all:",
            mapped_image_no_texture_field.len()
        );
        for line in &mapped_image_no_texture_field {
            println!("  {line}");
        }
    }
    if !texture_missing.is_empty() {
        println!(
            "\n{} resolved MappedImage(s) whose texture was not found (first {MAX_LISTED}):",
            texture_missing.len()
        );
        for (name, line) in &texture_missing {
            println!("  {name}: {line}");
        }
    }

    println!("\n{:.2} s total", started.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

fn share(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let (p, w) = (part as f64, whole as f64);
        p / w * 100.0
    }
}

fn stem_lower(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
    stem.to_ascii_lowercase()
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
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
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
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

fn ini_sources(archives: &[PathBuf]) -> MemorySources {
    let mut sources = MemorySources::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            if !entry.path.as_str().starts_with(INI_ROOT) {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                continue;
            };
            sources.insert(entry.path.as_str(), bytes);
        }
    }
    sources
}

fn top_level_paths(sources: &MemorySources) -> Vec<String> {
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut included: BTreeSet<String> = BTreeSet::new();
    for path in &paths {
        let Some(bytes) = Sources::read(sources, path) else {
            continue;
        };
        for item in mint_sage_ini::lex::lex(&mint_sage_ini::lex::decode_latin1(&bytes)) {
            if let mint_sage_ini::lex::Lexed::Directive {
                directive: mint_sage_ini::lex::Directive::Include(target),
                ..
            } = item
            {
                included.insert(mint_sage_ini::resolve_include(path, target));
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !included.contains(path))
        .collect()
}

fn texture_stems(archives: &[PathBuf]) -> HashSet<String> {
    const IMAGE_EXTENSIONS: [&str; 2] = ["tga", "dds"];
    let mut out = HashSet::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            let is_image = IMAGE_EXTENSIONS
                .iter()
                .any(|ext| lower.rsplit_once('.').is_some_and(|(_, e)| e == *ext));
            if is_image {
                out.insert(stem_lower(&lower));
            }
        }
    }
    out
}
