use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_image::dds::Dds;
use mint_image::{jpeg, tga};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: decode <dir> [<dir> ...]");
        eprintln!("  walks each directory for .tga, .dds and .jpg files and decodes every one");
        return ExitCode::FAILURE;
    }

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        println!("== {} ==", root.display());
        let files = walk(root);
        if files.is_empty() {
            println!("  no .tga, .dds or .jpg files found");
            continue;
        }
        for path in files {
            decode_one(&path, &mut totals);
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);

    println!("\n== totals ==");
    println!(
        "  {} of {} files decoded",
        totals.decoded,
        totals.decoded.saturating_add(totals.failures.len())
    );
    println!(
        "  {} bytes read, {} pixels decoded",
        totals.bytes, totals.pixels
    );
    println!(
        "  {:.3}s, {:.1} MiB/s of source",
        elapsed,
        bytes_per_second(totals.bytes, elapsed) / (1024.0 * 1024.0)
    );

    println!("\n  what was found:");
    for (kind, count) in &totals.kinds {
        println!("    {kind}: {count}");
    }

    println!("\n  cross-checks (each would fail if the layout were misread):");
    println!(
        "    {} of {} DDS files agree with their own dwPitchOrLinearSize",
        totals.linear_size_agrees, totals.linear_size_checked
    );
    println!(
        "    {} of {} DDS files have a mip chain exactly filling the payload",
        totals.chain_exact, totals.dds_total
    );
    println!(
        "    {} of {} TGA files declare an alpha-bit count matching their depth",
        totals.alpha_bits_agree, totals.tga_total
    );
    println!(
        "    {} of {} JPEG files decoded to exactly width * height * 4 bytes",
        totals.jpeg_size_agrees, totals.jpeg_total
    );

    if !totals.failures.is_empty() {
        println!("\n  {} failures:", totals.failures.len());
        for line in &totals.failures {
            println!("    {line}");
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[derive(Default)]
struct Totals {
    decoded: usize,
    bytes: u64,
    pixels: u64,
    kinds: BTreeMap<String, usize>,
    failures: Vec<String>,
    dds_total: usize,
    tga_total: usize,
    jpeg_total: usize,
    jpeg_size_agrees: usize,
    linear_size_checked: usize,
    linear_size_agrees: usize,
    chain_exact: usize,
    alpha_bits_agree: usize,
}

fn bytes_per_second(bytes: u64, seconds: f64) -> f64 {
    let scaled = u32::try_from(bytes / 1024).unwrap_or(u32::MAX);
    f64::from(scaled) * 1024.0 / seconds
}

fn decode_one(path: &Path, totals: &mut Totals) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            totals.failures.push(format!("{}: {error}", path.display()));
            return;
        }
    };
    totals.bytes = totals
        .bytes
        .saturating_add(u64::try_from(bytes.len()).unwrap_or(0));

    let is_dds = has_extension(path, "dds");
    if is_dds {
        totals.dds_total = totals.dds_total.saturating_add(1);
        match Dds::parse(&bytes) {
            Ok(dds) => {
                let key = format!("dds {:?}, {} levels", dds.format(), dds.levels().len());
                *totals.kinds.entry(key).or_default() += 1;

                if dds.declared_linear_size() != 0 {
                    totals.linear_size_checked = totals.linear_size_checked.saturating_add(1);
                    if u64::from(dds.declared_linear_size())
                        == u64::try_from(dds.computed_linear_size()).unwrap_or(u64::MAX)
                    {
                        totals.linear_size_agrees = totals.linear_size_agrees.saturating_add(1);
                    } else {
                        totals.failures.push(format!(
                            "{}: declares linear size {} but the top level is {} bytes",
                            path.display(),
                            dds.declared_linear_size(),
                            dds.computed_linear_size()
                        ));
                    }
                }

                let chain: usize = dds
                    .levels()
                    .iter()
                    .map(|level| level.blocks().len())
                    .fold(0usize, |sum, len| sum.saturating_add(len));
                if chain.saturating_add(128) == bytes.len() {
                    totals.chain_exact = totals.chain_exact.saturating_add(1);
                }

                match dds.decode() {
                    Ok(image) => {
                        totals.decoded = totals.decoded.saturating_add(1);
                        totals.pixels = totals
                            .pixels
                            .saturating_add(u64::from(image.width()) * u64::from(image.height()));
                    }
                    Err(error) => totals
                        .failures
                        .push(format!("{}: decode: {error}", path.display())),
                }
            }
            Err(error) => totals.failures.push(format!("{}: {error}", path.display())),
        }
        return;
    }

    if has_extension(path, "jpg") || has_extension(path, "jpeg") {
        totals.jpeg_total = totals.jpeg_total.saturating_add(1);
        match jpeg::decode(&bytes) {
            Ok(image) => {
                let key = format!("jpeg {}x{}", image.width(), image.height());
                *totals.kinds.entry(key).or_default() += 1;
                totals.decoded = totals.decoded.saturating_add(1);
                let pixels = u64::from(image.width()) * u64::from(image.height());
                totals.pixels = totals.pixels.saturating_add(pixels);
                if pixels.saturating_mul(4) == u64::try_from(image.pixels().len()).unwrap_or(0) {
                    totals.jpeg_size_agrees = totals.jpeg_size_agrees.saturating_add(1);
                } else {
                    totals.failures.push(format!(
                        "{}: {}x{} decoded to {} bytes",
                        path.display(),
                        image.width(),
                        image.height(),
                        image.pixels().len()
                    ));
                }
            }
            Err(error) => totals.failures.push(format!("{}: {error}", path.display())),
        }
        return;
    }

    totals.tga_total = totals.tga_total.saturating_add(1);
    match tga::info(&bytes) {
        Ok(info) => {
            let key = format!("tga type {}, {} bpp", info.image_type, info.bits_per_pixel);
            *totals.kinds.entry(key).or_default() += 1;

            let expected_alpha_bits = if info.bits_per_pixel == 32 { 8 } else { 0 };
            if info.attribute_bits == expected_alpha_bits {
                totals.alpha_bits_agree = totals.alpha_bits_agree.saturating_add(1);
            }

            match tga::decode(&bytes) {
                Ok(image) => {
                    totals.decoded = totals.decoded.saturating_add(1);
                    totals.pixels = totals
                        .pixels
                        .saturating_add(u64::from(image.width()) * u64::from(image.height()));
                }
                Err(error) => totals
                    .failures
                    .push(format!("{}: decode: {error}", path.display())),
            }
        }
        Err(error) => totals.failures.push(format!("{}: {error}", path.display())),
    }
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(wanted))
}

fn walk(root: &Path) -> Vec<PathBuf> {
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
                    if has_extension(&path, "tga")
                        || has_extension(&path, "dds")
                        || has_extension(&path, "jpg")
                        || has_extension(&path, "jpeg")
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
