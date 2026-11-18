use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Entry {
    name: &'static str,
    needle: &'static str,
    pitch: f32,
    yaw: f32,
    zoom: f32,
    water_time: f32,
    particle_time: f32,
}

const ENTRIES: [Entry; 4] = [
    Entry {
        name: "01-rhun",
        needle: "wor rhun",
        pitch: 0.6,
        yaw: 0.6,
        zoom: 1.5,
        water_time: 2.5,
        particle_time: 1.0,
    },
    Entry {
        name: "02-fornost",
        needle: "evil fornost",
        pitch: 0.9,
        yaw: 1.2,
        zoom: 3.0,
        water_time: 0.0,
        particle_time: 0.5,
    },
    Entry {
        name: "03-blue-mountain",
        needle: "good blue mountain",
        pitch: 0.4,
        yaw: 2.0,
        zoom: 1.0,
        water_time: 5.0,
        particle_time: 2.0,
    },
    Entry {
        name: "04-fangorn",
        needle: "wor fangorn",
        pitch: 1.0,
        yaw: 0.3,
        zoom: 2.5,
        water_time: 0.0,
        particle_time: 3.0,
    },
];

const RENDER_TREES: [&str; 5] = [
    "crates/core/render",
    "crates/sage/terrain",
    "crates/sage/scene",
    "crates/sage/model",
    "crates/sage/particle",
];

const RENDER_LOCK_PACKAGES: [&str; 3] = ["wgpu", "zune-jpeg", "winit"];

pub(crate) fn run(root: &Path) -> bool {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let options = match Options::parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\n\n{}", Options::usage());
            return false;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&options.out) {
        eprintln!("FAILED: could not create {}: {e}", options.out.display());
        return false;
    }

    let commit =
        git_output(root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "?".to_owned());

    println!(
        "=== matrix: {} camera(s), out = {} ===",
        ENTRIES.len(),
        options.out.display()
    );

    let mut cache_key_text = None;
    let used_cache = match &options.cache {
        Some(cache_dir) => {
            let key = match cache_key(root) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("FAILED: computing before-run cache key: {e}");
                    return false;
                }
            };
            println!("before-run cache key (this run):");
            for line in key.lines() {
                println!("  {line}");
            }
            match try_use_cache(cache_dir, &key, &options) {
                Ok(hit) => {
                    println!(
                        "{}",
                        if hit {
                            format!(
                                "before-run: cache key unchanged since last render - reused {} \
                                 without invoking cargo",
                                cache_dir.display()
                            )
                        } else {
                            format!(
                                "before-run: cache miss at {} - rendering fresh and refreshing it",
                                cache_dir.display()
                            )
                        }
                    );
                    cache_key_text = Some(key);
                    hit
                }
                Err(e) => {
                    eprintln!("FAILED: cache check: {e}");
                    return false;
                }
            }
        }
        None => false,
    };

    let started = Instant::now();
    let mut rendered_ok = 0usize;
    let mut baseline_matches = 0usize;
    let mut baseline_checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (index, entry) in ENTRIES.iter().enumerate() {
        let n = index.saturating_add(1);
        let tga = options.out.join(format!("{}.tga", entry.name));
        let log_path = options.out.join(format!("{}.log", entry.name));
        let sidecar_path = options.out.join(format!("{}.sidecar.txt", entry.name));

        if used_cache {
            println!("[{n}/{}] {}: reused from cache", ENTRIES.len(), entry.name);
            rendered_ok = rendered_ok.saturating_add(1);
        } else {
            let entry_started = Instant::now();
            println!("[{n}/{}] {}: rendering...", ENTRIES.len(), entry.name);
            let cmd_line = command_line(&options, entry, &tga);
            let mut command = build_command(&options, entry, &tga);
            match run_streamed(&mut command, &log_path, entry.name) {
                Ok((log_text, true)) => {
                    let elapsed = entry_started.elapsed().as_secs_f32();
                    println!(
                        "[{n}/{}] {}: done in {elapsed:.1}s",
                        ENTRIES.len(),
                        entry.name
                    );
                    if let Err(e) = write_sidecar(
                        &sidecar_path,
                        &options,
                        entry,
                        &tga,
                        &commit,
                        &cmd_line,
                        &log_text,
                    ) {
                        failures.push(format!("{}: sidecar: {e}", entry.name));
                    } else {
                        rendered_ok = rendered_ok.saturating_add(1);
                    }
                }
                Ok((_, false)) => {
                    failures.push(format!(
                        "{}: terrain_view exited non-zero, see {}",
                        entry.name,
                        log_path.display()
                    ));
                }
                Err(e) => {
                    failures.push(format!("{}: {e}", entry.name));
                }
            }
        }

        if let Some(baseline) = &options.baseline {
            let reference = baseline.join(format!("{}.tga", entry.name));
            baseline_checked = baseline_checked.saturating_add(1);
            match compare_files(&tga, &reference) {
                Ok(true) => {
                    baseline_matches = baseline_matches.saturating_add(1);
                    println!("  matches baseline {}", reference.display());
                }
                Ok(false) => {
                    failures.push(format!(
                        "{}: differs from baseline {}",
                        entry.name,
                        reference.display()
                    ));
                }
                Err(e) => failures.push(format!("{}: baseline compare: {e}", entry.name)),
            }
        }
    }

    if !used_cache {
        if let (Some(cache_dir), Some(key)) = (&options.cache, &cache_key_text) {
            if let Err(e) = refresh_cache(cache_dir, &options.out, key) {
                eprintln!(
                    "warning: could not refresh cache at {}: {e}",
                    cache_dir.display()
                );
            }
        }
    }

    let total = ENTRIES.len();
    let elapsed = started.elapsed().as_secs_f32();
    let ok = failures.is_empty() && rendered_ok == total;
    println!(
        "\nmatrix: {} ({rendered_ok}/{total} rendered{}) in {elapsed:.1}s",
        if ok { "PASS" } else { "FAIL" },
        if options.baseline.is_some() {
            format!(", {baseline_matches}/{baseline_checked} match baseline")
        } else {
            String::new()
        }
    );
    for f in &failures {
        eprintln!("  {f}");
    }
    ok
}

