use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_audio_format::{decode_wav, probe_wav, WaveTag};
use mint_sage_audio::{load_library, AudioLibrary};
use mint_sage_big::BigArchive;
use mint_sage_ini::audio::{AudioEventKind, AudioPriority};
use mint_vfs::Vfs;

const MAX_REPORTED: usize = 20;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: audio_corpus <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last, so its archives shadow BFME II's");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    let mut vfs = Vfs::new();
    let mut archives = 0usize;
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                eprintln!("could not open {}", path.display());
                continue;
            };
            archives = archives.saturating_add(1);
            vfs.mount(Arc::new(archive));
        }
    }
    println!(
        "{archives} archives mounted, {} distinct paths, in {:.2}s",
        vfs.len(),
        started.elapsed().as_secs_f64()
    );
    if vfs.is_empty() {
        eprintln!("no archive entries found under any given install - wrong path?");
        return ExitCode::FAILURE;
    }

    let ok_wav = wav_census(&vfs);
    let ok_data = data_census(&vfs);

    println!("\ntotal wall clock {:.2}s", started.elapsed().as_secs_f64());
    if ok_wav && ok_data {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn wav_census(vfs: &Vfs) -> bool {
    println!("\n=== every .wav ===");
    let paths: Vec<String> = vfs
        .paths()
        .filter(|p| p.as_str().ends_with(".wav"))
        .map(|p| p.as_str().to_string())
        .collect();
    println!("{} .wav entries", paths.len());

    let started = Instant::now();
    let mut combinations: BTreeMap<(u16, u32, u16, u16), usize> = BTreeMap::new();
    let mut probe_failures: Vec<String> = Vec::new();
    let mut decode_failures: Vec<String> = Vec::new();
    let mut block_relation_breaks: Vec<String> = Vec::new();
    let mut fact_disagreements: Vec<String> = Vec::new();
    let mut rate_disagreements: Vec<String> = Vec::new();
    let mut truncated: Vec<String> = Vec::new();
    let mut adpcm = 0usize;
    let mut pcm = 0usize;
    let mut stereo = 0usize;
    let mut with_fact = 0usize;
    let mut trailing_pad_block = 0usize;
    let mut compressed_bytes = 0usize;
    let mut decoded_samples = 0usize;
    let mut total_ms = 0u64;

    for path in &paths {
        let Ok(bytes) = vfs.read(path) else {
            probe_failures.push(format!("{path}: not readable from the archive"));
            continue;
        };
        compressed_bytes = compressed_bytes.saturating_add(bytes.len());
        let info = match probe_wav(&bytes) {
            Ok(info) => info,
            Err(error) => {
                probe_failures.push(format!("{path}: {error}"));
                continue;
            }
        };
        let format = info.format;
        *combinations
            .entry((
                format.raw_tag,
                format.sample_rate,
                format.channels,
                format.bits_per_sample,
            ))
            .or_insert(0) += 1;
        match format.tag {
            WaveTag::ImaAdpcm => adpcm = adpcm.saturating_add(1),
            WaveTag::Pcm => pcm = pcm.saturating_add(1),
            _ => {}
        }
        if format.channels > 1 {
            stereo = stereo.saturating_add(1);
        }
        if info.is_truncated() {
            truncated.push(format!(
                "{path}: data chunk declares {} bytes, {} present",
                info.declared_data_bytes, info.data_bytes
            ));
        }
        total_ms = total_ms.saturating_add(info.duration_ms());

        if format.tag == WaveTag::ImaAdpcm {
            match format.stock_samples_per_block() {
                Some(stock) if stock == format.samples_per_block => {}
                other => block_relation_breaks.push(format!(
                    "{path}: blockAlign {} channels {} declares {} samples/block, relation gives {:?}",
                    format.block_align, format.channels, format.samples_per_block, other
                )),
            }
        }

        if let Some(fact) = info.fact_frames {
            with_fact = with_fact.saturating_add(1);
            let fact = u64::from(fact);
            let spb = u64::from(format.samples_per_block).max(1);
            let blocks_in_data = (info.data_bytes as u64).div_ceil(u64::from(format.block_align));
            let blocks_for_fact = fact.div_ceil(spb);
            let extra = blocks_in_data.saturating_sub(blocks_for_fact);
            if blocks_in_data < blocks_for_fact || extra > 1 {
                fact_disagreements.push(format!(
                    "{path}: fact needs {blocks_for_fact} blocks of {spb} frames,                      the data holds {blocks_in_data}"
                ));
            } else if extra == 1 {
                trailing_pad_block = trailing_pad_block.saturating_add(1);
            }
        }

        if let Some(implied) = format.implied_bytes_per_second() {
            if format.declared_bytes_per_second != 0 && format.declared_bytes_per_second != implied
            {
                rate_disagreements.push(format!(
                    "{path}: nAvgBytesPerSec {} declared, {implied} implied",
                    format.declared_bytes_per_second
                ));
            }
        }

        match decode_wav(&bytes) {
            Ok(pcm) => decoded_samples = decoded_samples.saturating_add(pcm.samples.len()),
            Err(error) => decode_failures.push(format!("{path}: {error}")),
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    let mib = compressed_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "{} probed, {} failed | {adpcm} IMA ADPCM, {pcm} PCM, {stereo} stereo, {with_fact} with a fact chunk",
        paths.len().saturating_sub(probe_failures.len()),
        probe_failures.len()
    );
    println!(
        "{} distinct (tag, rate, channels, bits) combinations",
        combinations.len()
    );
    for ((tag, rate, channels, bits), count) in &combinations {
        println!("  tag {tag:>5}  {rate:>6} Hz  {channels} ch  {bits:>2} bit   {count}");
    }
    println!(
        "decoded {:.1} M samples from {mib:.1} MiB in {elapsed:.2}s = {:.1} MiB/s, \
         {:.1} minutes of audio",
        decoded_samples as f64 / 1.0e6,
        mib / elapsed.max(1.0e-9),
        total_ms as f64 / 60_000.0
    );

    let mut ok = true;
    ok &= report("probe failures", &probe_failures);
    ok &= report("decode failures", &decode_failures);
    ok &= report(
        "cross-check 1, the IMA block relation, broken by",
        &block_relation_breaks,
    );
    note(
        "cross-check 2, files whose `fact` needs more blocks than the data holds",
        &fact_disagreements,
    );
    println!(
        "  (of {with_fact} files with a `fact` chunk, {trailing_pad_block} carry one whole          trailing block the sample count does not need)"
    );
    ok &= report(
        "cross-check 3, nAvgBytesPerSec against the implied rate, disagreeing in",
        &rate_disagreements,
    );
    note(
        "files declaring more data than they carry (clamped, not believed)",
        &truncated,
    );
    ok
}

fn data_census(vfs: &Vfs) -> bool {
    println!("\n=== the audio data model ===");
    let started = Instant::now();
    let library: AudioLibrary = match load_library(vfs) {
        Ok(library) => library,
        Err(error) => {
            eprintln!("could not load the audio library: {error}");
            return false;
        }
    };
    let index = library.index();
    println!(
        "{} definitions ({} blocks, {} redefined) in {:.2}s, {} numeric #define(s)",
        library.len(),
        index.definitions(),
        index.redefinitions(),
        started.elapsed().as_secs_f64(),
        index.macros()
    );
    println!(
        "AudioSettings block found: {} | SampleCount2D {} SampleCount3D {} StreamCount {} \
         MinSampleVolume {:.2} AudioFootprint {} bytes",
        index.saw_settings(),
        library.settings().sample_count_2d,
        library.settings().sample_count_3d,
        library.settings().stream_count,
        library.settings().min_sample_volume,
        library.settings().audio_footprint_in_bytes
    );
    for (name, kind) in mint_sage_ini::audio::DEFAULT_DEFINITION_NAMES {
        println!(
            "  default `{name}` present: {}",
            index.default_for(kind).is_some()
        );
    }

    let mut by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_priority: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut variants = 0usize;
    let mut resolved = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut no_audio = 0usize;
    let mut unresolved_ranges = 0usize;
    let mut mp3_events = 0usize;
    let mut positional = 0usize;
    let mut limited = 0usize;

    for (_, def) in index.events() {
        *by_kind.entry(kind_name(def.kind)).or_insert(0) += 1;
        *by_priority.entry(priority_name(def.priority)).or_insert(0) += 1;
        if def.unresolved_range {
            unresolved_ranges = unresolved_ranges.saturating_add(1);
        }
        if def.types.is_positional() {
            positional = positional.saturating_add(1);
        }
        if def.limit > 0 {
            limited = limited.saturating_add(1);
        }
        if !def.has_audio() {
            no_audio = no_audio.saturating_add(1);
            continue;
        }
        for path in library.variant_paths(def) {
            variants = variants.saturating_add(1);
            if path.ends_with(".mp3") {
                mp3_events = mp3_events.saturating_add(1);
            }
            if vfs.contains(&path) {
                resolved = resolved.saturating_add(1);
            } else {
                missing.push(format!("{}: {path}", def.name));
            }
        }
    }

    println!("by block type:");
    for (kind, count) in &by_kind {
        println!("  {kind:<14} {count}");
    }
    println!("by priority (inherited from DefaultSoundEffect where unset):");
    for (priority, count) in &by_priority {
        println!("  {priority:<10} {count}");
    }
    println!(
        "{positional} positional, {limited} carry a Limit, {no_audio} name nothing playable, \
         {unresolved_ranges} still have an unresolved range"
    );
    println!(
        "cross-check 4: {resolved} of {variants} named files resolve in the archives \
         ({} missing, {mp3_events} of the named files are .mp3 and cannot be decoded by this build)",
        missing.len()
    );

    let inherited_high = by_priority.get("high").copied().unwrap_or(0);
    if index
        .default_for(AudioEventKind::Sound)
        .is_some_and(|d| d.priority == AudioPriority::High)
        && inherited_high < library.len() / 2
    {
        eprintln!(
            "cross-check 5 FAILED: DefaultSoundEffect is `high` but only {inherited_high} of {} \
             definitions came out high - the inheritance is not being applied",
            library.len()
        );
        return false;
    }
    println!("cross-check 5: the DefaultSoundEffect priority reached {inherited_high} definitions");

    note("named files that resolve to nothing", &missing);
    let miss_rate = if variants == 0 {
        0.0
    } else {
        missing.len() as f64 / variants as f64
    };
    println!("  miss rate {:.3}% (threshold 1%)", miss_rate * 100.0);
    if miss_rate > 0.01 {
        eprintln!("cross-check 4 FAILED: more than 1% of named files do not resolve");
        return false;
    }
    true
}

fn kind_name(kind: AudioEventKind) -> &'static str {
    match kind {
        AudioEventKind::Sound => "AudioEvent",
        AudioEventKind::Dialog => "DialogEvent",
        AudioEventKind::Music => "MusicTrack",
        AudioEventKind::Ambient => "AmbientStream",
        AudioEventKind::Stream => "StreamedSound",
    }
}

fn priority_name(priority: AudioPriority) -> &'static str {
    priority.as_str()
}

fn note(what: &str, lines: &[String]) {
    println!("{what}: {}", lines.len());
    for line in lines.iter().take(MAX_REPORTED) {
        println!("  {line}");
    }
    if lines.len() > MAX_REPORTED {
        println!(
            "  ... and {} more",
            lines.len().saturating_sub(MAX_REPORTED)
        );
    }
}

fn report(what: &str, lines: &[String]) -> bool {
    if lines.is_empty() {
        println!("{what}: 0");
        return true;
    }
    println!("{what}: {}", lines.len());
    for line in lines.iter().take(MAX_REPORTED) {
        println!("  {line}");
    }
    if lines.len() > MAX_REPORTED {
        println!("  ... and {} more", lines.len() - MAX_REPORTED);
    }
    false
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out = mint_sage_big::archives_under(root);
    out.sort();
    out
}
