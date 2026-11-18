use std::path::PathBuf;
use std::process::ExitCode;

use mint_sage_map::MapFile;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: map_dump <path>.map [x0 y0 x1 y1]");
        return ExitCode::FAILURE;
    };
    let region: Option<(u32, u32, u32, u32)> = {
        let rest: Vec<String> = args.filter_map(|a| a.into_string().ok()).collect();
        if rest.len() == 4 {
            let nums: Option<Vec<u32>> = rest.iter().map(|s| s.parse().ok()).collect();
            nums.and_then(|n| match (n.first(), n.get(1), n.get(2), n.get(3)) {
                (Some(&a), Some(&b), Some(&c), Some(&d)) => Some((a, b, c, d)),
                _ => None,
            })
        } else {
            None
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let map = match MapFile::parse(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("parse {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let hm = &map.height_map;
    let bt = &map.blend_tile;
    println!("file: {} ({} bytes)", path.display(), bytes.len());
    println!(
        "HeightMapData: width={} height={} border={} boundaries={:?}",
        hm.width, hm.height, hm.border_size, hm.boundaries
    );
    println!(
        "BlendTileData: version={} cell_count={} planes={}",
        bt.version,
        bt.cell_count,
        bt.bit_planes.len()
    );

    let row_stride = hm.width.div_ceil(8) as usize;
    for (i, plane) in bt.bit_planes.iter().enumerate() {
        let set: u64 = plane.iter().map(|b| u64::from(b.count_ones())).sum();
        let total = (hm.width as u64) * (hm.height as u64);
        let pct = if total == 0 {
            0.0
        } else {
            100.0 * set as f64 / total as f64
        };
        let plane_bytes = plane.len();
        println!(
            "  plane[{i}]: {plane_bytes} bytes, row_stride={row_stride}, {set} bits set ({pct:.3}% of {total} cells)"
        );
    }
    if let Some(bytes) = &bt.unknown_cell_byte {
        let nonzero = bytes.iter().filter(|&&b| b != 0).count();
        println!(
            "  unknown_cell_byte: {} entries, {nonzero} nonzero",
            bytes.len()
        );
    }

    if region.is_none() {
        if let Some(bytes) = &bt.unknown_cell_byte {
            let mut printed = 0usize;
            for (idx, &b) in bytes.iter().enumerate() {
                if b == 0 {
                    continue;
                }
                let idx_u32 = u32::try_from(idx).unwrap_or(u32::MAX);
                let x = idx_u32 % hm.width.max(1);
                let y = idx_u32 / hm.width.max(1);
                println!("  unknown_cell_byte[{x},{y}] = {b}");
                printed += 1;
                if printed >= 400 {
                    println!("  ... (capped at 400)");
                    break;
                }
            }
        }
        for (i, plane) in bt.bit_planes.iter().enumerate() {
            let set: u64 = plane.iter().map(|b| u64::from(b.count_ones())).sum();
            let total = (hm.width as u64) * (hm.height as u64);
            let majority_one = set * 2 > total;
            let mut printed = 0usize;
            for y in 0..hm.height {
                for x in 0..hm.width {
                    let bit = bt.plane_bit(i, x, y).unwrap_or(false);
                    if bit != majority_one {
                        println!("  plane[{i}] minority bit at ({x},{y}) = {bit}");
                        printed += 1;
                        if printed >= 400 {
                            println!("  ... (capped at 400)");
                            break;
                        }
                    }
                }
                if printed >= 400 {
                    break;
                }
            }
        }
    }

    if let Some((x0, y0, x1, y1)) = region {
        println!("\nregion ({x0},{y0})-({x1},{y1}):");
        for i in 0..bt.bit_planes.len() {
            println!("plane[{i}]:");
            for y in y0..=y1 {
                let mut line = String::new();
                for x in x0..=x1 {
                    let bit = bt.plane_bit(i, x, y).unwrap_or(false);
                    line.push(if bit { '#' } else { '.' });
                }
                println!("  y={y:4} {line}");
            }
        }
        println!("heights:");
        for y in y0..=y1 {
            let mut line = String::new();
            for x in x0..=x1 {
                let h = hm.sample(x, y).unwrap_or(0);
                line.push_str(&format!("{h:5} "));
            }
            println!("  y={y:4} {line}");
        }
    }

    ExitCode::SUCCESS
}