struct Options {
    out: PathBuf,
    bfme2: PathBuf,
    rotwk: PathBuf,
    baseline: Option<PathBuf>,
    cache: Option<PathBuf>,
}

impl Options {
    fn usage() -> &'static str {
        "usage: cargo xtask matrix --out <dir> --bfme2 <install dir> --rotwk <install dir> \
         [--baseline <dir>] [--cache <dir>]\n\
         \n\
         --out       scratch directory for TGAs, logs and sidecars (never committed)\n\
         --bfme2     The Battle for Middle-earth II install (water/model textures resolve from here)\n\
         --rotwk     The Rise of the Witch-king install, mounted on top with --install\n\
         --baseline  directory of previously-rendered TGAs to compare byte-for-byte\n\
         --cache     before-run cache: skip rendering when the render-relevant tree,\n\
         \x20           Cargo.lock and GPU adapter/driver all match a prior run's cache"
    }

    fn parse(args: &[String]) -> Result<Self, String> {
        let mut out = None;
        let mut bfme2 = None;
        let mut rotwk = None;
        let mut baseline = None;
        let mut cache = None;
        let mut i = 0usize;
        while let Some(arg) = args.get(i) {
            let mut value = || -> Result<PathBuf, String> {
                i = i.saturating_add(1);
                args.get(i)
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("{arg} needs a value"))
            };
            match arg.as_str() {
                "--out" => out = Some(value()?),
                "--bfme2" => bfme2 = Some(value()?),
                "--rotwk" => rotwk = Some(value()?),
                "--baseline" => baseline = Some(value()?),
                "--cache" => cache = Some(value()?),
                other => return Err(format!("unknown argument: {other}")),
            }
            i = i.saturating_add(1);
        }
        Ok(Self {
            out: out.ok_or("--out is required")?,
            bfme2: bfme2.ok_or("--bfme2 is required")?,
            rotwk: rotwk.ok_or("--rotwk is required")?,
            baseline,
            cache,
        })
    }
}

