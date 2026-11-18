use std::path::PathBuf;
use std::process::ExitCode;

use mint_sage_map::{Boundary, HeightMapData, MapFile, MAP_XY_FACTOR};

const VERTICAL_SCALE: f32 = 0.0390625;

struct RowLayout {
    y0: u32,
    depth: u32,
    start_x: u32,
    run: u32,
    gap: u32,
}

const ROW: RowLayout = RowLayout {
    y0: 60,
    depth: 8,
    start_x: 40,
    run: 4,
    gap: 2,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut start_deg: f64 = 10.0;
    let mut end_deg: f64 = 70.0;
    let mut step_deg: f64 = 5.0;
    let mut i = 0;
    while i < args.len() {
        let Some(current) = args.get(i) else {
            break;
        };
        match current.as_str() {
            "--start-deg" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(v) => start_deg = v,
                    None => {
                        eprintln!("--start-deg needs a number");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--end-deg" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(v) => end_deg = v,
                    None => {
                        eprintln!("--end-deg needs a number");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--step-deg" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(v) => step_deg = v,
                    None => {
                        eprintln!("--step-deg needs a number");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    let (Some(input), Some(output)) = (positional.first(), positional.get(1)) else {
        eprintln!(
            "usage: ramp_map <in>.map <out>.map [--start-deg N] [--end-deg N] [--step-deg N]"
        );
        return ExitCode::FAILURE;
    };
    if step_deg <= 0.0 || end_deg < start_deg {
        eprintln!("need start-deg <= end-deg and a positive step-deg");
        return ExitCode::FAILURE;
    }

    let angles = angle_list(start_deg, end_deg, step_deg);
    if angles.is_empty() {
        eprintln!("no angles in range");
        return ExitCode::FAILURE;
    }

    match run(&PathBuf::from(input), &PathBuf::from(output), &angles) {
        Ok(()) => {
            println!(
                "wrote {} with {} ramp(s): {}",
                output,
                angles.len(),
                angles
                    .iter()
                    .map(|d| format!("{d:.0}deg"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn angle_list(start: f64, end: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut a = start;
    let mut guard = 0;
    while a <= end + 1e-9 && guard < 100_000 {
        out.push(a);
        a += step;
        guard += 1;
    }
    out
}

fn run(input: &PathBuf, output: &PathBuf, angles: &[f64]) -> Result<(), String> {
    let file_bytes = std::fs::read(input).map_err(|e| format!("read {}: {e}", input.display()))?;
    let map = MapFile::parse(&file_bytes).map_err(|e| format!("parse {}: {e}", input.display()))?;

    let stream = mint_sage_refpack::strip_ear_wrapper(&file_bytes)
        .ok_or_else(|| format!("{} is not EAR\\0-wrapped", input.display()))?;
    let mut body = mint_sage_refpack::decompress(stream).map_err(|e| format!("decompress: {e}"))?;

    let hm_info = map
        .chunks
        .iter()
        .find(|c| c.name == "HeightMapData")
        .ok_or("no HeightMapData chunk")?;

    let new_samples = ramp_samples(&map.height_map, angles)?;
    let new_payload = encode_height_map_payload(&map.height_map, &new_samples);
    if new_payload.len() != hm_info.len {
        return Err(format!(
            "internal error: rebuilt HeightMapData payload is {} bytes, expected {}",
            new_payload.len(),
            hm_info.len
        ));
    }
    let start = hm_info.offset;
    let end = start
        .checked_add(hm_info.len)
        .ok_or("HeightMapData span overflowed")?;
    let dest = body
        .get_mut(start..end)
        .ok_or("HeightMapData span is outside the decompressed body")?;
    dest.copy_from_slice(&new_payload);

    let out_bytes = wrap_ear_refpack(&body)?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
    }
    std::fs::write(output, &out_bytes).map_err(|e| format!("write {}: {e}", output.display()))?;

    let round_trip =
        MapFile::parse(&out_bytes).map_err(|e| format!("round-trip parse failed: {e}"))?;
    if round_trip.chunks.len() != map.chunks.len() {
        return Err(format!(
            "round-trip chunk count changed: {} -> {}",
            map.chunks.len(),
            round_trip.chunks.len()
        ));
    }
    for (before, after) in map.chunks.iter().zip(round_trip.chunks.iter()) {
        if before.name != "HeightMapData"
            && map.raw_payload(before) != round_trip.raw_payload(after)
        {
            return Err(format!(
                "round-trip changed chunk {:?}, which should have been untouched",
                before.name
            ));
        }
    }
    println!(
        "round-trip check: {} chunks, every one but HeightMapData byte-identical",
        round_trip.chunks.len()
    );
    Ok(())
}

fn ramp_samples(hm: &HeightMapData, angles: &[f64]) -> Result<Vec<u16>, String> {
    let width = hm.width;
    let height = hm.height;
    if ROW.y0.saturating_add(ROW.depth) > height {
        return Err(format!(
            "ramp row (y {}..{}) does not fit a {}-tall map",
            ROW.y0,
            ROW.y0 + ROW.depth,
            height
        ));
    }
    let block = ROW.run.saturating_add(ROW.gap);
    let last_ramp_end = ROW
        .start_x
        .saturating_add(block.saturating_mul(u32::try_from(angles.len()).unwrap_or(u32::MAX)));
    if last_ramp_end > width {
        return Err(format!(
            "{} ramps at {} cells each do not fit a {}-wide map starting at x={}",
            angles.len(),
            block,
            width,
            ROW.start_x
        ));
    }

    let base = hm.sample(ROW.start_x, ROW.y0).unwrap_or(0);

    let mut row_heights = vec![base; usize::try_from(width).unwrap_or(0)];
    for (i, &angle_deg) in angles.iter().enumerate() {
        let ramp_start = ROW.start_x + u32::try_from(i).unwrap_or(0) * block;
        let radians = angle_deg.to_radians();
        let raw_per_world_unit = f64::from(MAP_XY_FACTOR) / f64::from(VERTICAL_SCALE);
        let step_raw = (raw_per_world_unit * mint_math::f64::tan(radians)).round();
        for cell in 0..ROW.run {
            let x = ramp_start + cell;
            let Some(slot) = row_heights.get_mut(usize::try_from(x).unwrap_or(usize::MAX)) else {
                continue;
            };
            let rise = step_raw * f64::from(cell.saturating_add(1));
            let value = f64::from(base) + rise;
            let clamped = value.round().clamp(0.0, f64::from(u16::MAX));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let as_u16 = clamped as u16;
            *slot = as_u16;
        }
    }

    let mut samples = hm.samples.clone();
    for y in ROW.y0..(ROW.y0 + ROW.depth) {
        for x in 0..width {
            let idx = usize::try_from(y).unwrap_or(0) * usize::try_from(width).unwrap_or(1)
                + usize::try_from(x).unwrap_or(0);
            if let (Some(dst), Some(&h)) = (
                samples.get_mut(idx),
                row_heights.get(usize::try_from(x).unwrap_or(usize::MAX)),
            ) {
                *dst = h;
            }
        }
    }
    Ok(samples)
}

fn encode_height_map_payload(hm: &HeightMapData, samples: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + hm.boundaries.len() * 8 + 4 + samples.len() * 2);
    out.extend_from_slice(&hm.width.to_le_bytes());
    out.extend_from_slice(&hm.height.to_le_bytes());
    out.extend_from_slice(&hm.border_size.to_le_bytes());
    let boundary_count = u32::try_from(hm.boundaries.len()).unwrap_or(0);
    out.extend_from_slice(&boundary_count.to_le_bytes());
    for Boundary { x, y } in &hm.boundaries {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
    }
    let sample_count = u32::try_from(samples.len()).unwrap_or(0);
    out.extend_from_slice(&sample_count.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn wrap_ear_refpack(body: &[u8]) -> Result<Vec<u8>, String> {
    let len = u32::try_from(body.len()).map_err(|_| "body too large for a u32 length")?;
    if len >= (1 << 24) {
        return Err("body too large for this tool's 3-byte length field".to_string());
    }
    let mut out = Vec::with_capacity(13 + body.len() + body.len() / 100);
    out.extend_from_slice(b"EAR\0");
    out.extend_from_slice(&len.to_le_bytes());
    out.push(0x00);
    out.push(0xFB);
    let be = len.to_be_bytes();
    out.extend_from_slice(&be[1..4]);
    out.extend_from_slice(&encode_literal_only(body));
    Ok(out)
}

fn encode_literal_only(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while body.len().saturating_sub(i) >= 4 {
        let remaining = body.len() - i;
        let take = remaining.min(112) / 4 * 4;
        let blocks = u8::try_from(take / 4).unwrap_or(28);
        let opcode = 0xE0u8 | blocks.saturating_sub(1);
        out.push(opcode);
        out.extend_from_slice(body.get(i..i + take).unwrap_or(&[]));
        i += take;
    }
    let leftover = body.len() - i;
    let stop_opcode = 0xFCu8 | u8::try_from(leftover).unwrap_or(3);
    out.push(stop_opcode);
    out.extend_from_slice(body.get(i..).unwrap_or(&[]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_stream_round_trips_through_the_real_decompressor() {
        for body in [
            Vec::new(),
            vec![1, 2, 3],
            vec![7; 4],
            (0u16..2000).map(|n| (n % 256) as u8).collect(),
        ] {
            let wrapped = wrap_ear_refpack(&body).unwrap();
            let stream = mint_sage_refpack::strip_ear_wrapper(&wrapped).unwrap();
            let round_tripped = mint_sage_refpack::decompress(stream).unwrap();
            assert_eq!(round_tripped, body);
        }
    }

    #[test]
    fn ear_mirror_matches_the_refpack_header_field() {
        let body = vec![9u8; 500];
        let wrapped = wrap_ear_refpack(&body).unwrap();
        let mirror = u32::from_le_bytes(wrapped[4..8].try_into().unwrap());
        assert_eq!(mirror, 500);
        let declared = mint_sage_refpack::declared_len(&wrapped[8..]).unwrap();
        assert_eq!(declared, 500);
    }

    #[test]
    fn angle_list_covers_the_default_range_inclusive() {
        let degs = angle_list(10.0, 70.0, 5.0);
        assert_eq!(degs.len(), 13);
        assert!((degs[0] - 10.0).abs() < 1e-9);
        assert!((degs[12] - 70.0).abs() < 1e-9);
    }

    #[test]
    fn a_zero_or_negative_step_produces_no_infinite_loop() {
        let degs = angle_list(10.0, 70.0, 0.0);
        assert!(degs.len() <= 100_000);
    }

    #[test]
    fn ramp_step_conversion_matches_the_documented_formula() {
        let raw_per_world_unit = f64::from(MAP_XY_FACTOR) / f64::from(VERTICAL_SCALE);
        assert!((raw_per_world_unit - 256.0).abs() < 1e-9);
        let step = raw_per_world_unit * mint_math::f64::tan((45.0f64).to_radians());
        assert!((step - 256.0).abs() < 1e-9);
    }
}
