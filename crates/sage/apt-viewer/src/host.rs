use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mint_image::dds::Dds;
use mint_render::{
    MaskShape, Renderer, ScreenDraw, TextureData, TextureHandle, UiGeometry, UiQuad,
};
use mint_sage_apt::{parse_ru, standalone_texture_path, Character, TextureLocation};
use mint_sage_apt_player::{
    CharacterRef, ColorTransform, HitArea, HitMode, InstanceId, Library, Limits, MovieId, Outward,
    Player, RenderCommand, RenderList, RenderOptions, SharedGlobal,
};
use mint_sage_asset::Resolver;
use mint_sage_ini::{CommandButtonIndex, MappedImageIndex, MappedRect};
use mint_sage_str::StringTable;
use mint_ui::{FontId, GlyphAtlas, GlyphAtlasConfig};
use mint_vfs::{Vfs, VfsPath};

use crate::gadget::check::{
    check_box, check_label, CheckContent, CheckStyle, CheckView, State as GadgetState,
};
use crate::gadget::combo::{self as combo_gadget, ComboContent, ComboStyle, ComboView, Nav};
use crate::gadget::entry::{self as entry_gadget, EntryContent, EntryLook};
use crate::gadget::list::{
    bar_row, image_cell, max_display, row_height, text_top, thumb_rect, wrapped, Align, ListCell,
    ListContent, ListFrame, ListPart, ListStyle, ListView, POINTS_TO_UNITS, TEXT_WIDTH_OFFSET,
    TEXT_X_OFFSET,
};
use crate::gadget::map_window::{
    picture_rect, spot_rects, MapWindowContent, MapWindowStyle, MapWindowView, BAR, LINE,
};
use crate::gadget::slider::{
    segments, thumb_rect as slider_thumb, SliderContent, SliderStyle, SliderView, SEGMENT_GAP,
};
use crate::scene::{
    apply, can_show, on_screen, plan, rect_triangles, Affine, Item, MaskMode, MeshCache, Plan,
    ShapeMesh, TextCache, TextKey, TextLayout,
};
use crate::{
    multiply_color, resolved_font_file, ru_base_name, unpack_edittext_color, AnchorCache,
    ImageModes, Letterbox, MovieLibrary, ScaleMode, TextAnchor, TextBox,
};

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TIMER_RIM_HOLE: f32 = 0.905;
const GLYPHS_PER_FRAME: usize = 64;
pub const DISABLED_TINT: f32 = 0.4;

pub const GRAYSCALE_MODE: &str = "GRAYSCALE";

pub const DISABLED_IMAGE_TYPE: &str = "RenderImageDisabled";

#[must_use]
pub fn draws_grey(surface_type: Option<&str>, mode: Option<&str>) -> bool {
    mode.is_some_and(|m| m.eq_ignore_ascii_case(GRAYSCALE_MODE))
        || surface_type.is_some_and(|t| t.eq_ignore_ascii_case(DISABLED_IMAGE_TYPE))
}

pub fn stage_space_aspect(player: &Player, id: InstanceId) -> Option<f32> {
    let [l, t, r, b] = player.local_bounds(id)?;
    let m = player.world_matrix(id)?;
    let corners = [m.apply(l, t), m.apply(r, t), m.apply(l, b), m.apply(r, b)];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some(w / h)
}