fn build_command(options: &Options, entry: &Entry, tga: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "-q",
        "-p",
        "mint-sage-terrain",
        "--example",
        "terrain_view",
        "--",
    ]);
    command.arg(&options.bfme2);
    command.arg(entry.needle);
    command.arg("--install");
    command.arg(&options.rotwk);
    command.args([
        "--pitch",
        &entry.pitch.to_string(),
        "--yaw",
        &entry.yaw.to_string(),
        "--zoom",
        &entry.zoom.to_string(),
        "--water-time",
        &entry.water_time.to_string(),
        "--particle-time",
        &entry.particle_time.to_string(),
        "--screenshot",
    ]);
    command.arg(tga);
    command
}

fn command_line(options: &Options, entry: &Entry, tga: &Path) -> String {
    format!(
        "cargo run -q -p mint-sage-terrain --example terrain_view -- \"{}\" \"{}\" --install \"{}\" \
         --pitch {} --yaw {} --zoom {} --water-time {} --particle-time {} --screenshot \"{}\"",
        options.bfme2.display(),
        entry.needle,
        options.rotwk.display(),
        entry.pitch,
        entry.yaw,
        entry.zoom,
        entry.water_time,
        entry.particle_time,
        tga.display(),
    )
}

fn run_streamed(
    command: &mut Command,
    log_path: &Path,
    label: &str,
) -> Result<(String, bool), String> {
    let log_file =
        std::fs::File::create(log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;

    let stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let captured = Arc::new(Mutex::new(String::new()));

    let out_log = log_file.try_clone().map_err(|e| e.to_string())?;
    let out_captured = Arc::clone(&captured);
    let out_label = label.to_owned();
    let out_thread =
        std::thread::spawn(move || stream_lines(stdout, out_log, &out_label, Some(out_captured)));

    let err_log = log_file.try_clone().map_err(|e| e.to_string())?;
    let err_label = label.to_owned();
    let err_thread = std::thread::spawn(move || stream_lines(stderr, err_log, &err_label, None));

    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = out_thread.join();
    let _ = err_thread.join();

    let text = captured.lock().map(|s| s.clone()).unwrap_or_default();
    Ok((text, status.success()))
}

fn stream_lines(
    pipe: impl std::io::Read,
    mut log: std::fs::File,
    label: &str,
    capture: Option<Arc<Mutex<String>>>,
) {
    let reader = BufReader::new(pipe);
    for line in reader.lines().map_while(Result::ok) {
        println!("  [{label}] {line}");
        let _ = writeln!(log, "{line}");
        if let Some(buf) = &capture {
            if let Ok(mut buf) = buf.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
        }
    }
}

fn write_sidecar(
    path: &Path,
    options: &Options,
    entry: &Entry,
    tga: &Path,
    commit: &str,
    cmd_line: &str,
    log_text: &str,
) -> Result<(), String> {
    let device = extract_after(log_text, "device: ").unwrap_or_else(|| "unknown".to_owned());
    let camera =
        extract_after(log_text, "camera resolved: ").unwrap_or_else(|| "unknown".to_owned());
    let image = extract_after(log_text, "wrote ").unwrap_or_else(|| "unknown".to_owned());
    let modes = mode_lines(log_text);

    let mut out = String::new();
    out.push_str("mint screenshot provenance\n");
    out.push_str(&format!("map: {}\n", entry.needle));
    out.push_str(&format!("bfme2 install: {}\n", options.bfme2.display()));
    out.push_str(&format!(
        "rotwk install (mounted with --install): {}\n",
        options.rotwk.display()
    ));
    out.push_str(&format!("commit: {commit}\n"));
    out.push_str(&format!("device: {device}\n"));
    out.push_str(&format!("camera resolved: {camera}\n"));
    out.push_str(&format!(
        "water-time: {}s  particle-time: {}s\n",
        entry.water_time, entry.particle_time
    ));
    out.push_str(&format!("image: {image}\n"));
    out.push_str(&format!("screenshot file: {}\n", tga.display()));
    out.push_str(&format!(
        "{} mode setting line(s) this run reported:\n",
        modes.len()
    ));
    for line in &modes {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("regenerate with:\n");
    out.push_str(cmd_line);
    out.push('\n');

    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

fn extract_after(log: &str, prefix: &str) -> Option<String> {
    log.lines()
        .find(|line| line.starts_with(prefix))
        .map(|line| line.get(prefix.len()..).unwrap_or("").trim().to_owned())
}

fn mode_lines(log: &str) -> Vec<&str> {
    log.lines()
        .map(str::trim)
        .filter(|line| line.contains(" = ") && line.ends_with(')'))
        .collect()
}

fn compare_files(a: &Path, b: &Path) -> Result<bool, String> {
    let a_bytes = std::fs::read(a).map_err(|e| format!("{}: {e}", a.display()))?;
    let b_bytes = match std::fs::read(b) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };
    Ok(a_bytes == b_bytes)
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
}

fn lock_version(text: &str, package: &str) -> String {
    let mut in_pkg = false;
    for line in text.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            in_pkg = false;
            continue;
        }
        if let Some(name) = t.strip_prefix("name = \"") {
            in_pkg = name.trim_end_matches('"') == package;
            continue;
        }
        if in_pkg {
            if let Some(v) = t.strip_prefix("version = \"") {
                return v.trim_end_matches('"').to_owned();
            }
        }
    }
    "?".to_owned()
}

