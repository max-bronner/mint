#![allow(clippy::print_stdout, missing_docs)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_wnd::{labelled, Layout};

fn main() {
    let started = Instant::now();
    let (mut files, mut parsed, mut windows, mut bytes) = (0usize, 0usize, 0usize, 0usize);
    let mut failures = Vec::new();
    let mut mismatches = Vec::new();
    let mut images = BTreeSet::new();
    let mut draw_lines = 0usize;
    for root in std::env::args().skip(1).map(PathBuf::from) {
        let Ok(dir) = std::fs::read_dir(&root) else {
            println!("{}: not a directory", root.display());
            continue;
        };
        let mut bigs: Vec<PathBuf> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("big"))
            })
            .collect();
        bigs.sort();
        for big in bigs {
            let Ok(archive) = BigArchive::open(&big) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.name.to_ascii_lowercase().ends_with(".wnd") {
                    continue;
                }
                let label = format!("{} in {}", entry.name, big.display());
                let Ok(data) = archive.read_entry(entry) else {
                    failures.push(format!("{label}: unreadable"));
                    continue;
                };
                files += 1;
                bytes += data.len();
                let text = String::from_utf8_lossy(&data);
                let layout = match Layout::parse(&text) {
                    Ok(layout) => layout,
                    Err(error) => {
                        failures.push(format!("{label}: {error}"));
                        continue;
                    }
                };
                parsed += 1;
                let all = layout.all_windows();
                windows += all.len();
                let window_lines = text.lines().filter(|l| l.trim() == "WINDOW").count();
                if window_lines != all.len() {
                    mismatches.push(format!(
                        "{label}: {window_lines} WINDOW line(s), {} window(s) parsed",
                        all.len()
                    ));
                }
                let statements =
                    layout.fields.len() + all.iter().map(|w| w.fields.len()).sum::<usize>();
                let semicolons = text.matches(';').count();
                if semicolons != statements {
                    mismatches.push(format!(
                        "{label}: {semicolons} `;`, {statements} statement(s) parsed"
                    ));
                }
                for w in &all {
                    let name = w.name().unwrap_or_default();
                    match w.screen_rect() {
                        Some(r)
                            if r.upper_left[0] <= r.bottom_right[0]
                                && r.upper_left[1] <= r.bottom_right[1] => {}
                        other => {
                            mismatches.push(format!("{label}: {name}: SCREENRECT {other:?}"));
                        }
                    }
                    for key in w.draw_keys() {
                        draw_lines += 1;
                        let items = labelled(w.field(key).unwrap_or_default());
                        let count = |l: &str| items.iter().filter(|(x, _)| x == l).count();
                        let (i, c, b) = (count("IMAGE"), count("COLOR"), count("BORDERCOLOR"));
                        if (i, c, b) != (9, 9, 9) {
                            mismatches.push(format!(
                                "{label}: {name}: {key} has {i} IMAGE, {c} COLOR, {b} BORDERCOLOR"
                            ));
                        }
                        for item in w.draw_data(key).unwrap_or_default() {
                            images.extend(item.image);
                        }
                    }
                }
            }
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    println!(
        "{parsed} of {files} .wnd file(s) parsed, {windows} window(s), {draw_lines} draw-data \
         line(s), {} distinct image name(s); {bytes} bytes in {seconds:.3} s",
        images.len()
    );
    println!("{} failure(s):", failures.len());
    for f in &failures {
        println!("  {f}");
    }
    println!("{} cross-check mismatch(es):", mismatches.len());
    for m in &mismatches {
        println!("  {m}");
    }
}
