use std::path::{Path, PathBuf};

const GUARD_FRAMES: u32 = 2;

#[derive(Debug, Default)]
pub(crate) struct PictureRequest {
    drawn: u32,
    asked: bool,
}

impl PictureRequest {
    pub(crate) fn prepare(&mut self) {
        self.drawn = 0;
    }

    pub(crate) fn guard_drawn(&mut self) {
        self.drawn = self.drawn.saturating_add(1);
    }

    pub(crate) fn ask(&mut self) {
        self.asked = self.drawn >= GUARD_FRAMES;
    }

    pub(crate) fn take(&mut self) -> bool {
        std::mem::take(&mut self.asked)
    }
}

pub(crate) fn picture_stem(name: &str, unix_seconds: u64) -> String {
    let days = i64::try_from(unix_seconds / 86_400).unwrap_or(0);
    let secs = unix_seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let stem = format!(
        "{name} {month:02}-{day:02}-{year} {:02}-{:02}-{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    );
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn encode_bmp(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    if rgba.len() != w.checked_mul(h)?.checked_mul(4)? || w == 0 || h == 0 {
        return None;
    }
    let row = w.checked_mul(3)?.checked_add(3)? & !3;
    let image = row.checked_mul(h)?;
    let total = image.checked_add(54)?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&u32::try_from(total).ok()?.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&i32::try_from(width).ok()?.to_le_bytes());
    out.extend_from_slice(&i32::try_from(height).ok()?.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&u32::try_from(image).ok()?.to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    for line in rgba.chunks_exact(w * 4).rev() {
        for texel in line.chunks_exact(4) {
            if let [r, g, b, _] = *texel {
                out.extend_from_slice(&[b, g, r]);
            }
        }
        out.resize(out.len() + (row - w * 3), 0);
    }
    Some(out)
}

pub(crate) fn save(
    dir: &Path,
    stem: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<PathBuf, String> {
    let bytes = encode_bmp(width, height, rgba).ok_or("the frame was not width * height texels")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{stem}.bmp"));
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_waits_for_two_drawn_guard_frames() {
        let mut request = PictureRequest::default();
        request.prepare();
        request.guard_drawn();
        request.ask();
        assert!(!request.take());
        request.guard_drawn();
        request.ask();
        assert!(request.take());
        assert!(!request.take());
    }

    #[test]
    fn a_bmp_is_bottom_up_bgr_with_rows_padded_to_four_bytes() {
        let rgba = [
            1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18,
            255,
        ];
        let bmp = encode_bmp(3, 2, &rgba).unwrap();
        assert_eq!(bmp.len(), 54 + 2 * 12);
        assert_eq!(&bmp[54..57], &[12, 11, 10]);
        assert_eq!(&bmp[66..69], &[3, 2, 1]);
        assert_eq!(encode_bmp(3, 2, &rgba[..8]), None);
    }

    #[test]
    fn the_stem_carries_the_name_and_the_utc_stamp_with_forbidden_characters_replaced() {
        assert_eq!(
            picture_stem("a/b:c", 1_700_000_000),
            "a-b-c 11-14-2023 22-13-20"
        );
    }
}
