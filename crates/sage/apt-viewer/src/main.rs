use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mint_app::{App, InputEvent, Key, WindowConfig};
use mint_render::{
    Camera, MaskShape, Renderer, Scene, ScreenDraw, TextureData, TextureHandle, UiGeometry, UiQuad,
};
use mint_sage_apt::{parse_ru, standalone_texture_path, Character, TextureLocation};
use mint_sage_apt_player::{
    CharacterRef, ColorTransform, HitArea, HitMode, InstanceId, Library, Limits, Outward, Player,
    RenderCommand, RenderList, RenderOptions,
};
use mint_sage_apt_viewer::scene::{
    apply, can_show, on_screen, plan, rect_triangles, Affine, Item, MaskMode, MeshCache, Plan,
    ShapeMesh, TextCache, TextKey, TextLayout,
};
use mint_sage_apt_viewer::{
    load_string_table, localise, resolved_font_file, ru_base_name, text_origin, unpack_color,
    unpack_edittext_color, AnchorCache, Letterbox, MovieLibrary, ScaleMode, TextAnchor, TextBox,
};
use mint_sage_str::StringTable;
use mint_ui::{FontId, GlyphAtlas, GlyphAtlasConfig};

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DEFAULT_FPS: f32 = 30.0;
const SCREENSHOT_WARMUP_FRAMES: u32 = 24;
const GLYPHS_PER_FRAME: usize = 64;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(message) => {
            eprintln!("{message}\n\n{}", Options::usage());
            std::process::exit(1);
        }
    };
    let started = Instant::now();
    let (library, available) =
        mint_sage_apt_viewer::load_player_library(std::slice::from_ref(&options.install));
    if available.is_empty() {
        eprintln!(
            "no playable apt/*.big found under {}",
            options.install.join("apt").display()
        );
        std::process::exit(1);
    }
    let (decoded, failed) = library.block_counts();
    println!(
        "{} movies loaded in {:.2}s; {decoded} action blocks decoded at load ({failed} failed)",
        available.len(),
        started.elapsed().as_secs_f64()
    );
    let start_movie = options
        .movie
        .clone()
        .map(|m| m.to_ascii_lowercase())
        .filter(|name| available.contains(name))
        .or_else(|| available.iter().find(|a| a.as_str() == "mainmenu").cloned())
        .unwrap_or_else(|| available.first().cloned().unwrap_or_default());

    let font = options.font.as_ref().and_then(|path| {
        match std::fs::read(path)
            .ok()
            .and_then(|bytes| mint_ui::Font::from_bytes(bytes).ok())
        {
            Some(f) => {
                println!("HUD font loaded from {}", path.display());
                Some(f)
            }
            None => {
                eprintln!("{}: could not load as a font; HUD disabled", path.display());
                None
            }
        }
    });

    if let Some(path) = options.screenshot.clone() {
        run_screenshot(library, available, &start_movie, font, &path, &options);
        return;
    }

    let config = WindowConfig {
        title: "mint APT viewer".to_owned(),
        width: options.width,
        height: options.height,
        ..WindowConfig::default()
    };
    let fps = options.fps;
    let install = options.install.clone();
    let localise_text = !options.no_localise;
    if let Err(error) = mint_app::run(&config, move |renderer| {
        let mut app = Viewer::new(
            library,
            available,
            &start_movie,
            fps,
            font,
            &install,
            renderer,
        )?;
        app.localise_text = localise_text;
        app.print_controls();
        Ok(app)
    }) {
        eprintln!("error: {error}");
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            eprintln!("  caused by: {cause}");
            source = cause.source();
        }
        std::process::exit(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Move([f32; 2]),
    Click([f32; 2]),
}

struct Options {
    install: PathBuf,
    movie: Option<String>,
    width: u32,
    height: u32,
    fps: f32,
    font: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    mask_mode: Option<MaskMode>,
    atlas_srgb: bool,
    steps: Vec<Step>,
    dump_hits: bool,
    hit_mesh: bool,
    show_hit_regions: bool,
    verbose: bool,
    frames: u32,
    surfaces_as_stored: bool,
    anchor_top: bool,
    anchor_centred: bool,
    no_localise: bool,
    profile: bool,
    scale_mode: ScaleMode,
}

impl Options {
    fn usage() -> &'static str {
        "usage: mint-sage-apt-viewer --install <dir> [movie] [--size WxH] [--fps N] [--font <ttf>] \
         [--screenshot <out.png>] [--frames N] [--pointer X,Y ...] [--click X,Y ...] [--no-localise] \
         [--no-clip-masks] [--mask-mode off|box|stencil] [--atlas-srgb] [--dump-hits] [--hit-mesh] \
         [--show-hit-regions] [--verbose] [--surfaces-as-stored] [--anchor-top] [--profile] \
         [--ui-scale stretch|anchored]"
    }

    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut install = None;
        let mut movie = None;
        let mut width = 1024u32;
        let mut height = 768u32;
        let mut fps = DEFAULT_FPS;
        let mut font = None;
        let mut screenshot = None;
        let mut steps: Vec<Step> = Vec::new();
        let mut dump_hits = false;
        let mut hit_mesh = false;
        let mut show_hit_regions = false;
        let mut verbose = false;
        let mut frames = 0u32;
        let mut surfaces_as_stored = false;
        let mut mask_mode = None;
        let mut atlas_srgb = false;
        let mut anchor_top = false;
        let mut anchor_centred = false;
        let mut no_localise = false;
        let mut profile = false;
        let mut scale_mode = ScaleMode::default();
        let point = |name: &str, text: &str| -> Result<[f32; 2], String> {
            let (x, y) = text
                .split_once(',')
                .ok_or_else(|| format!("{name} wants X,Y, got {text}"))?;
            Ok([
                x.trim().parse().map_err(|_| format!("bad {name} x: {x}"))?,
                y.trim().parse().map_err(|_| format!("bad {name} y: {y}"))?,
            ])
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
            match arg.as_str() {
                "--install" => install = Some(PathBuf::from(value("--install")?)),
                "--size" => {
                    let text = value("--size")?;
                    let (w, h) = text
                        .split_once(['x', 'X'])
                        .ok_or_else(|| format!("--size wants WxH, got {text}"))?;
                    width = w.parse().map_err(|_| format!("bad width: {w}"))?;
                    height = h.parse().map_err(|_| format!("bad height: {h}"))?;
                }
                "--fps" => {
                    let text = value("--fps")?;
                    fps = text.parse().map_err(|_| format!("bad --fps: {text}"))?;
                }
                "--font" => font = Some(PathBuf::from(value("--font")?)),
                "--screenshot" => screenshot = Some(PathBuf::from(value("--screenshot")?)),
                "--pointer" => steps.push(Step::Move(point("--pointer", &value("--pointer")?)?)),
                "--click" => steps.push(Step::Click(point("--click", &value("--click")?)?)),
                "--frames" => {
                    let text = value("--frames")?;
                    frames = text.parse().map_err(|_| format!("bad --frames: {text}"))?;
                }
                "--surfaces-as-stored" => surfaces_as_stored = true,
                "--no-clip-masks" => mask_mode = Some(MaskMode::Off),
                "--mask-mode" => {
                    let text = value("--mask-mode")?;
                    mask_mode = Some(match text.as_str() {
                        "off" => MaskMode::Off,
                        "box" => MaskMode::BoundingBox,
                        "stencil" => MaskMode::Stencil,
                        other => return Err(format!("bad --mask-mode: {other} (off|box|stencil)")),
                    });
                }
                "--atlas-srgb" => atlas_srgb = true,
                "--dump-hits" => dump_hits = true,
                "--hit-mesh" => hit_mesh = true,
                "--show-hit-regions" => show_hit_regions = true,
                "--verbose" => verbose = true,
                "--anchor-top" => anchor_top = true,
                "--anchor-centred" | "--anchor-centered" => anchor_centred = true,
                "--no-localise" | "--no-localize" => no_localise = true,
                "--profile" => profile = true,
                "--ui-scale" => {
                    let text = value("--ui-scale")?;
                    scale_mode = ScaleMode::parse(&text)
                        .ok_or_else(|| format!("bad --ui-scale: {text} (stretch|anchored)"))?;
                }
                "--help" | "-h" => return Err(Self::usage().to_owned()),
                other if other.starts_with("--") => return Err(format!("unknown option {other}")),
                other => movie = Some(other.to_owned()),
            }
        }
        Ok(Self {
            install: install.ok_or("--install is required")?,
            movie,
            width,
            height,
            fps,
            font,
            screenshot,
            mask_mode,
            atlas_srgb,
            steps,
            dump_hits,
            hit_mesh,
            show_hit_regions,
            verbose,
            frames,
            surfaces_as_stored,
            anchor_top,
            anchor_centred,
            no_localise,
            profile,
            scale_mode,
        })
    }
}

