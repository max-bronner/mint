use std::path::PathBuf;
use std::process::ExitCode;

use mint_render::{Camera, Renderer, Scene, UiQuad};
use mint_ui::{Font, FontId, GlyphAtlas, GlyphAtlasConfig};

const CANVAS_WIDTH: u32 = 1800;
const LEFT_MARGIN: f32 = 28.0;
const MARKER_BAR_WIDTH: f32 = 14.0;
const TEXT_X: f32 = LEFT_MARGIN + MARKER_BAR_WIDTH + 16.0;
const ROW_HEIGHT: f32 = 152.0;
const ROW_GAP: f32 = 12.0;
const LEGEND_HEIGHT: f32 = 190.0;
const BOTTOM_MARGIN: f32 = 30.0;
const LEGEND_LINE_Y0: f32 = 96.0;
const LEGEND_LINE_HEIGHT: f32 = 26.0;

const LABEL_SIZE: u32 = 22;
const SPECIMEN_SIZE: u32 = 28;
const NOTE_SIZE: u32 = 15;

const SPECIMEN_LINE_1: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz";
const SPECIMEN_LINE_2: &str = "0123456789 .,:;!?'\"()-";

const BG_COLOR: [f32; 4] = [0.07, 0.08, 0.10, 1.0];
const LABEL_COLOR: [f32; 4] = [0.95, 0.95, 0.97, 1.0];
const SPECIMEN_COLOR: [f32; 4] = [0.90, 0.90, 0.93, 1.0];
const NOTE_COLOR: [f32; 4] = [0.60, 0.62, 0.68, 1.0];
const TITLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Game,
    Host,
    Sub,
}

impl Kind {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "GAME" => Some(Self::Game),
            "HOST" => Some(Self::Host),
            "SUB" => Some(Self::Sub),
            _ => None,
        }
    }

    fn color(self) -> [f32; 4] {
        match self {
            Self::Game => [0.95, 0.78, 0.20, 1.0],
            Self::Host => [0.35, 0.68, 0.95, 1.0],
            Self::Sub => [0.45, 0.85, 0.55, 1.0],
        }
    }

    fn legend_text(self) -> &'static str {
        match self {
            Self::Game => {
                "GAME  - ships inside the install itself. The only rung the fallback \
                 stack can promise (owner-rulings.md 5.10)."
            }
            Self::Host => {
                "HOST  - found as an installed system font on THIS machine (Windows). \
                 Presence on macOS/Linux is not checked from here."
            }
            Self::Sub => {
                "SUB   - a cross-platform substitute candidate found somewhere on this \
                 machine; exact provenance is noted per row, since not all of these are \
                 installed OS fonts."
            }
        }
    }
}

