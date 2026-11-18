use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;

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
                    if has_extension(&path, "big") {
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

fn has_extension(path: &Path, want: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(want))
}

fn corpus_roots() -> Option<Vec<PathBuf>> {
    let value = std::env::var_os("MINT_CORPUS")?;
    Some(
        value
            .to_string_lossy()
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

#[derive(Default)]
struct Totals {
    seen: usize,
    not_ear_wrapped: Vec<String>,
    decode_failures: Vec<String>,
    length_mismatches: Vec<String>,
    ear_mirror_mismatches: Vec<String>,
    max_decompressed: u64,
    max_decompressed_name: String,
    total_compressed_bytes: u64,
    total_decompressed_bytes: u64,
}

impl Totals {
    fn absorb(&mut self, name: &str, bytes: &[u8]) {
        self.seen += 1;
        self.total_compressed_bytes += bytes.len() as u64;

        let Some(stream) = mint_sage_refpack::strip_ear_wrapper(bytes) else {
            self.not_ear_wrapped.push(name.to_owned());
            return;
        };

        let declared = mint_sage_refpack::declared_len(stream).ok();
        let mirrored = mint_sage_refpack::ear_declared_len(bytes);

        match mint_sage_refpack::decompress(stream) {
            Ok(decompressed) => {
                let actual = decompressed.len() as u64;
                self.total_decompressed_bytes += actual;
                if actual > self.max_decompressed {
                    self.max_decompressed = actual;
                    self.max_decompressed_name = name.to_owned();
                }
                if declared != Some(actual) {
                    self.length_mismatches.push(format!(
                        "{name}: decompressed to {actual} bytes, header declared {declared:?}"
                    ));
                }
                if mirrored.map(u64::from) != Some(actual) {
                    self.ear_mirror_mismatches.push(format!(
                        "{name}: decompressed to {actual} bytes, EAR wrapper mirror said {mirrored:?}"
                    ));
                }
            }
            Err(error) => {
                self.decode_failures.push(format!("{name}: {error}"));
            }
        }
    }

    fn report(&self) {
        let ok = self.seen
            - self.not_ear_wrapped.len()
            - self.decode_failures.len()
            - self.length_mismatches.len();
        println!(
            "  {ok} of {} .map/.bse files decompressed to exactly their declared size",
            self.seen
        );
        println!(
            "  {:.1} MiB compressed -> {:.1} MiB decompressed",
            self.total_compressed_bytes as f64 / (1024.0 * 1024.0),
            self.total_decompressed_bytes as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  largest decompressed body: {} bytes ({})",
            self.max_decompressed, self.max_decompressed_name
        );
        println!(
            "  EAR-wrapper mirror agrees with actual output: {} of {}",
            self.seen - self.ear_mirror_mismatches.len(),
            self.seen
        );
        if !self.not_ear_wrapped.is_empty() {
            println!("  {} not EAR\\0-wrapped:", self.not_ear_wrapped.len());
            for name in &self.not_ear_wrapped {
                println!("    {name}");
            }
        }
        if !self.decode_failures.is_empty() {
            println!("  {} decode failures:", self.decode_failures.len());
            for failure in &self.decode_failures {
                println!("    {failure}");
            }
        }
        if !self.length_mismatches.is_empty() {
            println!("  {} length mismatches:", self.length_mismatches.len());
            for mismatch in &self.length_mismatches {
                println!("    {mismatch}");
            }
        }
        if !self.ear_mirror_mismatches.is_empty() {
            println!(
                "  {} EAR-mirror mismatches:",
                self.ear_mirror_mismatches.len()
            );
            for mismatch in self.ear_mirror_mismatches.iter().take(20) {
                println!("    {mismatch}");
            }
        }
    }
}

fn walk(root: &Path) -> Totals {
    let mut totals = Totals::default();
    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                continue;
            }
            match archive.read_entry(entry) {
                Ok(bytes) => totals.absorb(&format!("{label}::{}", entry.name), &bytes),
                Err(error) => {
                    totals.seen += 1;
                    totals
                        .decode_failures
                        .push(format!("{label}::{}: {error}", entry.name));
                }
            }
        }
    }
    totals
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_shipped_map_decompresses_to_its_declared_size() {
    let Some(roots) = corpus_roots() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let mut totals = Totals::default();
    for root in &roots {
        println!("== {} ==", root.display());
        let this_install = walk(root);
        println!("  {} .map/.bse entries found", this_install.seen);
        totals.seen += this_install.seen;
        totals.not_ear_wrapped.extend(this_install.not_ear_wrapped);
        totals.decode_failures.extend(this_install.decode_failures);
        totals
            .length_mismatches
            .extend(this_install.length_mismatches);
        totals
            .ear_mirror_mismatches
            .extend(this_install.ear_mirror_mismatches);
        totals.total_compressed_bytes += this_install.total_compressed_bytes;
        totals.total_decompressed_bytes += this_install.total_decompressed_bytes;
        if this_install.max_decompressed > totals.max_decompressed {
            totals.max_decompressed = this_install.max_decompressed;
            totals.max_decompressed_name = this_install.max_decompressed_name;
        }
    }

    println!("== totals ==");
    totals.report();

    assert!(
        totals.seen > 0,
        "no .map/.bse entries found under any of {:?}; is MINT_CORPUS pointing at an install?",
        roots
    );

    assert!(
        totals.not_ear_wrapped.is_empty(),
        "{} .map/.bse entries were not EAR\\0-wrapped: {:?}",
        totals.not_ear_wrapped.len(),
        totals.not_ear_wrapped
    );
    assert!(
        totals.decode_failures.is_empty(),
        "{} of {} files failed to decompress: {:?}",
        totals.decode_failures.len(),
        totals.seen,
        totals.decode_failures
    );
    assert!(
        totals.length_mismatches.is_empty(),
        "{} of {} files decompressed to a length disagreeing with their own header: {:?}",
        totals.length_mismatches.len(),
        totals.seen,
        totals.length_mismatches
    );
    assert!(
        totals.ear_mirror_mismatches.is_empty(),
        "{} of {} files disagree between the EAR wrapper's mirrored size and the actual \
         decompressed length - this is the finding in the module docs; a violation here means \
         it does not hold as generally as measured",
        totals.ear_mirror_mismatches.len(),
        totals.seen
    );
}
