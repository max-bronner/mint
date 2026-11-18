use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_str::{parse_csf, parse_str, StringTable};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: corpus <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut failures: Vec<String> = Vec::new();
    let mut ok = true;
    for root in &roots {
        println!("== {} ==", root.display());
        if !sweep(root, &mut failures) {
            ok = false;
        }
        println!();
    }

    if failures.is_empty() {
        println!("no failures");
    } else {
        println!("{} failure(s):", failures.len());
        for failure in &failures {
            println!("  {failure}");
        }
        ok = false;
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn sweep(root: &Path, failures: &mut Vec<String>) -> bool {
    let started = Instant::now();
    let mut str_seen = 0usize;
    let mut str_ok = 0usize;
    let mut csf_seen = 0usize;
    let mut csf_ok = 0usize;
    let mut records = 0usize;
    let mut bytes_read = 0usize;
    let mut speech = 0usize;
    let mut multiline = 0usize;
    let mut non_ascii_text = 0usize;
    let mut non_ascii_label = 0usize;
    let mut structural_mismatches = 0usize;
    let mut leftover_escapes = 0usize;
    let mut untrimmed = 0usize;
    let mut main_table = StringTable::new();
    let mut map_records = 0usize;

    for archive_path in mint_sage_big::archives_under(root) {
        let Ok(archive) = BigArchive::open(&archive_path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            let is_str = lower.ends_with(".str");
            let is_csf = lower.ends_with(".csf");
            if !is_str && !is_csf {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                failures.push(format!("{}: could not read", entry.name));
                continue;
            };
            bytes_read = bytes_read.saturating_add(bytes.len());
            if is_csf {
                csf_seen = csf_seen.saturating_add(1);
                match parse_csf(&bytes) {
                    Ok(parsed) => {
                        csf_ok = csf_ok.saturating_add(1);
                        records = records.saturating_add(parsed.len());
                        println!("  {} - {} labels (csf)", entry.name, parsed.len());
                    }
                    Err(error) => failures.push(format!("{}: {error}", entry.name)),
                }
                continue;
            }

            str_seen = str_seen.saturating_add(1);
            match parse_str(&bytes) {
                Ok(parsed) => {
                    str_ok = str_ok.saturating_add(1);
                    records = records.saturating_add(parsed.len());
                    let declared = count_label_lines(&bytes);
                    if declared != parsed.len() {
                        structural_mismatches = structural_mismatches.saturating_add(1);
                        failures.push(format!(
                            "{}: {declared} label lines but {} records parsed",
                            entry.name,
                            parsed.len()
                        ));
                    }
                    for record in &parsed {
                        if record.speech.is_some() {
                            speech = speech.saturating_add(1);
                        }
                        if record.text.contains('\n') {
                            multiline = multiline.saturating_add(1);
                        }
                        if !record.text.is_ascii() {
                            non_ascii_text = non_ascii_text.saturating_add(1);
                        }
                        if !record.label.is_ascii() {
                            non_ascii_label = non_ascii_label.saturating_add(1);
                            failures.push(format!(
                                "{}: non-ASCII label {:?}",
                                entry.name, record.label
                            ));
                        }
                        if record.text.contains("\\n") || record.text.contains("\\\"") {
                            leftover_escapes = leftover_escapes.saturating_add(1);
                        }
                        if record.text.starts_with(' ') || record.text.ends_with(' ') {
                            untrimmed = untrimmed.saturating_add(1);
                        }
                    }
                    if lower.contains("lotr.str") {
                        println!("  {} - {} records", entry.name, parsed.len());
                        main_table.insert(parsed);
                    } else {
                        map_records = map_records.saturating_add(parsed.len());
                    }
                }
                Err(error) => failures.push(format!("{}: {error}", entry.name)),
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "  {str_ok} of {str_seen} .str parsed, {csf_ok} of {csf_seen} .csf parsed, \
         {records} records, {} distinct labels in lotr.str ({} duplicate labels dropped), \
         {map_records} records in per-map files",
        main_table.len(),
        main_table.duplicates()
    );
    println!(
        "  {speech} records name a speech clip, {multiline} contain a newline, \
         {non_ascii_text} contain a non-ASCII character"
    );
    println!(
        "  cross-checks: {structural_mismatches} files whose label-line count disagrees with the \
         record count, {leftover_escapes} texts with an undecoded escape, {untrimmed} with \
         untrimmed edge whitespace, {non_ascii_label} non-ASCII labels"
    );
    #[allow(clippy::cast_precision_loss)]
    let throughput = (bytes_read as f64) / 1_048_576.0 / elapsed.max(1e-9);
    println!("  {elapsed:.3}s, {throughput:.1} MiB/s");

    structural_mismatches == 0 && leftover_escapes == 0 && untrimmed == 0 && non_ascii_label == 0
}

fn count_label_lines(bytes: &[u8]) -> usize {
    let mut count = 0usize;
    let mut in_record = false;
    let mut in_string = false;
    for line in bytes.split(|byte| *byte == b'\n') {
        let line = trim(line);
        if in_string {
            if unescaped_quotes(line) >= 1 {
                in_string = false;
            }
            continue;
        }
        if line.is_empty() || line.starts_with(b"//") {
            continue;
        }
        if line.first() == Some(&b'"') {
            if unescaped_quotes(line) < 2 {
                in_string = true;
            }
            continue;
        }
        if line.eq_ignore_ascii_case(b"END") {
            in_record = false;
            continue;
        }
        if !in_record {
            count = count.saturating_add(1);
            in_record = true;
        }
    }
    count
}

fn unescaped_quotes(line: &[u8]) -> usize {
    let mut count = 0usize;
    let mut escaped = false;
    for &byte in line {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            count = count.saturating_add(1);
        }
    }
    count
}

fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start = start.saturating_add(1);
    }
    while end > start
        && bytes
            .get(end.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        end = end.saturating_sub(1);
    }
    bytes.get(start..end).unwrap_or(&[])
}
