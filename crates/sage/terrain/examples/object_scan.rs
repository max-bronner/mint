use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;
use mint_sage_terrain::{place_objects, surface_normal, ALIGN_KEY};

const VERTICAL_SCALE: f32 = 0.039_062_5;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: object_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

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
                    totals.unreadable.push(entry.name.clone());
                    continue;
                };
                match MapFile::parse(&raw) {
                    Ok(map) => totals.add(&lower, &map),
                    Err(error) => totals.failures.push(format!("{}: {error}", entry.name)),
                }
            }
        }
    }
    totals.report(started.elapsed().as_secs_f64());
    if totals.failures.is_empty() && totals.non_finite == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct Totals {
    files: u64,
    objects: u64,
    on_map: u64,
    aligned: u64,
    carry_align_key: u64,
    waypoints: u64,
    nonzero_z: u64,
    non_finite: u64,
    biggest_z: Option<(f32, String)>,
    tilt_degrees: BTreeMap<u32, u64>,
    per_map: Vec<(u64, String)>,
    aligned_per_map: Vec<(u64, String)>,
    failures: Vec<String>,
    unreadable: Vec<String>,
}

impl Totals {
    fn add(&mut self, name: &str, map: &MapFile) {
        self.files = self.files.saturating_add(1);
        let placed = place_objects(&map.height_map, VERTICAL_SCALE, &map.objects, &[]);
        self.per_map
            .push((u64::try_from(placed.len()).unwrap_or(0), name.to_owned()));
        let mut aligned_here = 0u64;
        for proxy in &placed {
            self.objects = self.objects.saturating_add(1);
            if proxy.ground.is_some() {
                self.on_map = self.on_map.saturating_add(1);
            }
            if proxy.waypoint {
                self.waypoints = self.waypoints.saturating_add(1);
            }
            if map
                .objects
                .get(proxy.index)
                .is_some_and(|object| object.properties.get(ALIGN_KEY).is_some())
            {
                self.carry_align_key = self.carry_align_key.saturating_add(1);
            }
            let z = map.objects.get(proxy.index).map_or(0.0, |object| object.z);
            if z != 0.0 {
                self.nonzero_z = self.nonzero_z.saturating_add(1);
                let bigger = self
                    .biggest_z
                    .as_ref()
                    .is_none_or(|(best, _)| z.abs() > *best);
                if bigger {
                    self.biggest_z = Some((z.abs(), name.to_owned()));
                }
            }
            if !proxy
                .placement
                .to_cols_array()
                .into_iter()
                .all(f32::is_finite)
            {
                self.non_finite = self.non_finite.saturating_add(1);
            }
            if proxy.align_to_terrain {
                aligned_here = aligned_here.saturating_add(1);
                self.aligned = self.aligned.saturating_add(1);
                if let Some(normal) = surface_normal(
                    &map.height_map,
                    VERTICAL_SCALE,
                    proxy.position.x,
                    proxy.position.y,
                ) {
                    *self.tilt_degrees.entry(tilt_bucket(normal.z)).or_default() += 1;
                }
            }
        }
        self.aligned_per_map.push((aligned_here, name.to_owned()));
    }

    #[allow(clippy::cast_precision_loss)]
    fn report(&self, seconds: f64) {
        let percent = |part: u64| -> f64 {
            if self.objects == 0 {
                0.0
            } else {
                (part as f64) * 100.0 / (self.objects as f64)
            }
        };
        println!("{} map file(s) parsed in {seconds:.1} s", self.files);
        println!("{} object(s) placed", self.objects);
        println!(
            "  {} ({:.2}%) have terrain under them; {} do not",
            self.on_map,
            percent(self.on_map),
            self.objects.saturating_sub(self.on_map)
        );
        println!(
            "  {} ({:.2}%) ask for alignToTerrain; {} ({:.2}%) carry the key at all",
            self.aligned,
            percent(self.aligned),
            self.carry_align_key,
            percent(self.carry_align_key)
        );
        println!(
            "  {} ({:.2}%) carry waypointID",
            self.waypoints,
            percent(self.waypoints)
        );
        println!(
            "  {} ({:.2}%) carry a nonzero z; largest |z| {}",
            self.nonzero_z,
            percent(self.nonzero_z),
            self.biggest_z
                .as_ref()
                .map_or_else(|| "-".to_owned(), |(z, map)| format!("{z} on {map}"))
        );
        println!(
            "  {} proxy transform(s) hold a non-finite element",
            self.non_finite
        );

        let sampled: u64 = self.tilt_degrees.values().sum();
        println!(
            "ground tilt under the {sampled} aligned object(s) on the map, at vertical scale {VERTICAL_SCALE}:"
        );
        let mut cumulative = 0u64;
        for (degrees, count) in &self.tilt_degrees {
            cumulative = cumulative.saturating_add(*count);
            if *degrees < 5 || degrees.checked_rem(5) == Some(0) {
                let share = if sampled == 0 {
                    0.0
                } else {
                    (cumulative as f64) * 100.0 / (sampled as f64)
                };
                println!("  <= {degrees:>2} deg  {cumulative:>7} ({share:.1}% cumulative)");
            }
        }

        let mut busiest = self.per_map.clone();
        busiest.sort_by(|a, b| b.cmp(a));
        println!("busiest maps by object count:");
        for (count, name) in busiest.iter().take(12) {
            println!("  {count:>6}  {name}");
        }

        let mut aligned = self.aligned_per_map.clone();
        aligned.sort_by(|a, b| b.cmp(a));
        println!("maps with the most alignToTerrain objects:");
        for (count, name) in aligned.iter().take(8) {
            println!("  {count:>6}  {name}");
        }

        for failure in &self.failures {
            println!("PARSE FAILURE {failure}");
        }
        for name in &self.unreadable {
            println!("UNREADABLE {name}");
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn tilt_bucket(up_component: f32) -> u32 {
    let degrees = mint_math::f32::acos(up_component.clamp(-1.0, 1.0)).to_degrees();
    if !degrees.is_finite() || degrees < 0.0 {
        return 0;
    }
    degrees.min(180.0) as u32
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