struct Row {
    kind: Kind,
    label: String,
    note: String,
    font_path: PathBuf,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(manifest_path), Some(out_path)) = (args.first(), args.get(1)) else {
        eprintln!("usage: font_sheet <manifest file> <out.tga>");
        return ExitCode::FAILURE;
    };

    let manifest = match std::fs::read_to_string(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{manifest_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rows = parse_manifest(&manifest);
    if rows.is_empty() {
        eprintln!("{manifest_path}: no rows parsed");
        return ExitCode::FAILURE;
    }

    let chrome_font_path = rows
        .iter()
        .find(|r| r.label.starts_with("Times New Roman"))
        .map(|r| r.font_path.clone());
    let Some(chrome_font_path) = chrome_font_path else {
        eprintln!("manifest must include a \"Times New Roman\" row - used as the chrome font");
        return ExitCode::FAILURE;
    };
    let chrome_font = match load_font(&chrome_font_path) {
        Some(f) => f,
        None => {
            eprintln!("{}: chrome font failed to load", chrome_font_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut loaded: Vec<(Row, Font)> = Vec::new();
    for row in rows {
        match load_font(&row.font_path) {
            Some(f) => loaded.push((row, f)),
            None => eprintln!(
                "skipped: {} ({}) - could not be parsed as a font",
                row.label,
                row.font_path.display()
            ),
        }
    }
    if loaded.is_empty() {
        eprintln!("no row's font loaded; nothing to render");
        return ExitCode::FAILURE;
    }

    let row_count = loaded.len();
    #[allow(clippy::cast_precision_loss)]
    let rows_height =
        (row_count as f32) * ROW_HEIGHT + (row_count.saturating_sub(1)) as f32 * ROW_GAP;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let canvas_height = (LEGEND_HEIGHT + rows_height + BOTTOM_MARGIN).ceil() as u32;

    let mut renderer = match Renderer::offscreen(CANVAS_WIDTH, canvas_height) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("offscreen renderer: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut atlas = match GlyphAtlas::new(GlyphAtlasConfig {
        max_new_glyphs_per_frame: 4096,
        ..GlyphAtlasConfig::default_config()
    }) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("atlas: {e}");
            return ExitCode::FAILURE;
        }
    };

    let placeholder = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]);
    let Some(placeholder) = placeholder else {
        eprintln!("unreachable: 1x1 rgba8 is always valid");
        return ExitCode::FAILURE;
    };
    let mut texture = match renderer.upload_texture(&placeholder) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("texture upload: {e}");
            return ExitCode::FAILURE;
        }
    };

    let runs = build_runs(&loaded, &chrome_font);

    atlas.begin_frame();
    for run in &runs {
        let _ = mint_ui::layout_text(
            run.text,
            run.origin,
            run.color,
            run.font_id,
            run.font,
            &mut atlas,
            run.size_px,
            texture,
        );
    }

    if atlas.take_dirty() {
        let (aw, ah) = atlas.dimensions();
        let data = match mint_render::TextureData::rgba8(aw, ah, atlas.pixels().to_vec()) {
            Some(d) => d,
            None => {
                eprintln!("unreachable: atlas pixel buffer size does not match its own dimensions");
                return ExitCode::FAILURE;
            }
        };
        texture = match renderer.upload_texture(&data) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("atlas texture upload: {e}");
                return ExitCode::FAILURE;
            }
        };
    }

    atlas.begin_frame();
    let mut quads: Vec<UiQuad> = Vec::new();
    quads.push(background_quad(CANVAS_WIDTH, canvas_height));
    for (i, (row, _)) in loaded.iter().enumerate() {
        let y = row_top(i);
        quads.push(marker_bar(row.kind, y));
    }
    for (i, kind) in [Kind::Game, Kind::Host, Kind::Sub].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = LEGEND_LINE_Y0 + (i as f32) * LEGEND_LINE_HEIGHT;
        quads.push(
            UiQuad::new(LEFT_MARGIN, y - 14.0, 18.0, 18.0)
                .with_color_transform(kind.color(), [0.0, 0.0, 0.0, 0.0]),
        );
    }
    for run in &runs {
        quads.extend(mint_ui::layout_text(
            run.text,
            run.origin,
            run.color,
            run.font_id,
            run.font,
            &mut atlas,
            run.size_px,
            texture,
        ));
    }

    let scene = Scene::new(Camera::default()).with_ui_quads(&quads);
    if let Err(e) = renderer.render(&scene) {
        eprintln!("render: {e}");
        return ExitCode::FAILURE;
    }
    let rgba = match renderer.read_pixels() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("read_pixels: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = write_tga(
        std::path::Path::new(out_path),
        CANVAS_WIDTH,
        canvas_height,
        &rgba,
    ) {
        eprintln!("{out_path}: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "wrote {out_path} ({CANVAS_WIDTH}x{canvas_height}, {row_count} rows rendered; \
         any skipped rows were reported on stderr above)"
    );
    ExitCode::SUCCESS
}

struct Run<'a> {
    text: &'a str,
    origin: [f32; 2],
    color: [f32; 4],
    font_id: FontId,
    font: Option<&'a Font>,
    size_px: u32,
}

fn row_top(row_index: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let i = row_index as f32;
    LEGEND_HEIGHT + i * (ROW_HEIGHT + ROW_GAP)
}