fn run_screenshot(
    library: Arc<Library>,
    available: Vec<String>,
    movie_name: &str,
    font: Option<mint_ui::Font>,
    out_path: &Path,
    options: &Options,
) {
    let (width, height) = (options.width, options.height);
    let mut renderer = match Renderer::offscreen(width, height) {
        Ok(r) => r,
        Err(error) => {
            eprintln!("offscreen renderer: {error}");
            std::process::exit(1);
        }
    };
    let mut viewer = match Viewer::new(
        library,
        available,
        movie_name,
        DEFAULT_FPS,
        font,
        &options.install,
        &mut renderer,
    ) {
        Ok(v) => v,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };
    viewer.localise_text = !options.no_localise;
    viewer.surfaces = if options.surfaces_as_stored {
        SurfaceMode::AsStored
    } else {
        SurfaceMode::Placeholder
    };
    if let Some(mode) = options.mask_mode {
        viewer.mask_mode = mode;
    }
    viewer.atlas_srgb = options.atlas_srgb;
    viewer.set_hit_mode(if options.hit_mesh {
        HitMode::MeshBox
    } else {
        HitMode::Records
    });
    viewer.show_hit_regions = options.show_hit_regions;
    viewer.trace_scripts = options.verbose;
    viewer.text_anchor = if options.anchor_top {
        TextAnchor::Top
    } else if options.anchor_centred {
        TextAnchor::Centred
    } else {
        TextAnchor::default()
    };
    viewer.scale_mode = options.scale_mode;
    viewer.paused = true;
    if options.profile {
        viewer.profile = Some(Profile::default());
    }
    let play = |viewer: &mut Viewer, renderer: &mut Renderer| {
        for _ in 0..options.frames {
            viewer.step_tick();
            if let Err(error) = viewer.draw(renderer, 0.0) {
                eprintln!("render: {error}");
                std::process::exit(1);
            }
        }
    };
    play(&mut viewer, &mut renderer);
    for step in &options.steps {
        let (Step::Move(position) | Step::Click(position)) = *step;
        viewer.pointer_to(position);
        play(&mut viewer, &mut renderer);
        if matches!(step, Step::Move(_)) {
            continue;
        }
        for pressed in [true, false] {
            println!(
                "[click] {} at stage ({:.0},{:.0})",
                if pressed { "press" } else { "release" },
                position[0],
                position[1]
            );
            viewer.pointer_press(pressed);
            play(&mut viewer, &mut renderer);
        }
    }
    let mut settled_after_previous = false;
    for pass in 0..SCREENSHOT_WARMUP_FRAMES {
        let is_last = settled_after_previous || pass == SCREENSHOT_WARMUP_FRAMES - 1;
        viewer.report_stats_next_draw = is_last;
        if let Err(error) = viewer.draw(&mut renderer, 0.0) {
            eprintln!("render: {error}");
            std::process::exit(1);
        }
        if is_last {
            if !settled_after_previous && !viewer.texts.is_settled() {
                eprintln!(
                    "viewer: text still not settled after {SCREENSHOT_WARMUP_FRAMES} pass(es) \
                     - the screenshot may show incomplete text"
                );
            }
            break;
        }
        settled_after_previous = viewer.texts.is_settled();
    }
    if options.dump_hits {
        viewer.dump_hit_areas();
    }
    if let Some(p) = viewer.profile {
        p.print(&viewer.movie_name);
    }
    let (w, h) = renderer.size();
    let rgba = match renderer.read_pixels() {
        Ok(pixels) => pixels,
        Err(error) => {
            eprintln!("read_pixels: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = write_png(out_path, w, h, &rgba) {
        eprintln!("{}: {error}", out_path.display());
        std::process::exit(1);
    }
    println!("wrote {w}x{h} screenshot to {}", out_path.display());
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let stride = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(4))
        .ok_or_else(|| format!("{width}: implausible width"))?;
    let expected = usize::try_from(height)
        .ok()
        .and_then(|h| h.checked_mul(stride))
        .ok_or_else(|| format!("{height}: implausible height"))?;
    if width == 0 || height == 0 || rgba.len() != expected {
        return Err(format!(
            "screenshot size mismatch: {width}x{height} wants {expected} bytes, got {}",
            rgba.len()
        ));
    }
    let mut raw = Vec::with_capacity(expected.saturating_add(expected / stride.max(1)));
    for row in rgba.chunks_exact(stride) {
        raw.push(0u8);
        raw.extend_from_slice(row);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(&mut out, b"IHDR", &ihdr)?;
    push_chunk(&mut out, b"IDAT", &zlib_stored(&raw))?;
    push_chunk(&mut out, b"IEND", &[])?;
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let len = u32::try_from(data.len()).map_err(|_| "chunk too large".to_owned())?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(data.len().saturating_add(4));
    crc_input.extend_from_slice(tag);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    Ok(())
}

fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    if raw.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    } else {
        let mut chunks = raw.chunks(0xFFFF).peekable();
        while let Some(chunk) = chunks.next() {
            let last = chunks.peek().is_none();
            out.push(u8::from(last));
            let len = u16::try_from(chunk.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod option_tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        Options::parse(args.iter().map(|s| (*s).to_owned())).expect("should parse")
    }

    #[test]
    fn install_and_movie_parse() {
        let options = parse(&["--install", "C:/rotwk", "mainmenu"]);
        assert_eq!(options.install, PathBuf::from("C:/rotwk"));
        assert_eq!(options.movie.as_deref(), Some("mainmenu"));
        assert_eq!(options.fps, DEFAULT_FPS);
    }

    #[test]
    fn size_and_fps_and_font_parse() {
        let options = parse(&[
            "--install",
            "x",
            "--size",
            "800x600",
            "--fps",
            "24",
            "--font",
            "c:/f.ttf",
        ]);
        assert_eq!((options.width, options.height), (800, 600));
        assert_eq!(options.fps, 24.0);
        assert_eq!(options.font, Some(PathBuf::from("c:/f.ttf")));
    }

    #[test]
    fn steps_parse_in_order_and_reject_a_bad_coordinate() {
        let options = parse(&[
            "--install",
            "x",
            "--click",
            "510, 710",
            "--pointer",
            "1,2",
            "--click",
            "3,4",
        ]);
        assert_eq!(
            options.steps,
            vec![
                Step::Click([510.0, 710.0]),
                Step::Move([1.0, 2.0]),
                Step::Click([3.0, 4.0])
            ]
        );
        for bad in ["510", "a,1", "1,b"] {
            assert!(Options::parse(
                ["--install", "x", "--click", bad]
                    .iter()
                    .map(|s| (*s).to_owned())
            )
            .is_err());
        }
    }

    #[test]
    fn missing_install_is_an_error() {
        assert!(Options::parse(["mainmenu".to_owned()].into_iter()).is_err());
    }

    #[test]
    fn bad_size_is_an_error_not_a_panic() {
        assert!(Options::parse(
            [
                "--install".to_owned(),
                "x".to_owned(),
                "--size".to_owned(),
                "nope".to_owned()
            ]
            .into_iter()
        )
        .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceMode {
    Placeholder,
    AsStored,
}

impl SurfaceMode {
    fn toggled(self) -> Self {
        match self {
            Self::Placeholder => Self::AsStored,
            Self::AsStored => Self::Placeholder,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Placeholder => "placeholder (outline + _type)",
            Self::AsStored => "as stored in the file",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SeenStats {
    commands: usize,
    shapes: u32,
    text: u32,
    buttons: usize,
    surfaces: u32,
    unresolved: u32,
}

#[derive(Clone, Copy, Default)]
struct FrameStats {
    shapes: u32,
    runs: u32,
    text: u32,
    text_empty: u32,
    text_font_unresolved: u32,
    text_localised: u32,
    text_key_missing: u32,
    surfaces: u32,
    unresolved: u32,
    masks: u32,
    compound_masks: u32,
    culled: u32,
}

#[derive(Clone, Copy, Default)]
struct Profile {
    frames: u32,
    ticks: u32,
    tick: Duration,
    render_list: Duration,
    plan: Duration,
    text: Duration,
    build: Duration,
    submit: Duration,
}

impl Profile {
    fn print(&self, movie: &str) {
        let per = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(self.frames.max(1));
        let tick = self.tick.as_secs_f64() * 1000.0 / f64::from(self.ticks.max(1));
        let total = self.tick + self.render_list + self.plan + self.text + self.build + self.submit;
        println!(
            "[profile] {movie}: {} draws, {} ticks; ms per draw: tick {tick:.3} (per tick), render list {:.3}, \
             meshes+plan {:.3}, text {:.3}, build {:.3}, submit {:.3}; total {:.3}",
            self.frames,
            self.ticks,
            per(self.render_list),
            per(self.plan),
            per(self.text),
            per(self.build),
            per(self.submit),
            per(total),
        );
    }
}

enum Op {
    Run {
        mesh: usize,
        run: usize,
        world: Affine,
        color: [f32; 4],
        add: [f32; 4],
        clip: [f32; 4],
        texture: Option<(TextureHandle, [[f32; 2]; 2], [f32; 2])>,
    },
    Lines {
        index: usize,
        world: Affine,
        color: [f32; 4],
        add: [f32; 4],
        clip: [f32; 4],
    },
    Quads(std::ops::Range<usize>),
    Push(usize),
    Pop(usize),
}

struct Viewer {
    library: Arc<Library>,
    assets: MovieLibrary,
    available: Vec<String>,
    movie_name: String,
    player: Player,
    list: RenderList,
    plan: Plan,
    meshes: MeshCache,
    texts: TextCache,
    hit_areas: Vec<HitArea>,
    frame_accum: f32,
    paused: bool,
    fps: f32,
    window_size: (f32, f32),
    pointer_stage: [f32; 2],
    show_background: bool,
    apply_color_transform: bool,
    mask_mode: MaskMode,
    atlas_srgb: bool,
    hit_mode: HitMode,
    show_hit_regions: bool,
    movie_page: usize,
    seen: HashMap<String, SeenStats>,
    string_table: StringTable,
    localise_text: bool,
    texture_cache: HashMap<(String, String), Option<TextureHandle>>,
    text_anchor: TextAnchor,
    surfaces: SurfaceMode,
    trace_scripts: bool,
    last_status: String,
    last_status_at: Instant,
    last_tick: Instant,
    font: Option<mint_ui::Font>,
    atlas: Option<GlyphAtlas>,
    hud_texture: Option<TextureHandle>,
    hud_cache: Option<(String, Vec<UiQuad>)>,
    report_stats_next_draw: bool,
    movie_fonts: HashMap<String, mint_ui::Font>,
    text_atlas: Option<GlyphAtlas>,
    text_texture: Option<TextureHandle>,
    atlas_busy: bool,
    atlas_near_full: bool,
    stats: FrameStats,
    profile: Option<Profile>,
    scale_mode: ScaleMode,
    anchor_cache: AnchorCache,
}

impl Viewer {
    fn new(
        library: Arc<Library>,
        available: Vec<String>,
        movie_name: &str,
        fps: f32,
        font: Option<mint_ui::Font>,
        install: &Path,
        renderer: &mut Renderer,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (w, h) = renderer.size();
        let mut white = || {
            TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
                .and_then(|placeholder| renderer.upload_texture(&placeholder).ok())
        };
        let atlas = if font.is_some() {
            GlyphAtlas::new(GlyphAtlasConfig::default_config()).ok()
        } else {
            None
        };
        let hud_texture = if atlas.is_some() { white() } else { None };
        let (string_table, str_files, str_failures) = load_string_table(install);
        println!(
            "localisation: {} labels from {str_files} .str file(s) ({} duplicate label(s) dropped, \
             {str_failures} file(s) failed to parse)",
            string_table.len(),
            string_table.duplicates()
        );
        let movie_fonts =
            mint_sage_apt_viewer::load_bundled_fonts(std::slice::from_ref(&install.to_path_buf()));
        let mut names: Vec<&String> = movie_fonts.keys().collect();
        names.sort();
        println!(
            "bundled fonts: {}",
            if names.is_empty() {
                "none (EditText draws as tofu)".to_owned()
            } else {
                names
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        let text_atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).ok();
        let text_texture = if text_atlas.is_some() { white() } else { None };
        let id = library
            .movie_id(movie_name)
            .ok_or_else(|| format!("no movie {movie_name}"))?;
        let mut player = Player::new(Arc::clone(&library), id, Limits::default())?;
        player.set_trace_log(true);
        let mut viewer = Self {
            library,
            assets: MovieLibrary::new(install),
            available,
            movie_name: movie_name.to_owned(),
            player,
            list: RenderList::default(),
            plan: Plan::default(),
            meshes: MeshCache::default(),
            texts: TextCache::default(),
            hit_areas: Vec::new(),
            frame_accum: 0.0,
            paused: false,
            fps,
            window_size: (w as f32, h as f32),
            pointer_stage: [0.0, 0.0],
            show_background: false,
            apply_color_transform: true,
            mask_mode: MaskMode::default(),
            atlas_srgb: false,
            hit_mode: HitMode::Records,
            show_hit_regions: false,
            movie_page: 0,
            seen: HashMap::new(),
            string_table,
            localise_text: true,
            texture_cache: HashMap::new(),
            text_anchor: TextAnchor::default(),
            surfaces: SurfaceMode::Placeholder,
            trace_scripts: true,
            last_status: String::new(),
            last_status_at: Instant::now() - Duration::from_secs(1),
            last_tick: Instant::now(),
            font,
            atlas,
            hud_texture,
            hud_cache: None,
            report_stats_next_draw: true,
            movie_fonts,
            text_atlas,
            text_texture,
            atlas_busy: false,
            atlas_near_full: false,
            stats: FrameStats::default(),
            profile: None,
            scale_mode: ScaleMode::default(),
            anchor_cache: AnchorCache::new(),
        };
        viewer.announce_movie();
        viewer.drain_messages();
        Ok(viewer)
    }

    fn print_controls(&self) {
        println!("\ncontrols:");
        println!("  space       play/pause");
        println!("  , / .       step the root timeline one frame back/forward (pauses)");
        println!("  arrow keys  scrub the root timeline 10 frames (pauses)");
        println!(
            "  [ / ]       previous/next movie ({} available)",
            self.available.len()
        );
        println!("  m / n / p   list this page of movies / next / previous page");
        println!("  0-9         open a movie from the printed page");
        println!("  r           restart the movie");
        println!(
            "  b           BackgroundColor paint (currently {})",
            on_off(self.show_background)
        );
        println!(
            "  v           text anchor (currently {})",
            self.text_anchor.label()
        );
        println!(
            "  e           engine-owned surfaces (currently {})",
            self.surfaces.label()
        );
        println!(
            "  t           scripts' trace() on the console (currently {})",
            on_off(self.trace_scripts)
        );
        println!(
            "  c           colour transforms (currently {})",
            on_off(self.apply_color_transform)
        );
        println!(
            "  u           atlas colour space (currently {})",
            if self.atlas_srgb {
                "Srgb (pre-fix)"
            } else {
                "Unorm (fixed)"
            }
        );
        println!(
            "  l           localisation (currently {})",
            on_off(self.localise_text)
        );
        println!(
            "  k           ClipDepth masking (currently {})",
            self.mask_mode.label()
        );
        println!(
            "  h           click areas from (currently {})",
            hit_label(self.hit_mode)
        );
        println!(
            "  j           draw click areas (currently {})",
            on_off(self.show_hit_regions)
        );
        println!("  g           (no longer applies: the player's VM routes every clip call)");
        println!(
            "  y           window fit: stretch/anchored (currently {})",
            self.scale_mode.as_str()
        );
        println!();
    }

    fn announce_movie(&mut self) {
        let id = self.library.movie_id(&self.movie_name);
        if let Some(m) = id.and_then(|id| self.library.movie(id)) {
            println!(
                "{}: {} characters, {} frames, {}x{}, {} imports",
                m.movie.name,
                m.movie.characters.len(),
                m.movie.record.frame_count,
                m.movie.record.screen_width,
                m.movie.record.screen_height,
                m.movie.imports.len()
            );
        }
        if let Some(index) = self.available.iter().position(|a| a == &self.movie_name) {
            println!(
                "  movie {} of {} - `m` lists them, 0-9 opens one",
                index.saturating_add(1),
                self.available.len()
            );
            self.movie_page = index.saturating_div(10);
        }
    }

    fn switch_movie(&mut self, name: &str) {
        let Some(id) = self.library.movie_id(name) else {
            eprintln!("{name}: not in this install");
            return;
        };
        match Player::new(Arc::clone(&self.library), id, Limits::default()) {
            Ok(mut player) => {
                player.set_trace_log(true);
                player.set_hit_mode(self.hit_mode);
                self.player = player;
                self.movie_name = name.to_owned();
                self.frame_accum = 0.0;
                self.paused = false;
                self.report_stats_next_draw = true;
                self.announce_movie();
                self.drain_messages();
                self.force_status();
            }
            Err(e) => eprintln!("{name}: {e}"),
        }
    }

    fn set_hit_mode(&mut self, mode: HitMode) {
        self.hit_mode = mode;
        self.player.set_hit_mode(mode);
    }

    fn stage_size(&self) -> (f32, f32) {
        self.library
            .movie_id(&self.movie_name)
            .and_then(|id| self.library.movie(id))
            .map_or((1024.0, 768.0), |m| {
                (
                    m.movie.record.screen_width as f32,
                    m.movie.record.screen_height as f32,
                )
            })
    }

    fn letterbox(&self) -> Letterbox {
        let (sw, sh) = self.stage_size();
        Letterbox::stretch(sw, sh, self.window_size.0, self.window_size.1)
    }

    fn anchors(&mut self) -> HashMap<InstanceId, Letterbox> {
        let (sw, sh) = self.stage_size();
        self.anchor_cache.letterboxes(
            &self.player,
            sw,
            sh,
            self.window_size.0,
            self.window_size.1,
            mint_sage_apt_viewer::ANCHOR_EDGE_SLACK,
        )
    }

    fn resolve_stage_point(&mut self, window_pos: [f32; 2]) -> [f32; 2] {
        let (sw, sh) = self.stage_size();
        self.anchor_cache.resolve_point(
            &self.player,
            self.scale_mode,
            sw,
            sh,
            self.window_size.0,
            self.window_size.1,
            mint_sage_apt_viewer::ANCHOR_EDGE_SLACK,
            window_pos,
        )
    }

    fn drain_messages(&mut self) {
        for o in self.player.take_outward() {
            match o {
                Outward::FsCommand { command, args } => {
                    println!("[outward] FSCommand {command} ({args})");
                }
                Outward::GetUrl { url, target } => println!("[outward] getURL {url} ({target})"),
                Outward::GameCode { name, args } => {
                    println!(
                        "[outward] {name}({}) - no script defines it",
                        args.join(", ")
                    );
                }
            }
        }
        let traces = self.player.take_traces();
        if self.trace_scripts {
            for t in traces {
                println!("  [trace] {t}");
            }
        }
    }

    fn step_tick(&mut self) {
        let started = Instant::now();
        if let Err(e) = self.player.tick() {
            eprintln!("tick: {e}");
        }
        if let Some(p) = self.profile.as_mut() {
            p.tick += started.elapsed();
            p.ticks = p.ticks.saturating_add(1);
        }
        self.drain_messages();
    }

    fn pointer_to(&mut self, p: [f32; 2]) {
        self.pointer_stage = p;
        if let Err(e) = self.player.pointer_move(p[0], p[1]) {
            eprintln!("pointer: {e}");
        }
        self.drain_messages();
    }

    fn pointer_press(&mut self, down: bool) {
        if let Err(e) = self.player.pointer_button(down) {
            eprintln!("pointer: {e}");
        }
        self.drain_messages();
    }

    fn advance_playback(&mut self, elapsed: Duration) {
        if self.paused || self.fps <= 0.0 {
            return;
        }
        self.frame_accum += elapsed.as_secs_f32() * self.fps;
        let owed = self.frame_accum.floor().clamp(0.0, 8.0);
        self.frame_accum -= owed;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..owed as u32 {
            self.step_tick();
        }
    }

    fn root_frame(&self) -> (u16, u16, bool) {
        let root = self.player.root();
        self.player
            .instance(root)
            .and_then(|i| i.clip())
            .map_or((0, 0, false), |c| {
                let total = self
                    .library
                    .timeline(c.timeline)
                    .map_or(0, |t| t.frame_count());
                (c.current_frame, total, c.playing)
            })
    }

    fn scrub(&mut self, delta: i32) {
        self.paused = true;
        let (current, total, _) = self.root_frame();
        if total == 0 {
            return;
        }
        let target = (i32::from(current) + delta).clamp(1, i32::from(total));
        let root = self.player.root();
        if let Err(e) = self
            .player
            .goto_frame(root, u16::try_from(target).unwrap_or(1), true)
        {
            eprintln!("goto: {e}");
        }
        let _ = self.player.run_actions();
        self.drain_messages();
        self.force_status();
    }

    fn status_line(&self) -> String {
        let (frame, total, playing) = self.root_frame();
        let hovered = self
            .player
            .hovered()
            .map_or_else(|| "-".to_owned(), |h| self.player.target_path(h));
        let heap = self.player.heap_stats();
        format!(
            "movie={} frame={frame}/{total} paused={} playing={playing} pointer=({:.0},{:.0}) \
             hovered={hovered} instances={} objects={} background={} cxform={} masks={} atlas={} \
             hits={}{} anchor={} surfaces={}",
            self.movie_name,
            self.paused,
            self.pointer_stage[0],
            self.pointer_stage[1],
            self.player.live_instances(),
            heap.objects,
            on_off(self.show_background),
            on_off(self.apply_color_transform),
            self.mask_mode.label(),
            if self.atlas_srgb { "srgb" } else { "unorm" },
            hit_label(self.hit_mode),
            if self.show_hit_regions {
                " (drawn)"
            } else {
                ""
            },
            self.text_anchor.label(),
            self.surfaces.label(),
        )
    }

    fn force_status(&mut self) {
        let line = self.status_line();
        println!("[state] {line}");
        self.last_status = line;
        self.last_status_at = Instant::now();
    }

    fn maybe_print_status(&mut self) {
        if self.last_status_at.elapsed() <= Duration::from_millis(500) {
            return;
        }
        let line = self.status_line();
        if line != self.last_status && self.last_status_at.elapsed() > Duration::from_millis(500) {
            println!("[state] {line}");
            self.last_status = line;
            self.last_status_at = Instant::now();
        }
    }

    fn dump_hit_areas(&mut self) {
        self.player.hit_areas(&mut self.hit_areas);
        let hovered = self.player.hovered();
        println!(
            "[hits] {} click area(s), topmost first, pointer=({:.1},{:.1}), source={}",
            self.hit_areas.len(),
            self.pointer_stage[0],
            self.pointer_stage[1],
            hit_label(self.hit_mode)
        );
        for (index, area) in self.hit_areas.iter().rev().enumerate() {
            let xs = area.corners.map(|c| c[0]);
            let ys = area.corners.map(|c| c[1]);
            let (x0, x1) = (
                xs.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
            );
            let (y0, y1) = (
                ys.iter().copied().fold(f32::MAX, f32::min),
                ys.iter().copied().fold(f32::MIN, f32::max),
            );
            println!(
                "  {index:3} {} x=[{x0:.1},{x1:.1}] y=[{y0:.1},{y1:.1}] {}",
                if hovered == Some(area.button) {
                    "HOVERED"
                } else {
                    "       "
                },
                self.player.target_path(area.button)
            );
        }
    }

    fn print_movie_page(&self) {
        let pages = self.available.len().div_ceil(10).max(1);
        let page = self.movie_page.min(pages.saturating_sub(1));
        let start = page.saturating_mul(10);
        println!(
            "[movies] page {}/{pages} - press 0-9 to open, n/p for another page, m to reprint",
            page.saturating_add(1)
        );
        for digit in 0..10usize {
            let Some(name) = self.available.get(start.saturating_add(digit)) else {
                break;
            };
            let here = if name == &self.movie_name { "*" } else { " " };
            match self.seen.get(name) {
                Some(s) => println!(
                    "  {digit} {here} {name:<34} {} commands, {} shapes, {} text, {} click areas, \
                     {} engine surfaces, {} unresolved",
                    s.commands, s.shapes, s.text, s.buttons, s.surfaces, s.unresolved
                ),
                None => println!("  {digit} {here} {name:<34} (not opened yet)"),
            }
        }
    }

    fn open_movie_digit(&mut self, digit: usize) {
        let pages = self.available.len().div_ceil(10).max(1);
        let page = self.movie_page.min(pages.saturating_sub(1));
        let index = page.saturating_mul(10).saturating_add(digit);
        match self.available.get(index).cloned() {
            Some(name) => self.switch_movie(&name),
            None => println!("[movies] no movie at slot {digit} on this page"),
        }
    }

    fn turn_movie_page(&mut self, delta: i32) {
        let pages = i32::try_from(self.available.len().div_ceil(10).max(1)).unwrap_or(1);
        let current = i32::try_from(self.movie_page).unwrap_or(0).min(pages - 1);
        self.movie_page = usize::try_from((current + delta).rem_euclid(pages)).unwrap_or(0);
        self.print_movie_page();
    }

    fn cycle_movie(&mut self, delta: i32) {
        let len = i32::try_from(self.available.len()).unwrap_or(0);
        if len == 0 {
            return;
        }
        let current = self
            .available
            .iter()
            .position(|a| a == &self.movie_name)
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(0);
        let next = usize::try_from((current + delta).rem_euclid(len)).unwrap_or(0);
        if let Some(name) = self.available.get(next).cloned() {
            self.switch_movie(&name);
        }
    }

    fn mesh_for(&mut self, character: CharacterRef) -> Option<Arc<ShapeMesh>> {
        let library = &self.library;
        let assets = &mut self.assets;
        self.meshes.get_or_build(character, || {
            let Some(Character::Shape { geometry_id, .. }) = library.character(character) else {
                return None;
            };
            let name = library.movie(character.movie)?.movie.name.clone();
            assets.ensure_loaded(&name).ok()?;
            let text = assets
                .assets(&name)?
                .ru
                .get(&ru_base_name(&name, *geometry_id))?;
            let ru = parse_ru(text).ok()?;
            let image_of = |image_id: u32| -> Option<(String, u32)> {
                let image = library.resolve(character.movie, image_id)?;
                match library.character(image) {
                    Some(Character::Image { texture }) => {
                        Some((library.movie(image.movie)?.movie.name.clone(), *texture))
                    }
                    _ => None,
                }
            };
            let mut owners = Vec::new();
            let _ = ShapeMesh::from_ru(&ru, |id| {
                owners.extend(image_of(id).map(|(m, _)| m));
                None
            });
            for owner in owners {
                let _ = assets.ensure_loaded(&owner);
            }
            let assets = &*assets;
            Some(ShapeMesh::from_ru(&ru, |id| {
                let (owner, texture) = image_of(id)?;
                let entry = assets
                    .assets(&owner)?
                    .dat_entries
                    .iter()
                    .find(|e| e.id == texture)?;
                let path = match entry.location {
                    TextureLocation::Shared { page } => {
                        mint_sage_apt::shared_atlas_path(&owner, page)
                    }
                    TextureLocation::Standalone { .. } => standalone_texture_path(&owner, texture),
                };
                let base = path.rsplit('/').next().unwrap_or(&path).to_owned();
                Some((owner, base))
            }))
        })
    }

    fn texture_for(
        &mut self,
        key: &(String, String),
        renderer: &mut Renderer,
    ) -> Option<TextureHandle> {
        if let Some(found) = self.texture_cache.get(key) {
            return *found;
        }
        let handle = (|| {
            self.assets.ensure_loaded(&key.0).ok()?;
            let bytes = self.assets.assets(&key.0)?.tga.get(&key.1)?;
            let image = mint_image::tga::decode(bytes).ok()?;
            let mut data = TextureData::rgba8(image.width(), image.height(), image.into_pixels())?;
            if self.atlas_srgb {
                data.format = mint_render::PixelFormat::Rgba8Srgb;
            }
            renderer.upload_texture(&data).ok()
        })();
        self.texture_cache.insert(key.clone(), handle);
        handle
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn hit_label(mode: HitMode) -> &'static str {
    match mode {
        HitMode::Records => "hit records x their own matrix",
        HitMode::MeshBox => "hit mesh box only (pre-D2j)",
    }
}

fn font_id_for(fonts: &HashMap<String, mint_ui::Font>, file: Option<&str>) -> u32 {
    let Some(file) = file else { return 0 };
    let mut names: Vec<&String> = fonts.keys().collect();
    names.sort();
    names
        .iter()
        .position(|n| n.as_str() == file)
        .and_then(|i| u32::try_from(i).ok())
        .map_or(0, |i| i.saturating_add(1))
}

fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

fn rect_lines(r: [f32; 4]) -> Vec<[f32; 2]> {
    let c = [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
    let mut lines = Vec::with_capacity(8);
    for (start, end) in c.iter().zip(c.iter().cycle().skip(1)) {
        lines.push(*start);
        lines.push(*end);
    }
    lines
}

impl App for Viewer {
    fn input(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::Resized { width, height } => {
                self.window_size = (width as f32, height as f32);
                self.texts.clear();
            }
            InputEvent::PointerMoved { x, y } => {
                let p = self.resolve_stage_point([x, y]);
                self.pointer_to(p);
            }
            InputEvent::PointerButton { button, pressed } => {
                if button == mint_app::PointerButton::Primary {
                    self.pointer_press(pressed);
                }
            }
            InputEvent::Key { key, pressed: true } => match key {
                Key::Space => {
                    self.paused = !self.paused;
                    self.force_status();
                }
                Key::Character(',') => self.scrub(-1),
                Key::Character('.') => self.scrub(1),
                Key::ArrowLeft => self.scrub(-10),
                Key::ArrowRight => self.scrub(10),
                Key::Character('[') => self.cycle_movie(-1),
                Key::Character(']') => self.cycle_movie(1),
                Key::Character('m') => self.print_movie_page(),
                Key::Character('n') => self.turn_movie_page(1),
                Key::Character('p') => self.turn_movie_page(-1),
                Key::Character(digit @ '0'..='9') => {
                    let slot = usize::from(digit as u8).saturating_sub(usize::from(b'0'));
                    self.open_movie_digit(slot);
                }
                Key::Character('r') => {
                    let name = self.movie_name.clone();
                    println!("restart: {name} from its first frame");
                    self.switch_movie(&name);
                }
                Key::Character('b') => {
                    self.show_background = !self.show_background;
                    self.force_status();
                }
                Key::Character('c') => {
                    self.apply_color_transform = !self.apply_color_transform;
                    self.force_status();
                }
                Key::Character('k') => {
                    self.mask_mode = self.mask_mode.cycled();
                    println!("mask mode = {}", self.mask_mode.label());
                    self.force_status();
                }
                Key::Character('u') => {
                    self.atlas_srgb = !self.atlas_srgb;
                    self.texture_cache.clear();
                    println!(
                        "atlas colour space = {}",
                        if self.atlas_srgb {
                            "Srgb (pre-fix)"
                        } else {
                            "Unorm (fixed)"
                        }
                    );
                    self.force_status();
                }
                Key::Character('h') => {
                    let next = match self.hit_mode {
                        HitMode::Records => HitMode::MeshBox,
                        HitMode::MeshBox => HitMode::Records,
                    };
                    self.set_hit_mode(next);
                    println!("click areas = {}", hit_label(next));
                    self.force_status();
                }
                Key::Character('j') => {
                    self.show_hit_regions = !self.show_hit_regions;
                    self.force_status();
                }
                Key::Character('g') => {
                    println!(
                        "g no longer applies: every Child.gotoAndPlay/gotoAndStop/play/stop runs in \
                         the player's VM against the live tree (ADR 0011 P2)"
                    );
                }
                Key::Character('v') => {
                    self.text_anchor = self.text_anchor.toggled();
                    println!("text anchor = {}", self.text_anchor.label());
                    self.force_status();
                }
                Key::Character('e') => {
                    self.surfaces = self.surfaces.toggled();
                    println!("engine surfaces = {}", self.surfaces.label());
                    self.report_stats_next_draw = true;
                    self.force_status();
                }
                Key::Character('t') => {
                    self.trace_scripts = !self.trace_scripts;
                    println!("script trace() = {}", on_off(self.trace_scripts));
                }
                Key::Character('l') => {
                    self.localise_text = !self.localise_text;
                    self.report_stats_next_draw = true;
                    println!("localise = {}", on_off(self.localise_text));
                    self.force_status();
                }
                Key::Character('y') => {
                    self.scale_mode = self.scale_mode.toggled();
                    println!("--ui-scale {}", self.scale_mode.as_str());
                    self.force_status();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick);
        self.last_tick = now;
        self.advance_playback(elapsed);
        self.maybe_print_status();
        self.draw_frame(renderer)
    }
}

impl Viewer {
    #[allow(clippy::too_many_lines)]
    fn draw_frame(&mut self, renderer: &mut Renderer) -> mint_render::Result<()> {
        let mut clock = Instant::now();
        let mut lap = |slot: fn(&mut Profile) -> &mut Duration, profile: &mut Option<Profile>| {
            let now = Instant::now();
            if let Some(p) = profile.as_mut() {
                *slot(p) += now.duration_since(clock);
            }
            clock = now;
        };
        let stretch = self.letterbox();
        let anchors = if self.scale_mode == ScaleMode::Anchored {
            self.anchors()
        } else {
            HashMap::new()
        };
        self.player.render_with(
            &mut self.list,
            RenderOptions {
                surfaces: self.surfaces == SurfaceMode::Placeholder,
            },
        );
        self.stats = FrameStats::default();
        self.texts.begin_frame();
        self.atlas_busy = false;
        if let Some(atlas) = self.text_atlas.as_mut() {
            atlas.begin_frame();
        }
        if self.atlas_near_full {
            self.texts.clear();
        }

        lap(|p| &mut p.render_list, &mut self.profile);
        let shape_chars: Vec<CharacterRef> = self
            .list
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Shape { character, .. } => Some(*character),
                _ => None,
            })
            .collect();
        let mut frame_meshes: HashMap<CharacterRef, Arc<ShapeMesh>> = HashMap::new();
        for c in shape_chars {
            if frame_meshes.contains_key(&c) {
                continue;
            }
            match self.mesh_for(c) {
                Some(mesh) => {
                    frame_meshes.insert(c, mesh);
                }
                None => self.stats.unresolved = self.stats.unresolved.saturating_add(1),
            }
        }
        {
            let library = &self.library;
            let player = &self.player;
            let scale_mode = self.scale_mode;
            let letterbox_of = |instance: Option<InstanceId>| match (scale_mode, instance) {
                (ScaleMode::Stretch, _) | (ScaleMode::Anchored, None) => stretch,
                (ScaleMode::Anchored, Some(id)) => {
                    let top = mint_sage_apt_viewer::top_level_ancestor(player, id);
                    anchors.get(&top).copied().unwrap_or(stretch)
                }
            };
            plan(
                &self.list.commands,
                &letterbox_of,
                self.mask_mode,
                |command, world, out| match command {
                    RenderCommand::Shape { character, .. } => {
                        if let Some(mesh) = frame_meshes.get(character) {
                            out.extend(mesh.fill.iter().map(|p| apply(world, *p)));
                        }
                    }
                    RenderCommand::EditText { character, .. }
                    | RenderCommand::StaticText { character, .. } => {
                        if let Some(b) = library.character_bounds(*character, 0) {
                            out.extend(rect_triangles(b).iter().map(|p| apply(world, *p)));
                        }
                    }
                    RenderCommand::Surface { instance, .. } => {
                        if let Some(b) = player.local_bounds(*instance) {
                            out.extend(rect_triangles(b).iter().map(|p| apply(world, *p)));
                        }
                    }
                    _ => {}
                },
                &mut self.plan,
            );
        }
        self.stats.masks = self.plan.mask_count;
        self.stats.compound_masks = self.plan.compound_masks.saturating_add(
            u32::try_from(
                self.plan
                    .single_pieces
                    .iter()
                    .filter(|i| {
                        self.player
                            .instance(**i)
                            .is_some_and(|i| i.clip_depth() == 0)
                    })
                    .count(),
            )
            .unwrap_or(u32::MAX),
        );

        lap(|p| &mut p.plan, &mut self.profile);
        let mut text_time = Duration::ZERO;
        let mut ops: Vec<Op> = Vec::with_capacity(self.plan.items.len());
        let mut meshes: Vec<Arc<ShapeMesh>> = Vec::new();
        let mut mesh_index: HashMap<CharacterRef, usize> = HashMap::new();
        let mut lines: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut quads: Vec<UiQuad> = Vec::new();
        let mut atlas_dirty = false;
        let items = std::mem::take(&mut self.plan.items);
        for item in &items {
            match *item {
                Item::Shape {
                    character,
                    world,
                    color,
                    clip,
                } => {
                    let Some(mesh) = frame_meshes.get(&character).cloned() else {
                        continue;
                    };
                    let index = *mesh_index.entry(character).or_insert_with(|| {
                        meshes.push(Arc::clone(&mesh));
                        meshes.len() - 1
                    });
                    let cx = if self.apply_color_transform {
                        color
                    } else {
                        ColorTransform::IDENTITY
                    };
                    let size = self.window_size;
                    if !mesh
                        .bounds
                        .is_some_and(|b| on_screen(&world, b, clip, size))
                    {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        continue;
                    }
                    self.stats.shapes = self.stats.shapes.saturating_add(1);
                    for (run_index, run) in mesh.runs.iter().enumerate() {
                        let texture = match &run.texture {
                            Some(t) => self
                                .texture_for(&t.key, renderer)
                                .map(|h| (h, t.pixel_transform, t.pixel_translate)),
                            None => None,
                        };
                        if run.texture.is_some() && texture.is_none() {
                            self.stats.unresolved = self.stats.unresolved.saturating_add(1);
                        }
                        let run_color = multiply(run.color, cx.mult);
                        if !can_show(run_color, cx.add) {
                            self.stats.culled = self.stats.culled.saturating_add(1);
                            continue;
                        }
                        self.stats.runs = self.stats.runs.saturating_add(1);
                        ops.push(Op::Run {
                            mesh: index,
                            run: run_index,
                            world,
                            color: run_color,
                            add: cx.add,
                            clip,
                            texture,
                        });
                    }
                }
                Item::Text {
                    instance,
                    character,
                    world,
                    color,
                    clip,
                } => {
                    let cx = if self.apply_color_transform {
                        color
                    } else {
                        ColorTransform::IDENTITY
                    };
                    if !can_show(cx.mult, cx.add) {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        continue;
                    }
                    let start = quads.len();
                    let started = Instant::now();
                    if self.layout_field(
                        instance,
                        character,
                        &world,
                        cx,
                        clip,
                        &mut quads,
                        &mut atlas_dirty,
                    ) {
                        ops.push(Op::Quads(start..quads.len()));
                    }
                    text_time += started.elapsed();
                }
                Item::Surface {
                    instance,
                    bounds,
                    world,
                    color,
                    clip,
                } => {
                    if !on_screen(&world, bounds, clip, self.window_size) {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        continue;
                    }
                    self.stats.surfaces = self.stats.surfaces.saturating_add(1);
                    let kind = self
                        .player
                        .own_string(instance, "_type")
                        .map(|k| k.to_string())
                        .unwrap_or_default();
                    lines.push(rect_lines(bounds));
                    ops.push(Op::Lines {
                        index: lines.len() - 1,
                        world,
                        color: [0.45, 0.5, 0.6, 1.0],
                        add: [0.0; 4],
                        clip,
                    });
                    let alpha = if self.apply_color_transform {
                        color.mult[3].clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let corner = apply(&world, [bounds[0], bounds[1]]);
                    if self.report_stats_next_draw {
                        let far = apply(&world, [bounds[2], bounds[3]]);
                        println!(
                            "  [surface] {} _type={kind} window=({:.0},{:.0})-({:.0},{:.0}) \
                             multiply={:?} add={:?}",
                            self.player.target_path(instance),
                            corner[0],
                            corner[1],
                            far[0],
                            far[1],
                            color.mult,
                            color.add
                        );
                    }
                    let start = quads.len();
                    let label = if alpha < 1.0 {
                        format!(
                            "[engine surface: {kind}, movie alpha {:.0}%]",
                            alpha * 100.0
                        )
                    } else {
                        format!("[engine surface: {kind}]")
                    };
                    self.layout_label(
                        &label,
                        [corner[0] + 6.0, corner[1] + 20.0],
                        [0.7, 0.75, 0.85, 1.0],
                        clip,
                        &mut quads,
                        &mut atlas_dirty,
                    );
                    ops.push(Op::Quads(start..quads.len()));
                }
                Item::PushMask(i) => ops.push(Op::Push(i)),
                Item::PopMask(i) => ops.push(Op::Pop(i)),
            }
        }
        self.plan.items = items;

        if self.show_hit_regions {
            self.player.hit_areas(&mut self.hit_areas);
            let hovered = self.player.hovered();
            let identity: Affine = ([[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
            for area in &self.hit_areas {
                let c = area.corners.map(|p| apply(&(stretch.as_transform()), p));
                let mut l = Vec::with_capacity(8);
                for (s, e) in c.iter().zip(c.iter().cycle().skip(1)) {
                    l.push(*s);
                    l.push(*e);
                }
                lines.push(l);
                ops.push(Op::Lines {
                    index: lines.len() - 1,
                    world: identity,
                    color: if hovered == Some(area.button) {
                        [0.2, 1.0, 0.4, 1.0]
                    } else {
                        [1.0, 0.35, 0.1, 0.75]
                    },
                    add: [0.0; 4],
                    clip: UiQuad::UNCLIPPED,
                });
            }
        }

        if let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture.as_mut())
        {
            atlas_dirty |= atlas.take_dirty();
            if atlas_dirty {
                let (aw, ah) = atlas.dimensions();
                if let Some(data) = TextureData::rgba8(aw, ah, atlas.pixels().to_vec()) {
                    if let Ok(handle) = renderer.upload_texture(&data) {
                        *texture = handle;
                    }
                }
                let spare = atlas.capacity().saturating_sub(atlas.len());
                self.atlas_near_full = spare < GLYPHS_PER_FRAME;
            }
            let current = *texture;
            for q in &mut quads {
                if q.texture.is_some() {
                    q.texture = Some(current);
                }
            }
        }

        let hud_quads = if self.font.is_some() {
            let status = self.status_line();
            self.hud_quads(renderer, &status)
        } else {
            Vec::new()
        };

        let mut ordered: Vec<ScreenDraw<'_>> =
            Vec::with_capacity(ops.len().saturating_add(quads.len()));
        for op in &ops {
            match op {
                Op::Run {
                    mesh,
                    run,
                    world,
                    color,
                    add,
                    clip,
                    texture,
                } => {
                    let Some(r) = meshes.get(*mesh).and_then(|m| m.runs.get(*run)) else {
                        continue;
                    };
                    let mut g = UiGeometry::new()
                        .with_transform(world.0, world.1)
                        .with_triangles(&r.triangles)
                        .with_lines(&r.lines)
                        .with_color(*color)
                        .with_color_add(*add)
                        .with_clip(*clip);
                    if let Some((tex, transform, translate)) = texture {
                        g = g.with_texture(*tex, *transform, *translate);
                    }
                    ordered.push(ScreenDraw::Geometry(g));
                }
                Op::Lines {
                    index,
                    world,
                    color,
                    add,
                    clip,
                } => {
                    if let Some(l) = lines.get(*index) {
                        ordered.push(ScreenDraw::Geometry(
                            UiGeometry::new()
                                .with_transform(world.0, world.1)
                                .with_lines(l)
                                .with_color(*color)
                                .with_color_add(*add)
                                .with_clip(*clip),
                        ));
                    }
                }
                Op::Quads(range) => {
                    if let Some(qs) = quads.get(range.clone()) {
                        ordered.extend(qs.iter().copied().map(ScreenDraw::Quad));
                    }
                }
                Op::Push(i) => {
                    if let Some(m) = self.plan.masks.get(*i) {
                        ordered.push(ScreenDraw::PushMask(MaskShape::new(m)));
                    }
                }
                Op::Pop(i) => {
                    if let Some(m) = self.plan.masks.get(*i) {
                        ordered.push(ScreenDraw::PopMask(MaskShape::new(m)));
                    }
                }
            }
        }
        ordered.extend(hud_quads.iter().copied().map(ScreenDraw::Quad));

        if !self.seen.contains_key(&self.movie_name) {
            self.seen
                .insert(self.movie_name.clone(), SeenStats::default());
        }
        if let Some(seen) = self.seen.get_mut(&self.movie_name) {
            seen.commands = self.list.commands.len();
            seen.shapes = self.stats.shapes;
            seen.text = self.stats.text;
            seen.surfaces = self.stats.surfaces;
            seen.unresolved = self.stats.unresolved;
            if self.report_stats_next_draw {
                self.player.hit_areas(&mut self.hit_areas);
                seen.buttons = self.hit_areas.len();
            }
        }
        if self.report_stats_next_draw {
            let s = self.stats;
            let (frame, total, _) = self.root_frame();
            println!(
                "  frame {frame}/{total}: {} render commands -> {} shapes ({} runs), {} masks ({} sprite masks), {} text \
                 fields ({} empty, {} unbundled font, {} localised, {} key missing), {} engine \
                 surfaces ({}), {} unresolved, {} culled; text cache {} layouts / {} hits, {} meshes built; \
                 {} live instances, {} heap objects",
                self.list.commands.len(),
                s.shapes,
                s.runs,
                s.masks,
                s.compound_masks,
                s.text,
                s.text_empty,
                s.text_font_unresolved,
                s.text_localised,
                s.text_key_missing,
                s.surfaces,
                self.surfaces.label(),
                s.unresolved,
                s.culled,
                self.texts.layouts,
                self.texts.hits,
                self.meshes.built,
                self.player.live_instances(),
                self.player.heap_stats().objects,
            );
            self.report_stats_next_draw = false;
        }

        let mut scene = Scene::new(Camera::default()).with_ordered_screen(&ordered);
        if self.show_background {
            if let Some(bg) = self.player.background() {
                let [r, g, b, a] = unpack_color(bg);
                scene.clear_color = [f64::from(r), f64::from(g), f64::from(b), f64::from(a)];
            }
        }
        lap(|p| &mut p.build, &mut self.profile);
        if let Some(p) = self.profile.as_mut() {
            p.text += text_time;
            p.build = p.build.saturating_sub(text_time);
        }
        let result = renderer.render(&scene);
        lap(|p| &mut p.submit, &mut self.profile);
        if let Some(p) = self.profile.as_mut() {
            p.frames = p.frames.saturating_add(1);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_field(
        &mut self,
        instance: InstanceId,
        character: CharacterRef,
        world: &Affine,
        cx: ColorTransform,
        clip: [f32; 4],
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) -> bool {
        let Some(Character::EditText {
            bounds,
            font,
            alignment,
            color,
            font_height,
            multiline,
            wordwrap,
            ..
        }) = self.library.character(character)
        else {
            return false;
        };
        let field_anchor = self.text_anchor.for_field(*multiline != 0, *wordwrap != 0);
        let raw = self.player.text(instance).unwrap_or_default();
        if raw.is_empty() {
            self.stats.text_empty = self.stats.text_empty.saturating_add(1);
            return false;
        }
        let style = mint_sage_apt_viewer::text_style(&raw);
        let text = if self.localise_text {
            let drawn = localise(&raw, &self.string_table);
            if raw.starts_with('$') {
                if drawn.starts_with("MISSING: '") {
                    self.stats.text_key_missing = self.stats.text_key_missing.saturating_add(1);
                } else {
                    self.stats.text_localised = self.stats.text_localised.saturating_add(1);
                }
            }
            drawn
        } else {
            raw
        };
        let font_file = self.library.resolve(character.movie, *font).and_then(|f| {
            match self.library.character(f) {
                Some(Character::Font { name, .. }) => name.as_deref().and_then(resolved_font_file),
                _ => None,
            }
        });
        if font_file.is_none() {
            self.stats.text_font_unresolved = self.stats.text_font_unresolved.saturating_add(1);
        }
        let scale = (world.0[0][0] * world.0[0][0] + world.0[1][0] * world.0[1][0]).sqrt();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let size_px = (font_height * scale).round().clamp(1.0, 512.0) as u32;
        let key = TextKey {
            text,
            font: font_file,
            size_px,
            bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom].map(f32::to_bits),
            alignment: *alignment,
            anchor: field_anchor,
            scale: scale.to_bits(),
            x_scale: scale.to_bits(),
            wrap: 0,
        };
        let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture) else {
            return false;
        };
        let fonts = &self.movie_fonts;
        let busy = &mut self.atlas_busy;
        let box_bounds = *bounds;
        let (fh, align) = (*font_height, *alignment);
        let anchor = field_anchor;
        let layout: &TextLayout = self.texts.get_or_layout(&key, |key| {
            let loaded = key.font.and_then(|f| fonts.get(f));
            let to_local = if scale.abs() < f32::EPSILON {
                1.0
            } else {
                1.0 / scale
            };
            let width = mint_ui::measure_text(&key.text, loaded, size_px) * to_local;
            let metrics = loaded.and_then(|f| f.metrics(size_px).ok());
            let (ascent, descent, win_ascent, win_descent) =
                metrics.map_or((fh * 0.8, fh * 0.2, fh * 0.8, fh * 0.2), |m| {
                    (
                        m.ascent * to_local,
                        m.descent * to_local,
                        m.win_ascent * to_local,
                        m.win_descent * to_local,
                    )
                });
            let pen = text_origin(
                TextBox {
                    bounds: box_bounds,
                    alignment: align,
                    text_width: width,
                    ascent,
                    descent,
                    win_ascent,
                    win_descent,
                },
                anchor,
            );
            let quads = mint_ui::layout_text(
                &key.text,
                [0.0, 0.0],
                WHITE,
                FontId(font_id_for(fonts, key.font)),
                loaded,
                atlas,
                size_px,
                texture,
            );
            (
                TextLayout { pen, quads },
                note_rasterised(atlas, busy, atlas_dirty),
            )
        });
        let pen = apply(world, layout.pen);
        let authored = mint_sage_apt_viewer::apply_text_color(
            unpack_edittext_color(*color),
            self.player.own_value(instance, "textColor").as_ref(),
        );
        let text_color = multiply(cx.mult, authored);
        if style.drop_shadow {
            let offset = mint_sage_apt_viewer::host::DROP_SHADOW_OFFSET;
            let shadow_color = [0.0, 0.0, 0.0, text_color[3]];
            out.extend(layout.quads.iter().map(|q| {
                let mut q = *q;
                q.translation[0] += pen[0] + offset;
                q.translation[1] += pen[1] + offset;
                q.color_multiply = shadow_color;
                q.color_add = [0.0, 0.0, 0.0, 0.0];
                q.clip = clip;
                q
            }));
        }
        out.extend(layout.quads.iter().map(|q| {
            let mut q = *q;
            q.translation[0] += pen[0];
            q.translation[1] += pen[1];
            q.color_multiply = text_color;
            q.color_add = cx.add;
            q.clip = clip;
            q
        }));
        self.stats.text = self.stats.text.saturating_add(1);
        true
    }

    fn layout_label(
        &mut self,
        text: &str,
        at: [f32; 2],
        color: [f32; 4],
        clip: [f32; 4],
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        const LABEL_PX: u32 = 14;
        let key = TextKey {
            text: text.to_owned(),
            font: Some("albertusmt.otf"),
            size_px: LABEL_PX,
            bounds: [0; 4],
            alignment: 0,
            anchor: TextAnchor::Top,
            scale: 1.0f32.to_bits(),
            x_scale: 1.0f32.to_bits(),
            wrap: 0,
        };
        let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture) else {
            return;
        };
        let fonts = &self.movie_fonts;
        let busy = &mut self.atlas_busy;
        let layout = self.texts.get_or_layout(&key, |key| {
            let quads = mint_ui::layout_text(
                &key.text,
                [0.0, 0.0],
                WHITE,
                FontId(font_id_for(fonts, key.font)),
                key.font.and_then(|f| fonts.get(f)),
                atlas,
                LABEL_PX,
                texture,
            );
            (
                TextLayout {
                    pen: [0.0, 0.0],
                    quads,
                },
                note_rasterised(atlas, busy, atlas_dirty),
            )
        });
        out.extend(layout.quads.iter().map(|q| {
            let mut q = *q;
            q.translation[0] += at[0];
            q.translation[1] += at[1];
            q.color_multiply = color;
            q.clip = clip;
            q
        }));
    }

    fn hud_quads(&mut self, renderer: &mut Renderer, text: &str) -> Vec<UiQuad> {
        let (Some(font), Some(atlas), Some(texture)) = (
            self.font.as_ref(),
            self.atlas.as_mut(),
            self.hud_texture.as_mut(),
        ) else {
            return Vec::new();
        };
        if let Some((cached, quads)) = &self.hud_cache {
            if cached == text {
                return quads.clone();
            }
        }
        atlas.begin_frame();
        let size_px = 16u32;
        let _ = mint_ui::layout_text(
            text,
            [8.0, 8.0],
            WHITE,
            FontId(0),
            Some(font),
            atlas,
            size_px,
            *texture,
        );
        if atlas.take_dirty() {
            let (aw, ah) = atlas.dimensions();
            if let Some(data) = TextureData::rgba8(aw, ah, atlas.pixels().to_vec()) {
                if let Ok(handle) = renderer.upload_texture(&data) {
                    *texture = handle;
                }
            }
        }
        let glyphs = mint_ui::layout_text(
            text,
            [8.0, 8.0],
            WHITE,
            FontId(0),
            Some(font),
            atlas,
            size_px,
            *texture,
        );
        let mut out = vec![
            UiQuad::new(4.0, 4.0, (self.window_size.0 - 8.0).max(0.0), 22.0)
                .with_color_transform([0.0, 0.0, 0.0, 0.65], [0.0, 0.0, 0.0, 0.0]),
        ];
        out.extend(glyphs);
        self.hud_cache = Some((text.to_owned(), out.clone()));
        out
    }
}

fn note_rasterised(atlas: &mut GlyphAtlas, busy: &mut bool, dirty: &mut bool) -> bool {
    if atlas.take_dirty() {
        *busy = true;
        *dirty = true;
    }
    *busy
}
