#![allow(clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;
use mint_sage_model::Resolver;
use mint_vfs::Vfs;

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            for part in corpus.to_string_lossy().split(';') {
                let part = part.trim();
                if !part.is_empty() {
                    roots.push(PathBuf::from(part));
                }
            }
        }
    }
    if roots.is_empty() {
        eprintln!("usage: macro_texture_scan <install dir> [...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    let resolver = Resolver::new();

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&raw) else {
                    continue;
                };
                totals.maps += 1;
                let Some(env) = &map.environment else {
                    continue;
                };
                totals.with_environment += 1;
                totals.layer(&vfs, &resolver, Layer::Macro, &env.macro_texture, &lower);
                totals.layer(&vfs, &resolver, Layer::Cloud, &env.cloud_texture, &lower);
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);

    println!(
        "{} map file(s), {} with EnvironmentData, in {:.1}s",
        totals.maps, totals.with_environment, elapsed
    );
    for layer in [Layer::Macro, Layer::Cloud] {
        let counts = &totals.counts[layer as usize];
        println!(
            "\n{} layer: {} named, {} loaded, {} named but not loaded, {} not named",
            layer.label(),
            counts.named,
            counts.loaded,
            counts.named.saturating_sub(counts.loaded),
            totals.with_environment.saturating_sub(counts.named)
        );
        println!("  by texture, maps that name it -> what happened:");
        for (name, outcome) in &counts.by_texture {
            println!(
                "    {:<24} {:>4} map(s)  {}",
                name, outcome.maps, outcome.result
            );
        }
    }

    println!(
        "\n  cross-check: {} of {} decoded textures have a level whose byte \
         length matches its own dimensions",
        totals.dimensions_agree, totals.decodes
    );

    if !totals.failures.is_empty() {
        println!("\n  {} failure(s):", totals.failures.len());
        for line in &totals.failures {
            println!("    {line}");
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Macro = 0,
    Cloud = 1,
}

impl Layer {
    fn label(self) -> &'static str {
        match self {
            Layer::Macro => "macro",
            Layer::Cloud => "cloud",
        }
    }
}

struct Outcome {
    maps: usize,
    result: String,
}

#[derive(Default)]
struct LayerCounts {
    named: usize,
    loaded: usize,
    by_texture: BTreeMap<String, Outcome>,
}

#[derive(Default)]
struct Totals {
    maps: usize,
    with_environment: usize,
    counts: [LayerCounts; 2],
    decodes: usize,
    dimensions_agree: usize,
    failures: Vec<String>,
}

impl Totals {
    fn layer(&mut self, vfs: &Vfs, resolver: &Resolver, layer: Layer, name: &str, map: &str) {
        if name.is_empty() {
            return;
        }
        let counts = &mut self.counts[layer as usize];
        counts.named += 1;
        let key = name.to_ascii_lowercase();

        if let Some(existing) = counts.by_texture.get_mut(&key) {
            existing.maps += 1;
            if existing.result.starts_with("loaded") {
                counts.loaded += 1;
            }
            return;
        }

        let result = match mint_sage_model::load_texture(vfs, resolver, name, false) {
            Ok(loaded) => {
                counts.loaded += 1;
                self.decodes += 1;
                let level = loaded.data.levels.first();
                let agrees = level.is_some_and(|level| {
                    u64::from(level.width) * u64::from(level.height) * 4 == level.data.len() as u64
                });
                if agrees {
                    self.dimensions_agree += 1;
                } else {
                    self.failures.push(format!(
                        "{map}: {name} decoded to a level whose bytes do not match its size"
                    ));
                }
                let (width, height) = level.map_or((0, 0), |level| (level.width, level.height));
                let stats = level.map_or_else(
                    || "no level".to_owned(),
                    |level| {
                        let (mut sums, mut lo, mut hi) = ([0u64; 3], 255u8, 0u8);
                        let mut texels = 0u64;
                        for texel in level.data.chunks_exact(4) {
                            texels += 1;
                            for channel in 0..3 {
                                let value = texel[channel];
                                sums[channel] += u64::from(value);
                                lo = lo.min(value);
                                hi = hi.max(value);
                            }
                        }
                        let mean = |sum: u64| {
                            if texels == 0 {
                                0.0
                            } else {
                                sum as f64 / texels as f64 / 255.0
                            }
                        };
                        format!(
                            "mean {:.3} {:.3} {:.3}, range {:.2} - {:.2}",
                            mean(sums[0]),
                            mean(sums[1]),
                            mean(sums[2]),
                            f64::from(lo) / 255.0,
                            f64::from(hi) / 255.0
                        )
                    },
                );
                format!("loaded from {} ({width}x{height}), {stats}", loaded.path)
            }
            Err(error) => format!("NOT LOADED: {error}"),
        };
        counts.by_texture.insert(key, Outcome { maps: 1, result });
    }
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
