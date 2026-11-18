use super::*;

pub(crate) fn write_tga(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(usize::try_from(height).ok()?))
        .and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(rgba.len()) || width == 0 || height == 0 {
        return Err(format!(
            "screenshot size mismatch: {width}x{height} wants {expected:?} bytes, got {}",
            rgba.len()
        ));
    }
    let w16 = u16::try_from(width).map_err(|_| format!("{width}: too wide for a TGA"))?;
    let h16 = u16::try_from(height).map_err(|_| format!("{height}: too tall for a TGA"))?;

    let mut out = Vec::with_capacity(18usize.saturating_add(rgba.len()));
    out.push(0);
    out.push(0);
    out.push(2);
    out.extend_from_slice(&[0; 5]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&w16.to_le_bytes());
    out.extend_from_slice(&h16.to_le_bytes());
    out.push(32);
    out.push(0x28);

    for pixel in rgba.chunks_exact(4) {
        match pixel {
            [r, g, b, a] => out.extend_from_slice(&[*b, *g, *r, *a]),
            _ => return Err("unreachable: chunks_exact(4) always yields 4 bytes".to_owned()),
        }
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) struct MapMatch {
    pub(crate) archive: Option<PathBuf>,
    pub(crate) entry_name: String,
}

pub(crate) fn find_maps(root: &Path, needle: &str) -> Vec<MapMatch> {
    let direct = Path::new(needle);
    if direct.is_file() {
        return vec![MapMatch {
            archive: None,
            entry_name: needle.to_owned(),
        }];
    }
    let needle = needle.to_ascii_lowercase();
    let mut found = Vec::new();
    for archive_path in mint_sage_big::archives_under(root) {
        let Ok(archive) = BigArchive::open(&archive_path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            let is_map = lower.ends_with(".map") || lower.ends_with(".bse");
            if is_map && lower.contains(&needle) {
                found.push(MapMatch {
                    archive: Some(archive_path.clone()),
                    entry_name: entry.name.clone(),
                });
            }
        }
    }
    found.sort_by(|a, b| a.entry_name.cmp(&b.entry_name));
    found
}

pub(crate) fn read_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>, String> {
    let archive =
        BigArchive::open(archive_path).map_err(|e| format!("{}: {e}", archive_path.display()))?;
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name == entry_name)
        .ok_or_else(|| {
            format!(
                "{entry_name}: no longer present in {}",
                archive_path.display()
            )
        })?;
    archive
        .read_entry(entry)
        .map_err(|e| format!("{entry_name}: {e}"))
}