fn cache_key(root: &Path) -> Result<String, String> {
    let mut out = String::new();
    for rel in RENDER_TREES {
        let hash = git_output(root, &["rev-parse", &format!("HEAD:{rel}")])
            .unwrap_or_else(|| "?".to_owned());
        out.push_str(&format!("tree {rel} {hash}\n"));
    }
    let lock_text = std::fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    for package in RENDER_LOCK_PACKAGES {
        out.push_str(&format!(
            "lock {package} {}\n",
            lock_version(&lock_text, package)
        ));
    }
    for entry in &ENTRIES {
        out.push_str(&format!(
            "camera {} pitch={} yaw={} zoom={} water_time={} particle_time={}\n",
            entry.name, entry.pitch, entry.yaw, entry.zoom, entry.water_time, entry.particle_time
        ));
    }
    let gpu = gpu_probe()?;
    out.push_str(&format!("gpu {gpu}\n"));
    Ok(out)
}

fn gpu_probe() -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "mint-render", "--example", "gpu_probe"])
        .output()
        .map_err(|e| format!("gpu_probe: {e}"))?;
    if !output.status.success() {
        return Err("gpu_probe exited non-zero".to_owned());
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("gpu_probe: non-utf8 output: {e}"))
}

fn try_use_cache(cache_dir: &Path, key: &str, options: &Options) -> Result<bool, String> {
    let stored = match std::fs::read_to_string(cache_dir.join("key.txt")) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    if stored != key {
        return Ok(false);
    }
    let images_dir = cache_dir.join("images");
    for entry in &ENTRIES {
        for ext in ["tga", "log", "sidecar.txt"] {
            if !images_dir.join(format!("{}.{ext}", entry.name)).is_file() {
                return Ok(false);
            }
        }
    }
    for entry in &ENTRIES {
        for ext in ["tga", "log", "sidecar.txt"] {
            let name = format!("{}.{ext}", entry.name);
            std::fs::copy(images_dir.join(&name), options.out.join(&name))
                .map_err(|e| format!("copying cached {name}: {e}"))?;
        }
    }
    Ok(true)
}

