use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, TimeOfDay};

#[derive(Clone, Copy, Default)]
struct Light {
    color: [f32; 3],
    toward: [f32; 3],
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn channel(colour: [f32; 3], index: usize) -> f32 {
    colour.get(index).copied().unwrap_or(0.0)
}

fn peak_lit(ambient: [f32; 3], lights: [Light; 3]) -> [f32; 3] {
    let mut peak = ambient;
    for (index, slot) in peak.iter_mut().enumerate() {
        let mut best = channel(ambient, index);
        for mask in 1u8..8 {
            let mut direction = [0.0f32; 3];
            for (bit, light) in lights.iter().enumerate() {
                if mask & (1 << bit) == 0 {
                    continue;
                }
                let weight = channel(light.color, index);
                for (axis, value) in direction.iter_mut().zip(light.toward) {
                    *axis += value * weight;
                }
            }
            let length = dot(direction, direction).sqrt();
            if length <= 0.0 || !length.is_finite() {
                continue;
            }
            let unit = direction.map(|value| value / length);
            let mut total = channel(ambient, index);
            for light in &lights {
                total += channel(light.color, index) * dot(unit, light.toward).max(0.0);
            }
            best = best.max(total);
        }
        *slot = best;
    }
    peak
}

fn toward(direction: [f32; 3]) -> [f32; 3] {
    let length = dot(direction, direction).sqrt();
    if length > 0.0 && length.is_finite() {
        direction.map(|value| -value / length)
    } else {
        [0.0; 3]
    }
}

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            roots.push(PathBuf::from(corpus));
        }
    }
    if roots.is_empty() {
        eprintln!("usage: lighting_peak_scan <install dir> [...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }

    let mut files = 0u64;
    let mut configs = 0u64;
    let mut clips: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut worst: BTreeMap<&'static str, (f32, String)> = BTreeMap::new();
    let mut failures = 0u64;

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
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&bytes) else {
                    failures += 1;
                    continue;
                };
                let lighting = &map.global_lighting;
                let name = &entry.name;
                files += 1;
                let gain = lighting.overbright;
                for time in TimeOfDay::ALL {
                    let config = lighting.config(time);
                    configs += 1;
                    for (model, sun_gain) in [("all", gain), ("accents", 1.0)] {
                        let mut ambient = [0.0f32; 3];
                        let mut lights = [Light::default(); 3];
                        for (index, (light, source)) in
                            lights.iter_mut().zip(config.terrain).enumerate()
                        {
                            for (total, part) in ambient.iter_mut().zip(source.ambient) {
                                *total += part * gain;
                            }
                            let scale = if index == 0 { sun_gain } else { gain };
                            light.color = source.color.map(|value| value * scale);
                            light.toward = toward(source.direction);
                        }
                        let peak = peak_lit(ambient, lights);
                        let highest = peak.iter().copied().fold(0.0f32, f32::max);
                        if highest > gain {
                            *clips.entry(model).or_default() += 1;
                        }
                        let entry = worst.entry(model).or_insert((0.0, String::new()));
                        if highest > entry.0 {
                            *entry = (highest, format!("{name} {}", time.name()));
                        }
                    }
                }
            }
        }
    }

    println!("{files} file(s) with a GlobalLighting chunk, {configs} configuration(s), {failures} parse failure(s)");
    for model in ["all", "accents"] {
        let count = clips.get(model).copied().unwrap_or(0);
        let (peak, where_) = worst
            .get(model)
            .cloned()
            .unwrap_or((0.0, String::from("-")));
        let percent = if configs == 0 {
            0.0
        } else {
            100.0 * count as f64 / configs as f64
        };
        println!(
            "  overbright = {model:<8} {count} of {configs} configuration(s) ({percent:.1}%) can reach the map's own ceiling on SOME normal; highest peak {peak:.3} on {where_}"
        );
    }
    ExitCode::SUCCESS
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
        {
            out.push(path);
        }
    }
    out.sort();
    out
}