fn contain_quad(world: &Affine, local: [f32; 4], aspect: f32) -> ([[f32; 2]; 2], [f32; 2]) {
    let [l, t, r, b] = local;
    let corners = [
        apply(world, [l, t]),
        apply(world, [r, t]),
        apply(world, [l, b]),
        apply(world, [r, b]),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for [x, y] in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (box_w, box_h) = ((x1 - x0).max(0.0), (y1 - y0).max(0.0));
    let (fit_w, fit_h) = if box_h <= 0.0 || box_w / box_h > aspect {
        (box_h * aspect, box_h)
    } else {
        (box_w, box_w / aspect)
    };
    let translation = [x0 + (box_w - fit_w) * 0.5, y0 + (box_h - fit_h) * 0.5];
    ([[fit_w, 0.0], [0.0, fit_h]], translation)
}

#[cfg(test)]
mod contain_quad_tests {
    use super::{apply, contain_quad};

    #[test]
    fn a_uniform_world_places_it_unchanged() {
        let world = ([[2.0, 0.0], [0.0, 2.0]], [10.0, 20.0]);
        let local = [0.0, 0.0, 100.0, 50.0];
        let (affine, translation) = contain_quad(&world, local, 2.0);
        assert_eq!(translation, apply(&world, [local[0], local[1]]));
        assert_eq!(affine, [[200.0, 0.0], [0.0, 100.0]]);
    }

    #[test]
    fn a_non_uniform_world_keeps_the_authored_aspect() {
        let world = ([[4.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
        let local = [-50.0, -50.0, 50.0, 50.0];
        let (affine, _) = contain_quad(&world, local, 2.0);
        let (fit_w, fit_h) = (affine[0][0], affine[1][1]);
        assert!((fit_w / fit_h - 2.0).abs() < 1e-6, "{fit_w}/{fit_h}");
        assert!((fit_w - 200.0).abs() < 1e-6);
        assert!((fit_h - 100.0).abs() < 1e-6);
    }

    #[test]
    fn it_centres_in_the_non_uniform_boxes_slack_axis() {
        let world = ([[4.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
        let local = [-50.0, -50.0, 50.0, 50.0];
        let (affine, translation) = contain_quad(&world, local, 2.0);
        let (fit_w, _) = (affine[0][0], affine[1][1]);
        assert!(
            (translation[0] - -100.0).abs() < 1e-6,
            "{translation:?} {fit_w}"
        );
        assert!((translation[1] - -50.0).abs() < 1e-6);
    }
}

pub const DROP_SHADOW_OFFSET: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct SurfaceIcon {
    pub texture: TextureHandle,
    pub uv_rect: [f32; 4],
    pub tint: [f32; 4],
    pub premultiplied: bool,
    pub rotated: bool,
    pub ignores_color_transform: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TimerClock {
    pub covered: f32,
    pub tint: [f32; 4],
    pub rim: [f32; 4],
    pub fill: Option<SurfaceIcon>,
    pub ring: Option<SurfaceIcon>,
}

#[derive(Debug, Clone)]
pub enum SurfaceContent {
    Icon(SurfaceIcon),
    List(Arc<ListContent>),
    Check(Arc<CheckContent>),
    Slider(SliderContent),
    Combo(Arc<ComboContent>),
    Entry(Arc<EntryContent>),
    MapWindow(Arc<MapWindowContent>),
    Text(Arc<TextBoxContent>),
    Progress(TimerClock),
    Graph(Arc<GraphContent>),
    Placed(Arc<PlacedText>),
    Hidden,
}

#[derive(Debug, Clone, Default)]
pub struct GraphContent {
    pub select: String,
    pub plots: Vec<(String, GraphPlot)>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphPlot {
    pub lines: Vec<GraphLine>,
    pub markers: Vec<GraphMarker>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphLine {
    pub points: Vec<[f32; 2]>,
    pub color: [f32; 4],
    pub width: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphMarker {
    pub at: [f32; 2],
    pub size: [f32; 2],
    pub icon: SurfaceIcon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub font: String,
    pub point_size: f32,
    pub bold: bool,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlacedText {
    pub rows: Vec<PlacedRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedRow {
    pub text: String,
    pub font: String,
    pub point_size: f32,
    pub x: f32,
    pub y: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextBoxContent {
    pub lines: Vec<TextLine>,
    pub align: TextAlign,
}

pub const NATIVE_TEXT_BASE: [f32; 2] = [1024.0, 768.0];

#[must_use]
pub fn native_text_px(points: f32, window: [f32; 2]) -> u32 {
    let scale = (window[0] / NATIVE_TEXT_BASE[0]).min(window[1] / NATIVE_TEXT_BASE[1]);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = (points.max(0.0) * scale).round().clamp(1.0, 256.0) as u32;
    px
}

#[derive(Debug, Clone, PartialEq)]
struct TextRow {
    text: String,
    font: Option<&'static str>,
    size_px: u32,
    color: [f32; 4],
    x: f32,
    baseline: f32,
}

fn lay_out_text_box(
    lines: &[TextLine],
    align: TextAlign,
    width: f32,
    scale: [f32; 2],
    window: [f32; 2],
    fonts: &HashMap<String, mint_ui::Font>,
) -> (Vec<TextRow>, f32) {
    let x_scale = scale[0].abs().max(f32::EPSILON);
    let y_scale = scale[1].abs().max(f32::EPSILON);
    let width = width.max(0.0);
    let mut rows = Vec::new();
    let mut top = 0.0f32;
    for line in lines {
        let font_file = resolved_font_file(&line.font);
        let font = font_file.and_then(|f| fonts.get(f));
        let size_px = native_text_px(line.point_size, window);
        #[allow(clippy::cast_precision_loss)]
        let px = size_px as f32;
        let (ascent, line_height) = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or((px * 0.8, px * 1.2), |m| {
                (m.win_ascent, m.win_ascent + m.win_descent)
            });
        let (ascent, line_height) = (ascent / y_scale, line_height / y_scale);
        let measure = |s: &str| mint_ui::measure_text(s, font, size_px) / x_scale;
        for text in wrapped(&line.text, width, measure) {
            let x = match align {
                TextAlign::Left => 0.0,
                TextAlign::Centre => ((width - measure(&text)) * 0.5).max(0.0),
            };
            rows.push(TextRow {
                text,
                font: font_file,
                size_px,
                color: line.color,
                x,
                baseline: top + ascent,
            });
            top += line_height;
        }
    }
    (rows, top)
}

pub struct IconProvider {
    vfs: Arc<Vfs>,
    resolver: Resolver,
    mapped_images: Arc<MappedImageIndex>,
    command_buttons: Arc<CommandButtonIndex>,
    textures: HashMap<String, Option<(TextureHandle, u32, u32)>>,
    black: Option<TextureHandle>,
    palettes: HashMap<String, Option<Arc<crate::color_picker::Palette>>>,
}

impl IconProvider {
    #[must_use]
    pub fn new(
        vfs: Arc<Vfs>,
        mapped_images: Arc<MappedImageIndex>,
        command_buttons: Arc<CommandButtonIndex>,
    ) -> Self {
        Self {
            vfs,
            resolver: Resolver::new(),
            mapped_images,
            command_buttons,
            textures: HashMap::new(),
            black: None,
            palettes: HashMap::new(),
        }
    }

    pub fn black_icon(&mut self, renderer: &mut Renderer) -> Option<SurfaceIcon> {
        if self.black.is_none() {
            self.black = black_texture(renderer);
        }
        Some(SurfaceIcon {
            texture: self.black?,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            premultiplied: false,
            rotated: false,
            ignores_color_transform: false,
        })
    }

    #[must_use]
    pub const fn whole_texture_icon(
        texture: TextureHandle,
        _width: u32,
        _height: u32,
    ) -> SurfaceIcon {
        SurfaceIcon {
            texture,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            premultiplied: false,
            rotated: false,
            ignores_color_transform: false,
        }
    }

    #[must_use]
    pub fn cache_len(&self) -> usize {
        self.textures.len()
    }

    pub fn icon(&mut self, renderer: &mut Renderer, command_button: &str) -> Option<SurfaceIcon> {
        let button = self.command_buttons.get(command_button)?;
        let image_name = button.button_image.clone()?;
        self.icon_by_image(renderer, &image_name)
    }

    pub fn icon_by_image(
        &mut self,
        renderer: &mut Renderer,
        image_name: &str,
    ) -> Option<SurfaceIcon> {
        self.icon_by_image_as(renderer, image_name, false)
    }

    pub fn palette(&mut self, image_name: &str) -> Option<Arc<crate::color_picker::Palette>> {
        if !self.palettes.contains_key(image_name) {
            let built = self.build_palette(image_name);
            self.palettes.insert(image_name.to_owned(), built);
        }
        self.palettes.get(image_name)?.clone()
    }

    fn build_palette(&self, image_name: &str) -> Option<Arc<crate::color_picker::Palette>> {
        let mapped = self.mapped_images.get(image_name)?;
        let texture_name = mapped.texture.as_deref()?;
        let path = self.resolver.resolve(&self.vfs, texture_name).ok()?;
        let image = decode_image(&self.vfs, path.as_str())?;
        let (texture_width, texture_height) = (image.width(), image.height());
        let (origin, size) = match mapped.coords {
            Some(c) => (
                [u32::try_from(c.left).ok()?, u32::try_from(c.top).ok()?],
                [
                    u32::try_from(c.right.checked_sub(c.left)?).ok()?,
                    u32::try_from(c.bottom.checked_sub(c.top)?).ok()?,
                ],
            ),
            None => ([0, 0], [texture_width, texture_height]),
        };
        Some(Arc::new(crate::color_picker::Palette {
            texture_width,
            texture_height,
            rgba: image.into_pixels(),
            origin,
            size,
        }))
    }

    #[must_use]
    pub fn image_size(&self, image_name: &str) -> Option<[f32; 2]> {
        let c = self.mapped_images.get(image_name)?.coords?;
        #[allow(clippy::cast_precision_loss)]
        Some([(c.right - c.left) as f32, (c.bottom - c.top) as f32])
    }

    #[must_use]
    pub fn image_extent(&self, image_name: &str) -> Option<[f32; 2]> {
        let [w, h] = self.image_size(image_name)?;
        let rotated = self
            .mapped_images
            .get(image_name)
            .is_some_and(|image| is_rotated(image.status.as_deref()));
        Some(if rotated { [h, w] } else { [w, h] })
    }

    pub fn grey_icon(
        &mut self,
        renderer: &mut Renderer,
        command_button: &str,
    ) -> Option<SurfaceIcon> {
        let button = self.command_buttons.get(command_button)?;
        let image_name = button.button_image.clone()?;
        self.icon_by_image_as(renderer, &image_name, true)
    }

    #[must_use]
    pub fn needs_decode(&self, image_name: &str) -> bool {
        self.mapped_images
            .get(image_name)
            .and_then(|mapped| mapped.texture.as_deref())
            .is_some_and(|texture| !self.textures.contains_key(&texture_stem(texture)))
    }

    #[must_use]
    pub fn button_image(&self, command_button: &str) -> Option<&str> {
        self.command_buttons
            .get(command_button)?
            .button_image
            .as_deref()
    }

    pub fn icon_by_image_as(
        &mut self,
        renderer: &mut Renderer,
        image_name: &str,
        grey: bool,
    ) -> Option<SurfaceIcon> {
        let mapped = self.mapped_images.get(image_name)?;
        let texture_name = mapped.texture.as_deref()?;
        let stem = texture_stem(texture_name);
        let key = if grey { format!("{stem}#grey") } else { stem };
        if !self.textures.contains_key(&key) {
            let loaded = self
                .resolver
                .resolve(&self.vfs, texture_name)
                .ok()
                .and_then(|path| load_texture_as(&self.vfs, renderer, path.as_str(), grey));
            self.textures.insert(key.clone(), loaded);
        }
        let &(texture, width, height) = self.textures.get(&key)?.as_ref()?;
        if width == 0 || height == 0 {
            return None;
        }
        Some(SurfaceIcon {
            texture,
            uv_rect: mapped_uv(
                mapped.coords,
                (mapped.texture_width, mapped.texture_height),
                (width, height),
            ),
            tint: [1.0, 1.0, 1.0, 1.0],
            premultiplied: false,
            rotated: is_rotated(mapped.status.as_deref()),
            ignores_color_transform: false,
        })
    }
}

#[must_use]
pub fn mapped_uv(
    coords: Option<MappedRect>,
    stated: (Option<u32>, Option<u32>),
    decoded: (u32, u32),
) -> [f32; 4] {
    let Some(c) = coords else {
        return [0.0, 0.0, 1.0, 1.0];
    };
    let (w, h) = (stated.0.unwrap_or(decoded.0), stated.1.unwrap_or(decoded.1));
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    #[allow(clippy::cast_precision_loss)]
    let (left, top, right, bottom) = (
        c.left as f32 / w,
        c.top as f32 / h,
        c.right as f32 / w,
        c.bottom as f32 / h,
    );
    [left, top, (right - left).max(0.0), (bottom - top).max(0.0)]
}

fn is_rotated(status: Option<&str>) -> bool {
    status.is_some_and(|status| status.eq_ignore_ascii_case("ROTATED_90_CLOCKWISE"))
}

fn texture_stem(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower
        .rsplit_once('.')
        .map_or(lower.clone(), |(stem, _)| stem.to_owned())
}

#[must_use]
pub fn load_texture_at_path(
    vfs: &Vfs,
    renderer: &mut Renderer,
    path: &str,
) -> Option<(TextureHandle, u32, u32)> {
    load_texture_as(vfs, renderer, path, false)
}

pub fn greyscale_rgba(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        if let [r, g, b, _] = px {
            let luma =
                (u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114 + 500) / 1000;
            let luma = u8::try_from(luma.min(255)).unwrap_or(u8::MAX);
            (*r, *g, *b) = (luma, luma, luma);
        }
    }
}

fn decode_image(vfs: &Vfs, path: &str) -> Option<mint_image::RgbaImage> {
    let bytes = vfs.read(path).ok()?;
    let extension = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    Some(match extension.as_str() {
        "dds" => Dds::parse(&bytes).ok()?.decode().ok()?,
        "tga" => mint_image::tga::decode(&bytes).ok()?,
        "jpg" | "jpeg" => {
            let colour = mint_image::jpeg::decode(&bytes).ok()?;
            match mint_sage_asset::alpha_sidecar(&VfsPath::new(path))
                .and_then(|sidecar| vfs.read(sidecar.as_str()).ok())
            {
                Some(alpha) => colour.with_alpha_of(&mint_image::png::decode(&alpha).ok()?)?,
                None => colour,
            }
        }
        _ => return None,
    })
}

fn load_texture_as(
    vfs: &Vfs,
    renderer: &mut Renderer,
    path: &str,
    grey: bool,
) -> Option<(TextureHandle, u32, u32)> {
    let image = decode_image(vfs, path)?;
    let (width, height) = (image.width(), image.height());
    let mut pixels = image.into_pixels();
    if grey {
        greyscale_rgba(&mut pixels);
    }
    let data = TextureData::rgba8(width, height, pixels)?;
    let handle = renderer.upload_texture(&data).ok()?;
    Some((handle, width, height))
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(missing_docs)]
pub struct OverlayStats {
    pub shapes: u32,
    pub runs: u32,
    pub text: u32,
    pub text_empty: u32,
    pub text_font_unresolved: u32,
    pub surfaces: u32,
    pub unresolved: u32,
    pub masks: u32,
    pub culled: u32,
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
        clip: [f32; 4],
    },
    Quads(std::ops::Range<usize>),
    Push(usize),
    Pop(usize),
}

pub struct MovieView {
    library: Arc<Library>,
    movie_id: MovieId,
    limits: Limits,
    shared_global: Option<SharedGlobal>,
    assets: MovieLibrary,
    pub player: Player,
    list: RenderList,
    plan: Plan,
    meshes: MeshCache,
    texts: TextCache,
    texture_cache: HashMap<(String, String), Option<TextureHandle>>,
    text_atlas: Option<GlyphAtlas>,
    text_texture: Option<TextureHandle>,
    known_atlas_textures: std::collections::HashSet<TextureHandle>,
    atlas_page: Option<TextureHandle>,
    fonts: Arc<HashMap<String, mint_ui::Font>>,
    strings: Arc<StringTable>,
    frame_accum: f32,
    pub fps: f32,
    pub window_size: (f32, f32),
    ops: Vec<Op>,
    frame_meshes: Vec<Arc<ShapeMesh>>,
    lines: Vec<Vec<[f32; 2]>>,
    quads: Vec<UiQuad>,
    atlas_busy: bool,
    text_warm: Vec<(String, Option<&'static str>, u32)>,
    atlas_near_full: bool,
    pub stats: OverlayStats,
    pub scale_mode: ScaleMode,
    pub host: Option<Letterbox>,
    anchor_cache: AnchorCache,
    pub last_build: BuildProfile,
    surface_icons: HashMap<InstanceId, SurfaceContent>,
    named_images: HashMap<String, SurfaceContent>,
    dynamic_strings: HashMap<String, String>,
    lists: HashMap<InstanceId, ListView>,
    list_layouts: HashMap<InstanceId, (LaidKey, Vec<Vec<Laid>>, Vec<f32>)>,
    list_styles: HashMap<String, Option<Arc<ListStyle>>>,
    plain_list: Arc<ListStyle>,
    check_styles: HashMap<String, Option<Arc<CheckStyle>>>,
    plain_check: Arc<CheckStyle>,
    slider_styles: HashMap<String, Option<Arc<SliderStyle>>>,
    plain_slider: Arc<SliderStyle>,
    entry_styles: HashMap<String, Option<Arc<EntryLook>>>,
    plain_entry: Arc<EntryLook>,
    checks: HashMap<InstanceId, CheckView>,
    sliders: HashMap<InstanceId, SliderView>,
    combo_styles: HashMap<String, Option<Arc<ComboStyle>>>,
    plain_combo: Arc<ComboStyle>,
    combos: HashMap<InstanceId, ComboView>,
    map_window_styles: HashMap<String, Option<Arc<MapWindowStyle>>>,
    plain_map_window: Arc<MapWindowStyle>,
    map_windows: HashMap<InstanceId, MapWindowView>,
    graph_mode: Option<String>,
    pressed_spot: Option<(InstanceId, usize)>,
    pending_popup: Option<PendingPopup>,
    popup: Option<PlacedPopup>,
    wanted_looks: Vec<(GadgetKind, String)>,
    wanted_images: Vec<String>,
    asked_images: HashSet<String>,
    color_pickers: crate::color_picker::ColorPickers,
    placed_gadgets: Vec<PlacedGadget>,
    pub text_anchor: TextAnchor,
    pub image_modes: ImageModes,
    pub disc_pictures: bool,
    pub annotate_surfaces: bool,
    pub timer_overlay_covers_remaining: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BuildProfile {
    pub render_list: Duration,
    pub meshes: Duration,
    pub plan: Duration,
    pub shapes: Duration,
    pub text_fields: Duration,
    pub surfaces: Duration,
    pub atlas_upload: Duration,
}

impl BuildProfile {
    #[must_use]
    pub fn total(&self) -> Duration {
        self.render_list
            + self.meshes
            + self.plan
            + self.shapes
            + self.text_fields
            + self.surfaces
            + self.atlas_upload
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("no movie named {0:?} in the player library")]
    UnknownMovie(String),
    #[error(transparent)]
    Player(#[from] mint_sage_apt_player::Error),
}

fn white_texture(renderer: &mut Renderer) -> Option<TextureHandle> {
    TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
        .and_then(|d| renderer.upload_texture(&d).ok())
}

fn black_texture(renderer: &mut Renderer) -> Option<TextureHandle> {
    TextureData::rgba8(1, 1, vec![0, 0, 0, 255]).and_then(|d| renderer.upload_texture(&d).ok())
}

impl MovieView {
    pub fn load(
        library: Arc<Library>,
        asset_roots: impl IntoIterator<Item = PathBuf>,
        movie: &str,
        fonts: Arc<HashMap<String, mint_ui::Font>>,
        strings: Arc<StringTable>,
        window_size: (f32, f32),
        limits: Limits,
        renderer: &mut Renderer,
    ) -> Result<Self, Error> {
        let id = library
            .movie_id(movie)
            .ok_or_else(|| Error::UnknownMovie(movie.to_owned()))?;
        let mut player = Player::new(Arc::clone(&library), id, limits)?;
        player.set_hit_mode(HitMode::Records);
        let text_atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).ok();
        let text_texture = if text_atlas.is_some() {
            white_texture(renderer)
        } else {
            None
        };
        let known_atlas_textures = text_texture.into_iter().collect();
        Ok(Self {
            library,
            movie_id: id,
            limits,
            shared_global: None,
            assets: MovieLibrary::new_with_roots(asset_roots),
            player,
            list: RenderList::default(),
            plan: Plan::default(),
            meshes: MeshCache::default(),
            texts: TextCache::default(),
            texture_cache: HashMap::new(),
            text_atlas,
            text_texture,
            known_atlas_textures,
            atlas_page: None,
            fonts,
            strings,
            frame_accum: 0.0,
            fps: 30.0,
            window_size,
            ops: Vec::new(),
            frame_meshes: Vec::new(),
            lines: Vec::new(),
            quads: Vec::new(),
            atlas_busy: false,
            text_warm: Vec::new(),
            atlas_near_full: false,
            stats: OverlayStats::default(),
            scale_mode: ScaleMode::default(),
            host: None,
            anchor_cache: AnchorCache::new(),
            last_build: BuildProfile::default(),
            text_anchor: TextAnchor::default(),
            image_modes: ImageModes::default(),
            disc_pictures: false,
            annotate_surfaces: false,
            timer_overlay_covers_remaining: true,
            surface_icons: HashMap::new(),
            named_images: HashMap::new(),
            dynamic_strings: HashMap::new(),
            lists: HashMap::new(),
            list_layouts: HashMap::new(),
            list_styles: HashMap::new(),
            plain_list: Arc::new(ListStyle::plain()),
            check_styles: HashMap::new(),
            plain_check: Arc::new(CheckStyle::plain()),
            slider_styles: HashMap::new(),
            plain_slider: Arc::new(SliderStyle::plain()),
            entry_styles: HashMap::new(),
            plain_entry: Arc::new(EntryLook::plain()),
            checks: HashMap::new(),
            sliders: HashMap::new(),
            combo_styles: HashMap::new(),
            plain_combo: Arc::new(ComboStyle::plain()),
            combos: HashMap::new(),
            map_window_styles: HashMap::new(),
            plain_map_window: Arc::new(MapWindowStyle::plain()),
            map_windows: HashMap::new(),
            graph_mode: None,
            pressed_spot: None,
            pending_popup: None,
            popup: None,
            wanted_looks: Vec::new(),
            wanted_images: Vec::new(),
            asked_images: HashSet::new(),
            color_pickers: crate::color_picker::ColorPickers::default(),
            placed_gadgets: Vec::new(),
        })
    }

    pub fn share_global(&mut self, shared: &SharedGlobal) {
        self.player.share_global(shared);
        self.shared_global = Some(shared.clone());
    }

    pub fn restart(&mut self) -> Result<(), Error> {
        let mut player = Player::new(Arc::clone(&self.library), self.movie_id, self.limits)?;
        player.set_hit_mode(HitMode::Records);
        if let Some(shared) = &self.shared_global {
            player.share_global(shared);
        }
        self.player = player;
        self.list = RenderList::default();
        self.frame_accum = 0.0;
        self.anchor_cache.clear();
        self.surface_icons.clear();
        self.named_images.clear();
        self.asked_images.clear();
        self.color_pickers.clear();
        self.dynamic_strings.clear();
        self.text_warm.clear();
        self.lists.clear();
        self.list_layouts.clear();
        self.checks.clear();
        self.sliders.clear();
        self.combos.clear();
        self.map_windows.clear();
        self.pending_popup = None;
        self.popup = None;
        self.placed_gadgets.clear();
        self.ops.clear();
        self.quads.clear();
        self.lines.clear();
        self.frame_meshes.clear();
        Ok(())
    }

    pub fn set_surface_icon(&mut self, instance: InstanceId, icon: Option<SurfaceIcon>) {
        self.set_surface(instance, icon.map(SurfaceContent::Icon));
    }

    pub fn set_surface(&mut self, instance: InstanceId, content: Option<SurfaceContent>) {
        match content {
            Some(content) => {
                self.surface_icons.insert(instance, content);
            }
            None => {
                self.surface_icons.remove(&instance);
            }
        }
    }

    pub fn set_named_image(&mut self, key: &str, content: Option<SurfaceContent>) {
        match content {
            Some(content) => {
                self.named_images.insert(key.to_owned(), content);
            }
            None => {
                self.named_images.remove(key);
            }
        }
    }

    pub fn set_string(&mut self, key: &str, value: Option<String>) {
        match value {
            Some(value) => {
                self.dynamic_strings.insert(key.to_owned(), value);
            }
            None => {
                self.dynamic_strings.remove(key);
            }
        }
    }

    fn surface_content(&self, instance: InstanceId) -> Option<SurfaceContent> {
        if let Some(content) = self.surface_icons.get(&instance) {
            return Some(content.clone());
        }
        pulled_content(&self.player, instance, &self.named_images)
    }

    #[must_use]
    pub fn movie_id(&self) -> MovieId {
        self.movie_id
    }

    #[must_use]
    pub fn stage_size(&self) -> (f32, f32) {
        self.library
            .movie(self.movie_id())
            .map_or((1024.0, 768.0), |m| {
                (
                    m.movie.record.screen_width as f32,
                    m.movie.record.screen_height as f32,
                )
            })
    }

    #[must_use]
    pub fn letterbox(&self) -> Letterbox {
        let (sw, sh) = self.stage_size();
        let stretch = Letterbox::stretch(sw, sh, self.window_size.0, self.window_size.1);
        self.host.map_or(stretch, |host| stretch.after(host))
    }

    #[must_use]
    pub fn anchors(&mut self) -> HashMap<InstanceId, Letterbox> {
        let (sw, sh) = self.stage_size();
        let table = self.anchor_cache.letterboxes(
            &self.player,
            sw,
            sh,
            self.window_size.0,
            self.window_size.1,
            crate::ANCHOR_EDGE_SLACK,
        );
        match self.host {
            None => table,
            Some(host) => table
                .into_iter()
                .map(|(id, lb)| (id, lb.after(host)))
                .collect(),
        }
    }

    #[must_use]
    pub fn top_level_ancestor(&self, id: InstanceId) -> InstanceId {
        crate::top_level_ancestor(&self.player, id)
    }

    #[must_use]
    pub fn resolve_stage_point(&mut self, window_pos: [f32; 2]) -> [f32; 2] {
        let (sw, sh) = self.stage_size();
        if self.host.is_some() {
            return match self.scale_mode {
                ScaleMode::Stretch => self.letterbox().to_stage(window_pos),
                ScaleMode::Anchored => {
                    let anchors = self.anchors();
                    crate::resolve_hosted(&self.player, self.letterbox(), &anchors, window_pos)
                }
            };
        }
        self.anchor_cache.resolve_point(
            &self.player,
            self.scale_mode,
            sw,
            sh,
            self.window_size.0,
            self.window_size.1,
            crate::ANCHOR_EDGE_SLACK,
            window_pos,
        )
    }

    pub fn advance(&mut self, elapsed: Duration) {
        for _ in 0..self.ticks_owed(elapsed) {
            let _ = self.player.tick();
        }
    }

    pub fn ticks_owed(&mut self, elapsed: Duration) -> u32 {
        if self.fps <= 0.0 {
            return 0;
        }
        self.frame_accum += elapsed.as_secs_f32() * self.fps;
        let owed = self.frame_accum.floor().clamp(0.0, 8.0);
        self.frame_accum -= owed;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let owed = owed as u32;
        owed
    }

    pub fn take_outward(&mut self) -> Vec<Outward> {
        self.player.take_outward()
    }

    pub fn hit_areas(&mut self, out: &mut Vec<HitArea>) {
        self.player.hit_areas(out);
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
            let data = movie_texture(self.assets.assets(&key.0)?.tga.get(&key.1)?)?;
            renderer.upload_texture(&data).ok()
        })();
        self.texture_cache.insert(key.clone(), handle);
        handle
    }

    pub fn preload_textures(&mut self, renderer: &mut Renderer) -> (usize, usize) {
        let mut keys: Vec<(String, String)> = Vec::new();
        let mut seen = HashSet::new();
        for id in self.import_closure() {
            let Some(name) = self.library.movie(id).map(|m| m.movie.name.clone()) else {
                continue;
            };
            if self.assets.ensure_loaded(&name).is_err() {
                continue;
            }
            let Some(assets) = self.assets.assets(&name) else {
                continue;
            };
            for entry in &assets.dat_entries {
                let path = match entry.location {
                    TextureLocation::Shared { page } => {
                        mint_sage_apt::shared_atlas_path(&name, page)
                    }
                    TextureLocation::Standalone { .. } => standalone_texture_path(&name, entry.id),
                };
                let base = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let key = (name.clone(), base);
                if !self.texture_cache.contains_key(&key) && seen.insert(key.clone()) {
                    keys.push(key);
                }
            }
        }
        let assets = &self.assets;
        let decoded = mint_pool::map_ordered(&keys, |(owner, base)| {
            movie_texture(assets.assets(owner)?.tga.get(base)?)
        });
        let (mut count, mut bytes) = (0usize, 0usize);
        for (key, data) in keys.into_iter().zip(decoded) {
            let handle = data.and_then(|data| {
                let handle = renderer.upload_texture(&data).ok()?;
                count = count.saturating_add(1);
                bytes = bytes.saturating_add(data.levels.iter().map(|l| l.data.len()).sum());
                Some(handle)
            });
            self.texture_cache.insert(key, handle);
        }
        (count, bytes)
    }

    fn import_closure(&self) -> Vec<MovieId> {
        let mut order = vec![self.movie_id];
        let mut next = 0usize;
        while let Some(&id) = order.get(next) {
            next = next.saturating_add(1);
            let Some(movie) = self.library.movie(id) else {
                continue;
            };
            for import in &movie.movie.imports {
                if let Some(target) = self.library.movie_id(&import.movie) {
                    if !order.contains(&target) {
                        order.push(target);
                    }
                }
            }
        }
        order
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
        let wrap = u8::from(*multiline != 0) | (u8::from(*wordwrap != 0) << 1);
        let raw = self.player.text(instance).unwrap_or_default();
        if raw.is_empty() {
            self.stats.text_empty = self.stats.text_empty.saturating_add(1);
            return false;
        }
        let text = crate::localise_with(&raw, &self.dynamic_strings, &self.strings);
        let style = crate::text_style(&raw);
        let font_file = self.library.resolve(character.movie, *font).and_then(|f| {
            match self.library.character(f) {
                Some(Character::Font { name, .. }) => name.as_deref().and_then(resolved_font_file),
                _ => None,
            }
        });
        if font_file.is_none() {
            self.stats.text_font_unresolved = self.stats.text_font_unresolved.saturating_add(1);
        }
        let x_scale = (world.0[0][0] * world.0[0][0] + world.0[1][0] * world.0[1][0]).sqrt();
        let scale = (world.0[0][1] * world.0[0][1] + world.0[1][1] * world.0[1][1]).sqrt();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let size_px = (font_height * scale).round().clamp(1.0, 512.0) as u32;
        let anchor = self.text_anchor.for_field(*multiline != 0, *wordwrap != 0);
        let key = TextKey {
            text,
            font: font_file,
            size_px,
            bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom].map(f32::to_bits),
            alignment: *alignment,
            anchor,
            scale: scale.to_bits(),
            x_scale: x_scale.to_bits(),
            wrap,
        };
        let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture) else {
            return false;
        };
        let fonts = &self.fonts;
        let busy = &mut self.atlas_busy;
        let box_bounds = *bounds;
        let (fh, align) = (*font_height, *alignment);
        let layout: &TextLayout = self.texts.get_or_layout(&key, |key| {
            let loaded = key.font.and_then(|f| fonts.get(f));
            let inverse = |s: f32| if s.abs() < f32::EPSILON { 1.0 } else { 1.0 / s };
            let to_local = inverse(scale);
            let measure = |s: &str| mint_ui::measure_text(s, loaded, size_px) * inverse(x_scale);
            let lines = crate::edit_text_lines(
                &key.text,
                key.wrap & 1 != 0,
                box_bounds.right - box_bounds.left - 2.0 * crate::TEXT_GUTTER,
                measure,
            );
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
            let line_height = win_ascent + win_descent;
            let origin = |line: usize, text: &str| {
                crate::line_origin(
                    TextBox {
                        bounds: box_bounds,
                        alignment: align,
                        text_width: measure(text),
                        ascent,
                        descent,
                        win_ascent,
                        win_descent,
                    },
                    key.anchor,
                    line,
                    lines.len(),
                    line_height,
                )
            };
            let pen = origin(0, lines.first().map_or("", String::as_str));
            let mut quads = Vec::new();
            for (line, text) in lines.iter().enumerate() {
                let [x, y] = origin(line, text);
                let offset = [(x - pen[0]) * x_scale, (y - pen[1]) * scale];
                quads.extend(
                    mint_ui::layout_text(
                        text,
                        [0.0, 0.0],
                        WHITE,
                        FontId(font_id_for(fonts, key.font)),
                        loaded,
                        atlas,
                        size_px,
                        texture,
                    )
                    .into_iter()
                    .map(|mut q| {
                        q.translation[0] += offset[0];
                        q.translation[1] += offset[1];
                        q
                    }),
                );
            }
            (
                TextLayout { pen, quads },
                note_rasterised(atlas, busy, atlas_dirty),
            )
        });
        let pen = apply(world, layout.pen);
        let authored = crate::apply_text_color(
            unpack_edittext_color(*color),
            self.player.own_value(instance, "textColor").as_ref(),
        );
        let text_color = multiply_color(cx.mult, authored);
        if style.drop_shadow {
            let offset = DROP_SHADOW_OFFSET;
            let shadow_color = multiply_color(cx.mult, [0.0, 0.0, 0.0, 1.0]);
            out.extend(layout.quads.iter().map(|q| {
                let mut q = *q;
                q.translation[0] += pen[0] + offset;
                q.translation[1] += pen[1] + offset;
                q.color_multiply = shadow_color;
                q.color_add = cx.add;
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
        self.layout_text_at(
            text,
            Some("albertusmt.otf"),
            LABEL_PX,
            at,
            color,
            clip,
            out,
            atlas_dirty,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_text_at(
        &mut self,
        text: &str,
        font: Option<&'static str>,
        size_px: u32,
        at: [f32; 2],
        color: [f32; 4],
        clip: [f32; 4],
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let key = TextKey {
            text: text.to_owned(),
            font,
            size_px,
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
        let fonts = &self.fonts;
        let busy = &mut self.atlas_busy;
        let layout = self.texts.get_or_layout(&key, |key| {
            let quads = mint_ui::layout_text(
                &key.text,
                [0.0, 0.0],
                WHITE,
                FontId(font_id_for(fonts, key.font)),
                key.font.and_then(|f| fonts.get(f)),
                atlas,
                key.size_px,
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

    pub fn build(&mut self, renderer: &mut Renderer) {
        let mut clock = std::time::Instant::now();
        let stretch = self.letterbox();
        let anchors = if self.scale_mode == ScaleMode::Anchored {
            self.anchors()
        } else {
            HashMap::new()
        };
        self.player
            .render_with(&mut self.list, RenderOptions { surfaces: true });
        self.stats = OverlayStats::default();
        self.placed_gadgets.clear();
        let player = &self.player;
        self.lists.retain(|id, _| player.instance(*id).is_some());
        self.checks.retain(|id, _| player.instance(*id).is_some());
        self.sliders.retain(|id, _| player.instance(*id).is_some());
        self.combos.retain(|id, _| player.instance(*id).is_some());
        self.pending_popup = None;
        self.popup = None;
        self.list_layouts
            .retain(|id, _| player.instance(*id).is_some());
        self.begin_text_frame();
        let mut profile = BuildProfile {
            render_list: lap(&mut clock),
            ..BuildProfile::default()
        };

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
        profile.meshes = lap(&mut clock);
        {
            let library = &self.library;
            let player = &self.player;
            let scale_mode = self.scale_mode;
            let letterbox_of = |instance: Option<InstanceId>| match (scale_mode, instance) {
                (ScaleMode::Stretch, _) | (ScaleMode::Anchored, None) => stretch,
                (ScaleMode::Anchored, Some(id)) => {
                    let top = crate::top_level_ancestor(player, id);
                    anchors.get(&top).copied().unwrap_or(stretch)
                }
            };
            plan(
                &self.list.commands,
                &letterbox_of,
                MaskMode::Stencil,
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
        profile.plan = lap(&mut clock);

        let mut ops: Vec<Op> = Vec::with_capacity(self.plan.items.len());
        let mut meshes: Vec<Arc<ShapeMesh>> = Vec::new();
        let mut mesh_index: HashMap<CharacterRef, usize> = HashMap::new();
        let mut lines: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut quads: Vec<UiQuad> = Vec::new();
        let mut atlas_dirty = false;
        let mut shapes_time = Duration::ZERO;
        let mut text_time = Duration::ZERO;
        let mut surfaces_time = Duration::ZERO;
        let items = std::mem::take(&mut self.plan.items);
        for item in &items {
            let item_started = std::time::Instant::now();
            match *item {
                Item::Shape {
                    character,
                    world,
                    color,
                    clip,
                } => {
                    let Some(mesh) = frame_meshes.get(&character).cloned() else {
                        shapes_time += item_started.elapsed();
                        continue;
                    };
                    let index = *mesh_index.entry(character).or_insert_with(|| {
                        meshes.push(Arc::clone(&mesh));
                        meshes.len() - 1
                    });
                    let size = self.window_size;
                    if !mesh
                        .bounds
                        .is_some_and(|b| on_screen(&world, b, clip, size))
                    {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        shapes_time += item_started.elapsed();
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
                        let run_color = multiply_color(run.color, color.mult);
                        if !can_show(run_color, color.add) {
                            self.stats.culled = self.stats.culled.saturating_add(1);
                            continue;
                        }
                        self.stats.runs = self.stats.runs.saturating_add(1);
                        ops.push(Op::Run {
                            mesh: index,
                            run: run_index,
                            world,
                            color: run_color,
                            add: color.add,
                            clip,
                            texture,
                        });
                    }
                    shapes_time += item_started.elapsed();
                }
                Item::Text {
                    instance,
                    character,
                    world,
                    color,
                    clip,
                } => {
                    if !can_show(color.mult, color.add) {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        text_time += item_started.elapsed();
                        continue;
                    }
                    let start = quads.len();
                    if self.layout_field(
                        instance,
                        character,
                        &world,
                        color,
                        clip,
                        &mut quads,
                        &mut atlas_dirty,
                    ) {
                        ops.push(Op::Quads(start..quads.len()));
                    }
                    text_time += item_started.elapsed();
                }
                Item::Surface {
                    instance,
                    bounds,
                    world,
                    color,
                    clip,
                } => {
                    let colorless = matches!(
                        self.surface_icons.get(&instance),
                        Some(SurfaceContent::Icon(SurfaceIcon {
                            ignores_color_transform: true,
                            ..
                        }))
                    );
                    if !on_screen(&world, bounds, clip, self.window_size)
                        || (!colorless && !can_show(color.mult, color.add))
                    {
                        self.stats.culled = self.stats.culled.saturating_add(1);
                        surfaces_time += item_started.elapsed();
                        continue;
                    }
                    self.stats.surfaces = self.stats.surfaces.saturating_add(1);
                    if self.player.own_string(instance, "_type").as_deref() == Some("ColorPicker") {
                        if let Some([l, t, r, b]) = self.stage_rect(instance) {
                            self.color_pickers
                                .seen(&mut self.player, instance, [r - l, b - t]);
                        }
                    }
                    let content = self.surface_content(instance);
                    if content.is_none() {
                        self.note_wanted_image(instance);
                    }
                    if let Some(
                        gadget @ (SurfaceContent::List(_)
                        | SurfaceContent::Check(_)
                        | SurfaceContent::Slider(_)
                        | SurfaceContent::Combo(_)
                        | SurfaceContent::Entry(_)
                        | SurfaceContent::MapWindow(_)
                        | SurfaceContent::Graph(_)
                        | SurfaceContent::Text(_)
                        | SurfaceContent::Placed(_)),
                    ) = &content
                    {
                        let start = quads.len();
                        let letterbox = match self.scale_mode {
                            ScaleMode::Stretch => stretch,
                            ScaleMode::Anchored => anchors
                                .get(&crate::top_level_ancestor(&self.player, instance))
                                .copied()
                                .unwrap_or(stretch),
                        };
                        let place = GadgetPlace {
                            instance,
                            local: bounds,
                            letterbox,
                            color,
                            clip,
                        };
                        match gadget {
                            SurfaceContent::List(list) => {
                                self.draw_list(
                                    place,
                                    Arc::clone(list),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Check(check) => {
                                self.draw_check(
                                    place,
                                    Arc::clone(check),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Slider(slider) => {
                                self.draw_slider(place, *slider, &mut quads);
                            }
                            SurfaceContent::Combo(combo) => {
                                self.draw_combo_box(
                                    place,
                                    Arc::clone(combo),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::MapWindow(map) => {
                                self.draw_map_window(
                                    place,
                                    Arc::clone(map),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Graph(graph) => {
                                self.draw_graph(place, graph, &mut quads);
                            }
                            SurfaceContent::Text(text) => {
                                self.draw_text_box(
                                    place,
                                    Arc::clone(text),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Placed(text) => {
                                self.draw_placed_text(
                                    &place,
                                    &Arc::clone(text),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Entry(entry) => {
                                self.draw_text_entry(
                                    place,
                                    Arc::clone(entry),
                                    &mut quads,
                                    &mut atlas_dirty,
                                );
                            }
                            SurfaceContent::Icon(_)
                            | SurfaceContent::Progress(_)
                            | SurfaceContent::Hidden => {}
                        }
                        ops.push(Op::Quads(start..quads.len()));
                        surfaces_time += item_started.elapsed();
                        continue;
                    }
                    if let Some(SurfaceContent::Hidden) = content {
                        surfaces_time += item_started.elapsed();
                        continue;
                    }
                    if let Some(SurfaceContent::Icon(icon)) = content {
                        let [l, t, r, b] = bounds;
                        let (w, h) = (r - l, b - t);
                        let keep_aspect = self.scale_mode == ScaleMode::Anchored
                            && self
                                .player
                                .own_value(instance, "_KeepAspectRatio")
                                .is_some_and(|v| v.to_bool());
                        let keep_aspect = keep_aspect
                            .then(|| stage_space_aspect(&self.player, instance))
                            .flatten();
                        let (affine, translation) = match keep_aspect {
                            Some(aspect) if w > 0.0 && h > 0.0 => {
                                contain_quad(&world, [l, t, r, b], aspect)
                            }
                            _ => {
                                let (m, _tr) = world;
                                (
                                    [[m[0][0] * w, m[0][1] * h], [m[1][0] * w, m[1][1] * h]],
                                    apply(&world, [l, t]),
                                )
                            }
                        };
                        let surface_type = self.player.own_string(instance, "_type");
                        let mode = self.player.own_string(instance, "_mode");
                        let disc = self.disc_pictures && self.image_modes.disc(mode.as_deref());
                        let (mult, add) = if icon.ignores_color_transform {
                            (icon.tint, [0.0; 4])
                        } else {
                            (multiply_color(color.mult, icon.tint), color.add)
                        };
                        let quad = UiQuad {
                            affine,
                            translation,
                            texture: Some(icon.texture),
                            uv_rect: icon.uv_rect,
                            color_multiply: mult,
                            color_add: add,
                            clip,
                            premultiplied: icon.premultiplied,
                            uv_rotated_cw: icon.rotated,
                            greyscale: draws_grey(surface_type.as_deref(), mode.as_deref()),
                            opaque: self
                                .image_modes
                                .opaque(surface_type.as_deref(), mode.as_deref())
                                || disc,
                            additive: self
                                .image_modes
                                .additive(surface_type.as_deref(), mode.as_deref()),
                            disc,
                            wedge: 1.0,
                            hole: 0.0,
                        };
                        let start = quads.len();
                        quads.push(quad);
                        ops.push(Op::Quads(start..quads.len()));
                        surfaces_time += item_started.elapsed();
                        continue;
                    }
                    if let Some(SurfaceContent::Progress(clock)) = content {
                        let [l, t, r, b] = bounds;
                        let (w, h) = (r - l, b - t);
                        let (m, _tr) = world;
                        let affine = [[m[0][0] * w, m[0][1] * h], [m[1][0] * w, m[1][1] * h]];
                        let translation = apply(&world, [l, t]);
                        let wedge = if self.timer_overlay_covers_remaining {
                            clock.covered
                        } else {
                            1.0 - clock.covered
                        };
                        let layer = |icon: Option<SurfaceIcon>, tint: [f32; 4]| UiQuad {
                            affine,
                            translation,
                            texture: icon.map(|i| i.texture),
                            uv_rect: icon.map_or([0.0, 0.0, 1.0, 1.0], |i| i.uv_rect),
                            color_multiply: multiply_color(color.mult, tint),
                            color_add: color.add,
                            clip,
                            premultiplied: false,
                            greyscale: false,
                            opaque: false,
                            additive: false,
                            disc: false,
                            wedge,
                            hole: 0.0,
                            uv_rotated_cw: icon.is_some_and(|i| i.rotated),
                        };
                        let fill = layer(clock.fill, clock.tint);
                        let rim = match clock.ring {
                            Some(_) => layer(clock.ring, clock.rim),
                            None => UiQuad {
                                disc: true,
                                hole: TIMER_RIM_HOLE,
                                ..layer(None, clock.rim)
                            },
                        };
                        let start = quads.len();
                        quads.push(fill);
                        quads.push(rim);
                        ops.push(Op::Quads(start..quads.len()));
                        surfaces_time += item_started.elapsed();
                        continue;
                    }
                    self.annotate_surface(
                        instance,
                        bounds,
                        world,
                        color,
                        clip,
                        &mut lines,
                        &mut ops,
                        &mut quads,
                        &mut atlas_dirty,
                    );
                    surfaces_time += item_started.elapsed();
                }
                Item::PushMask(i) => ops.push(Op::Push(i)),
                Item::PopMask(i) => ops.push(Op::Pop(i)),
            }
        }
        self.plan.items = items;
        if let Some(popup) = self.pending_popup.take() {
            let start = quads.len();
            self.draw_popup(popup, &mut quads, &mut atlas_dirty);
            ops.push(Op::Quads(start..quads.len()));
        }
        let mut warmed = Vec::new();
        let mut again = Vec::new();
        if let Some(atlas) = self.text_atlas.as_mut() {
            let fonts = &self.fonts;
            let runs: Vec<_> = self
                .text_warm
                .iter()
                .filter_map(|(text, font, size_px)| {
                    let loaded = font.and_then(|f| fonts.get(f))?;
                    Some((
                        FontId(font_id_for(fonts, *font)),
                        loaded,
                        *size_px,
                        text.as_str(),
                    ))
                })
                .collect();
            atlas.prerasterize(&runs);
        }
        for (text, font, size_px) in std::mem::take(&mut self.text_warm) {
            self.layout_text_at(
                &text,
                font,
                size_px,
                [0.0, 0.0],
                WHITE,
                UiQuad::UNCLIPPED,
                &mut warmed,
                &mut atlas_dirty,
            );
            warmed.clear();
            if self.atlas_busy {
                again.push((text, font, size_px));
            }
        }
        self.text_warm = again;
        profile.shapes = shapes_time;
        profile.text_fields = text_time;
        profile.surfaces = surfaces_time;
        clock = std::time::Instant::now();

        if let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture.as_mut())
        {
            atlas_dirty |= atlas.take_dirty();
            if atlas_dirty {
                if let Some(page) =
                    mint_ui::sync_atlas_texture(atlas, &mut self.atlas_page, renderer)
                {
                    *texture = page;
                    self.known_atlas_textures.insert(page);
                }
                let spare = atlas.capacity().saturating_sub(atlas.len());
                self.atlas_near_full = spare < GLYPHS_PER_FRAME;
            }
            let current = *texture;
            for q in &mut quads {
                if q.texture
                    .is_some_and(|t| self.known_atlas_textures.contains(&t))
                {
                    q.texture = Some(current);
                }
            }
        }

        profile.atlas_upload = lap(&mut clock);
        self.last_build = profile;

        self.ops = ops;
        self.frame_meshes = meshes;
        self.lines = lines;
        self.quads = quads;
    }

    #[allow(clippy::too_many_arguments)]
    fn annotate_surface(
        &mut self,
        instance: InstanceId,
        bounds: [f32; 4],
        world: Affine,
        color: ColorTransform,
        clip: [f32; 4],
        lines: &mut Vec<Vec<[f32; 2]>>,
        ops: &mut Vec<Op>,
        quads: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        if !self.annotate_surfaces {
            return;
        }
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
            clip,
        });
        let alpha = color.mult[3].clamp(0.0, 1.0);
        let corner = apply(&world, [bounds[0], bounds[1]]);
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
            quads,
            atlas_dirty,
        );
        ops.push(Op::Quads(start..quads.len()));
    }

    #[must_use]
    pub fn text_settled(&self) -> bool {
        self.texts.is_settled()
    }

    pub fn build_settled(&mut self, renderer: &mut Renderer, max_passes: u32) -> u32 {
        let mut passes = 0u32;
        loop {
            self.build(renderer);
            passes = passes.saturating_add(1);
            if self.text_settled() || passes >= max_passes {
                break;
            }
        }
        if !self.text_settled() {
            eprintln!(
                "apt viewer: text still not settled after {max_passes} pass(es) - the \
                 captured frame may show incomplete text (a heavier screen than this \
                 bound anticipated)"
            );
        }
        passes
    }

    #[must_use]
    pub fn slot_rects(&mut self, ids: &[InstanceId]) -> HashMap<InstanceId, [f32; 4]> {
        let stretch = self.letterbox();
        let anchors = match self.scale_mode {
            ScaleMode::Stretch => HashMap::new(),
            ScaleMode::Anchored => self.anchors(),
        };
        let mut out = HashMap::with_capacity(ids.len());
        for &id in ids {
            let (Some(bounds), Some(world)) =
                (self.player.local_bounds(id), self.player.world_matrix(id))
            else {
                continue;
            };
            let lb = match self.scale_mode {
                ScaleMode::Stretch => stretch,
                ScaleMode::Anchored => {
                    let top = crate::top_level_ancestor(&self.player, id);
                    anchors.get(&top).copied().unwrap_or(stretch)
                }
            };
            let corners = [
                world.apply(bounds[0], bounds[1]),
                world.apply(bounds[2], bounds[1]),
                world.apply(bounds[0], bounds[3]),
                world.apply(bounds[2], bounds[3]),
            ];
            let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in corners {
                let (wx, wy) = (
                    x * lb.scale[0] + lb.offset[0],
                    y * lb.scale[1] + lb.offset[1],
                );
                l = l.min(wx);
                t = t.min(wy);
                r = r.max(wx);
                b = b.max(wy);
            }
            out.insert(id, [l, t, r, b]);
        }
        out
    }

    #[must_use]
    pub fn slot_rect(&mut self, id: InstanceId) -> Option<[f32; 4]> {
        self.slot_rects(std::slice::from_ref(&id)).remove(&id)
    }

    pub fn surface_instances(&mut self) -> Vec<InstanceId> {
        self.player
            .render_with(&mut self.list, RenderOptions { surfaces: true });
        self.list
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::Surface { instance, .. } => Some(*instance),
                _ => None,
            })
            .collect()
    }

    #[must_use]
    pub fn surface_with_key(&self, ids: &[InstanceId], key: &str) -> Option<InstanceId> {
        ids.iter()
            .copied()
            .find(|&id| self.gadget_key(id).as_deref() == Some(key))
    }

    pub fn draw_label(
        &mut self,
        renderer: &mut Renderer,
        text: &str,
        at: [f32; 2],
        color: [f32; 4],
        clip: [f32; 4],
    ) -> Vec<UiQuad> {
        let mut out = Vec::new();
        let mut atlas_dirty = false;
        self.layout_label(text, at, color, clip, &mut out, &mut atlas_dirty);
        self.upload_label_atlas(renderer, atlas_dirty, &mut out);
        out
    }

    pub fn begin_text_frame(&mut self) {
        self.texts.begin_frame();
        self.atlas_busy = false;
        if let Some(atlas) = self.text_atlas.as_mut() {
            atlas.begin_frame();
        }
        if self.atlas_near_full {
            self.texts.clear();
        }
    }

    pub fn draw_text_rect(
        &mut self,
        renderer: &mut Renderer,
        content: &TextBoxContent,
        rect: [f32; 4],
    ) -> Vec<UiQuad> {
        let fonts = Arc::clone(&self.fonts);
        let (rows, _) = lay_out_text_box(
            &content.lines,
            content.align,
            rect[2] - rect[0],
            [1.0, 1.0],
            [self.window_size.0, self.window_size.1],
            &fonts,
        );
        let mut out = Vec::new();
        let mut atlas_dirty = false;
        for row in rows {
            self.layout_text_at(
                &row.text,
                row.font,
                row.size_px,
                [rect[0] + row.x, rect[1] + row.baseline],
                row.color,
                UiQuad::UNCLIPPED,
                &mut out,
                &mut atlas_dirty,
            );
        }
        self.upload_label_atlas(renderer, atlas_dirty, &mut out);
        out
    }

    fn upload_label_atlas(&mut self, renderer: &mut Renderer, dirty: bool, out: &mut [UiQuad]) {
        if !dirty {
            return;
        }
        if let (Some(atlas), Some(texture)) = (self.text_atlas.as_mut(), self.text_texture.as_mut())
        {
            if let Some(page) = mint_ui::sync_atlas_texture(atlas, &mut self.atlas_page, renderer) {
                *texture = page;
                self.known_atlas_textures.insert(page);
            }
            let current = *texture;
            for q in out {
                if q.texture.is_some() {
                    q.texture = Some(current);
                }
            }
        }
    }

    fn note_wanted_image(&mut self, instance: InstanceId) {
        let Some(name) = self
            .player
            .own_string(instance, "_imageName")
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        if self.asked_images.insert(name.to_string()) {
            self.wanted_images.push(name.to_string());
        }
    }

    pub fn take_wanted_images(&mut self) -> Vec<String> {
        std::mem::take(&mut self.wanted_images)
    }

    pub fn service_color_pickers(
        &mut self,
        palette: &mut dyn FnMut(&str) -> Option<Arc<crate::color_picker::Palette>>,
    ) {
        self.color_pickers.service(&mut self.player, palette);
    }

    pub fn take_wanted_looks(&mut self) -> Vec<(GadgetKind, String)> {
        std::mem::take(&mut self.wanted_looks)
    }

    pub fn set_list_style(&mut self, load: &str, style: Option<Arc<ListStyle>>) {
        self.list_styles.insert(load.to_owned(), style);
    }

    pub fn set_check_style(&mut self, load: &str, style: Option<Arc<CheckStyle>>) {
        self.check_styles.insert(load.to_owned(), style);
    }

    pub fn set_slider_style(&mut self, load: &str, style: Option<Arc<SliderStyle>>) {
        self.slider_styles.insert(load.to_owned(), style);
    }

    pub fn set_entry_style(&mut self, load: &str, style: Option<Arc<EntryLook>>) {
        self.entry_styles.insert(load.to_owned(), style);
    }

    pub fn set_combo_style(&mut self, load: &str, style: Option<Arc<ComboStyle>>) {
        self.combo_styles.insert(load.to_owned(), style);
    }

    pub fn set_map_window_style(&mut self, load: &str, style: Option<Arc<MapWindowStyle>>) {
        self.map_window_styles.insert(load.to_owned(), style);
    }

    #[must_use]
    pub fn combo_open(&self) -> bool {
        self.open_combo().is_some()
    }

    fn open_combo(&self) -> Option<InstanceId> {
        self.combos
            .iter()
            .find(|(_, view)| view.open)
            .map(|(id, _)| *id)
    }

    #[must_use]
    pub fn gadget_thumb_drag_diag(&self) -> Option<f32> {
        let open_list = self.combos.values().find_map(|view| view.list.as_ref());
        self.lists
            .values()
            .chain(open_list)
            .find(|view| view.thumb_grab.is_some())
            .map(|view| view.display_pos)
    }

    #[must_use]
    pub fn map_window_hover(&self, key: &str) -> Option<usize> {
        self.map_windows
            .iter()
            .filter(|(instance, _)| self.gadget_key(**instance).as_deref() == Some(key))
            .find_map(|(_, view)| view.hover)
    }

    #[must_use]
    pub fn gadget_hover_row_diag(&self) -> Option<usize> {
        self.combos
            .values()
            .find_map(|view| view.list.as_ref())
            .filter(|list| list.tracking)
            .and_then(|list| list.hover_row)
    }

    fn gadget_local(&self, instance: InstanceId, p: [f32; 2]) -> Option<[f32; 2]> {
        self.placed_gadgets
            .iter()
            .find(|g| g.instance == instance)
            .and_then(|g| invert(&g.world, p))
    }

    fn want(&mut self, kind: GadgetKind, load: String) {
        if !self
            .wanted_looks
            .iter()
            .any(|(k, l)| *k == kind && *l == load)
        {
            self.wanted_looks.push((kind, load));
        }
    }

    fn load_of(&self, instance: InstanceId) -> String {
        self.player
            .own_string(instance, "_Load")
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn gadget_under(&self, p: [f32; 2]) -> Option<(InstanceId, GadgetKind, [f32; 2])> {
        if let Some(popup) = &self.popup {
            if inside_rect(popup.clip, p) {
                return invert(&popup.world, p)
                    .map(|local| (popup.instance, GadgetKind::Combo, local));
            }
        }
        self.placed_gadgets.iter().rev().find_map(|placed| {
            let c = placed.clip;
            if !(p[0] >= c[0] && p[0] < c[2] && p[1] >= c[1] && p[1] < c[3]) {
                return None;
            }
            let local = invert(&placed.world, p)?;
            let hit = match placed.kind {
                GadgetKind::List => self.lists.get(&placed.instance)?.part_at(local).is_some(),
                GadgetKind::Check => self.checks.get(&placed.instance)?.contains(local),
                GadgetKind::Slider => self.sliders.get(&placed.instance)?.contains(local),
                GadgetKind::Combo => self.combos.get(&placed.instance)?.contains(local),
                GadgetKind::MapWindow => self
                    .map_windows
                    .get(&placed.instance)?
                    .spot_at(local)
                    .is_some(),
                GadgetKind::Entry => false,
            };
            hit.then_some((placed.instance, placed.kind, local))
        })
    }

    #[must_use]
    pub fn gadget_at(&self, p: [f32; 2]) -> bool {
        self.gadget_under(p).is_some()
    }

    pub fn gadget_pointer_moved(&mut self, p: [f32; 2], held: bool) -> Option<GadgetEvent> {
        let mut moved = None;
        if held {
            let dragging: Vec<InstanceId> = self
                .sliders
                .iter()
                .filter(|(_, view)| view.dragging)
                .map(|(id, _)| *id)
                .collect();
            for id in dragging {
                let local = self
                    .placed_gadgets
                    .iter()
                    .find(|g| g.instance == id)
                    .and_then(|g| invert(&g.world, p));
                let value = local.and_then(|local| self.sliders.get_mut(&id)?.drag(local));
                if let Some(value) = value {
                    moved = Some((id, value));
                }
            }
            let dragging_lists: Vec<InstanceId> = self
                .lists
                .iter()
                .filter(|(_, view)| view.thumb_grab.is_some())
                .map(|(id, _)| *id)
                .collect();
            for id in dragging_lists {
                let local = self
                    .placed_gadgets
                    .iter()
                    .find(|g| g.instance == id)
                    .and_then(|g| invert(&g.world, p));
                if let Some(local) = local {
                    if let Some(view) = self.lists.get_mut(&id) {
                        view.drag_thumb(local);
                    }
                }
            }
            if let Some(instance) = self.open_combo() {
                let local = self.gadget_local(instance, p);
                if let Some(local) = local {
                    if let Some(view) = self.combos.get_mut(&instance) {
                        if let Some(list) = &mut view.list {
                            if list.thumb_grab.is_some() {
                                list.drag_thumb(local);
                            }
                        }
                    }
                }
            }
        }
        let under = self.gadget_under(p);
        for view in self.lists.values_mut() {
            view.hover = None;
        }
        for view in self.checks.values_mut() {
            view.hover = false;
        }
        for view in self.sliders.values_mut() {
            view.hover = false;
        }
        for view in self.combos.values_mut() {
            view.hover = false;
            if let Some(list) = &mut view.list {
                list.hover = None;
            }
        }
        for view in self.map_windows.values_mut() {
            view.hover = None;
        }
        if let Some((instance, kind, local)) = under {
            match kind {
                GadgetKind::List => {
                    if let Some(view) = self.lists.get_mut(&instance) {
                        view.hover = view.part_at(local);
                    }
                }
                GadgetKind::Check => {
                    if let Some(view) = self.checks.get_mut(&instance) {
                        view.hover = true;
                    }
                }
                GadgetKind::Slider => {
                    if let Some(view) = self.sliders.get_mut(&instance) {
                        view.hover = true;
                    }
                }
                GadgetKind::MapWindow => {
                    if let Some(view) = self.map_windows.get_mut(&instance) {
                        view.hover = view.spot_at(local);
                    }
                }
                GadgetKind::Combo => {
                    if let Some(view) = self.combos.get_mut(&instance) {
                        if view.list_contains(local) {
                            if let Some(list) = &mut view.list {
                                let part = list.part_at(local);
                                list.hover = part;
                                if list.tracking {
                                    match part {
                                        Some(ListPart::Row(row)) => list.hover_row = Some(row),
                                        Some(ListPart::Body) => list.hover_row = None,
                                        _ => {}
                                    }
                                }
                            }
                        } else {
                            view.hover = true;
                        }
                    }
                }
                GadgetKind::Entry => {}
            }
        }
        moved.map(|(instance, value)| GadgetEvent {
            instance,
            key: self.gadget_key(instance),
            change: Change::Value(value),
        })
    }

    pub fn gadget_pressed(&mut self, p: [f32; 2]) -> bool {
        if let Some(instance) = self.open_combo() {
            if let Some(local) = self.gadget_local(instance, p) {
                if let Some(view) = self.combos.get_mut(&instance) {
                    if view.list_contains(local) {
                        if let Some(list) = &mut view.list {
                            list.start_thumb_drag(local);
                        }
                    }
                }
            }
            return true;
        }
        let Some((instance, kind, local)) = self.gadget_under(p) else {
            return false;
        };
        if kind == GadgetKind::MapWindow {
            self.pressed_spot = self
                .map_windows
                .get(&instance)
                .and_then(|view| view.spot_at(local))
                .map(|spot| (instance, spot));
        }
        if kind == GadgetKind::List {
            if let Some(view) = self.lists.get_mut(&instance) {
                view.start_thumb_drag(local);
            }
        }
        if kind == GadgetKind::Slider {
            if let Some(view) = self.sliders.get_mut(&instance) {
                view.dragging = true;
            }
        }
        true
    }

    pub fn gadget_released(&mut self, p: [f32; 2]) -> GadgetClick {
        if let Some(instance) = self.open_combo() {
            return self.open_combo_released(instance, p);
        }
        self.pressed_spot = None;
        let dragged = self.sliders.values().any(|view| view.dragging)
            || self.lists.values().any(|view| view.thumb_grab.is_some());
        for view in self.sliders.values_mut() {
            view.dragging = false;
        }
        for view in self.lists.values_mut() {
            view.end_thumb_drag();
        }
        let Some((instance, kind, local)) = self.gadget_under(p) else {
            return if dragged {
                GadgetClick::Taken
            } else {
                GadgetClick::Missed
            };
        };
        let change = match kind {
            GadgetKind::List if dragged => None,
            GadgetKind::List => self
                .lists
                .get_mut(&instance)
                .and_then(|view| view.click(local))
                .map(Change::Row),
            GadgetKind::Check => self
                .checks
                .get_mut(&instance)
                .and_then(|view| view.click(local))
                .map(Change::Checked),
            GadgetKind::Slider if dragged => None,
            GadgetKind::Slider => self
                .sliders
                .get_mut(&instance)
                .and_then(|view| view.click(local))
                .map(Change::Value),
            GadgetKind::Combo => {
                if let Some(view) = self.combos.get_mut(&instance) {
                    if view.contains(local) {
                        view.toggle();
                    }
                }
                None
            }
            GadgetKind::MapWindow => self
                .map_windows
                .get(&instance)
                .and_then(|view| view.spot_at(local))
                .map(Change::Spot),
            GadgetKind::Entry => None,
        };
        match change {
            Some(change) => GadgetClick::Changed(GadgetEvent {
                instance,
                key: self.gadget_key(instance),
                change,
            }),
            None => GadgetClick::Taken,
        }
    }

    pub fn gadget_right_released(&mut self, p: [f32; 2]) -> GadgetClick {
        if self.combo_open() {
            return GadgetClick::Taken;
        }
        let Some((instance, GadgetKind::MapWindow, local)) = self.gadget_under(p) else {
            return GadgetClick::Missed;
        };
        match self
            .map_windows
            .get(&instance)
            .and_then(|view| view.spot_at(local))
        {
            Some(spot) => GadgetClick::Changed(GadgetEvent {
                instance,
                key: self.gadget_key(instance),
                change: Change::SpotCleared(spot),
            }),
            None => GadgetClick::Missed,
        }
    }

    fn open_combo_released(&mut self, instance: InstanceId, p: [f32; 2]) -> GadgetClick {
        let local = self.gadget_local(instance, p);
        let Some(view) = self.combos.get_mut(&instance) else {
            return GadgetClick::Taken;
        };
        let was_dragging_thumb = view.list.as_ref().is_some_and(|l| l.thumb_grab.is_some());
        if let Some(list) = &mut view.list {
            list.end_thumb_drag();
        }
        if was_dragging_thumb {
            return GadgetClick::Taken;
        }
        let Some(local) = local.filter(|l| view.list_contains(*l)) else {
            view.close();
            return GadgetClick::Taken;
        };
        let part = view.list.as_ref().and_then(|list| list.part_at(local));
        let chosen = match part {
            Some(ListPart::Row(row)) => view.choose(row),
            _ => {
                if let Some(list) = &mut view.list {
                    let _ = list.click(local);
                }
                None
            }
        };
        match chosen {
            Some(row) => GadgetClick::Changed(GadgetEvent {
                instance,
                key: self.gadget_key(instance),
                change: Change::Row(row),
            }),
            None => GadgetClick::Taken,
        }
    }

    pub fn gadget_nav(&mut self, nav: Nav) -> GadgetClick {
        let Some(instance) = self.open_combo() else {
            return GadgetClick::Missed;
        };
        let row = self
            .combos
            .get_mut(&instance)
            .and_then(|view| view.key(nav));
        match row {
            Some(row) => GadgetClick::Changed(GadgetEvent {
                instance,
                key: self.gadget_key(instance),
                change: Change::Row(row),
            }),
            None => GadgetClick::Taken,
        }
    }

    pub fn list_scrolled(&mut self, p: [f32; 2], lines: f32) -> bool {
        if let Some(instance) = self.open_combo() {
            let local = self.gadget_local(instance, p);
            if let Some(view) = self.combos.get_mut(&instance) {
                let over = local.is_some_and(|l| view.list_contains(l));
                if let (true, Some(list)) = (over, view.list.as_mut()) {
                    #[allow(clippy::cast_possible_truncation)]
                    let rows = (-lines).round().clamp(-100.0, 100.0) as i32;
                    list.scroll(rows);
                }
            }
            return true;
        }
        let Some((instance, GadgetKind::List, _)) = self.gadget_under(p) else {
            return false;
        };
        if let Some(view) = self.lists.get_mut(&instance) {
            #[allow(clippy::cast_possible_truncation)]
            let rows = (-lines).round().clamp(-100.0, 100.0) as i32;
            view.scroll(rows);
        }
        true
    }

    fn stage_frame(&self, place: &GadgetPlace) -> ([f32; 4], Affine) {
        let local = place.local;
        let bounds = self.player.world_matrix(place.instance).map_or(local, |m| {
            let corners = [
                m.apply(local[0], local[1]),
                m.apply(local[2], local[1]),
                m.apply(local[0], local[3]),
                m.apply(local[2], local[3]),
            ];
            corners
                .iter()
                .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, &(x, y)| {
                    [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]
                })
        });
        let lb = place.letterbox;
        (
            bounds,
            ([[lb.scale[0], 0.0], [0.0, lb.scale[1]]], lb.offset),
        )
    }

    fn gadget_key(&self, instance: InstanceId) -> Option<String> {
        gadget_key_of(&self.player, instance)
    }

    fn draw_list(
        &mut self,
        place: GadgetPlace,
        content: Arc<ListContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let GadgetPlace {
            instance,
            letterbox,
            color,
            clip,
            ..
        } = place;
        let load = self.load_of(instance);
        let style = match self.list_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_list),
            None => {
                self.want(GadgetKind::List, load);
                return;
            }
        };
        let (bounds, stage) = self.stage_frame(&place);
        let mut view = self
            .lists
            .remove(&instance)
            .unwrap_or_else(|| ListView::new(Arc::clone(&content)));
        view.set_content(content);
        let paint = ListPaint {
            layout_key: instance,
            bounds,
            world: stage,
            letterbox,
            color,
            clip,
        };
        self.paint_list(paint, &style, &mut view, out, atlas_dirty);
        self.placed_gadgets.push(PlacedGadget {
            instance,
            kind: GadgetKind::List,
            world: stage,
            clip: Painter {
                world: stage,
                color,
            }
            .clip(bounds, clip),
        });
        self.lists.insert(instance, view);
    }

    #[allow(clippy::too_many_lines)]
    fn paint_list(
        &mut self,
        paint: ListPaint,
        style: &ListStyle,
        view: &mut ListView,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let ListPaint {
            layout_key: instance,
            bounds,
            world: stage,
            letterbox,
            color,
            clip,
        } = paint;
        let world = &stage;
        let content = Arc::clone(&view.content);

        let x_scale = letterbox.scale[0].abs().max(f32::EPSILON);
        let y_scale = letterbox.scale[1].abs().max(f32::EPSILON);
        let font_file = resolved_font_file(&style.font);
        let fonts = Arc::clone(&self.fonts);
        let font = font_file.and_then(|f| fonts.get(f));
        let size_px = gadget_text_px(style.points, &letterbox);
        #[allow(clippy::cast_precision_loss)]
        let px = size_px as f32;
        let (ascent, line) = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or((px * 0.8, px * 1.2), |m| {
                (m.win_ascent, m.win_ascent + m.win_descent)
            });
        let (ascent, line) = (ascent / y_scale, line / y_scale);
        let measure = |s: &str| mint_ui::measure_text(s, font, size_px) / x_scale;
        let scroll_bar = content.scroll_bar.unwrap_or(style.scroll_bar);
        let frame = ListFrame::new(bounds, scroll_bar, &content.columns);

        let key = LaidKey {
            content: Arc::clone(&content),
            size_px,
            columns: frame
                .columns
                .iter()
                .map(|[a, b]| (b - a).to_bits())
                .collect(),
        };
        let (laid, heights) = match self.list_layouts.remove(&instance) {
            Some((old, laid, heights)) if old == key => (laid, heights),
            _ => lay_out_rows(&content.rows, &frame, line, content.text_inset, &measure),
        };
        let frame = if style.auto_hide && content.scroll_bar.is_none() {
            frame.hiding_bar_while_content_fits(&heights)
        } else {
            frame
        };
        view.laid_out(frame.clone(), heights.clone());

        let paint = Painter {
            world: *world,
            color,
        };
        let to_window = |r: [f32; 4]| paint.window(r);
        let tint = |c: [f32; 4]| multiply_color(color.mult, c);
        let lit = usize::from(view.hover.is_some());
        let solid = |out: &mut Vec<UiQuad>, r: [f32; 4], c: [f32; 4], clip: [f32; 4]| {
            paint.solid(out, r, c, clip);
        };
        let outline = |out: &mut Vec<UiQuad>, r: [f32; 4], c: [f32; 4], clip: [f32; 4]| {
            paint.outline(out, r, c, clip);
        };
        let picture = |out: &mut Vec<UiQuad>, r: [f32; 4], icon: SurfaceIcon, clip: [f32; 4]| {
            paint.picture(out, r, icon, clip);
        };
        let window_clip = |r: [f32; 4]| paint.clip(r, clip);

        let body = [bounds[0], bounds[1], bounds[2], bounds[3]];
        solid(
            out,
            [body[0] + 1.0, body[1] + 1.0, body[2] - 1.0, body[3] - 1.0],
            style.body.fill,
            clip,
        );
        outline(out, body, style.body.border, clip);

        let rows_clip = window_clip(frame.clip);
        let highlight_row = bar_row(view.tracking, view.hover_row, view.selected);
        let mut top = frame.rows[1] - view.display_pos;
        for (i, (h, cells)) in view.heights.iter().zip(&laid).enumerate() {
            let pitch = h + 1.0;
            if top > frame.clip[3] {
                break;
            }
            if top + pitch < frame.clip[1] {
                top += pitch;
                continue;
            }
            if highlight_row == Some(i) {
                let slot = style.selected.get(lit).copied().unwrap_or_default();
                let r = [frame.rows[0] + 1.0, top, frame.rows[2], top + pitch];
                match slot.image {
                    Some(icon) => picture(out, r, icon, rows_clip),
                    None => {
                        solid(out, r, slot.fill, rows_clip);
                        outline(out, r, slot.border, rows_clip);
                    }
                }
            }
            for (j, cell) in cells.iter().enumerate() {
                let Some(&[x0, x1]) = frame.columns.get(j) else {
                    break;
                };
                let cell_clip = {
                    let w = to_window([x0, top, x1, top + h]);
                    [
                        w[0].max(rows_clip[0]),
                        w[1].max(rows_clip[1]),
                        w[2].min(rows_clip[2]),
                        w[3].min(rows_clip[3]),
                    ]
                };
                match cell {
                    Laid::Lines(lines) => {
                        let align = content.columns.get(j).map_or(Align::Left, |c| c.align);
                        #[allow(clippy::cast_precision_loss)]
                        let first = text_top(top, *h, line * lines.len() as f32);
                        for (k, text) in lines.iter().enumerate() {
                            let x = match align {
                                Align::Left => x0 + TEXT_X_OFFSET + (x1 - x0) * content.text_inset,
                                Align::Centre => x0 + (x1 - x0 - measure(text)) * 0.5,
                            };
                            #[allow(clippy::cast_precision_loss)]
                            let baseline = first + line * k as f32 + ascent;
                            let pen = apply(world, [x, baseline]);
                            let shadow = [0.0, 0.0, 0.0, tint(style.text)[3]];
                            self.layout_text_at(
                                text,
                                font_file,
                                size_px,
                                [pen[0] + 1.0, pen[1] + 1.0],
                                shadow,
                                cell_clip,
                                out,
                                atlas_dirty,
                            );
                            self.layout_text_at(
                                text,
                                font_file,
                                size_px,
                                pen,
                                tint(style.text),
                                cell_clip,
                                out,
                                atlas_dirty,
                            );
                        }
                    }
                    Laid::Image(icon, [w, ih]) => {
                        let Some(icon) = icon else {
                            continue;
                        };
                        let w = if j == 0 { w - 1.0 } else { *w };
                        let column = x1 - x0;
                        let x = if w < column {
                            x0 + (column - w) * 0.5
                        } else {
                            x0
                        };
                        let y = if *ih < *h { top + (h - ih) * 0.5 } else { top } + 1.0;
                        let x = x.max(bounds[0] + 1.0);
                        picture(out, [x, y, x + w, y + ih], *icon, cell_clip);
                    }
                    Laid::Swatch(colour) => {
                        let side = (h - 4.0).max(0.0);
                        let (x, y) = (x0 + TEXT_X_OFFSET, top + (h - side) * 0.5);
                        solid(out, [x, y, x + side, y + side], *colour, cell_clip);
                    }
                }
            }
            top += pitch;
        }

        if let (Some(up), Some(down), Some(track)) = (frame.up, frame.down, frame.track) {
            let button = |art: [Option<SurfaceIcon>; 2], part: ListPart| {
                let [plain, hilite] = art;
                if view.hover == Some(part) {
                    hilite.or(plain)
                } else {
                    plain
                }
            };
            for (rect, art, part) in [
                (up, style.up, ListPart::Up),
                (down, style.down, ListPart::Down),
            ] {
                match button(art, part) {
                    Some(icon) => picture(out, rect, icon, clip),
                    None => solid(out, rect, [0.3, 0.35, 0.4, 1.0], clip),
                }
            }
            solid(out, track, style.track.fill, clip);
            outline(out, track, style.track.border, clip);
            let max = max_display(&view.heights, frame.display_height());
            let thumb = thumb_rect(track, view.display_pos, max);
            let art = style
                .thumb
                .get(usize::from(view.hover == Some(ListPart::Thumb)))
                .copied()
                .unwrap_or_default();
            let width = thumb[2] - thumb[0];
            let (cap_top, cap_bottom) = (art.caps[0] * width, art.caps[1] * width);
            match (art.top, art.middle, art.bottom) {
                (Some(t), Some(m), Some(b)) => {
                    let mid = [thumb[1] + cap_top, thumb[3] - cap_bottom];
                    picture(out, [thumb[0], thumb[1], thumb[2], mid[0]], t, clip);
                    picture(out, [thumb[0], mid[0], thumb[2], mid[1]], m, clip);
                    picture(out, [thumb[0], mid[1], thumb[2], thumb[3]], b, clip);
                }
                _ => solid(out, thumb, [0.6, 0.7, 0.8, 1.0], clip),
            }
        }

        self.list_layouts.insert(instance, (key, laid, heights));
    }

    fn draw_text_box(
        &mut self,
        place: GadgetPlace,
        content: Arc<TextBoxContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let (bounds, world) = self.stage_frame(&place);
        let fonts = Arc::clone(&self.fonts);
        let (rows, _) = lay_out_text_box(
            &content.lines,
            content.align,
            bounds[2] - bounds[0],
            place.letterbox.scale,
            [self.window_size.0, self.window_size.1],
            &fonts,
        );
        for row in rows {
            let pen = apply(&world, [bounds[0] + row.x, bounds[1] + row.baseline]);
            self.layout_text_at(
                &row.text,
                row.font,
                row.size_px,
                pen,
                multiply_color(place.color.mult, row.color),
                place.clip,
                out,
                atlas_dirty,
            );
        }
    }

    fn draw_placed_text(
        &mut self,
        place: &GadgetPlace,
        content: &PlacedText,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let (bounds, world) = self.stage_frame(place);
        let origin = apply(&world, [bounds[0], bounds[1]]);
        let window = [self.window_size.0, self.window_size.1];
        let fonts = Arc::clone(&self.fonts);
        for row in &content.rows {
            let file = resolved_font_file(&row.font);
            let size_px = native_text_px(row.point_size, window);
            #[allow(clippy::cast_precision_loss)]
            let ascent = file
                .and_then(|f| fonts.get(f))
                .and_then(|f| f.metrics(size_px).ok())
                .map_or(size_px as f32 * 0.8, |m| m.win_ascent);
            let pen = [origin[0] + row.x, origin[1] + row.y + ascent];
            let shadow = multiply_color(place.color.mult, [0.0, 0.0, 0.0, row.color[3]]);
            let colour = multiply_color(place.color.mult, row.color);
            for (at, colour) in [([pen[0] + 1.0, pen[1] + 1.0], shadow), (pen, colour)] {
                self.layout_text_at(
                    &row.text,
                    file,
                    size_px,
                    at,
                    colour,
                    place.clip,
                    out,
                    atlas_dirty,
                );
            }
        }
    }

    #[must_use]
    pub fn measure_native_text(&self, text: &str, font: &str, point_size: f32) -> [f32; 2] {
        let file = resolved_font_file(font);
        let font = file.and_then(|f| self.fonts.get(f));
        let size_px = native_text_px(point_size, [self.window_size.0, self.window_size.1]);
        #[allow(clippy::cast_precision_loss)]
        let height = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or(size_px as f32 * 1.2, |m| m.win_ascent + m.win_descent);
        [mint_ui::measure_text(text, font, size_px), height]
    }

    fn draw_text_entry(
        &mut self,
        place: GadgetPlace,
        content: Arc<EntryContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let load = self.load_of(place.instance);
        let style = match self.entry_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_entry),
            None => {
                self.want(GadgetKind::Entry, load);
                return;
            }
        };
        let (bounds, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let x_scale = place.letterbox.scale[0].abs().max(f32::EPSILON);
        let y_scale = place.letterbox.scale[1].abs().max(f32::EPSILON);
        let file = resolved_font_file(&style.font);
        let size_px = gadget_text_px(style.points, &place.letterbox);
        #[allow(clippy::cast_precision_loss)]
        let px = size_px as f32;
        let fonts = Arc::clone(&self.fonts);
        let font = file.and_then(|f| fonts.get(f));
        let (ascent, line) = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or((px * 0.8, px * 1.2), |m| {
                (m.ascent, m.win_ascent + m.win_descent)
            });
        let (ascent, line) = (ascent / y_scale, line / y_scale);
        let (colour, shadow) = style.colours(GadgetState::of(content.enabled, false));
        let colour = multiply_color(place.color.mult, colour);
        let shadow = multiply_color(place.color.mult, shadow);
        let clip = paint.clip(bounds, place.clip);
        let text = entry_gadget::shown_text(&content.text, style.masked);
        let [x, top] = entry_gadget::text_origin(bounds, line, style.one_line);
        if !text.is_empty() {
            let pen = apply(&world, [x, top + ascent]);
            self.layout_text_at(
                &text,
                file,
                size_px,
                [pen[0] + 1.0, pen[1] + 1.0],
                shadow,
                clip,
                out,
                atlas_dirty,
            );
            self.layout_text_at(&text, file, size_px, pen, colour, clip, out, atlas_dirty);
        }
        if entry_gadget::caret_lit(content.focused, caret_clock_millis()) {
            let end = x + mint_ui::measure_text(&text, font, size_px) / x_scale;
            paint.solid(
                out,
                entry_gadget::caret_rect(bounds, end, line),
                colour,
                clip,
            );
        }
    }

    pub fn warm_text_box(&mut self, lines: &[TextLine], width: f32, at: Option<InstanceId>) {
        let scale = self.text_scale(at);
        let (rows, _) = lay_out_text_box(
            lines,
            TextAlign::Left,
            width,
            scale,
            [self.window_size.0, self.window_size.1],
            &self.fonts,
        );
        self.text_warm
            .extend(rows.into_iter().map(|r| (r.text, r.font, r.size_px)));
    }

    fn text_scale(&mut self, at: Option<InstanceId>) -> [f32; 2] {
        let stretch = self.letterbox();
        match (self.scale_mode, at) {
            (ScaleMode::Anchored, Some(id)) => {
                let top = crate::top_level_ancestor(&self.player, id);
                self.anchors().get(&top).copied().unwrap_or(stretch).scale
            }
            _ => stretch.scale,
        }
    }

    #[must_use]
    pub fn stage_rect(&self, id: InstanceId) -> Option<[f32; 4]> {
        let bounds = self.player.local_bounds(id)?;
        let world = self.player.world_matrix(id)?;
        let corners = [
            world.apply(bounds[0], bounds[1]),
            world.apply(bounds[2], bounds[1]),
            world.apply(bounds[0], bounds[3]),
            world.apply(bounds[2], bounds[3]),
        ];
        let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in corners {
            l = l.min(x);
            t = t.min(y);
            r = r.max(x);
            b = b.max(y);
        }
        Some([l, t, r, b])
    }

    pub fn measure_text_box_height(
        &mut self,
        lines: &[TextLine],
        width: f32,
        at: Option<InstanceId>,
    ) -> f32 {
        let scale = self.text_scale(at);
        lay_out_text_box(
            lines,
            TextAlign::Left,
            width,
            scale,
            [self.window_size.0, self.window_size.1],
            &self.fonts,
        )
        .1
    }

    fn draw_check(
        &mut self,
        place: GadgetPlace,
        content: Arc<CheckContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let instance = place.instance;
        let load = self.load_of(instance);
        let style = match self.check_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_check),
            None => {
                self.want(GadgetKind::Check, load);
                return;
            }
        };
        let (bounds, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let mut view = self
            .checks
            .remove(&instance)
            .unwrap_or_else(|| CheckView::new(Arc::clone(&content)));
        view.set_content(content);
        view.bounds = Some(bounds);
        let state = view.state() as usize;
        let y_scale = place.letterbox.scale[1].abs().max(f32::EPSILON);
        let aspect = y_scale / place.letterbox.scale[0].abs().max(f32::EPSILON);
        let square = check_box(bounds, aspect, 1.0 / y_scale);
        let art = style
            .boxes
            .get(state)
            .and_then(|pair| pair.get(usize::from(view.checked)))
            .copied()
            .flatten();
        match art {
            Some(icon) => paint.picture(out, square, icon, place.clip),
            None => {
                paint.outline(out, square, [0.8, 0.8, 0.8, 1.0], place.clip);
                if view.checked {
                    let inset = (square[2] - square[0]) * 0.25;
                    let inner = [
                        square[0] + inset,
                        square[1] + inset,
                        square[2] - inset,
                        square[3] - inset,
                    ];
                    paint.solid(out, inner, [0.8, 0.8, 0.8, 1.0], place.clip);
                }
            }
        }
        if let Some(label) = view.content.label.clone() {
            let y_scale = place.letterbox.scale[1].abs().max(f32::EPSILON);
            let file = resolved_font_file(&style.font);
            let size_px = gadget_text_px(style.points, &place.letterbox);
            #[allow(clippy::cast_precision_loss)]
            let px = size_px as f32;
            let fonts = Arc::clone(&self.fonts);
            let (ascent, line) = file
                .and_then(|f| fonts.get(f))
                .and_then(|f| f.metrics(size_px).ok())
                .map_or((px * 0.8, px * 1.2), |m| {
                    (m.win_ascent, m.win_ascent + m.win_descent)
                });
            let [x, centre] = check_label(bounds);
            let baseline = centre - line / y_scale * 0.5 + ascent / y_scale;
            let pen = apply(&world, [x, baseline]);
            let colour = multiply_color(
                place.color.mult,
                style.text.get(state).copied().unwrap_or(WHITE),
            );
            let text_clip = paint.clip(bounds, place.clip);
            self.layout_text_at(
                &label,
                file,
                size_px,
                [pen[0] + 1.0, pen[1] + 1.0],
                [0.0, 0.0, 0.0, colour[3]],
                text_clip,
                out,
                atlas_dirty,
            );
            self.layout_text_at(
                &label,
                file,
                size_px,
                pen,
                colour,
                text_clip,
                out,
                atlas_dirty,
            );
        }
        self.placed_gadgets.push(PlacedGadget {
            instance,
            kind: GadgetKind::Check,
            world,
            clip: paint.clip(bounds, place.clip),
        });
        self.checks.insert(instance, view);
    }

    fn draw_graph(&mut self, place: GadgetPlace, graph: &GraphContent, out: &mut Vec<UiQuad>) {
        let chosen = self
            .player
            .own_string(place.instance, &graph.select)
            .map(|mode| mode.to_string());
        let Some((name, plot)) = graph
            .plots
            .iter()
            .find(|(name, _)| Some(name) == chosen.as_ref())
            .or_else(|| graph.plots.first())
        else {
            return;
        };
        self.graph_mode = Some(name.clone());
        let (b, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let at = |p: [f32; 2]| {
            apply(
                &world,
                [b[0] + p[0] * (b[2] - b[0]), b[1] + p[1] * (b[3] - b[1])],
            )
        };
        for line in &plot.lines {
            let color = multiply_color(paint.color.mult, line.color);
            for pair in line.points.windows(2) {
                let &[a, z] = pair else {
                    continue;
                };
                let [a, z] = [a, z].map(at);
                let (dx, dy) = (z[0] - a[0], z[1] - a[1]);
                let length = (dx * dx + dy * dy).sqrt();
                if length <= f32::EPSILON {
                    continue;
                }
                let (nx, ny) = (-dy / length * line.width, dx / length * line.width);
                let mut q =
                    UiQuad::new(0.0, 0.0, 1.0, 1.0).with_color_transform(color, paint.color.add);
                q.affine = [[dx, nx], [dy, ny]];
                q.translation = [a[0] - nx * 0.5, a[1] - ny * 0.5];
                q.clip = place.clip;
                out.push(q);
            }
        }
        for marker in &plot.markers {
            let [x, y] = [
                b[0] + marker.at[0] * (b[2] - b[0]),
                b[1] + marker.at[1] * (b[3] - b[1]),
            ];
            let [hw, hh] = [marker.size[0] * 0.5, marker.size[1] * 0.5];
            paint.picture(
                out,
                [x - hw, y - hh, x + hw, y + hh],
                marker.icon,
                place.clip,
            );
        }
    }

    #[must_use]
    pub fn graph_mode(&self) -> Option<&str> {
        self.graph_mode.as_deref()
    }

    fn draw_map_window(
        &mut self,
        place: GadgetPlace,
        content: Arc<MapWindowContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let instance = place.instance;
        let load = self.load_of(instance);
        let style = match self.map_window_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_map_window),
            None => {
                self.want(GadgetKind::MapWindow, load);
                return;
            }
        };
        let (bounds, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let clip = place.clip;
        let x_scale = place.letterbox.scale[0].abs().max(f32::EPSILON);
        let y_scale = place.letterbox.scale[1].abs().max(f32::EPSILON);
        let picture = picture_rect(bounds, content.extent);
        let (px, py) = (1.0 / x_scale, 1.0 / y_scale);
        if picture[0] > bounds[0] {
            paint.solid(
                out,
                [bounds[0], bounds[1], picture[0], bounds[3]],
                BAR,
                clip,
            );
            paint.solid(
                out,
                [picture[2], bounds[1], bounds[2], bounds[3]],
                BAR,
                clip,
            );
            paint.solid(
                out,
                [picture[0] - px, bounds[1], picture[0], bounds[3]],
                LINE,
                clip,
            );
            paint.solid(
                out,
                [picture[2], bounds[1], picture[2] + px, bounds[3]],
                LINE,
                clip,
            );
        } else if picture[1] > bounds[1] {
            paint.solid(
                out,
                [bounds[0], bounds[1], bounds[2], picture[1]],
                BAR,
                clip,
            );
            paint.solid(
                out,
                [bounds[0], picture[3], bounds[2], bounds[3]],
                BAR,
                clip,
            );
            paint.solid(
                out,
                [bounds[0], picture[1] - py, bounds[2], picture[1]],
                LINE,
                clip,
            );
            paint.solid(
                out,
                [bounds[0], picture[3], bounds[2], picture[3] + py],
                LINE,
                clip,
            );
        }
        match (content.backdrop, content.picture) {
            (None, None) => paint.solid(out, picture, LINE, clip),
            (backdrop, art) => {
                for icon in [backdrop, art].into_iter().flatten() {
                    paint.picture(out, picture, icon, clip);
                }
            }
        }
        let mut view = self
            .map_windows
            .remove(&instance)
            .unwrap_or_else(|| MapWindowView::new(Arc::clone(&content)));
        if !Arc::ptr_eq(&view.content, &content) {
            view.content = Arc::clone(&content);
        }
        let (sw, sh) = self.stage_size();
        let size = style.spot_units([sw, sh]);
        view.rects = spot_rects(picture, &content.spots, size);
        let file = resolved_font_file(&style.font);
        let size_px = gadget_text_px(style.points, &place.letterbox);
        #[allow(clippy::cast_precision_loss)]
        let em = size_px as f32;
        let fonts = Arc::clone(&self.fonts);
        let font = file.and_then(|f| fonts.get(f));
        let (ascent, line) = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or((em * 0.8, em * 1.2), |m| {
                (m.win_ascent, m.win_ascent + m.win_descent)
            });
        for (i, (rect, spot)) in view.rects.iter().zip(&content.spots).enumerate() {
            let state = view.state(i) as usize;
            let art = if view.hover == Some(i) && self.pressed_spot(instance) == Some(i) {
                style.pushed
            } else {
                style.spot.get(state).copied().flatten()
            };
            match art {
                Some(icon) => paint.picture(out, *rect, icon, clip),
                None => paint.outline(out, *rect, [0.8, 0.6, 0.1, 1.0], clip),
            }
            let Some(label) = &spot.label else {
                continue;
            };
            let width = mint_ui::measure_text(label, font, size_px) / x_scale;
            let x = (rect[0] + rect[2] - width) * 0.5;
            let baseline = (rect[1] + rect[3]) * 0.5 - line / y_scale * 0.5 + ascent / y_scale;
            let pen = apply(&world, [x, baseline]);
            let colour = multiply_color(
                place.color.mult,
                style.text.get(state).copied().unwrap_or(WHITE),
            );
            for (at, c) in [
                ([pen[0] + 1.0, pen[1] + 1.0], [0.0, 0.0, 0.0, colour[3]]),
                (pen, colour),
            ] {
                self.layout_text_at(label, file, size_px, at, c, clip, out, atlas_dirty);
            }
        }
        self.placed_gadgets.push(PlacedGadget {
            instance,
            kind: GadgetKind::MapWindow,
            world,
            clip: paint.clip(bounds, clip),
        });
        self.map_windows.insert(instance, view);
    }

    fn pressed_spot(&self, instance: InstanceId) -> Option<usize> {
        self.pressed_spot
            .filter(|(id, _)| *id == instance)
            .map(|(_, i)| i)
    }

    fn draw_slider(&mut self, place: GadgetPlace, content: SliderContent, out: &mut Vec<UiQuad>) {
        let instance = place.instance;
        let load = self.load_of(instance);
        let style = match self.slider_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_slider),
            None => {
                self.want(GadgetKind::Slider, load);
                return;
            }
        };
        let (bounds, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let mut view = self
            .sliders
            .remove(&instance)
            .unwrap_or_else(|| SliderView::new(content));
        view.set_content(content);
        view.bounds = Some(bounds);
        view.range = style.range;
        let [w, h] = style.segment;
        let h = h.min(bounds[3] - bounds[1]).max(0.0);
        let top = bounds[1];
        let (starts, filled) = segments(bounds, w, view.value, style.range);
        if view.hover {
            if let (Some(outline), Some(&first)) = (style.outline, starts.first()) {
                let step = w + SEGMENT_GAP;
                for i in 0..=starts.len() {
                    #[allow(clippy::cast_precision_loss)]
                    let x = first - step * 0.5 + step * i as f32;
                    paint.picture(out, [x, top, x + step, top + h], outline, place.clip);
                }
            }
        }
        for (i, x) in starts.iter().enumerate() {
            let rect = [*x, top, x + w, top + h];
            match (i < filled, style.filled, style.empty) {
                (true, Some(icon), _) | (false, _, Some(icon)) => {
                    paint.picture(out, rect, icon, place.clip);
                }
                (true, None, _) => paint.solid(out, rect, [0.6, 0.7, 0.8, 1.0], place.clip),
                (false, _, None) => {}
            }
        }
        let thumb = slider_thumb(bounds, view.value, style.range);
        let art = style
            .thumb
            .get(usize::from(view.hover))
            .copied()
            .flatten()
            .or_else(|| style.thumb.first().copied().flatten());
        match art {
            Some(icon) => paint.picture(out, thumb, icon, place.clip),
            None => paint.solid(out, thumb, [0.8, 0.8, 0.8, 1.0], place.clip),
        }
        self.placed_gadgets.push(PlacedGadget {
            instance,
            kind: GadgetKind::Slider,
            world,
            clip: paint.clip(bounds, place.clip),
        });
        self.sliders.insert(instance, view);
    }

    fn draw_combo_box(
        &mut self,
        place: GadgetPlace,
        content: Arc<ComboContent>,
        out: &mut Vec<UiQuad>,
        atlas_dirty: &mut bool,
    ) {
        let instance = place.instance;
        let load = self.load_of(instance);
        let style = match self.combo_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            Some(None) => Arc::clone(&self.plain_combo),
            None => {
                self.want(GadgetKind::Combo, load);
                return;
            }
        };
        let (bounds, world) = self.stage_frame(&place);
        let paint = Painter {
            world,
            color: place.color,
        };
        let mut view = self
            .combos
            .remove(&instance)
            .unwrap_or_else(|| ComboView::new(Arc::clone(&content)));
        view.set_content(content);
        view.bounds = Some(bounds);
        view.max_display = style.max_display;
        let state = view.state() as usize;
        let y_scale = place.letterbox.scale[1].abs().max(f32::EPSILON);
        let file = resolved_font_file(&style.font);
        let size_px = gadget_text_px(style.points, &place.letterbox);
        #[allow(clippy::cast_precision_loss)]
        let px = size_px as f32;
        let fonts = Arc::clone(&self.fonts);
        let (ascent, line) = file
            .and_then(|f| fonts.get(f))
            .and_then(|f| f.metrics(size_px).ok())
            .map_or((px * 0.8, px * 1.2), |m| {
                (m.ascent, m.win_ascent + m.win_descent)
            });
        let (ascent, line) = (ascent / y_scale, line / y_scale);
        let aspect = y_scale / place.letterbox.scale[0].abs().max(f32::EPSILON);
        let area = combo_gadget::text_rect(bounds, aspect);
        let text_clip = paint.clip(area, place.clip);
        let item = view
            .selected
            .and_then(|i| view.content.items.get(i))
            .cloned();
        if let Some(item) = item {
            let centre = [(area[0] + area[2]) * 0.5, (area[1] + area[3]) * 0.5];
            match (item.image, item.swatch) {
                (Some((icon, [w, h])), _) => {
                    let w = w * aspect;
                    let r = [
                        centre[0] - w * 0.5,
                        centre[1] - h * 0.5,
                        centre[0] + w * 0.5,
                        centre[1] + h * 0.5,
                    ];
                    paint.picture(out, r, icon, text_clip);
                }
                (None, Some(colour)) => {
                    let side = combo_gadget::SWATCH.min(area[3] - area[1]);
                    let r = [
                        centre[0] - side * aspect * 0.5,
                        centre[1] - side * 0.5,
                        centre[0] + side * aspect * 0.5,
                        centre[1] + side * 0.5,
                    ];
                    paint.solid(out, r, colour, text_clip);
                }
                (None, None) if !item.text.is_empty() => {
                    let colour = multiply_color(
                        place.color.mult,
                        item.colour
                            .unwrap_or_else(|| style.text.get(state).copied().unwrap_or(WHITE)),
                    );
                    let baseline = area[1] + combo_gadget::TEXT_TOP + ascent;
                    let pen = apply(&world, [area[0] + TEXT_X_OFFSET, baseline]);
                    self.layout_text_at(
                        &item.text,
                        file,
                        size_px,
                        [pen[0] + 1.0, pen[1] + 1.0],
                        [0.0, 0.0, 0.0, colour[3]],
                        text_clip,
                        out,
                        atlas_dirty,
                    );
                    self.layout_text_at(
                        &item.text,
                        file,
                        size_px,
                        pen,
                        colour,
                        text_clip,
                        out,
                        atlas_dirty,
                    );
                }
                (None, None) => {}
            }
        }
        let button = combo_gadget::button_rect(bounds, aspect);
        let art = if view.open {
            style.pushed.or(style.button[0])
        } else {
            style.button.get(state).copied().flatten()
        };
        match art {
            Some(icon) => paint.picture(out, button, icon, place.clip),
            None => paint.solid(out, button, [0.3, 0.35, 0.4, 1.0], place.clip),
        }
        if view.open {
            let count = view.content.items.len();
            let height = combo_gadget::list_height(line, count, style.max_display);
            let top = invert(&world, [0.0, 0.0]).map_or(f32::MIN, |p| p[1]);
            let bottom = invert(&world, [0.0, self.window_size.1]).map_or(f32::MAX, |p| p[1]);
            view.upward = combo_gadget::opens_upward(bounds, height, top, bottom);
            let rect = combo_gadget::list_rect(bounds, height, view.upward);
            view.list_rect = Some(rect);
            self.pending_popup = Some(PendingPopup {
                instance,
                paint: ListPaint {
                    layout_key: instance,
                    bounds: rect,
                    world,
                    letterbox: place.letterbox,
                    color: place.color,
                    clip: UiQuad::UNCLIPPED,
                },
            });
        }
        self.placed_gadgets.push(PlacedGadget {
            instance,
            kind: GadgetKind::Combo,
            world,
            clip: paint.clip(bounds, place.clip),
        });
        self.combos.insert(instance, view);
    }

    fn draw_popup(&mut self, popup: PendingPopup, out: &mut Vec<UiQuad>, atlas_dirty: &mut bool) {
        let Some(mut view) = self.combos.remove(&popup.instance) else {
            return;
        };
        let load = self.load_of(popup.instance);
        let style = match self.combo_styles.get(&load) {
            Some(Some(style)) => Arc::clone(style),
            _ => Arc::clone(&self.plain_combo),
        };
        if let Some(mut list) = view.list.take() {
            let list_style = style.list.clone();
            self.paint_list(popup.paint, &list_style, &mut list, out, atlas_dirty);
            view.list = Some(list);
            self.popup = Some(PlacedPopup {
                instance: popup.instance,
                world: popup.paint.world,
                clip: Painter {
                    world: popup.paint.world,
                    color: popup.paint.color,
                }
                .window(popup.paint.bounds),
            });
        }
        self.combos.insert(popup.instance, view);
    }

    #[must_use]
    pub fn draws(&self) -> Vec<ScreenDraw<'_>> {
        let mut ordered: Vec<ScreenDraw<'_>> =
            Vec::with_capacity(self.ops.len().saturating_add(self.quads.len()));
        for op in &self.ops {
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
                    let Some(m) = self.frame_meshes.get(*mesh) else {
                        continue;
                    };
                    let Some(r) = m.runs.get(*run) else {
                        continue;
                    };
                    let mut g = UiGeometry::new()
                        .with_transform(world.0, world.1)
                        .with_triangles(&r.triangles)
                        .with_lines(&r.lines)
                        .with_color(*color)
                        .with_color_add(*add)
                        .with_clip(*clip);
                    if let Some(key) = m.retain_key(*run) {
                        g = g.with_retention(key);
                    }
                    if let Some((tex, transform, translate)) = texture {
                        g = g.with_texture(*tex, *transform, *translate);
                    }
                    ordered.push(ScreenDraw::Geometry(g));
                }
                Op::Lines {
                    index,
                    world,
                    color,
                    clip,
                } => {
                    if let Some(l) = self.lines.get(*index) {
                        ordered.push(ScreenDraw::Geometry(
                            UiGeometry::new()
                                .with_transform(world.0, world.1)
                                .with_lines(l)
                                .with_color(*color)
                                .with_clip(*clip),
                        ));
                    }
                }
                Op::Quads(range) => {
                    if let Some(qs) = self.quads.get(range.clone()) {
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
        ordered
    }
}

fn caret_clock_millis() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
}

fn gadget_text_px(points: f32, letterbox: &Letterbox) -> u32 {
    let x_scale = letterbox.scale[0].abs().max(f32::EPSILON);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = (points * POINTS_TO_UNITS * x_scale)
        .round()
        .clamp(1.0, 512.0) as u32;
    px
}

#[cfg(test)]
mod disabled_image_tests {
    use super::draws_grey;

    #[test]
    fn a_disabled_image_surface_or_a_grayscale_mode_draws_grey() {
        assert!(draws_grey(Some("RenderImage"), Some("GRAYSCALE")));
        assert!(draws_grey(Some("RenderImageDisabled"), None));
        assert!(draws_grey(Some("renderimagedisabled"), Some("SOLID")));
        assert!(!draws_grey(Some("RenderImage"), None));
        assert!(!draws_grey(Some("RenderImage"), Some("ALPHA")));
        assert!(!draws_grey(None, None));
    }
}

#[cfg(test)]
mod text_box_tests {
    use super::{lay_out_text_box, native_text_px, TextAlign, TextLine};
    use std::collections::HashMap;

    const WINDOW_16PX: [f32; 2] = [1024.0 * 4.0 / 3.0, 768.0 * 4.0 / 3.0];

    fn line(text: &str, point_size: f32) -> TextLine {
        TextLine {
            text: text.to_owned(),
            font: "Albertus MT".to_owned(),
            point_size,
            bold: false,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn native_text_scales_by_the_smaller_ui_axis_without_dpi() {
        assert_eq!(native_text_px(16.0, [1600.0, 1200.0]), 25);
        assert_eq!(native_text_px(14.0, [1600.0, 1200.0]), 22);
        assert_eq!(native_text_px(12.0, [1024.0, 768.0]), 12);
        assert_eq!(native_text_px(16.0, [2560.0, 1440.0]), 30);
        assert_eq!(native_text_px(12.0, [800.0, 600.0]), 9);
    }

    #[test]
    fn native_text_in_a_degenerate_window_is_its_point_size() {
        assert_eq!(native_text_px(12.0, [f32::NAN, 768.0]), 12);
        assert_eq!(native_text_px(12.0, [0.0, 0.0]), 12);
    }

    #[test]
    fn a_centred_row_is_centred_and_a_left_row_is_flush() {
        let fonts = HashMap::new();
        let lines = [line("hi", 12.0)];
        let (left, _) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            400.0,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        let (centre, _) = lay_out_text_box(
            &lines,
            TextAlign::Centre,
            400.0,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        let width = mint_ui::measure_text("hi", None, 16);
        assert_eq!(left[0].x, 0.0);
        assert!(
            (centre[0].x - (400.0 - width) * 0.5).abs() < 1e-3,
            "{centre:?}"
        );
    }

    #[test]
    fn rows_stack_and_the_height_is_their_sum() {
        let fonts = HashMap::new();
        let lines = [line("one", 12.0), line("two", 12.0)];
        let (rows, height) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            400.0,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        assert_eq!(rows.len(), 2);
        let row = 16.0 * 1.2;
        assert!((rows[1].baseline - rows[0].baseline - row).abs() < 1e-3);
        assert!((height - 2.0 * row).abs() < 1e-3, "{height}");
    }

    #[test]
    fn a_narrower_box_wraps_to_more_rows() {
        let fonts = HashMap::new();
        let lines = [line(
            "one two three four five six seven eight nine ten",
            12.0,
        )];
        let (wide_rows, wide) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            4000.0,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        let (narrow_rows, narrow) =
            lay_out_text_box(&lines, TextAlign::Left, 60.0, [1.0; 2], WINDOW_16PX, &fonts);
        assert_eq!(wide_rows.len(), 1);
        assert!(narrow_rows.len() > 1 && narrow > wide);
    }

    #[test]
    fn pixels_become_stage_units_through_the_scale() {
        let fonts = HashMap::new();
        let lines = [line("one two three four", 12.0)];
        let (_, at_one) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            4000.0,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        let (_, at_two) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            4000.0,
            [2.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        assert!((at_one - 2.0 * at_two).abs() < 1e-3);
        let tight = mint_ui::measure_text("one two three four", None, 16) * 0.75;
        let (one, _) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            tight,
            [1.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        let (two, _) = lay_out_text_box(
            &lines,
            TextAlign::Left,
            tight,
            [2.0; 2],
            WINDOW_16PX,
            &fonts,
        );
        assert!(one.len() > 1 && two.len() == 1);
    }

    #[test]
    fn a_degenerate_width_does_not_panic() {
        let fonts = HashMap::new();
        let (rows, height) = lay_out_text_box(
            &[line("a b", 12.0)],
            TextAlign::Centre,
            -5.0,
            [0.0; 2],
            [0.0; 2],
            &fonts,
        );
        assert_eq!(rows.len(), 2);
        assert!(height > 0.0 && height.is_finite());
    }
}

fn lap(clock: &mut std::time::Instant) -> Duration {
    let now = std::time::Instant::now();
    let elapsed = now.duration_since(*clock);
    *clock = now;
    elapsed
}

fn note_rasterised(atlas: &mut GlyphAtlas, busy: &mut bool, dirty: &mut bool) -> bool {
    if atlas.take_dirty() {
        *busy = true;
        *dirty = true;
    }
    *busy
}

fn movie_texture(bytes: &[u8]) -> Option<TextureData> {
    let image = mint_image::tga::decode(bytes).ok()?;
    TextureData::rgba8(image.width(), image.height(), image.into_pixels())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GadgetKind {
    List,
    Check,
    Slider,
    Combo,
    MapWindow,
    Entry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GadgetClick {
    Missed,
    Taken,
    Changed(GadgetEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GadgetEvent {
    pub instance: InstanceId,
    pub key: Option<String>,
    pub change: Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Row(usize),
    Checked(bool),
    Value(i32),
    Spot(usize),
    SpotCleared(usize),
}

#[derive(Debug, Clone, Copy)]
struct GadgetPlace {
    instance: InstanceId,
    local: [f32; 4],
    letterbox: Letterbox,
    color: ColorTransform,
    clip: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct ListPaint {
    layout_key: InstanceId,
    bounds: [f32; 4],
    world: Affine,
    letterbox: Letterbox,
    color: ColorTransform,
    clip: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct PendingPopup {
    instance: InstanceId,
    paint: ListPaint,
}

#[derive(Debug, Clone, Copy)]
struct PlacedPopup {
    instance: InstanceId,
    world: Affine,
    clip: [f32; 4],
}

fn inside_rect(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

struct PlacedGadget {
    instance: InstanceId,
    kind: GadgetKind,
    world: Affine,
    clip: [f32; 4],
}

#[derive(Clone, Copy)]
struct Painter {
    world: Affine,
    color: ColorTransform,
}

impl Painter {
    fn window(&self, r: [f32; 4]) -> [f32; 4] {
        let a = apply(&self.world, [r[0], r[1]]);
        let b = apply(&self.world, [r[2], r[3]]);
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ]
    }

    fn clip(&self, r: [f32; 4], clip: [f32; 4]) -> [f32; 4] {
        let w = self.window(r);
        [
            w[0].max(clip[0]),
            w[1].max(clip[1]),
            w[2].min(clip[2]),
            w[3].min(clip[3]),
        ]
    }

    fn solid(&self, out: &mut Vec<UiQuad>, r: [f32; 4], c: [f32; 4], clip: [f32; 4]) {
        if c[3] > 0.0 {
            let w = self.window(r);
            let mut q = UiQuad::new(w[0], w[1], w[2] - w[0], w[3] - w[1])
                .with_color_transform(multiply_color(self.color.mult, c), self.color.add);
            q.clip = clip;
            out.push(q);
        }
    }

    fn outline(&self, out: &mut Vec<UiQuad>, r: [f32; 4], c: [f32; 4], clip: [f32; 4]) {
        if c[3] > 0.0 {
            let w = self.window(r);
            for e in [
                [w[0], w[1], w[2], w[1] + 1.0],
                [w[0], w[3] - 1.0, w[2], w[3]],
                [w[0], w[1], w[0] + 1.0, w[3]],
                [w[2] - 1.0, w[1], w[2], w[3]],
            ] {
                let mut q = UiQuad::new(e[0], e[1], e[2] - e[0], e[3] - e[1])
                    .with_color_transform(multiply_color(self.color.mult, c), self.color.add);
                q.clip = clip;
                out.push(q);
            }
        }
    }

    fn picture(&self, out: &mut Vec<UiQuad>, r: [f32; 4], icon: SurfaceIcon, clip: [f32; 4]) {
        let w = self.window(r);
        let mut q = UiQuad::new(w[0], w[1], w[2] - w[0], w[3] - w[1]).with_color_transform(
            multiply_color(multiply_color(self.color.mult, icon.tint), WHITE),
            self.color.add,
        );
        q.texture = Some(icon.texture);
        q.uv_rect = icon.uv_rect;
        q.uv_rotated_cw = icon.rotated;
        q.premultiplied = icon.premultiplied;
        q.clip = clip;
        out.push(q);
    }
}

enum Laid {
    Lines(Vec<String>),
    Image(Option<SurfaceIcon>, [f32; 2]),
    Swatch([f32; 4]),
}

struct LaidKey {
    content: Arc<ListContent>,
    size_px: u32,
    columns: Vec<u32>,
}

impl PartialEq for LaidKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.content, &other.content)
            && self.size_px == other.size_px
            && self.columns == other.columns
    }
}

fn lay_out_rows(
    rows: &[Vec<ListCell>],
    frame: &ListFrame,
    line: f32,
    text_inset: f32,
    measure: &dyn Fn(&str) -> f32,
) -> (Vec<Vec<Laid>>, Vec<f32>) {
    let mut laid = Vec::with_capacity(rows.len());
    let mut heights = Vec::with_capacity(rows.len());
    for row in rows {
        let mut cells = Vec::with_capacity(row.len());
        let mut tall = Vec::with_capacity(row.len());
        for (j, cell) in row.iter().enumerate() {
            let [x0, x1] = frame.columns.get(j).copied().unwrap_or([0.0, 0.0]);
            let width = x1 - x0;
            match cell {
                ListCell::Text(text) => {
                    let extra = width * text_inset;
                    let lines = wrapped(text, width - TEXT_WIDTH_OFFSET - 2.0 * extra, measure);
                    #[allow(clippy::cast_precision_loss)]
                    tall.push(line * lines.len() as f32);
                    cells.push(Laid::Lines(lines));
                }
                ListCell::Image {
                    icon,
                    natural,
                    size,
                } => {
                    let size = image_cell(*size, *natural, width);
                    tall.push(size[1]);
                    cells.push(Laid::Image(*icon, size));
                }
                ListCell::Swatch(colour) => {
                    tall.push(line);
                    cells.push(Laid::Swatch(*colour));
                }
            }
        }
        heights.push(row_height(tall, line));
        laid.push(cells);
    }
    (laid, heights)
}

#[cfg(test)]
mod lay_out_rows_tests {
    use super::{lay_out_rows, Laid, ListCell};
    use crate::gadget::list::ListFrame;

    fn monospace(s: &str) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = s.chars().count() as f32;
        n
    }

    fn frame(width: f32) -> ListFrame {
        ListFrame::new([0.0, 0.0, width, 200.0], false, &[])
    }

    fn lines(rows: &[Vec<ListCell>], frame: &ListFrame, text_inset: f32) -> Vec<String> {
        let (laid, _) = lay_out_rows(rows, frame, 10.0, text_inset, &monospace);
        match laid.first().and_then(|row| row.first()) {
            Some(Laid::Lines(lines)) => lines.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn zero_inset_wraps_at_the_plain_column_width() {
        let rows = vec![vec![ListCell::Text("0123456789012345678901234".to_owned())]];
        let f = frame(33.0);
        assert_eq!(lines(&rows, &f, 0.0).len(), 1);
        let rows = vec![vec![ListCell::Text(
            "0123456789 0123456789 0123456789".to_owned(),
        )]];
        assert_eq!(
            lines(&rows, &f, 0.0),
            vec!["0123456789 0123456789", "0123456789"]
        );
    }

    #[test]
    fn a_positive_inset_narrows_the_wrap_width_from_both_sides() {
        let rows = vec![vec![ListCell::Text(
            "0123456789 0123456789 0123456789".to_owned(),
        )]];
        let f = frame(100.0);
        assert_eq!(lines(&rows, &f, 0.0).len(), 1);
        assert_eq!(
            lines(&rows, &f, 0.30).len(),
            2,
            "30% narrows enough to wrap"
        );
    }
}

fn pulled_content(
    player: &Player,
    instance: InstanceId,
    named: &HashMap<String, SurfaceContent>,
) -> Option<SurfaceContent> {
    let by = |property: &str| {
        player
            .own_string(instance, property)
            .filter(|name| !name.is_empty())
            .and_then(|name| named.get(name.as_ref()).cloned())
    };
    by("_imageMap")
        .or_else(|| by("_timerId"))
        .or_else(|| by("_imageName"))
        .or_else(|| gadget_key_of(player, instance).and_then(|key| named.get(&key).cloned()))
        .or_else(|| by("_type"))
}

fn window_name(id: &str) -> &str {
    match id.split_once('~') {
        Some((prefix, rest))
            if !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit()) =>
        {
            rest
        }
        _ => id,
    }
}

fn gadget_key_of(player: &Player, instance: InstanceId) -> Option<String> {
    let init = player
        .own_string(instance, "_Init")
        .filter(|init| !init.is_empty())?;
    let id = player
        .own_string(instance, "_WindowId")
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string())
        .or_else(|| {
            player
                .instance(instance)?
                .explicit_name()
                .map(str::to_owned)
        })?;
    Some(crate::gadget::gadget_key(&init, window_name(&id)))
}

fn invert(a: &Affine, p: [f32; 2]) -> Option<[f32; 2]> {
    let (m, t) = a;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det.abs() < f32::EPSILON {
        return None;
    }
    let (x, y) = (p[0] - t[0], p[1] - t[1]);
    Some([
        (m[1][1] * x - m[0][1] * y) / det,
        (m[0][0] * y - m[1][0] * x) / det,
    ])
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod pull_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use mint_sage_apt_player::expose::resolve_path;
    use mint_sage_apt_player::synth::{self, op, Asm, Synth};
    use mint_sage_apt_player::{Library, Limits, Player};

    use super::{invert, pulled_content, window_name, SurfaceContent};

    fn movie() -> Player {
        let mut s = Synth::new();
        let square = s.shape_box([0.0, 0.0, 10.0, 10.0]);
        let mut set = |pairs: &[(&str, &str)]| {
            let mut a = Asm::new();
            for (name, value) in pairs {
                a.push_str(name).push_str(value).op(op::SET_VARIABLE);
            }
            a.end();
            s.code(&mut a)
        };
        let plain = set(&[("_type", "ListBox"), ("_Init", "Owner::Init")]);
        let mapped = set(&[
            ("_type", "ListBox"),
            ("_Init", "Owner::Init"),
            ("_imageMap", "Picture"),
        ]);
        let windowed = set(&[
            ("_type", "CheckBox"),
            ("_Init", "Owner::Init"),
            ("_WindowId", "Row_7"),
        ]);
        let timer = set(&[("_type", "TimerOverlay"), ("_timerId", "Button_Timer")]);
        let component = set(&[("_type", "Swatch"), ("_imageName", "PictureTest")]);
        let surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(plain),
        ]]);
        let windowed_surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(windowed),
        ]]);
        let mapped_surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(mapped),
        ]]);
        let timer_surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(timer),
        ]]);
        let component_surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(component),
        ]]);
        let root = vec![vec![
            synth::Item::Place(synth::place(6, component_surface).name("Component")),
            synth::Item::Place(synth::place(1, surface).name("Named")),
            synth::Item::Place(synth::place(2, surface).name("Other")),
            synth::Item::Place(synth::place(3, mapped_surface).name("Mapped")),
            synth::Item::Place(synth::place(4, windowed_surface).name("Windowed")),
            synth::Item::Place(synth::place(5, timer_surface).name("Timer")),
        ]];
        let (c, apt) = s.build(&root);
        let mut lib = Library::new();
        let id = lib.add("test", &c, apt).expect("synthetic movie parses");
        lib.link();
        let mut player = Player::new(Arc::new(lib), id, Limits::default()).expect("starts");
        let _ = player.tick();
        player
    }

    #[test]
    fn a_numeric_prefix_is_not_part_of_a_window_name() {
        assert_eq!(window_name("3~ListName"), "ListName");
        assert_eq!(window_name("tab~ListName"), "tab~ListName");
        assert_eq!(window_name("Row_7"), "Row_7");
    }

    fn is_hidden(content: Option<SurfaceContent>) -> bool {
        matches!(content, Some(SurfaceContent::Hidden))
    }

    #[test]
    fn a_gadget_pulls_its_content_by_its_init_and_its_name() {
        let player = movie();
        let at = |path: &str| resolve_path(&player, path).expect("placed");
        let mut named = HashMap::new();
        named.insert("Owner::Init/Named".to_owned(), SurfaceContent::Hidden);
        assert!(is_hidden(pulled_content(&player, at("Named"), &named)));
        assert!(pulled_content(&player, at("Other"), &named).is_none());
        assert!(pulled_content(&player, at("Mapped"), &named).is_none());

        named.insert("Owner::Init/Mapped".to_owned(), SurfaceContent::Hidden);
        assert!(
            is_hidden(pulled_content(&player, at("Mapped"), &named)),
            "no picture: the key"
        );
        named.insert("Picture".to_owned(), SurfaceContent::List(Arc::default()));
        assert!(matches!(
            pulled_content(&player, at("Mapped"), &named),
            Some(SurfaceContent::List(_))
        ));
        named.insert("ListBox".to_owned(), SurfaceContent::List(Arc::default()));
        assert!(
            is_hidden(pulled_content(&player, at("Named"), &named)),
            "the key before _type"
        );
        assert!(matches!(
            pulled_content(&player, at("Other"), &named),
            Some(SurfaceContent::List(_))
        ));
    }

    #[test]
    fn a_timer_pulls_its_content_by_its_timer_id() {
        let player = movie();
        let timer = resolve_path(&player, "Timer").expect("placed");
        let mut named = HashMap::new();
        assert!(pulled_content(&player, timer, &named).is_none());
        named.insert(
            "TimerOverlay".to_owned(),
            SurfaceContent::List(Arc::default()),
        );
        assert!(
            matches!(
                pulled_content(&player, timer, &named),
                Some(SurfaceContent::List(_))
            ),
            "the type when the id names nothing"
        );
        let clock = super::TimerClock {
            covered: 0.25,
            tint: [1.0; 4],
            rim: [1.0; 4],
            fill: None,
            ring: None,
        };
        named.insert("Button_Timer".to_owned(), SurfaceContent::Progress(clock));
        assert!(
            matches!(
                pulled_content(&player, timer, &named),
                Some(SurfaceContent::Progress(clock)) if (clock.covered - 0.25).abs() < f32::EPSILON
            ),
            "the id before the type"
        );
    }

    #[test]
    fn a_surface_pulls_the_image_its_image_name_names() {
        let player = movie();
        let component = resolve_path(&player, "Component").expect("placed");
        let mut named = HashMap::new();
        named.insert("Swatch".to_owned(), SurfaceContent::Hidden);
        named.insert(
            "PictureTest".to_owned(),
            SurfaceContent::List(Arc::default()),
        );
        assert!(matches!(
            pulled_content(&player, component, &named),
            Some(SurfaceContent::List(_))
        ));
    }

    #[test]
    fn a_window_id_keys_a_gadget_before_its_name() {
        let player = movie();
        let windowed = resolve_path(&player, "Windowed").expect("placed");
        let mut named = HashMap::new();
        named.insert("Owner::Init/Windowed".to_owned(), SurfaceContent::Hidden);
        assert!(
            pulled_content(&player, windowed, &named).is_none(),
            "not by name"
        );
        named.insert("Owner::Init/Row_7".to_owned(), SurfaceContent::Hidden);
        assert!(is_hidden(pulled_content(&player, windowed, &named)));
    }

    #[test]
    fn a_window_point_maps_back_to_the_list() {
        let a = ([[2.0, 0.0], [0.0, 4.0]], [10.0, 20.0]);
        assert_eq!(invert(&a, [14.0, 28.0]), Some([2.0, 2.0]));
        assert_eq!(
            invert(&([[0.0, 0.0], [0.0, 0.0]], [0.0, 0.0]), [1.0, 1.0]),
            None
        );
    }
}

#[cfg(test)]
mod text_size_tests {
    use super::gadget_text_px;
    use crate::Letterbox;

    #[test]
    fn gadget_text_scales_with_the_stages_width() {
        let four_three = Letterbox::stretch(1024.0, 768.0, 1600.0, 1200.0);
        let wide = Letterbox::stretch(1024.0, 768.0, 1920.0, 1080.0);
        assert_eq!(gadget_text_px(10.0, &four_three), 21);
        assert_eq!(gadget_text_px(10.0, &wide), 25);
        let tall = Letterbox::stretch(1024.0, 768.0, 800.0, 1200.0);
        assert_eq!(gadget_text_px(10.0, &tall), 10, "not the height");
    }
}

#[cfg(test)]
mod grey_tests {
    use super::greyscale_rgba;

    #[test]
    fn grey_is_luma_and_keeps_alpha() {
        let mut px = [255, 0, 0, 200, 0, 255, 0, 10, 0, 0, 255, 0, 10, 20, 30, 255];
        greyscale_rgba(&mut px);
        assert_eq!(&px[..4], &[76, 76, 76, 200]);
        assert_eq!(&px[4..8], &[150, 150, 150, 10]);
        assert_eq!(&px[8..12], &[29, 29, 29, 0]);
        assert_eq!(&px[12..], &[18, 18, 18, 255]);
    }
}

#[cfg(test)]
mod mapped_uv_tests {
    use super::mapped_uv;
    use mint_sage_ini::MappedRect;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Option<MappedRect> {
        Some(MappedRect {
            left,
            top,
            right,
            bottom,
        })
    }

    #[test]
    fn coords_divide_by_the_stated_texture_size() {
        assert_eq!(
            mapped_uv(rect(0, 0, 600, 294), (Some(600), Some(294)), (704, 491)),
            [0.0, 0.0, 1.0, 1.0]
        );
        let [_, _, w, h] = mapped_uv(rect(0, 0, 352, 491), (None, None), (704, 491));
        assert!(
            (w - 0.5).abs() < 1e-6 && (h - 1.0).abs() < 1e-6,
            "unstated: decoded"
        );
    }

    #[test]
    fn right_and_bottom_are_not_widened() {
        let [x, y, w, h] = mapped_uv(rect(32, 0, 63, 63), (Some(64), Some(64)), (64, 64));
        assert!((x - 0.5).abs() < 1e-6 && y.abs() < 1e-6);
        assert!((w - 31.0 / 64.0).abs() < 1e-6 && (h - 63.0 / 64.0).abs() < 1e-6);
    }

    #[test]
    fn no_coords_or_no_size_is_still_a_rect() {
        assert_eq!(mapped_uv(None, (None, None), (0, 0)), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(
            mapped_uv(rect(0, 0, 1, 1), (Some(0), Some(0)), (0, 0)),
            [0.0, 0.0, 1.0, 1.0]
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod annotate_surface_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use mint_render::{Error, Renderer};
    use mint_sage_apt_player::synth::{self, op, Asm, Synth};
    use mint_sage_apt_player::{Library, Limits};
    use mint_sage_str::StringTable;

    use super::MovieView;

    const SIZE: u32 = 64;

    fn renderer() -> Option<Renderer> {
        match Renderer::offscreen(SIZE, SIZE) {
            Ok(renderer) => Some(renderer),
            Err(Error::NoAdapter) => {
                eprintln!(
                    "SKIPPED: no graphics adapter on this machine, so \
                     annotate_surface_tests was not actually verified"
                );
                None
            }
            Err(error) => panic!("creating an offscreen renderer failed: {error}"),
        }
    }

    fn unregistered_surface() -> Library {
        let mut s = Synth::new();
        let square = s.shape_box([0.0, 0.0, 10.0, 10.0]);
        let mut set = Asm::new();
        set.push_str("_type")
            .push_str("AptPalantir::RenderRadar")
            .op(op::SET_VARIABLE)
            .end();
        let set = s.code(&mut set);
        let surface = s.sprite(vec![vec![
            synth::Item::Place(synth::place(1, square)),
            synth::Item::Action(set),
        ]]);
        let root = vec![vec![synth::Item::Place(
            synth::place(1, surface).name("Surface"),
        )]];
        let (c, apt) = s.build(&root);
        let mut lib = Library::new();
        lib.add("test", &c, apt).expect("synthetic movie parses");
        lib.link();
        lib
    }

    fn view(renderer: &mut Renderer) -> MovieView {
        let lib = Arc::new(unregistered_surface());
        let mut view = MovieView::load(
            Arc::clone(&lib),
            std::iter::empty(),
            "test",
            Arc::new(HashMap::new()),
            Arc::new(StringTable::new()),
            (SIZE as f32, SIZE as f32),
            Limits::default(),
            renderer,
        )
        .expect("view loads");
        view.player.tick().expect("first tick places the surface");
        view
    }

    #[test]
    fn an_unregistered_surface_draws_only_when_annotated() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut view = view(&mut renderer);
        view.build(&mut renderer);
        assert!(
            view.ops.is_empty(),
            "nothing by default: {} op(s)",
            view.ops.len()
        );

        view.annotate_surfaces = true;
        view.build(&mut renderer);
        assert!(
            !view.ops.is_empty(),
            "an outline once annotate_surfaces opts in"
        );
    }
}