fn refresh_cache(cache_dir: &Path, out: &Path, key: &str) -> Result<(), String> {
    let images_dir = cache_dir.join("images");
    std::fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;
    for entry in &ENTRIES {
        for ext in ["tga", "log", "sidecar.txt"] {
            let name = format!("{}.{ext}", entry.name);
            let src = out.join(&name);
            if src.is_file() {
                std::fs::copy(&src, images_dir.join(&name))
                    .map_err(|e| format!("caching {name}: {e}"))?;
            }
        }
    }
    std::fs::write(cache_dir.join("key.txt"), key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_version_finds_the_named_package_only() {
        let text = "\
[[package]]
name = \"other\"
version = \"9.9\"

[[package]]
name = \"wgpu\"
version = \"0.20.1\"

[[package]]
name = \"winit\"
version = \"0.29.0\"
";
        assert_eq!(lock_version(text, "wgpu"), "0.20.1");
        assert_eq!(lock_version(text, "winit"), "0.29.0");
        assert_eq!(lock_version(text, "not-present"), "?");
    }

    #[test]
    fn extract_after_reads_the_first_matching_line_only() {
        let log = "anisotropy: 8x\ndevice: adapter \"X\" backend Vulkan driver \"1.2\"\nother\n";
        assert_eq!(
            extract_after(log, "device: ").as_deref(),
            Some("adapter \"X\" backend Vulkan driver \"1.2\"")
        );
        assert_eq!(extract_after(log, "missing: "), None);
    }

    #[test]
    fn mode_lines_keeps_settings_and_drops_narration() {
        let log = "\
frustum cull = on (--frustum-cull)
shadows = off (mode.shadows)
anisotropy: 8x
device: adapter \"X\" backend Vulkan driver \"1.2\"
camera resolved: eye (1, 2, 3) target (0, 0, 0) pitch 0.6 yaw 0.6 distance 9 (--pitch --yaw --zoom --focus)
wrote 1280x720 screenshot to out.tga
map cliff texture (cliff record 0) = class 18 CliffShelob02
";
        let modes = mode_lines(log);
        assert_eq!(
            modes,
            vec![
                "frustum cull = on (--frustum-cull)",
                "shadows = off (mode.shadows)"
            ]
        );
    }

    #[test]
    fn compare_files_true_only_for_identical_bytes() {
        let dir = std::env::temp_dir().join(format!("mint-matrix-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.tga");
        let b = dir.join("b.tga");
        let c = dir.join("c.tga");
        std::fs::write(&a, [1, 2, 3]).expect("test setup: write a");
        std::fs::write(&b, [1, 2, 3]).expect("test setup: write b");
        std::fs::write(&c, [1, 2, 4]).expect("test setup: write c");
        assert_eq!(compare_files(&a, &b), Ok(true));
        assert_eq!(compare_files(&a, &c), Ok(false));
        assert_eq!(compare_files(&a, &dir.join("missing.tga")), Ok(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_parse_rejects_a_missing_required_flag() {
        let args: Vec<String> = ["--out", "x", "--bfme2", "y"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(Options::parse(&args).is_err());
    }

    #[test]
    fn options_parse_accepts_the_required_four() {
        let args: Vec<String> = ["--out", "o", "--bfme2", "b", "--rotwk", "r"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let options = Options::parse(&args).expect("valid options");
        assert_eq!(options.out, PathBuf::from("o"));
        assert_eq!(options.baseline, None);
    }

    #[test]
    fn command_line_quotes_the_map_needle_and_install_paths() {
        let options = Options {
            out: PathBuf::from("out"),
            bfme2: PathBuf::from("D:\\bfme2"),
            rotwk: PathBuf::from("D:\\rotwk"),
            baseline: None,
            cache: None,
        };
        let entry = &ENTRIES[0];
        let line = command_line(&options, entry, Path::new("out/01-rhun.tga"));
        assert!(line.contains("\"wor rhun\""));
        assert!(line.contains("\"D:\\bfme2\""));
        assert!(line.contains("--pitch 0.6"));
    }
}
