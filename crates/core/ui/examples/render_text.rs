use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_render::{Camera, Renderer, Scene};
use mint_ui::{Font, FontId, GlyphAtlas, GlyphAtlasConfig};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(font_path), Some(text)) = (args.first(), args.get(1)) else {
        eprintln!("usage: render_text <font file> \"<text>\" [size px] [out.tga]");
        return ExitCode::FAILURE;
    };
    let font_path = font_path.clone();
    let text = text.clone();
    let size_px: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32);
    let out_path = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("render_text.tga"));

    let bytes = match std::fs::read(&font_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{font_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let font = match Font::from_bytes(bytes) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{font_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let width = 512u32;
    let height = 128u32;
    let mut renderer = match Renderer::offscreen(width, height) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("offscreen renderer: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut atlas = match GlyphAtlas::new(GlyphAtlasConfig::default_config()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("atlas: {e}");
            return ExitCode::FAILURE;
        }
    };
    atlas.begin_frame();

    let placeholder = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
        .unwrap_or_else(|| {
            eprintln!("unreachable: 1x1 rgba8 is always valid");
            std::process::exit(1);
        });
    let mut texture = match renderer.upload_texture(&placeholder) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("texture upload: {e}");
            return ExitCode::FAILURE;
        }
    };

    let _ = mint_ui::layout_text(
        &text,
        [8.0, f32::from(u16::try_from(size_px).unwrap_or(32))],
        [1.0, 1.0, 1.0, 1.0],
        FontId(0),
        Some(&font),
        &mut atlas,
        size_px,
        texture,
    );
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
    let quads = mint_ui::layout_text(
        &text,
        [8.0, f32::from(u16::try_from(size_px).unwrap_or(32))],
        [1.0, 1.0, 1.0, 1.0],
        FontId(0),
        Some(&font),
        &mut atlas,
        size_px,
        texture,
    );
    println!("{} glyph quads", quads.len());

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

    if let Err(e) = write_tga(&out_path, width, height, &rgba) {
        eprintln!("{}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    println!("wrote {}", out_path.display());
    ExitCode::SUCCESS
}

fn write_tga(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
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
