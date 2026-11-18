use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_save::{
    between_missions_defaults, AudioBlock, CampaignBlock, GameLogicBlock, GameStateBlock,
    GameStateMapBlock, LivingWorldBlock, SaveError, SaveFile, CHUNK_AUDIO, CHUNK_CAMPAIGN,
    CHUNK_GAME_LOGIC, CHUNK_GAME_STATE, CHUNK_GAME_STATE_MAP, CHUNK_LIVING_WORLD_LOGIC,
};

#[derive(Default)]
struct Tally {
    seen: usize,
    typed_exact: usize,
    raw: usize,
    failed: usize,
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("BfME2Campaign")
                || ext.eq_ignore_ascii_case("BfME2Skirmish")
            {
                out.push(path);
            }
        }
    }
}

fn diff(label: &str, got: &impl std::fmt::Debug, want: &impl std::fmt::Debug) -> Vec<String> {
    let g = format!("{got:#?}");
    let w = format!("{want:#?}");
    g.lines()
        .zip(w.lines())
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("{label}: {} (default {})", a.trim(), b.trim()))
        .collect()
}

fn main() {
    let mut paths = Vec::new();
    for arg in std::env::args().skip(1) {
        files(Path::new(&arg), &mut paths);
    }
    paths.sort();
    let (d_lw, d_au, d_gsm, d_gl) = between_missions_defaults();
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    let (mut parsed, mut round_trip, mut parse_failed) = (0usize, 0usize, 0usize);
    let mut nested_exact = 0usize;
    for path in &paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(bytes) = std::fs::read(path) else {
            println!("{name}: unreadable");
            parse_failed += 1;
            continue;
        };
        let file = match SaveFile::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                println!("{name}: container error: {e}");
                parse_failed += 1;
                continue;
            }
        };
        parsed += 1;
        if file.to_bytes() == bytes {
            round_trip += 1;
        }
        let is_campaign = name.to_ascii_lowercase().ends_with(".bfme2campaign");
        let mut diffs = Vec::new();
        for (i, block) in file.blocks.iter().enumerate() {
            let off = file.payload_offset(i).unwrap_or(0);
            let p = block.payload();
            let key = String::from_utf8_lossy(block.name()).into_owned();
            let t = tallies.entry(key).or_default();
            t.seen += 1;
            let n = block.name();
            let res: Result<Vec<String>, SaveError> = if n == CHUNK_CAMPAIGN {
                CampaignBlock::decode(p, off).and_then(|b| {
                    let again = b.encode(off);
                    if again != p {
                        return Err(SaveError::NotBetweenMissions("campaign re-encode differs"));
                    }
                    if b.campaign.is_some() {
                        nested_exact += 1;
                    }
                    Ok(Vec::new())
                })
            } else if n == CHUNK_GAME_STATE {
                GameStateBlock::decode(p, off).map(|b| {
                    vec![format!(
                        "GameState: date {:?}, type {}, auto {}",
                        b.date, b.save_file_type, b.is_auto_save
                    )]
                })
            } else if n == CHUNK_LIVING_WORLD_LOGIC {
                LivingWorldBlock::decode(p, off).map(|b| diff("LivingWorld", &b, &d_lw))
            } else if n == CHUNK_AUDIO {
                AudioBlock::decode(p, off).map(|b| diff("Audio", &b, &d_au))
            } else if n == CHUNK_GAME_STATE_MAP {
                GameStateMapBlock::decode(p, off).map(|b| diff("GameStateMap", &b, &d_gsm))
            } else if n == CHUNK_GAME_LOGIC {
                GameLogicBlock::decode(p, off).map(|b| diff("GameLogic", &b, &d_gl))
            } else {
                t.raw += 1;
                continue;
            };
            match res {
                Ok(d) => {
                    t.typed_exact += 1;
                    diffs.extend(d);
                }
                Err(SaveError::NotBetweenMissions(_)) => t.raw += 1,
                Err(e) => {
                    t.failed += 1;
                    println!("{name}: {}: {e}", String::from_utf8_lossy(n));
                }
            }
        }
        if is_campaign {
            println!(
                "{name}: {} blocks; fields differing from defaults:",
                file.blocks.len()
            );
            for d in diffs {
                println!("    {d}");
            }
        }
    }
    println!(
        "files {} / container parsed {parsed} / parse failed {parse_failed} / byte-identical round trip {round_trip}",
        paths.len()
    );
    println!("campaign blocks with a run whose nested block ends with its parent: {nested_exact}");
    for (name, t) in &tallies {
        println!(
            "{name}: seen {} typed-and-end-exact {} raw {} failed {}",
            t.seen, t.typed_exact, t.raw, t.failed
        );
    }
}