fn build_runs<'a>(loaded: &'a [(Row, Font)], chrome_font: &'a Font) -> Vec<Run<'a>> {
    let mut runs = Vec::new();
    let chrome_id = FontId(0);

    runs.push(Run {
        text: "mint font comparison sheet",
        origin: [LEFT_MARGIN, 46.0],
        color: TITLE_COLOR,
        font_id: chrome_id,
        font: Some(chrome_font),
        size_px: 34,
    });
    runs.push(Run {
        text: "docs/decisions/owner-rulings.md 5.10 / ui-substrate.md 7c - render-rebuild - mint's own text pipeline (Font -> outline -> raster -> atlas -> UiQuad -> Renderer)",
        origin: [LEFT_MARGIN, 70.0],
        color: NOTE_COLOR,
        font_id: chrome_id,
        font: Some(chrome_font),
        size_px: NOTE_SIZE,
    });

    for (i, kind) in [Kind::Game, Kind::Host, Kind::Sub].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = LEGEND_LINE_Y0 + (i as f32) * LEGEND_LINE_HEIGHT;
        runs.push(Run {
            text: kind.legend_text(),
            origin: [LEFT_MARGIN + 34.0, y],
            color: LABEL_COLOR,
            font_id: chrome_id,
            font: Some(chrome_font),
            size_px: NOTE_SIZE,
        });
    }

    for (i, (row, font)) in loaded.iter().enumerate() {
        let top = row_top(i);
        let font_id = FontId(u32::try_from(i.saturating_add(1)).unwrap_or(u32::MAX));
        runs.push(Run {
            text: &row.label,
            origin: [TEXT_X, top + 30.0],
            color: LABEL_COLOR,
            font_id: chrome_id,
            font: Some(chrome_font),
            size_px: LABEL_SIZE,
        });
        runs.push(Run {
            text: SPECIMEN_LINE_1,
            origin: [TEXT_X, top + 66.0],
            color: SPECIMEN_COLOR,
            font_id,
            font: Some(font),
            size_px: SPECIMEN_SIZE,
        });
        runs.push(Run {
            text: SPECIMEN_LINE_2,
            origin: [TEXT_X, top + 100.0],
            color: SPECIMEN_COLOR,
            font_id,
            font: Some(font),
            size_px: SPECIMEN_SIZE,
        });
        runs.push(Run {
            text: &row.note,
            origin: [TEXT_X, top + 132.0],
            color: NOTE_COLOR,
            font_id: chrome_id,
            font: Some(chrome_font),
            size_px: NOTE_SIZE,
        });
    }

    runs
}

fn background_quad(width: u32, height: u32) -> UiQuad {
    #[allow(clippy::cast_precision_loss)]
    UiQuad::new(0.0, 0.0, width as f32, height as f32)
        .with_color_transform(BG_COLOR, [0.0, 0.0, 0.0, 0.0])
}

fn marker_bar(kind: Kind, top: f32) -> UiQuad {
    UiQuad::new(LEFT_MARGIN, top + 4.0, MARKER_BAR_WIDTH, ROW_HEIGHT - 8.0)
        .with_color_transform(kind.color(), [0.0, 0.0, 0.0, 0.0])
}

fn load_font(path: &std::path::Path) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    Font::from_bytes(bytes).ok()
}

fn parse_manifest(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [kind, label, note, path] = fields.as_slice() else {
            eprintln!(
                "manifest line {}: expected 4 tab-separated fields, got {}",
                n.saturating_add(1),
                fields.len()
            );
            continue;
        };
        let Some(kind) = Kind::parse(kind) else {
            eprintln!(
                "manifest line {}: unknown kind {kind:?}",
                n.saturating_add(1)
            );
            continue;
        };
        rows.push(Row {
            kind,
            label: (*label).to_owned(),
            note: (*note).to_owned(),
            font_path: PathBuf::from(*path),
        });
    }
    rows
}

fn write_tga(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_expected_row_count_and_skips_comments_and_blanks() {
        let text = "# a comment\nGAME\tLabel One\tnote one\t/a/b.otf\n\nHOST\tLabel Two\tnote two\t/c/d.ttf\n";
        let rows = parse_manifest(text);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].kind == Kind::Game);
        assert_eq!(rows[0].label, "Label One");
        assert!(rows[1].kind == Kind::Host);
    }

    #[test]
    fn a_line_with_the_wrong_field_count_is_skipped_not_fatal() {
        let text = "GAME\tonly two fields\n";
        let rows = parse_manifest(text);
        assert!(rows.is_empty());
    }

    #[test]
    fn an_unknown_kind_is_skipped_not_fatal() {
        let text = "WEIRD\tLabel\tnote\t/a.ttf\n";
        let rows = parse_manifest(text);
        assert!(rows.is_empty());
    }

    #[test]
    fn a_missing_or_garbage_font_file_is_reported_as_none_not_a_panic() {
        assert!(load_font(std::path::Path::new(
            "definitely-does-not-exist-anywhere.ttf"
        ))
        .is_none());
    }

    #[test]
    fn row_top_is_monotonically_increasing_and_matches_legend_height() {
        assert_eq!(row_top(0), LEGEND_HEIGHT);
        assert!(row_top(1) > row_top(0));
    }
}
