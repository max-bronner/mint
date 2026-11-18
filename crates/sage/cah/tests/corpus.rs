#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::Path;

use mint_sage_big::{archives_under, BigArchive};

fn check(label: &str, bytes: &[u8], fails: &mut Vec<String>) -> bool {
    match mint_sage_cah::parse(bytes) {
        Ok(p) if p.crc_ok && p.trailing == 0 => {
            let rewritten = mint_sage_cah::write(&p.record);
            if rewritten == bytes {
                return true;
            }
            let at = rewritten.iter().zip(bytes).position(|(a, b)| a != b);
            fails.push(format!("{label}: round trip differs at byte {at:?}"));
        }
        Ok(p) => fails.push(format!(
            "{label}: crc_ok {} trailing {}",
            p.crc_ok, p.trailing
        )),
        Err(e) => fails.push(format!("{label}: {e}")),
    }
    false
}

#[test]
#[ignore = "needs MINT_CORPUS"]
fn retail_hero_files_round_trip_byte_exact() {
    let corpus = std::env::var("MINT_CORPUS").ok();
    let user = std::env::var("MINT_CAH_USER").ok();
    if corpus.is_none() && user.is_none() {
        eprintln!("skipped: MINT_CORPUS and MINT_CAH_USER unset");
        return;
    }
    let (mut total, mut exact) = (0usize, 0usize);
    let mut fails = Vec::new();
    for root in corpus.iter().flat_map(|c| c.split(';')) {
        for path in archives_under(Path::new(root)) {
            let Ok(big) = BigArchive::open(&path) else {
                continue;
            };
            for entry in big.entries() {
                if !entry.path.as_str().ends_with(".cah") {
                    continue;
                }
                let Ok(bytes) = big.read_entry(entry) else {
                    continue;
                };
                total += 1;
                exact += usize::from(check(&entry.name, &bytes, &mut fails));
            }
        }
    }
    for file in user.iter().flat_map(|u| u.split(';')) {
        let bytes = std::fs::read(file).unwrap();
        total += 1;
        exact += usize::from(check(file, &bytes, &mut fails));
    }
    eprintln!("cah corpus: {exact}/{total} byte-exact round trips");
    for f in &fails {
        eprintln!("  FAIL {f}");
    }
    assert!(fails.is_empty());
}
