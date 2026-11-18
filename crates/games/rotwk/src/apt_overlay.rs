use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mint_render::{Renderer, ScreenDraw, TextureHandle, UiQuad};
use mint_sage_apt_player::expose::resolve_path;
use mint_sage_apt_player::{Exposure, ImageChange, InstanceId, Limits, MovieEvent, Readiness};
use mint_sage_apt_viewer::gadget::check::{CheckLook, CheckStyle};
use mint_sage_apt_viewer::gadget::combo::{ComboContent, ComboItem, ComboLook, ComboStyle};
use mint_sage_apt_viewer::gadget::list::{
    Align, ListCell, ListColumn, ListContent, ListLook, ListStyle,
};
use mint_sage_apt_viewer::gadget::map_window::{
    inset_half_texel, MapWindowContent, MapWindowLook, MapWindowStyle, Spot,
};
use mint_sage_apt_viewer::gadget::slider::{SliderLook, SliderStyle};
use mint_sage_apt_viewer::host::{GadgetKind, IconProvider, MovieView, SurfaceContent};
use mint_sage_apt_viewer::{ImageModes, Letterbox, ScaleMode, TextAnchor};
use mint_sage_ini::{CommandButtonIndex, MappedImageIndex};
use mint_ui::{HitTarget, InputRouter, PointerButton, PointerRouting, Rect, WidgetId};
use mint_vfs::Vfs;

use crate::command_bar::{ResolvedSlot, SlotAction, BAR_SLOTS, RING_SLOTS};
use crate::help_box::{self, HelpBoxConfig, HelpBoxTimer, HelpText, HoverKey};
use crate::hud::SpellStoreEvent;
use crate::hud::{
    declare_command_bar, declare_help_box, declare_hero_select, declare_palantir,
    declare_player_magic, declare_radial, declare_spell_row, declare_spell_store, under, CellSpec,
    ComboSpec, HelpBoxState, HudState, ImageSource, ListSpec, MapWindowSpec, SideBarContent,
    SpellStoreSession, HERO_SELECT_CLIP, PALANTIR_READY, SIDE_BAR_CLIP, SPELL_BOOK_CLIP,
    SPELL_STORE_READY,
};
mod top_screens;

use crate::in_game_menu::{
    InGameMenus, MenuRequest, MENU_READY, OPTIONS_MOVIE, QUIT_MENU_MOVIE, RESTART_TEXT,
};
use crate::notification_host::{self, NotificationHost};
use crate::objectives_host::{self, ObjectivesHost};
use crate::radial::{RadialPage, BUTTON_STAGE};
use crate::spell_book::{SpellBookData, SpellBookView};

const PALANTIR_MOVIE: &str = "palantir";
const RADIAL_MOVIE: &str = "ingameradialmenustage";
const SPELL_STORE_MOVIE: &str = "spellstore";
pub(crate) const GUI_FX_MOVIE: &str = "guifx";
pub(crate) const GUI_FX_READY: &str = "OnInitialized";
const END_GAME_CLIPS: [&str; 2] = ["GoodMovie", "EvilMovie"];
const OPEN_SPELL_STORE: [&str; 2] = ["OnBttnSpellStore", "OpenSpellStore"];
pub(crate) const PLAY_SOUND: &str = "PlaySound";
const OPEN_QUIT_MENU: &str = "OnBttnOptions";
const OPEN_OBJECTIVES: &str = "OnBttnObjectives";
const NOTIFICATION_MOVIE: &str = "ingamenotificationbox";
const SPELL_BOOK_LOADED: &str = "OnAptInGameSpellBookLoaded";
const RADIAL_MAX_RADIUS: f32 = 0.4;
const READY_WARNING: Duration = Duration::from_secs(5);

const HELP_BOX_CLIP: &str = "helpBox";
const HELP_BOX_CONTENT_KEY: &str = "_level0.helpBox_Content";
pub(crate) const BACKDROP_MOVIE: &str = crate::shell::BACKGROUND_MOVIE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Backdrop {
    #[default]
    None,
    FrontEnd,
    InGame,
}

pub(crate) fn wanted_backdrop(options_shown: bool, quit_shown: bool, store_open: bool) -> Backdrop {
    if options_shown {
        Backdrop::FrontEnd
    } else if quit_shown || store_open {
        Backdrop::InGame
    } else {
        Backdrop::None
    }
}

pub(crate) fn backdrop_call(
    from: Backdrop,
    to: Backdrop,
) -> Option<(&'static str, Vec<mint_sage_apt_player::Value>)> {
    match (from, to) {
        (a, b) if a == b => None,
        (_, Backdrop::InGame) => Some(("ShowInGameBackground", Vec::new())),
        (_, Backdrop::FrontEnd) => Some(("ShowFrontEndBackground", Vec::new())),
        (Backdrop::InGame, Backdrop::None) => Some((
            "HideInGameBackground",
            vec![mint_sage_apt_player::Value::Bool(false)],
        )),
        (Backdrop::FrontEnd, Backdrop::None) => Some((
            "HideFrontEndBackground",
            vec![mint_sage_apt_player::Value::Bool(false)],
        )),
        (Backdrop::None, Backdrop::None) => None,
    }
}

#[must_use]
pub(crate) fn slot_widget_id(routing: PointerRouting, pressed: bool) -> Option<WidgetId> {
    if pressed {
        return None;
    }
    match routing {
        PointerRouting::Widget(id) => Some(id),
        PointerRouting::World => None,
    }
}

pub(crate) fn route_pointer_button(
    router: &mut InputRouter,
    targets: &[HitTarget],
    position: [f32; 2],
    button: PointerButton,
    pressed: bool,
) -> PointerRouting {
    router.pointer_button(targets, position, button, pressed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerTarget {
    Palantir,
    SpellStore,
}

fn pointer_button_target(
    pressed: bool,
    store_open: bool,
    held: &mut Option<PointerTarget>,
) -> PointerTarget {
    let now = if store_open {
        PointerTarget::SpellStore
    } else {
        PointerTarget::Palantir
    };
    if pressed {
        *held = Some(now);
        now
    } else {
        held.take().unwrap_or(now)
    }
}

fn log_rollover(
    movie: &str,
    player: &mint_sage_apt_player::Player,
    before: Option<InstanceId>,
    after: Option<InstanceId>,
) {
    if before == after {
        return;
    }
    if let Some(id) = before {
        crate::diag::diag!(
            "apt: rollover out movie={movie} path={}",
            player.target_path(id)
        );
    }
    if let Some(id) = after {
        crate::diag::diag!(
            "apt: rollover in movie={movie} path={}",
            player.target_path(id)
        );
    }
}

fn log_button(
    movie: &str,
    player: &mint_sage_apt_player::Player,
    hovered: Option<InstanceId>,
    pressed: bool,
) {
    let Some(id) = hovered else { return };
    let verb = if pressed { "press" } else { "release" };
    crate::diag::diag!("apt: {verb} movie={movie} path={}", player.target_path(id));
}

pub(crate) fn route_pointer_moved(
    router: &mut InputRouter,
    targets: &[HitTarget],
    position: [f32; 2],
) {
    let _: PointerRouting = router.pointer_moved(targets, position);
}

#[must_use]
pub(crate) fn minimap_world_at(bounds: &mint_render::Aabb, u: f32, v: f32) -> Option<(f32, f32)> {
    if bounds.is_empty() {
        return None;
    }
    let (lo, hi) = (bounds.min(), bounds.max());
    Some((lo.x + u * (hi.x - lo.x), lo.y + (1.0 - v) * (hi.y - lo.y)))
}

#[must_use]
pub(crate) fn minimap_uv_of(bounds: &mint_render::Aabb, x: f32, y: f32) -> Option<[f32; 2]> {
    if bounds.is_empty() {
        return None;
    }
    let (lo, hi) = (bounds.min(), bounds.max());
    let (width, height) = (hi.x - lo.x, hi.y - lo.y);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some([(x - lo.x) / width, 1.0 - (y - lo.y) / height])
}

#[must_use]
pub(crate) fn view_box_world(
    camera: &mint_render::Camera,
    viewport: (u32, u32),
    ground_z: f32,
) -> Option<[[f32; 2]; 4]> {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (viewport.0 as f32, viewport.1 as f32);
    let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    let mut out = [[0.0; 2]; 4];
    for (slot, (px, py)) in out.iter_mut().zip(corners) {
        let ray = camera.ray_through_pixel(viewport, px, py)?;
        let down = -ray.direction.z;
        let t = if down > 1e-6 {
            ((ray.origin.z - ground_z) / down).clamp(0.0, camera.far)
        } else {
            camera.far
        };
        let at = ray.at(t);
        *slot = [at.x, at.y];
    }
    Some(out)
}

#[must_use]
pub(crate) fn average_ground_z(map: &mint_sage_map::HeightMapData, vertical_scale: f32) -> f32 {
    let count = map.samples.len();
    if count == 0 {
        return 0.0;
    }
    let sum: u64 = map.samples.iter().map(|&s| u64::from(s)).sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / count as f64;
    #[allow(clippy::cast_possible_truncation)]
    let mean = mean as f32;
    mean * vertical_scale
}

const VIEW_BOX_FAR: [f32; 4] = [235.0 / 255.0, 195.0 / 255.0, 70.0 / 255.0, 1.0];
const VIEW_BOX_NEAR: [f32; 4] = [200.0 / 255.0, 150.0 / 255.0, 40.0 / 255.0, 1.0];

fn line_quad(a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4], clip: [f32; 4]) -> UiQuad {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = (dx * dx + dy * dy).sqrt();
    let (nx, ny) = if length > f32::EPSILON {
        (-dy / length * width, dx / length * width)
    } else {
        (0.0, width)
    };
    let mut quad = UiQuad::new(0.0, 0.0, 1.0, 1.0);
    quad.affine = [[dx, nx], [dy, ny]];
    quad.translation = [a[0] - nx * 0.5, a[1] - ny * 0.5];
    quad.color_multiply = color;
    quad.clip = clip;
    quad
}

fn view_box_quads(uv: [[f32; 2]; 4], rect: [f32; 4], width: f32) -> Vec<UiQuad> {
    let [l, t, r, b] = rect;
    let points = uv.map(|[u, v]| [l + u * (r - l), t + v * (b - t)]);
    let mut side = [0.0; 4];
    for (m, (f, n)) in side.iter_mut().zip(VIEW_BOX_FAR.iter().zip(VIEW_BOX_NEAR)) {
        *m = (f + n) * 0.5;
    }
    let colors = [VIEW_BOX_FAR, side, VIEW_BOX_NEAR, side];
    let mut next = points;
    next.rotate_left(1);
    colors
        .iter()
        .zip(points.iter().zip(next))
        .map(|(&color, (&a, b))| line_quad(a, b, width, color, rect))
        .collect()
}

pub(crate) const RADAR_CELLS: f32 = 128.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RadarMark {
    Blip,
    Hero,
    Ring(f32),
    Wall {
        facing: f32,
        half_major: f32,
        half_minor: f32,
        bounding_radius_cells: f32,
        scale: f32,
    },
}

const HERO_HALO_QUADRANT: [[u8; 3]; 3] =
    [[0x03, 0x24, 0x45], [0x24, 0x4F, 0x4F], [0x45, 0x4F, 0x4F]];

const HERO_DOT_QUADRANT: [[u8; 2]; 2] = [[0x00, 0x4A], [0x4A, 0xFF]];

fn quadrant_index(i: usize, half: usize) -> usize {
    if i < half {
        i
    } else {
        half.saturating_mul(2).saturating_sub(1).saturating_sub(i)
    }
}

fn mirrored_alpha<const N: usize>(quadrant: &[[u8; N]], dx: usize, dy: usize, half: usize) -> u8 {
    quadrant
        .get(quadrant_index(dy, half))
        .and_then(|row| row.get(quadrant_index(dx, half)))
        .copied()
        .unwrap_or(0)
}

fn wall_pixel_alpha(
    cell: (f32, f32),
    scale: f32,
    facing: f32,
    half_major: f32,
    half_minor: f32,
) -> Option<f32> {
    if scale <= 0.0 {
        return None;
    }
    let (wx, wy) = (cell.0 / scale, cell.1 / scale);
    let (cos, sin) = (mint_math::f32::cos(facing), mint_math::f32::sin(facing));
    let local_x = wx * cos + wy * sin;
    let local_y = wy * cos - wx * sin;
    let dx = (local_x.abs() - half_major).max(0.0);
    let dy = (local_y.abs() - half_minor).max(0.0);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= 5.0 {
        Some(1.0)
    } else if d <= 30.0 {
        Some(128.0 / 255.0)
    } else {
        None
    }
}

fn wall_cells(
    facing: f32,
    half_major: f32,
    half_minor: f32,
    bounding_radius_cells: f32,
    scale: f32,
) -> Vec<(f32, f32, f32)> {
    #[allow(clippy::cast_possible_truncation)]
    let r = bounding_radius_cells as i32;
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            #[allow(clippy::cast_precision_loss)]
            let (fx, fy) = (dx as f32, dy as f32);
            if let Some(alpha) = wall_pixel_alpha((fx, fy), scale, facing, half_major, half_minor) {
                out.push((fx, fy, alpha));
            }
        }
    }
    out
}

pub(crate) struct MinimapDot {
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) mark: RadarMark,
    pub(crate) layer: (bool, u8),
}

fn ring_cells(radius: i32, mut plot: impl FnMut(i32, i32)) {
    let (mut x, mut y, mut error) = (radius, 0_i32, 1_i32.saturating_sub(radius));
    while x >= y {
        for (dx, dy) in [
            (x, y),
            (y, x),
            (-y, x),
            (-x, y),
            (-x, -y),
            (-y, -x),
            (y, -x),
            (x, -y),
        ] {
            plot(dx, dy);
        }
        y = y.saturating_add(1);
        if error < 0 {
            error = error.saturating_add(y.saturating_mul(2).saturating_add(1));
        } else {
            x = x.saturating_sub(1);
            error = error.saturating_add(y.saturating_sub(x).saturating_mul(2).saturating_add(1));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Movie {
    Palantir,
    Radial,
}

struct Placed {
    movie: Movie,
    instance: InstanceId,
    action: SlotAction,
    hover: (HoverKey, String, Option<i64>),
    face: Option<mint_sage_ini::Stance>,
}

fn readiness(ready_event: &str) -> Readiness {
    if ready_event.is_empty() {
        Readiness::Immediate
    } else {
        Readiness::Event(ready_event.to_owned())
    }
}

fn hero_number(name: &str) -> Option<u8> {
    name.get(..4).filter(|p| p.eq_ignore_ascii_case("hero"))?;
    name.get(4..)?
        .parse::<u8>()
        .ok()
        .filter(|n| (1..=16).contains(n))
}

fn print_help_box(help: &help_box::HelpText) {
    println!("help box: {:?}", help.title);
    if let Some(stat) = &help.stat {
        println!("  {stat:?}");
    }
    if let Some(shortcut) = &help.shortcut {
        println!("  {shortcut:?}");
    }
    if let Some(description) = &help.description {
        println!("  {description:?}");
    }
}

type PreparedHelp = (HoverKey, f32, Arc<Vec<help_box::HelpLineSpec>>);

fn help_box_state(prepared: Option<&PreparedHelp>, shown: Option<HoverKey>) -> HelpBoxState {
    match (prepared, shown) {
        (Some((key, height, lines)), Some(showing)) if *key == showing => HelpBoxState::Shown {
            height: *height,
            lines: Arc::clone(lines),
        },
        _ => HelpBoxState::Hidden,
    }
}

pub(crate) struct Hosted {
    pub(crate) view: MovieView,
    pub(crate) expose: Exposure<ImageSource>,
    name: &'static str,
    ready_event: &'static str,
}

impl Hosted {
    pub(crate) fn new(view: MovieView, name: &'static str, ready_event: &'static str) -> Self {
        Self {
            view,
            expose: Exposure::new(readiness(ready_event)),
            name,
            ready_event,
        }
    }

    pub(crate) fn restart(&mut self) -> Result<(), String> {
        self.view
            .restart()
            .map_err(|error| format!("{}: {error}", self.name))?;
        self.expose = Exposure::new(readiness(self.ready_event));
        Ok(())
    }

    pub(crate) fn frame(&mut self, dt: Duration) {
        let ticks = self.view.ticks_owed(dt);
        self.expose.frame(&mut self.view.player, ticks);
        for (key, value) in self.expose.take_string_changes() {
            self.view.set_string(&key, value);
        }
    }

    pub(crate) fn warm_images(&self, images: &mut ImageResolver, renderer: &mut Renderer) -> bool {
        self.expose.pending_image_changes().iter().any(|change| {
            let (ImageChange::Name { content, .. } | ImageChange::Instance { content, .. }) =
                change;
            content
                .as_ref()
                .is_some_and(|source| images.warm(renderer, source))
        })
    }

    pub(crate) fn apply_images(&mut self, images: &mut ImageResolver, renderer: &mut Renderer) {
        for (kind, load) in self.view.take_wanted_looks() {
            let read = match kind {
                GadgetKind::List => {
                    let style = images.list_style(renderer, &load);
                    let read = style.is_some();
                    self.view.set_list_style(&load, style);
                    read
                }
                GadgetKind::Check => {
                    let style = images.check_style(renderer, &load);
                    let read = style.is_some();
                    self.view.set_check_style(&load, style);
                    read
                }
                GadgetKind::Slider => {
                    let style = images.slider_style(renderer, &load);
                    let read = style.is_some();
                    self.view.set_slider_style(&load, style);
                    read
                }
                GadgetKind::Combo => {
                    let style = images.combo_style(renderer, &load);
                    let read = style.is_some();
                    self.view.set_combo_style(&load, style);
                    read
                }
                GadgetKind::Entry => {
                    let style = images.entry_style(&load);
                    let read = style.is_some();
                    self.view.set_entry_style(&load, style);
                    read
                }
                GadgetKind::MapWindow => {
                    let style = images.map_window_style(renderer, &load);
                    let read = style.is_some();
                    self.view.set_map_window_style(&load, style);
                    read
                }
            };
            if !read {
                println!("gadget: {load} did not read - its {kind:?} gadgets draw plainly");
            }
        }
        for name in self.view.take_wanted_images() {
            let content = images.resolve(renderer, &ImageSource::MappedImage(name.clone()));
            self.view.set_named_image(&name, content);
        }
        self.view
            .service_color_pickers(&mut |name| images.icons.palette(name));
        for change in self.expose.take_image_changes() {
            match change {
                ImageChange::Name { key, content } => {
                    let content = content.and_then(|source| images.resolve(renderer, &source));
                    self.view.set_named_image(&key, content);
                }
                ImageChange::Instance { instance, content } => {
                    let content = content.and_then(|source| images.resolve(renderer, &source));
                    self.view.set_surface(instance, content);
                }
            }
        }
    }
}

pub(crate) struct ImageResolver {
    icons: IconProvider,
    templates: mint_sage_apt_viewer::gadget::Templates,
    vfs: Arc<Vfs>,
    files: HashMap<String, Option<(TextureHandle, u32, u32)>>,
    unresolved: std::collections::HashSet<ImageSource>,
}

impl ImageResolver {
    pub(crate) fn new(
        vfs: Arc<Vfs>,
        mapped_images: Arc<MappedImageIndex>,
        command_buttons: Arc<CommandButtonIndex>,
    ) -> Self {
        Self {
            icons: IconProvider::new(Arc::clone(&vfs), mapped_images, command_buttons),
            templates: mint_sage_apt_viewer::gadget::Templates::new(Arc::clone(&vfs)),
            vfs,
            files: HashMap::new(),
            unresolved: std::collections::HashSet::new(),
        }
    }

    pub(crate) fn template_report(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<mint_sage_apt_viewer::gadget::ImageReport> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(mint_sage_apt_viewer::gadget::image_report(
            &layout,
            |name| icons.icon_by_image(renderer, name),
        ))
    }

    pub(crate) fn list_style(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<Arc<ListStyle>> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(Arc::new(ListLook::read(&layout).resolved(
            |name: &String| {
                let icon = icons.icon_by_image(renderer, name)?;
                Some((icon, icons.image_size(name).unwrap_or([0.0; 2])))
            },
        )))
    }

    pub(crate) fn check_style(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<Arc<CheckStyle>> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(Arc::new(CheckLook::read(&layout).resolved(
            |name: &String| icons.icon_by_image(renderer, name),
        )))
    }

    pub(crate) fn slider_style(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<Arc<SliderStyle>> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(Arc::new(SliderLook::read(&layout).resolved(
            |name: &String| {
                let icon = icons.icon_by_image(renderer, name)?;
                Some((icon, icons.image_size(name).unwrap_or([0.0; 2])))
            },
        )))
    }

    pub(crate) fn entry_style(
        &mut self,
        load: &str,
    ) -> Option<Arc<mint_sage_apt_viewer::gadget::entry::EntryLook>> {
        let layout = self.templates.get(load)?;
        Some(Arc::new(
            mint_sage_apt_viewer::gadget::entry::EntryLook::read(&layout),
        ))
    }

    pub(crate) fn combo_style(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<Arc<ComboStyle>> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(Arc::new(ComboLook::read(&layout).resolved(
            |name: &String| {
                let icon = icons.icon_by_image(renderer, name)?;
                Some((icon, icons.image_size(name).unwrap_or([0.0; 2])))
            },
        )))
    }

    pub(crate) fn map_window_style(
        &mut self,
        renderer: &mut Renderer,
        load: &str,
    ) -> Option<Arc<MapWindowStyle>> {
        let layout = self.templates.get(load)?;
        let icons = &mut self.icons;
        Some(Arc::new(MapWindowLook::read(&layout).resolved(
            |name: &String| {
                let mut icon = icons.icon_by_image(renderer, name)?;
                if let Some(texels) = icons.image_size(name) {
                    icon.uv_rect = inset_half_texel(icon.uv_rect, texels);
                }
                Some(icon)
            },
        )))
    }

    fn file_texture(
        &mut self,
        renderer: &mut Renderer,
        path: &str,
    ) -> Option<(TextureHandle, u32, u32)> {
        let vfs = &self.vfs;
        *self.files.entry(path.to_owned()).or_insert_with(|| {
            mint_sage_apt_viewer::host::load_texture_at_path(vfs, renderer, path)
        })
    }

    fn graph_content(
        &mut self,
        renderer: &mut Renderer,
        spec: &crate::post_match::GraphSpec,
    ) -> mint_sage_apt_viewer::host::GraphContent {
        use mint_sage_apt_viewer::host::{GraphLine, GraphMarker, GraphPlot};
        let plots = spec
            .plots
            .iter()
            .map(|(name, plot)| {
                let lines = plot
                    .lines
                    .iter()
                    .map(|line| GraphLine {
                        points: line.points.clone(),
                        color: line.rgba,
                        width: line.width,
                    })
                    .collect();
                let markers = plot
                    .markers
                    .iter()
                    .filter_map(|marker| {
                        let icon = self.icons.icon_by_image(renderer, &marker.image)?;
                        Some(GraphMarker {
                            at: marker.at,
                            size: marker.size,
                            icon,
                        })
                    })
                    .collect();
                (name.clone(), GraphPlot { lines, markers })
            })
            .collect();
        mint_sage_apt_viewer::host::GraphContent {
            select: crate::post_match::GRAPH_MODE.to_owned(),
            plots,
        }
    }

    fn map_window_content(
        &mut self,
        renderer: &mut Renderer,
        spec: &MapWindowSpec,
    ) -> MapWindowContent {
        let [lx, ly, hx, hy] = spec.extent;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = ((hx - lx) as f32, (hy - ly) as f32);
        #[allow(clippy::cast_precision_loss)]
        let fraction = |[x, y]: [i32; 2]| {
            [
                if w > 0.0 { (x - lx) as f32 / w } else { 0.5 },
                if h > 0.0 { (y - ly) as f32 / h } else { 0.5 },
            ]
        };
        MapWindowContent {
            backdrop: spec
                .backdrop
                .as_deref()
                .and_then(|name| self.icons.icon_by_image(renderer, name)),
            picture: spec.art.as_deref().and_then(|path| {
                let (texture, tw, th) = self.file_texture(renderer, path)?;
                Some(preview_art_icon(IconProvider::whole_texture_icon(
                    texture, tw, th,
                )))
            }),
            extent: [w, h],
            spots: spec
                .spots
                .iter()
                .map(|(at, label)| Spot {
                    at: fraction(*at),
                    label: label.clone(),
                })
                .collect(),
            enabled: spec.enabled,
        }
    }

    fn combo_content(&mut self, renderer: &mut Renderer, spec: &ComboSpec) -> ComboContent {
        let unit = |c: [u8; 4]| c.map(|v| f32::from(v) / 255.0);
        ComboContent {
            items: spec
                .items
                .iter()
                .map(|item| ComboItem {
                    text: item.text.clone(),
                    colour: item.colour.map(unit),
                    swatch: item.swatch.map(unit),
                    image: item.image.as_deref().and_then(|name| {
                        let icon = self.icons.icon_by_image(renderer, name)?;
                        Some((icon, self.icons.image_size(name).unwrap_or([20.0; 2])))
                    }),
                })
                .collect(),
            selected: spec.selected,
            enabled: spec.enabled,
        }
    }

    fn list_content(&mut self, renderer: &mut Renderer, spec: &ListSpec) -> ListContent {
        ListContent {
            columns: spec
                .columns
                .iter()
                .map(|&(percent, centre)| ListColumn {
                    percent: f32::from(percent),
                    align: if centre { Align::Centre } else { Align::Left },
                })
                .collect(),
            rows: spec
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            CellSpec::Text(text) => ListCell::Text(text.clone()),
                            CellSpec::Image { image, size } => ListCell::Image {
                                icon: image
                                    .as_deref()
                                    .and_then(|name| self.icons.icon_by_image(renderer, name)),
                                natural: image
                                    .as_deref()
                                    .and_then(|name| self.icons.image_size(name)),
                                size: size.map(|[w, h]| [f32::from(w), f32::from(h)]),
                            },
                            CellSpec::ScaledImage { image, percent } => {
                                match self.icons.image_extent(image) {
                                    Some([w, h]) => {
                                        let side = (w * f32::from(*percent) / 100.0).trunc();
                                        ListCell::Image {
                                            icon: self.icons.icon_by_image(renderer, image),
                                            natural: Some([w, h]),
                                            size: Some([side, side]),
                                        }
                                    }
                                    None => ListCell::Text(String::new()),
                                }
                            }
                        })
                        .collect()
                })
                .collect(),
            selected: spec.selected,
            scroll_bar: spec.scroll_bar,
            text_inset: f32::from(spec.text_inset_percent) / 100.0,
        }
    }

    pub(crate) fn warm(&mut self, renderer: &mut Renderer, source: &ImageSource) -> bool {
        let name = match source {
            ImageSource::MappedImage(name) => name.as_str(),
            ImageSource::Button(button) => match self.icons.button_image(button) {
                Some(name) => name,
                None => return false,
            },
            _ => return false,
        };
        if !self.icons.needs_decode(name) {
            return false;
        }
        let name = name.to_owned();
        let _ = self.icons.icon_by_image(renderer, &name);
        true
    }

    pub(crate) fn cache_len(&self) -> usize {
        self.icons.cache_len().saturating_add(self.files.len())
    }

    pub(crate) fn resolve(
        &mut self,
        renderer: &mut Renderer,
        source: &ImageSource,
    ) -> Option<SurfaceContent> {
        let icon = match source {
            ImageSource::Hidden => return Some(SurfaceContent::Hidden),
            ImageSource::Progress(percent) => {
                let covered = 1.0 - f32::from(*percent).clamp(0.0, 100.0) / 100.0;
                return Some(SurfaceContent::Progress(covered));
            }
            ImageSource::List(spec) => {
                return Some(SurfaceContent::List(Arc::new(
                    self.list_content(renderer, spec),
                )))
            }
            ImageSource::Combo(spec) => {
                return Some(SurfaceContent::Combo(Arc::new(
                    self.combo_content(renderer, spec),
                )))
            }
            ImageSource::MapWindow(spec) => {
                return Some(SurfaceContent::MapWindow(Arc::new(
                    self.map_window_content(renderer, spec),
                )))
            }
            ImageSource::Graph(spec) => {
                return Some(SurfaceContent::Graph(Arc::new(
                    self.graph_content(renderer, spec),
                )))
            }
            ImageSource::HelpText(lines) => {
                return Some(SurfaceContent::Text(Arc::new(
                    mint_sage_apt_viewer::host::TextBoxContent {
                        lines: lines
                            .iter()
                            .map(help_box::HelpLineSpec::to_text_line)
                            .collect(),
                        align: mint_sage_apt_viewer::host::TextAlign::Centre,
                    },
                )));
            }
            ImageSource::Message(lines) => {
                return Some(SurfaceContent::Text(Arc::new(
                    mint_sage_apt_viewer::host::TextBoxContent {
                        lines: lines
                            .iter()
                            .map(help_box::HelpLineSpec::to_text_line)
                            .collect(),
                        align: mint_sage_apt_viewer::host::TextAlign::Left,
                    },
                )));
            }
            ImageSource::Entry(spec) => {
                return Some(SurfaceContent::Entry(Arc::new(
                    mint_sage_apt_viewer::gadget::entry::EntryContent {
                        text: spec.text.clone(),
                        focused: spec.focused,
                        enabled: true,
                    },
                )))
            }
            ImageSource::Check(spec) => {
                return Some(SurfaceContent::Check(Arc::new(
                    mint_sage_apt_viewer::gadget::check::CheckContent {
                        checked: spec.checked,
                        label: None,
                        enabled: spec.enabled,
                    },
                )))
            }
            ImageSource::Slider(spec) => {
                return Some(SurfaceContent::Slider(
                    mint_sage_apt_viewer::gadget::slider::SliderContent {
                        value: spec.value,
                        enabled: spec.enabled,
                    },
                ))
            }
            ImageSource::Black => self.icons.black_icon(renderer),
            ImageSource::MappedImage(name) => self.icons.icon_by_image(renderer, name),
            ImageSource::Button(name) => self.icons.icon(renderer, name),
            ImageSource::MapPicture(map_path) => map_path
                .strip_suffix(".map")
                .and_then(|stem| self.file_texture(renderer, &format!("{stem}_pic.tga")))
                .map(|(texture, w, h)| IconProvider::whole_texture_icon(texture, w, h))
                .or_else(|| self.icons.icon_by_image(renderer, "MissingMap")),
            ImageSource::MapArt(path) => self
                .file_texture(renderer, path)
                .or_else(|| {
                    crate::hud::minimap_image_fallback(path)
                        .and_then(|path| self.file_texture(renderer, &path))
                })
                .map(|(texture, w, h)| {
                    map_art_icon(IconProvider::whole_texture_icon(texture, w, h))
                }),
        };
        if icon.is_none() && self.unresolved.insert(source.clone()) {
            println!("apt: image {source:?} did not resolve - its surface keeps its placeholder");
        }
        icon.map(SurfaceContent::Icon)
    }
}

fn preload_hud_textures<'a>(
    hud: impl IntoIterator<Item = Option<&'a mut Hosted>>,
    renderer: &mut Renderer,
) {
    let started = std::time::Instant::now();
    let (mut count, mut bytes) = (0usize, 0usize);
    for hosted in hud.into_iter().flatten() {
        let (n, b) = hosted.view.preload_textures(renderer);
        count = count.saturating_add(n);
        bytes = bytes.saturating_add(b);
    }
    #[allow(clippy::cast_precision_loss)]
    let megabytes = bytes as f64 / 1e6;
    println!(
        "apt overlay: {count} HUD texture(s) preloaded, {megabytes:.1} MB, in {} ms",
        started.elapsed().as_millis()
    );
}

pub(crate) struct AptOverlay {
    palantir: Hosted,
    radial: Hosted,
    radial_page: RadialPage,
    radial_shown: Vec<ResolvedSlot>,
    radial_hover: Option<usize>,
    radial_cost: (Duration, u32),
    palantir_cost: (Duration, u32),
    images: ImageResolver,
    side_bar: SideBarContent,
    events: Vec<MovieEvent>,
    placed: Vec<Placed>,
    since_load: Duration,
    unready_reported: bool,
    spell_book_ready: bool,
    hero_row: crate::hud::HeroRowMemory,
    palantir_rank: crate::hud::PalantirMemory,
    strings: Arc<mint_sage_str::StringTable>,
    help_config: HelpBoxConfig,
    help_timer: HelpBoxTimer,
    help_hover: Option<HoverKey>,
    help_shown: Option<HoverKey>,
    command_buttons: Arc<CommandButtonIndex>,
    world_hover: Option<(mint_sim::Entity, help_box::ObjectHelp, Option<f32>)>,
    spell_store: Option<Hosted>,
    spell_session: Option<SpellStoreSession>,
    spell_store_requested: bool,
    script_ui: crate::rules::map_scripts::UiView,
    objectives_closes_seen: u32,
    pointer_held: Option<PointerTarget>,
    spell_data: SpellBookData,
    spell_view: SpellBookView,
    spell_purchases: Vec<String>,
    view3d: crate::view3d::View3d,
    spell_store_cost: (Duration, u32),
    help_box_width: Option<f32>,
    help_box_text: Option<PreparedHelp>,
    menus: InGameMenus,
    backdrop: Option<Hosted>,
    backdrop_mode: Backdrop,
    gui_fx: Option<Hosted>,
    end_screen: EndScreenState,
    radar_pings: crate::radar_pings::RadarPingDriver<mint_sage_apt_player::Value>,
    notification: Option<NotificationHost>,
    notification_callbacks: Vec<notification_host::Callback>,
    objectives: Option<ObjectivesHost>,
    objectives_feed: objectives_host::Feed,
    campaign: bool,
    top_held: Option<TopTarget>,
    objectives_flash: ObjectivesFlash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopTarget {
    Objectives,
    Notification,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ObjectivesFlash {
    new_objective: bool,
    timed: bool,
    sent_new: bool,
    sent_timed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum EndScreenState {
    #[default]
    Untouched,
    Shown,
    Hiding(bool),
    Hidden,
}

impl AptOverlay {
    pub(crate) fn load(
        ui: &crate::ui_resources::UiResources,
        install_roots: &[PathBuf],
        window_size: (f32, f32),
        renderer: &mut Renderer,
        vfs: Arc<Vfs>,
        mapped_images: Arc<MappedImageIndex>,
        command_buttons: Arc<CommandButtonIndex>,
    ) -> Result<Self, String> {
        let (library, fonts, strings) = (
            Arc::clone(&ui.library),
            Arc::clone(&ui.fonts),
            Arc::clone(&ui.strings),
        );
        println!("apt overlay: {} movie(s) in the library", ui.movies);
        let mut host = |name: &'static str, ready_event: &'static str| {
            MovieView::load(
                Arc::clone(&library),
                install_roots.to_vec(),
                name,
                Arc::clone(&fonts),
                Arc::clone(&strings),
                window_size,
                Limits::default(),
                renderer,
            )
            .map(|view| Hosted::new(view, name, ready_event))
            .map_err(|error| format!("{name}: {error}"))
        };
        let mut palantir = host(PALANTIR_MOVIE, PALANTIR_READY)?;
        let mut radial = host(RADIAL_MOVIE, "")?;
        radial.view.disc_pictures = true;
        let help_config = help_box::load_help_box_config(&vfs)?;
        let spell_store = match host(SPELL_STORE_MOVIE, SPELL_STORE_READY) {
            Ok(hosted) => Some(hosted),
            Err(error) => {
                println!("apt overlay: the spell book will not open - {error}");
                None
            }
        };
        let mut menu = |name: &'static str| match host(name, MENU_READY) {
            Ok(hosted) => Some(hosted),
            Err(error) => {
                println!("apt overlay: {name} will not open - {error}");
                None
            }
        };
        let menus = InGameMenus::new(
            menu(QUIT_MENU_MOVIE),
            menu(OPTIONS_MOVIE),
            strings.get(RESTART_TEXT).map(str::to_owned),
            &strings,
        )
        .with_save_load(
            menu(crate::save_load::MOVIE),
            Arc::clone(&vfs),
            Arc::clone(&strings),
        );
        let backdrop = match host(BACKDROP_MOVIE, "") {
            Ok(mut hosted) => {
                hosted
                    .expose
                    .set_extern("InGame", mint_sage_apt_player::Value::Bool(true));
                Some(hosted)
            }
            Err(error) => {
                println!("apt overlay: the in-game backdrop will not show - {error}");
                None
            }
        };
        let gui_fx = match host(GUI_FX_MOVIE, GUI_FX_READY) {
            Ok(hosted) => Some(hosted),
            Err(error) => {
                println!("apt overlay: the end screen will not show - {error}");
                None
            }
        };
        let mut notification = match host(NOTIFICATION_MOVIE, notification_host::READY) {
            Ok(hosted) => Some(NotificationHost::new(hosted)),
            Err(error) => {
                println!("apt overlay: the notification box will not show - {error}");
                None
            }
        };
        let objectives = match host(objectives_host::MOVIE, objectives_host::READY) {
            Ok(hosted) => Some(ObjectivesHost::new(hosted)),
            Err(error) => {
                println!("apt overlay: the objectives screen will not open - {error}");
                None
            }
        };
        preload_hud_textures(
            [
                Some(&mut palantir),
                Some(&mut radial),
                notification.as_mut().map(NotificationHost::hosted_mut),
            ],
            renderer,
        );
        let spell_data = SpellBookData::load(&vfs)?;
        let view3d = crate::view3d::View3d::new(Arc::clone(&vfs));
        Ok(Self {
            palantir,
            radial,
            radial_page: RadialPage::default(),
            radial_shown: Vec::new(),
            radial_hover: None,
            radial_cost: (Duration::ZERO, 0),
            palantir_cost: (Duration::ZERO, 0),
            images: ImageResolver::new(vfs, mapped_images, Arc::clone(&command_buttons)),
            side_bar: SideBarContent::default(),
            events: Vec::new(),
            strings,
            help_config,
            help_timer: HelpBoxTimer::default(),
            help_hover: None,
            help_shown: None,
            command_buttons,
            world_hover: None,
            spell_store,
            spell_session: None,
            spell_store_requested: false,
            script_ui: crate::rules::map_scripts::UiView::default(),
            objectives_closes_seen: 0,
            pointer_held: None,
            spell_data,
            spell_view: SpellBookView::default(),
            spell_purchases: Vec::new(),
            view3d,
            spell_store_cost: (Duration::ZERO, 0),
            help_box_width: None,
            help_box_text: None,
            menus,
            backdrop,
            backdrop_mode: Backdrop::None,
            placed: Vec::new(),
            since_load: Duration::ZERO,
            unready_reported: false,
            spell_book_ready: false,
            hero_row: crate::hud::HeroRowMemory::default(),
            palantir_rank: crate::hud::PalantirMemory::default(),
            gui_fx,
            end_screen: EndScreenState::Untouched,
            radar_pings: crate::radar_pings::RadarPingDriver::default(),
            notification,
            notification_callbacks: Vec::new(),
            objectives,
            objectives_feed: objectives_host::Feed::default(),
            campaign: false,
            top_held: None,
            objectives_flash: ObjectivesFlash::default(),
        })
    }

    pub(crate) fn set_options_defaults(&mut self, defaults: crate::options::OptionsPreferences) {
        self.menus.set_options_defaults(defaults);
    }

    pub(crate) fn set_load_enabled(&mut self, enabled: bool) {
        self.menus.set_load_enabled(enabled);
    }

    pub(crate) fn frame(&mut self, hud: &HudState, dt: Duration) {
        let palantir_started = std::time::Instant::now();
        self.spell_view = crate::spell_book::spell_book_view(
            &self.spell_data,
            &self.command_buttons,
            &self.strings,
            hud.template.as_deref(),
            &hud.spell_book,
        );
        declare_palantir(&mut self.palantir.expose, hud);
        let spell_store_enabled = true;
        declare_player_magic(&mut self.palantir.expose, spell_store_enabled);
        let store_open = self.spell_session.is_some();
        let level_up = self
            .palantir_rank
            .update(hud, store_open, spell_store_enabled);
        crate::hud::declare_palantir_rank_feedback(
            &mut self.palantir.expose,
            level_up,
            self.palantir_rank.highlighted(),
        );
        self.side_bar.update(&hud.command_bar.bar);
        declare_command_bar(
            &mut self.palantir.expose,
            hud,
            &self.side_bar,
            SIDE_BAR_CLIP,
        );
        let hero_select_path = format!("_level0.{HERO_SELECT_CLIP}");
        declare_hero_select(
            &mut self.palantir.expose,
            hud,
            HERO_SELECT_CLIP,
            &hero_select_path,
        );
        let effects = self.hero_row.effects(hud);
        crate::hud::declare_hero_effects(&mut self.palantir.expose, HERO_SELECT_CLIP, &effects);
        if self.spell_book_ready {
            declare_spell_row(&mut self.palantir.expose, hud, SPELL_BOOK_CLIP);
        }
        if self.menus.is_open() {
            self.help_hover = None;
            self.world_hover = None;
        }
        self.help_box_frame();
        self.palantir.frame(dt);
        if self.help_box_width.is_none() {
            self.help_box_width = self.help_box_content_width();
            if self.help_box_width.is_some() {
                self.warm_help_glyphs();
            }
        }
        let mut palantir_spent = palantir_started.elapsed();
        let started = std::time::Instant::now();
        self.radial_frame(hud, dt);
        self.radial_cost.0 = self.radial_cost.0.saturating_add(started.elapsed());
        self.radial_cost.1 = self.radial_cost.1.saturating_add(1);
        let started = std::time::Instant::now();
        let mut open_store = std::mem::take(&mut self.spell_store_requested);
        for event in self.palantir.expose.take_events() {
            if let Some(command) = event.command() {
                self.side_bar.on_event(command, &hud.command_bar.bar);
                if OPEN_SPELL_STORE.contains(&command) {
                    open_store = true;
                    continue;
                }
                if command == OPEN_QUIT_MENU {
                    self.toggle_quit_menu();
                    continue;
                }
                if command == OPEN_OBJECTIVES && self.campaign {
                    self.objectives_flash.new_objective = false;
                    self.open_objectives();
                    continue;
                }
                if command == SPELL_BOOK_LOADED {
                    self.spell_book_ready = true;
                }
            }
            self.events.push(event);
        }
        self.place_slots(hud);
        palantir_spent = palantir_spent.saturating_add(started.elapsed());
        let started = std::time::Instant::now();
        self.spell_store_frame(dt, open_store);
        self.spell_store_cost.0 = self.spell_store_cost.0.saturating_add(started.elapsed());
        self.objectives_frame(dt);
        self.notification_frame(dt);
        self.objectives_flash_frame();
        self.menus.frame(dt, &mut self.events);
        self.backdrop_frame(dt);
        self.palantir_cost.0 = self.palantir_cost.0.saturating_add(palantir_spent);
        self.palantir_cost.1 = self.palantir_cost.1.saturating_add(1);
        self.since_load = self.since_load.saturating_add(dt);
        if !self.unready_reported && self.since_load >= READY_WARNING {
            let hosted = &self.palantir;
            let stats = hosted.expose.stats();
            if !stats.ready {
                println!(
                    "apt overlay: {} has not sent {} after {} s - {} call(s) and {} \
                     value(s) waiting",
                    hosted.name,
                    hosted.ready_event,
                    READY_WARNING.as_secs(),
                    stats.pending_calls,
                    stats.pending_values
                );
            }
            self.unready_reported = true;
        }
    }

    fn help_box_frame(&mut self) {
        let shown =
            self.help_timer
                .update(self.help_hover, self.since_load, self.help_config.delay);
        if shown != self.help_shown {
            self.help_shown = shown;
            match (shown, shown.and_then(|key| self.build_help_text(key))) {
                (_, Some(help)) => print_help_box(&help),
                (Some(key), None) => {
                    println!("help box: {key:?} hovered - no text for it");
                }
                (None, None) => println!("help box: hidden"),
            }
        }
        let state = match self.help_box_width {
            None => HelpBoxState::Sample,
            Some(width) => {
                let pending = self.help_hover;
                if pending != self.help_box_text.as_ref().map(|(k, ..)| *k) {
                    self.help_box_text = pending.and_then(|key| {
                        let text = self.build_help_text(key)?;
                        let (height, lines) = self.prepare_help_text(&text, width);
                        Some((key, height, lines))
                    });
                }
                help_box_state(self.help_box_text.as_ref(), shown)
            }
        };
        declare_help_box(&mut self.palantir.expose, HELP_BOX_CONTENT_KEY, &state);
    }

    fn build_help_text(&self, key: HoverKey) -> Option<HelpText> {
        match key {
            HoverKey::Native(name) => help_box::native_help(name, &self.strings),
            HoverKey::Hero(1) => help_box::native_help("SelectNearestBuilder", &self.strings),
            HoverKey::Hero(_) => None,
            HoverKey::Ring(_) | HoverKey::Bar(_) | HoverKey::Radial(_) | HoverKey::Flyout(_) => {
                let placed = self.placed.iter().find(|p| p.hover.0 == key)?;
                let (_, command_button, cost) = &placed.hover;
                let def = self.command_buttons.get(command_button);
                let face = def.zip(placed.face).and_then(|(d, s)| d.stance_face(s));
                Some(help_box::command_button_help(
                    face.and_then(|f| f.text_label.as_deref())
                        .or_else(|| def.and_then(|d| d.text_label.as_deref())),
                    face.and_then(|f| f.descript_label.as_deref())
                        .or_else(|| def.and_then(|d| d.descript_label.as_deref())),
                    &placed.action,
                    *cost,
                    &self.strings,
                ))
            }
            HoverKey::World(entity) => {
                let (_, help, max_health) = self
                    .world_hover
                    .as_ref()
                    .filter(|(hovered, ..)| *hovered == entity)?;
                Some(help_box::world_help(help, *max_health, &self.strings))
            }
        }
    }

    fn prepare_help_text(
        &mut self,
        text: &HelpText,
        width: f32,
    ) -> (f32, Arc<Vec<help_box::HelpLineSpec>>) {
        let lines = help_box::help_text_lines(text, &self.help_config);
        let drawn: Vec<_> = lines
            .iter()
            .map(help_box::HelpLineSpec::to_text_line)
            .collect();
        let at = resolve_path(&self.palantir.view.player, HELP_BOX_CLIP);
        let view = &mut self.palantir.view;
        let height = view.measure_text_box_height(&drawn, width, at);
        view.warm_text_box(&drawn, width, at);
        (height, Arc::new(lines))
    }

    fn warm_help_glyphs(&mut self) {
        let glyphs: String = (' '..='~').collect();
        let config = &self.help_config;
        let lines: Vec<_> = [
            &config.name,
            &config.cost,
            &config.shortcut,
            &config.description,
        ]
        .into_iter()
        .map(|style| mint_sage_apt_viewer::host::TextLine {
            text: glyphs.clone(),
            font: style.font.clone(),
            point_size: style.point_size,
            bold: style.bold,
            color: [1.0; 4],
        })
        .collect();
        let at = resolve_path(&self.palantir.view.player, HELP_BOX_CLIP);
        self.palantir.view.warm_text_box(&lines, f32::MAX, at);
    }

    fn help_box_content_width(&self) -> Option<f32> {
        let content = resolve_path(
            &self.palantir.view.player,
            &format!("{HELP_BOX_CLIP}.box.content"),
        )?;
        let [l, _, r, _] = self.palantir.view.stage_rect(content)?;
        let width = r - l;
        (width > 1.0).then_some(width)
    }

    fn spell_store_frame(&mut self, dt: Duration, mut toggle: bool) {
        if self.script_ui.spell_store_disabled {
            if self.spell_session.as_ref().is_some_and(|s| !s.closing) {
                println!("spell book: a script disabled it - closing");
                self.close_spell_store();
            }
            if toggle && self.spell_session.is_none() {
                println!("spell book: a script disabled it - not opening");
                toggle = false;
            }
        }
        let Some(store) = self.spell_store.as_mut() else {
            return;
        };
        if toggle {
            match self.spell_session.as_mut() {
                Some(session) => session.closing = true,
                None => match store.restart() {
                    Ok(()) => {
                        println!("spell book: open");
                        self.spell_session = Some(SpellStoreSession::open(&self.spell_view));
                    }
                    Err(error) => println!("spell book: did not open - {error}"),
                },
            }
        }
        let Some(session) = self.spell_session.as_mut() else {
            return;
        };
        declare_spell_store(&mut store.expose, &self.spell_view, session);
        store.frame(dt);
        self.spell_store_cost.1 = self.spell_store_cost.1.saturating_add(1);
        for event in store.expose.take_events() {
            if event.command() == Some(PLAY_SOUND) {
                self.events.push(event);
                continue;
            }
            let mint_sage_apt_player::MovieEvent::Command { name, args, .. } = event else {
                continue;
            };
            match session.on_event(&self.spell_view, &name, &args) {
                SpellStoreEvent::Nothing => {}
                SpellStoreEvent::Closed(purchases) => {
                    println!("spell book: closed, buying {purchases:?}");
                    self.spell_purchases.extend(purchases);
                    self.spell_session = None;
                    return;
                }
            }
        }
    }

    fn backdrop_frame(&mut self, dt: Duration) {
        let wanted = wanted_backdrop(
            self.menus.options_shown(),
            self.menus.quit_shown(),
            self.spell_session
                .as_ref()
                .is_some_and(|session| !session.closing),
        );
        let Some(backdrop) = self.backdrop.as_mut() else {
            self.backdrop_mode = wanted;
            return;
        };
        if let Some((name, args)) = backdrop_call(self.backdrop_mode, wanted) {
            backdrop.expose.call("backdrop", name, args);
            crate::diag::diag!("apt: backdrop {name}");
        }
        self.backdrop_mode = wanted;
        backdrop.frame(dt);
        for event in backdrop.expose.take_events() {
            if event.command() == Some(PLAY_SOUND) {
                self.events.push(event);
            }
        }
    }

    pub(crate) fn provisional_spells(&self) -> &[String] {
        self.spell_session
            .as_ref()
            .map_or(&[], |session| session.provisional.as_slice())
    }

    pub(crate) fn show_end_game(&mut self, caption_label: &str, arguments: [String; 3]) -> bool {
        let Some(gui_fx) = self.gui_fx.as_mut() else {
            return false;
        };
        let caption = self.strings.fetch(caption_label);
        println!(
            "end game: {} = {caption:?}",
            crate::end_game::CAPTION_VARIABLE
        );
        gui_fx
            .expose
            .string(crate::end_game::CAPTION_VARIABLE, Some(caption));
        gui_fx.expose.fire(
            "ShowEndGame",
            arguments
                .into_iter()
                .map(|argument| mint_sage_apt_player::Value::String(argument.into()))
                .collect(),
        );
        self.end_screen = EndScreenState::Shown;
        true
    }

    pub(crate) fn hide_end_game(&mut self) {
        if let Some(gui_fx) = self.gui_fx.as_mut() {
            gui_fx.expose.fire("HideEndGame", Vec::new());
            self.end_screen = EndScreenState::Hiding(false);
        }
    }

    pub(crate) fn end_screen_hidden(&self) -> bool {
        self.gui_fx.is_none() || self.end_screen == EndScreenState::Hidden
    }

    pub(crate) fn end_screen_frame(&mut self, dt: Duration) -> Vec<String> {
        self.view3d.advance(dt);
        let (Some(gui_fx), true) = (
            self.gui_fx.as_mut(),
            self.end_screen != EndScreenState::Untouched,
        ) else {
            return Vec::new();
        };
        gui_fx.frame(dt);
        let sounds = gui_fx
            .expose
            .take_events()
            .into_iter()
            .filter(|event| event.command() == Some(PLAY_SOUND))
            .filter_map(|event| event.args().map(str::to_owned))
            .collect();
        if let EndScreenState::Hiding(seen_playing) = self.end_screen {
            let player = &gui_fx.view.player;
            let playing = END_GAME_CLIPS.iter().any(|name| {
                resolve_path(player, name)
                    .and_then(|id| player.instance(id))
                    .and_then(|instance| instance.clip())
                    .is_some_and(|clip| clip.playing)
            });
            self.end_screen = match (seen_playing, playing) {
                (false, true) => EndScreenState::Hiding(true),
                (true, false) => EndScreenState::Hidden,
                (seen, _) => EndScreenState::Hiding(seen),
            };
        }
        sounds
    }

    pub(crate) fn take_spell_purchases(&mut self) -> Vec<String> {
        std::mem::take(&mut self.spell_purchases)
    }

    fn radial_frame(&mut self, hud: &HudState, dt: Duration) {
        self.radial_page.follow(hud.radial.map(|r| r.entity));
        let anchor = hud.radial.and_then(|r| r.anchor);
        self.radial_shown = match anchor {
            Some(_) => crate::radial::visible(
                &hud.command_bar.radial,
                hud.command_bar.radial_root,
                self.radial_page.current(),
            )
            .into_iter()
            .take(crate::hud::RADIAL_SLOTS)
            .cloned()
            .collect(),
            None => Vec::new(),
        };
        let (sw, sh) = self.radial.view.stage_size();
        let (ww, wh) = self.radial.view.window_size;
        let stretch = Letterbox::stretch(sw, sh, ww, wh);
        let [sx, sy] = stretch.scale;
        let size = BUTTON_STAGE * sy;
        let rects: Vec<[f32; 4]> = anchor.map_or_else(Vec::new, |anchor| {
            crate::radial::layout(
                anchor,
                self.radial_shown.len(),
                size,
                wh * RADIAL_MAX_RADIUS,
            )
        });
        let stage: Vec<[f32; 4]> = rects
            .iter()
            .map(|&[x, y, w, h]| to_stage([x, y, w, h], sx, sy))
            .collect();
        let shown: Vec<&ResolvedSlot> = self.radial_shown.iter().collect();
        declare_radial(
            &mut self.radial.expose,
            &shown,
            &stage,
            self.radial_hover,
            &hud.flash,
        );
        self.radial.frame(dt);
    }

    pub(crate) fn radial_action(&mut self, action: &SlotAction) -> bool {
        match *action {
            SlotAction::RadialPush { start, count } => {
                self.radial_page.push(start, count);
                println!(
                    "radial menu: page slots {}-{}",
                    start.saturating_add(1),
                    start.saturating_add(count)
                );
                true
            }
            SlotAction::RadialPop => {
                self.radial_page.pop();
                println!("radial menu: back");
                true
            }
            _ => false,
        }
    }

    pub(crate) fn press_radial(&mut self, position: usize) -> Option<SlotAction> {
        let action = self.radial_shown.get(position)?.action.clone();
        if !self.radial_action(&action) {
            println!("radial menu: pressed {action:?}");
        }
        Some(action)
    }

    pub(crate) fn radial_cost(&self) -> (Duration, u32) {
        let (total, frames) = self.radial_cost;
        (total.checked_div(frames.max(1)).unwrap_or_default(), frames)
    }

    pub(crate) fn palantir_cost(&self) -> (Duration, u32) {
        let (total, frames) = self.palantir_cost;
        (total.checked_div(frames.max(1)).unwrap_or_default(), frames)
    }

    fn report_costs(&mut self) {
        if self.palantir_cost.1 < 120 {
            return;
        }
        if std::env::var_os("MINT_UI_COSTS").is_some() {
            let (palantir, frames) = self.palantir_cost();
            let (radial, _) = self.radial_cost();
            println!(
                "ui costs: Palantir {:.3} ms, radial menu {:.3} ms mean over {frames} frames \
                 (declare, tick, build)",
                palantir.as_secs_f64() * 1000.0,
                radial.as_secs_f64() * 1000.0
            );
            let (spent, open) = self.spell_store_cost;
            if open > 0 {
                let mean = spent.checked_div(open).unwrap_or_default();
                println!(
                    "ui costs: spell book {:.3} ms mean over the {open} of them it was open \
                     (declare, tick, View3D, build)",
                    mean.as_secs_f64() * 1000.0
                );
            }
        }
        self.palantir_cost = (Duration::ZERO, 0);
        self.radial_cost = (Duration::ZERO, 0);
        self.spell_store_cost = (Duration::ZERO, 0);
    }

    pub(crate) fn radial_shown(&self) -> &[ResolvedSlot] {
        &self.radial_shown
    }

    pub(crate) fn take_events(&mut self) -> Vec<MovieEvent> {
        let mut events = std::mem::take(&mut self.events);
        events.extend(self.palantir.expose.take_events());
        events.extend(self.radial.expose.take_events());
        events
    }

    fn place_slots(&mut self, hud: &HudState) {
        self.placed.clear();
        let ring_pos = |p: usize| u8::try_from(p).unwrap_or(u8::MAX);
        let face_of = |slot: &ResolvedSlot| {
            hud.stance
                .filter(|_| matches!(slot.action, SlotAction::ToggleStance(_)))
        };
        for (p, slot) in hud.command_bar.ring.iter().enumerate().take(RING_SLOTS) {
            let Some(slot) = slot else { continue };
            if let Some(instance) =
                resolve_path(&self.palantir.view.player, &format!("CommandButtons.{p}"))
            {
                self.placed.push(Placed {
                    movie: Movie::Palantir,
                    instance,
                    action: slot.action.clone(),
                    hover: (
                        HoverKey::Ring(ring_pos(p)),
                        slot.command_button.clone(),
                        slot.cost,
                    ),
                    face: face_of(slot),
                });
            }
        }
        for (i, slot) in hud.command_bar.bar.iter().enumerate().take(BAR_SLOTS) {
            if let Some(instance) = resolve_path(
                &self.palantir.view.player,
                &under(SIDE_BAR_CLIP, &format!("ButtonSet.Button{i}.Button")),
            ) {
                self.placed.push(Placed {
                    movie: Movie::Palantir,
                    instance,
                    action: slot.action.clone(),
                    hover: (
                        HoverKey::Bar(ring_pos(i)),
                        slot.command_button.clone(),
                        slot.cost,
                    ),
                    face: face_of(slot),
                });
            }
        }
        if let Some(position) = hud.stance_flyout {
            for (i, slot) in hud.command_bar.stance_flyout.iter().enumerate() {
                let Some(instance) = resolve_path(
                    &self.palantir.view.player,
                    &crate::hud::flyout_frame(position, i),
                ) else {
                    continue;
                };
                self.placed.push(Placed {
                    movie: Movie::Palantir,
                    instance,
                    action: slot.action.clone(),
                    hover: (
                        HoverKey::Flyout(ring_pos(i)),
                        slot.command_button.clone(),
                        slot.cost,
                    ),
                    face: face_of(slot),
                });
            }
        }
        for (i, slot) in self.radial_shown.iter().enumerate() {
            if let Some(instance) = resolve_path(&self.radial.view.player, &i.to_string()) {
                self.placed.push(Placed {
                    movie: Movie::Radial,
                    instance,
                    action: slot.action.clone(),
                    hover: (
                        HoverKey::Radial(ring_pos(i)),
                        slot.command_button.clone(),
                        slot.cost,
                    ),
                    face: face_of(slot),
                });
            }
        }
    }

    pub(crate) fn scale_mode(&self) -> mint_sage_apt_viewer::ScaleMode {
        self.palantir.view.scale_mode
    }

    pub(crate) fn toggle_scale_mode(&mut self) -> mint_sage_apt_viewer::ScaleMode {
        let next = self.scale_mode().toggled();
        self.set_scale_mode(next);
        next
    }

    pub(crate) fn set_scale_mode(&mut self, mode: mint_sage_apt_viewer::ScaleMode) {
        self.palantir.view.scale_mode = mode;
        self.radial.view.scale_mode = ScaleMode::Stretch;
        if let Some(store) = self.spell_store.as_mut() {
            store.view.scale_mode = mode;
        }
        if let Some(backdrop) = self.backdrop.as_mut() {
            backdrop.view.scale_mode = mode;
        }
        for menu in self.menus.all_mut() {
            menu.view.scale_mode = mode;
        }
        for hosted in self.top_hosted_mut() {
            hosted.view.scale_mode = mode;
        }
    }

    pub(crate) fn set_image_modes(&mut self, mode: ImageModes) {
        self.palantir.view.image_modes = mode;
        self.radial.view.image_modes = mode;
        if let Some(store) = self.spell_store.as_mut() {
            store.view.image_modes = mode;
        }
        if let Some(backdrop) = self.backdrop.as_mut() {
            backdrop.view.image_modes = mode;
        }
        for menu in self.menus.all_mut() {
            menu.view.image_modes = mode;
        }
        for hosted in self.top_hosted_mut() {
            hosted.view.image_modes = mode;
        }
    }

    pub(crate) fn toggle_image_modes(&mut self) -> ImageModes {
        let next = self.palantir.view.image_modes.next();
        self.set_image_modes(next);
        next
    }

    pub(crate) fn toggle_text_anchor(&mut self) -> TextAnchor {
        let next = self.palantir.view.text_anchor.toggled();
        self.set_text_anchor(next);
        next
    }

    pub(crate) fn toggle_timer_overlay_direction(&mut self) -> bool {
        let next = !self.palantir.view.timer_overlay_covers_remaining;
        self.palantir.view.timer_overlay_covers_remaining = next;
        self.radial.view.timer_overlay_covers_remaining = next;
        next
    }

    pub(crate) fn set_text_anchor(&mut self, anchor: TextAnchor) {
        self.palantir.view.text_anchor = anchor;
    }

    pub(crate) fn resize(&mut self, width: f32, height: f32) {
        self.palantir.view.window_size = (width, height);
        self.radial.view.window_size = (width, height);
        if let Some(store) = self.spell_store.as_mut() {
            store.view.window_size = (width, height);
        }
        if let Some(backdrop) = self.backdrop.as_mut() {
            backdrop.view.window_size = (width, height);
        }
        for menu in self.menus.all_mut() {
            menu.view.window_size = (width, height);
        }
        for hosted in self.top_hosted_mut() {
            hosted.view.window_size = (width, height);
        }
    }

    pub(crate) fn icon_cache_len(&self) -> usize {
        self.images.cache_len()
    }

    pub(crate) fn route_native_pointer_move(&mut self, position: [f32; 2]) -> bool {
        if self.menus.is_open() {
            self.menus.pointer_moved(position);
            self.help_hover = None;
            self.radial_hover = None;
            return true;
        }
        if let Some(claimed) = self.top_pointer_move(position) {
            return claimed;
        }
        if let (Some(store), true) = (self.spell_store.as_mut(), self.spell_session.is_some()) {
            let stage = store.view.resolve_stage_point(position);
            let before = store.view.player.hovered();
            let _ = store.view.player.pointer_move(stage[0], stage[1]);
            log_rollover(
                "spell_store",
                &store.view.player,
                before,
                store.view.player.hovered(),
            );
            self.help_hover = None;
            self.radial_hover = None;
            return true;
        }
        let view = &mut self.palantir.view;
        let stage = view.resolve_stage_point(position);
        let before = view.player.hovered();
        let _ = view.player.pointer_move(stage[0], stage[1]);
        let hovered = view.player.hovered();
        log_rollover("palantir", &view.player, before, hovered);
        let hit = hovered.is_some();
        self.radial_hover = self.radial_button_at(position);
        self.help_hover = hovered.and_then(|id| self.native_hover_key(id));
        hit
    }

    pub(crate) fn native_hover_target(&self) -> Option<(&'static str, String)> {
        if self.spell_session.is_some() {
            if let Some(store) = self.spell_store.as_ref() {
                if let Some(id) = store.view.player.hovered() {
                    return Some(("spell_store", store.view.player.target_path(id)));
                }
            }
        }
        let id = self.palantir.view.player.hovered()?;
        Some(("palantir", self.palantir.view.player.target_path(id)))
    }

    pub(crate) fn refine_command_hover(&mut self, hovered: Option<mint_ui::WidgetId>) {
        if self.help_hover.is_some() || self.spell_session.is_some() {
            return;
        }
        self.help_hover = hovered
            .and_then(|id| usize::try_from(id.0).ok())
            .and_then(|index| self.placed.get(index))
            .map(|placed| placed.hover.0);
    }

    pub(crate) fn set_world_hover(
        &mut self,
        hover: Option<(mint_sim::Entity, help_box::ObjectHelp, Option<f32>)>,
    ) {
        self.world_hover = hover;
        if self.help_hover.is_none() && self.spell_session.is_none() {
            self.help_hover = self.world_hover.as_ref().map(|(e, ..)| HoverKey::World(*e));
        }
    }

    fn native_hover_key(&self, id: InstanceId) -> Option<HoverKey> {
        let player = &self.palantir.view.player;
        let mut at = Some(id);
        for _ in 0..8 {
            let Some(current) = at else { break };
            let Some(inst) = player.instance(current) else {
                break;
            };
            let name = inst.name();
            if let Some(n) = hero_number(&name) {
                return Some(HoverKey::Hero(n));
            }
            let key: Option<&'static str> = if name.eq_ignore_ascii_case("Options") {
                Some("Options")
            } else if name.eq_ignore_ascii_case("PlayerMagic") {
                Some("PlayerMagic")
            } else if name.eq_ignore_ascii_case("Objectives") {
                Some("Objectives")
            } else if name.eq_ignore_ascii_case("Players") {
                Some("Players")
            } else {
                None
            };
            if let Some(key) = key {
                return Some(HoverKey::Native(key));
            }
            at = inst.parent();
        }
        None
    }

    pub(crate) fn pressed_hero(&self) -> Option<u8> {
        let id = self.palantir.view.player.hovered()?;
        match self.native_hover_key(id) {
            Some(HoverKey::Hero(n)) => Some(n),
            _ => None,
        }
    }

    fn radial_button_at(&mut self, position: [f32; 2]) -> Option<usize> {
        let ids: Vec<InstanceId> = (0..self.radial_shown.len())
            .filter_map(|i| resolve_path(&self.radial.view.player, &i.to_string()))
            .collect();
        let rects = self.radial.view.slot_rects(&ids);
        ids.iter().position(|id| {
            rects.get(id).is_some_and(|&[l, t, r, b]| {
                position[0] >= l && position[0] < r && position[1] >= t && position[1] < b
            })
        })
    }

    pub(crate) fn route_native_pointer_button(&mut self, pressed: bool) {
        if self.menus.is_open() {
            self.menus.pointer_button(pressed);
            return;
        }
        if self.top_pointer_button(pressed) {
            return;
        }
        let store_open = self.spell_session.is_some();
        match (
            self.spell_store.as_mut(),
            pointer_button_target(pressed, store_open, &mut self.pointer_held),
        ) {
            (Some(store), PointerTarget::SpellStore) => {
                let hovered = store.view.player.hovered();
                let _ = store.view.player.pointer_button(pressed);
                log_button("spell_store", &store.view.player, hovered, pressed);
            }
            _ => {
                let hovered = self.palantir.view.player.hovered();
                let _ = self.palantir.view.player.pointer_button(pressed);
                log_button("palantir", &self.palantir.view.player, hovered, pressed);
            }
        }
    }

    pub(crate) fn hit_targets(&mut self) -> Vec<HitTarget> {
        if self.spell_session.is_some() || self.menus.is_open() || self.objectives_open() {
            return Vec::new();
        }
        let ids = |movie: Movie| -> Vec<InstanceId> {
            self.placed
                .iter()
                .filter(|s| s.movie == movie)
                .map(|s| s.instance)
                .collect()
        };
        let (palantir_ids, radial_ids) = (ids(Movie::Palantir), ids(Movie::Radial));
        let palantir_rects = self.palantir.view.slot_rects(&palantir_ids);
        let radial_rects = self.radial.view.slot_rects(&radial_ids);
        let mut out = Vec::with_capacity(self.placed.len());
        for (index, slot) in self.placed.iter().enumerate() {
            let rects = match slot.movie {
                Movie::Palantir => &palantir_rects,
                Movie::Radial => &radial_rects,
            };
            let (Some(&rect), Ok(id)) = (rects.get(&slot.instance), u64::try_from(index)) else {
                continue;
            };
            out.push(target(id, rect));
        }
        out
    }

    pub(crate) fn action_for(&self, id: WidgetId) -> Option<SlotAction> {
        let index = usize::try_from(id.0).ok()?;
        self.placed.get(index).map(|slot| slot.action.clone())
    }

    pub(crate) fn button_for(&self, id: WidgetId) -> Option<String> {
        let index = usize::try_from(id.0).ok()?;
        self.placed.get(index).map(|slot| slot.hover.1.clone())
    }

    pub(crate) fn minimap_rect(&mut self) -> Option<[f32; 4]> {
        if self.spell_session.is_some() || self.menus.is_open() || self.objectives_open() {
            return None;
        }
        let clip = resolve_path(&self.palantir.view.player, "Radar.RadarClip")?;
        self.palantir.view.slot_rect(clip)
    }

    pub(crate) fn drive_radar_pings(
        &mut self,
        ring: &crate::radar_events::RadarEventRing,
        now: f32,
        to_pixel: impl FnMut((f32, f32)) -> Option<(f32, f32)>,
    ) {
        let Some(radar) = resolve_path(&self.palantir.view.player, "Radar") else {
            return;
        };
        let mut host = PalantirPingHost {
            player: &mut self.palantir.view.player,
            clip: radar,
        };
        self.radar_pings.drive(&mut host, ring, now, to_pixel);
    }

    pub(crate) fn text_settled(&self) -> bool {
        self.palantir.view.text_settled() && self.radial.view.text_settled()
    }

    pub(crate) fn settle_text(&mut self, renderer: &mut Renderer, max_passes: u32) -> u32 {
        self.apply_images(renderer, false);
        let mut passes = 0u32;
        loop {
            self.palantir.view.build(renderer);
            self.radial.view.build(renderer);
            passes = passes.saturating_add(1);
            if self.text_settled() || passes >= max_passes {
                break;
            }
        }
        passes
    }

    fn apply_images(&mut self, renderer: &mut Renderer, hud_hidden: bool) {
        if hud_hidden {
            let _ = self.palantir.warm_images(&mut self.images, renderer)
                || self.radial.warm_images(&mut self.images, renderer)
                || self
                    .notification
                    .as_ref()
                    .is_some_and(|n| n.hosted().warm_images(&mut self.images, renderer));
        } else {
            self.palantir.apply_images(&mut self.images, renderer);
            self.radial.apply_images(&mut self.images, renderer);
            if let Some(backdrop) = self.backdrop.as_mut() {
                backdrop.apply_images(&mut self.images, renderer);
            }
            if let (Some(store), true) = (self.spell_store.as_mut(), self.spell_session.is_some()) {
                store.apply_images(&mut self.images, renderer);
            }
        }
        if let Some(menu) = self.menus.shown_mut() {
            menu.apply_images(&mut self.images, renderer);
        }
        self.apply_top_images(renderer, hud_hidden);
        if let (Some(gui_fx), true) = (
            self.gui_fx.as_mut(),
            self.end_screen != EndScreenState::Untouched,
        ) {
            gui_fx.apply_images(&mut self.images, renderer);
        }
    }

    pub(crate) fn build_and_draw(
        &mut self,
        renderer: &mut Renderer,
        dots: &[MinimapDot],
        view_box: Option<[[f32; 2]; 4]>,
        hud_hidden: bool,
    ) -> Vec<ScreenDraw<'_>> {
        self.apply_images(renderer, hud_hidden);
        let started = std::time::Instant::now();
        if !hud_hidden {
            self.radial.view.build(renderer);
        }
        self.radial_cost.0 = self.radial_cost.0.saturating_add(started.elapsed());
        let started = std::time::Instant::now();
        if !hud_hidden {
            self.palantir.view.build(renderer);
        }
        self.palantir_cost.0 = self.palantir_cost.0.saturating_add(started.elapsed());
        if let (Some(backdrop), false) = (self.backdrop.as_mut(), hud_hidden) {
            self.view3d.draw(renderer, &mut backdrop.view);
            backdrop.view.build(renderer);
        }
        let store_open = self.spell_session.is_some() && !hud_hidden;
        if let (Some(store), true) = (self.spell_store.as_mut(), store_open) {
            let started = std::time::Instant::now();
            self.view3d.draw(renderer, &mut store.view);
            store.view.build(renderer);
            self.spell_store_cost.0 = self.spell_store_cost.0.saturating_add(started.elapsed());
        }
        self.build_top(renderer, hud_hidden);
        if let Some(menu) = self.menus.shown_mut() {
            self.view3d.draw(renderer, &mut menu.view);
            menu.view.build(renderer);
        }
        let end_screen_up = self.build_end_screen(renderer);
        self.report_costs();
        let minimap = if hud_hidden {
            Vec::new()
        } else {
            self.minimap_draws(dots, view_box)
        };
        let mut all = Vec::new();
        if !hud_hidden {
            all.extend(self.radial.view.draws());
            all.extend(self.palantir.view.draws());
        }
        all.extend(minimap);
        if let (Some(backdrop), false) = (self.backdrop.as_ref(), hud_hidden) {
            all.extend(backdrop.view.draws());
        }
        if let (Some(store), true) = (self.spell_store.as_ref(), store_open) {
            all.extend(store.view.draws());
        }
        all.extend(self.top_draws(hud_hidden));
        if let (Some(gui_fx), true) = (self.gui_fx.as_ref(), end_screen_up) {
            all.extend(gui_fx.view.draws());
        }
        if let Some(menu) = self.menus.shown() {
            all.extend(menu.view.draws());
        }
        all
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menus.is_open()
    }

    pub(crate) fn toggle_quit_menu(&mut self) {
        force_close_store_on_open(
            self.menus.is_open(),
            self.spell_session.as_mut(),
            &self.spell_view,
        );
        self.menus.toggle();
    }

    pub(crate) fn request_spell_store_toggle(&mut self) {
        self.spell_store_requested = true;
    }

    pub(crate) fn spell_store_showing(&self) -> bool {
        self.spell_session.as_ref().is_some_and(|s| !s.closing)
    }

    pub(crate) fn set_script_ui(&mut self, view: crate::rules::map_scripts::UiView) {
        self.script_ui = view;
    }

    pub(crate) fn spell_store_open(&self) -> bool {
        self.spell_session.is_some()
    }

    pub(crate) fn close_spell_store(&mut self) {
        commit_close(self.spell_session.as_mut(), &self.spell_view);
    }

    pub(crate) fn escape_spell_store(&mut self) {
        reset_then_close(self.spell_session.as_mut(), &self.spell_view);
    }

    pub(crate) fn take_menu_requests(&mut self) -> Vec<MenuRequest> {
        self.menus.take_requests()
    }

    fn build_end_screen(&mut self, renderer: &mut Renderer) -> bool {
        match (self.gui_fx.as_mut(), self.end_screen) {
            (Some(gui_fx), state) if state != EndScreenState::Untouched => {
                self.view3d.draw(renderer, &mut gui_fx.view);
                gui_fx.view.build(renderer);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn draw_banner(
        &mut self,
        renderer: &mut Renderer,
        lines: &[String],
    ) -> Vec<ScreenDraw<'_>> {
        let (width, _height) = self.palantir.view.window_size;
        let white = [1.0, 1.0, 1.0, 1.0];
        let line_height = 28.0;
        let top = 48.0;
        let left = (width * 0.5 - 220.0).max(16.0);
        let mut quads = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = top + line_height * row as f32;
            quads.extend(self.palantir.view.draw_label(
                renderer,
                line,
                [left, y],
                white,
                UiQuad::UNCLIPPED,
            ));
        }
        quads.into_iter().map(ScreenDraw::Quad).collect()
    }

    fn minimap_draws(
        &mut self,
        dots: &[MinimapDot],
        view_box: Option<[[f32; 2]; 4]>,
    ) -> Vec<ScreenDraw<'static>> {
        let Some([l, t, r, b]) = self.minimap_rect() else {
            return Vec::new();
        };
        let (w, h) = (r - l, b - t);
        let (cell_w, cell_h) = (w / RADAR_CELLS, h / RADAR_CELLS);
        let mut out = Vec::new();
        let mut cells = |from: [f32; 2], size: f32, color: [f32; 4]| {
            let mut quad = UiQuad::new(from[0], from[1], size * cell_w, size * cell_h);
            quad.color_multiply = color;
            quad.clip = [l, t, r, b];
            out.push(ScreenDraw::Quad(quad));
        };
        for dot in dots {
            let col = (dot.uv[0].clamp(0.0, 1.0) * RADAR_CELLS).floor();
            let row = ((1.0 - dot.uv[1].clamp(0.0, 1.0)) * RADAR_CELLS).floor();
            let at = |dx: f32, dy: f32| [l + (col + dx) * cell_w, b - (row + dy + 1.0) * cell_h];
            match dot.mark {
                RadarMark::Blip => cells(at(-1.0, 0.0), 2.0, dot.color),
                RadarMark::Hero => {
                    let [red, green, blue, alpha] = dot.color;
                    for dy in 0..6_usize {
                        for dx in 0..6_usize {
                            let a = mirrored_alpha(&HERO_HALO_QUADRANT, dx, dy, 3);
                            #[allow(clippy::cast_precision_loss)]
                            cells(
                                at(dx as f32 - 3.0, dy as f32 - 3.0),
                                1.0,
                                [red, green, blue, alpha * f32::from(a) / 255.0],
                            );
                        }
                    }
                    let light = |c: f32| c + (1.0 - c) * 0.5;
                    let lightened = [light(red), light(green), light(blue), alpha];
                    for dy in 0..4_usize {
                        for dx in 0..4_usize {
                            let a = mirrored_alpha(&HERO_DOT_QUADRANT, dx, dy, 2);
                            #[allow(clippy::cast_precision_loss)]
                            cells(
                                at(dx as f32 - 2.0, dy as f32 - 2.0),
                                1.0,
                                [
                                    lightened[0],
                                    lightened[1],
                                    lightened[2],
                                    lightened[3] * f32::from(a) / 255.0,
                                ],
                            );
                        }
                    }
                }
                RadarMark::Ring(radius) => {
                    #[allow(clippy::cast_possible_truncation)]
                    ring_cells(radius as i32, |dx, dy| {
                        #[allow(clippy::cast_precision_loss)]
                        cells(at(dx as f32, dy as f32), 1.0, dot.color);
                    });
                }
                RadarMark::Wall {
                    facing,
                    half_major,
                    half_minor,
                    bounding_radius_cells,
                    scale,
                } => {
                    for (dx, dy, alpha) in
                        wall_cells(facing, half_major, half_minor, bounding_radius_cells, scale)
                    {
                        cells(
                            at(dx, dy),
                            1.0,
                            [
                                dot.color[0],
                                dot.color[1],
                                dot.color[2],
                                dot.color[3] * alpha,
                            ],
                        );
                    }
                }
            }
        }
        if let Some(uv) = view_box {
            let (_, window_h) = self.palantir.view.window_size;
            let width = (window_h / 600.0).max(1.0);
            out.extend(
                view_box_quads(uv, [l, t, r, b], width)
                    .into_iter()
                    .map(ScreenDraw::Quad),
            );
        }
        out
    }
}

pub(crate) const RADAR_ART_ALPHA: f32 = 0.31;

pub(crate) fn map_art_icon(
    mut icon: mint_sage_apt_viewer::host::SurfaceIcon,
) -> mint_sage_apt_viewer::host::SurfaceIcon {
    (icon.uv_rect, icon.tint) = as_radar_draws(icon.uv_rect, icon.tint);
    icon
}

fn as_radar_draws(uv_rect: [f32; 4], tint: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    as_drawn(uv_rect, tint, RADAR_ART_ALPHA)
}

pub(crate) const PREVIEW_ART_ALPHA: f32 = 1.0;

pub(crate) fn preview_art_icon(
    mut icon: mint_sage_apt_viewer::host::SurfaceIcon,
) -> mint_sage_apt_viewer::host::SurfaceIcon {
    (icon.uv_rect, icon.tint) = as_drawn(icon.uv_rect, icon.tint, PREVIEW_ART_ALPHA);
    icon
}

fn as_drawn(uv_rect: [f32; 4], tint: [f32; 4], alpha: f32) -> ([f32; 4], [f32; 4]) {
    let [x, y, w, h] = uv_rect;
    let [r, g, b, a] = tint;
    ([x, y + h, w, -h], [r, g, b, a * alpha])
}

fn to_stage([x, y, w, h]: [f32; 4], sx: f32, sy: f32) -> [f32; 4] {
    if sx <= 0.0 || sy <= 0.0 || !sx.is_finite() || !sy.is_finite() {
        return [0.0; 4];
    }
    [x / sx, y / sy, w / sx, h / sy]
}

fn target(id: u64, [l, t, r, b]: [f32; 4]) -> HitTarget {
    HitTarget {
        id: WidgetId(id),
        region: Rect {
            min: [l, t],
            max: [r, b],
        },
        focusable: false,
    }
}

fn commit_close(session: Option<&mut SpellStoreSession>, view: &SpellBookView) {
    if let Some(session) = session {
        session.on_event(view, "OnBttnClose", "");
    }
}

fn reset_then_close(session: Option<&mut SpellStoreSession>, view: &SpellBookView) {
    if let Some(session) = session {
        session.on_event(view, "OnBttnReset", "");
        commit_close(Some(session), view);
    }
}

fn force_close_store_on_open(
    menu_already_open: bool,
    session: Option<&mut SpellStoreSession>,
    view: &SpellBookView,
) {
    if !menu_already_open {
        commit_close(session, view);
    }
}

#[cfg(test)]
mod backdrop_tests {
    use super::{backdrop_call, wanted_backdrop, Backdrop};
    use mint_sage_apt_player::Value;

    #[test]
    fn backdrop_call_names_the_right_function() {
        assert_eq!(
            backdrop_call(Backdrop::None, Backdrop::InGame),
            Some(("ShowInGameBackground", Vec::new()))
        );
        assert_eq!(
            backdrop_call(Backdrop::InGame, Backdrop::FrontEnd),
            Some(("ShowFrontEndBackground", Vec::new()))
        );
        assert_eq!(
            backdrop_call(Backdrop::FrontEnd, Backdrop::InGame),
            Some(("ShowInGameBackground", Vec::new()))
        );
        assert_eq!(
            backdrop_call(Backdrop::InGame, Backdrop::None),
            Some(("HideInGameBackground", vec![Value::Bool(false)]))
        );
        assert_eq!(
            backdrop_call(Backdrop::FrontEnd, Backdrop::None),
            Some(("HideFrontEndBackground", vec![Value::Bool(false)]))
        );
        for state in [Backdrop::None, Backdrop::InGame, Backdrop::FrontEnd] {
            assert_eq!(backdrop_call(state, state), None, "{state:?} -> itself");
        }
    }

    #[test]
    fn wanted_backdrop_picks_the_right_half() {
        assert_eq!(
            wanted_backdrop(true, false, true),
            Backdrop::FrontEnd,
            "Options over an open store"
        );
        assert_eq!(wanted_backdrop(false, true, false), Backdrop::InGame);
        assert_eq!(
            wanted_backdrop(false, false, true),
            Backdrop::InGame,
            "an open, not yet closing, spell-store visit"
        );
        assert_eq!(
            wanted_backdrop(false, false, false),
            Backdrop::None,
            "a closing store wants nothing (R2: hidden at the start of the close)"
        );
    }
}

#[cfg(test)]
mod spell_store_escape_tests {
    use super::{commit_close, force_close_store_on_open, reset_then_close};
    use crate::hud::SpellStoreSession;
    use crate::sciences::SpellState;
    use crate::spell_book::{SpellBookView, SpellSlotView};

    fn view_with_one_available() -> SpellBookView {
        SpellBookView {
            points: 10,
            slots: vec![Some(SpellSlotView {
                science: "SCIENCE_Heal".to_owned(),
                name: "Heal".to_owned(),
                image: None,
                description: String::new(),
                cost: 1,
                state: SpellState::Available,
            })],
        }
    }

    #[test]
    fn escape_drops_the_pick_then_closes() {
        let view = view_with_one_available();
        let mut session = SpellStoreSession::open(&view);
        session.on_event(&view, "OnBttnSpell", "Spell1");
        assert_eq!(session.provisional, vec!["SCIENCE_Heal".to_owned()]);
        reset_then_close(Some(&mut session), &view);
        assert!(session.provisional.is_empty(), "the pick must be dropped");
        assert!(session.closing, "and the store asked to close");
    }

    #[test]
    fn a_commit_close_keeps_the_pick_and_closes() {
        let view = view_with_one_available();
        let mut session = SpellStoreSession::open(&view);
        session.on_event(&view, "OnBttnSpell", "Spell1");
        commit_close(Some(&mut session), &view);
        assert_eq!(
            session.provisional,
            vec!["SCIENCE_Heal".to_owned()],
            "a commit close must not drop the pick"
        );
        assert!(session.closing);
    }

    #[test]
    fn with_no_session_both_calls_are_no_ops() {
        let view = view_with_one_available();
        reset_then_close(None, &view);
        commit_close(None, &view);
    }

    #[test]
    fn opening_the_quit_menu_force_closes_the_store() {
        let view = view_with_one_available();
        let mut session = SpellStoreSession::open(&view);
        session.on_event(&view, "OnBttnSpell", "Spell1");
        force_close_store_on_open(false, Some(&mut session), &view);
        assert!(session.closing, "the store must be told to close");
        assert_eq!(session.provisional, vec!["SCIENCE_Heal".to_owned()]);
    }

    #[test]
    fn closing_the_quit_menu_does_not_touch_the_store() {
        let view = view_with_one_available();
        let mut session = SpellStoreSession::open(&view);
        session.on_event(&view, "OnBttnSpell", "Spell1");
        force_close_store_on_open(true, Some(&mut session), &view);
        assert!(!session.closing, "the store must stay open");
    }
}

struct PalantirPingHost<'a> {
    player: &'a mut mint_sage_apt_player::Player,
    clip: InstanceId,
}

impl crate::radar_pings::RadarPingHost for PalantirPingHost<'_> {
    type Handle = mint_sage_apt_player::Value;

    fn create(&mut self, clip: &str) -> Self::Handle {
        println!("radar ping: CreateRadarPing({clip})");
        self.player
            .call(
                self.clip,
                "CreateRadarPing",
                vec![mint_sage_apt_player::Value::str(clip)],
            )
            .unwrap_or(mint_sage_apt_player::Value::Undefined)
    }

    fn move_ping(&mut self, ping: &Self::Handle, pixel: (f32, f32)) {
        let _ = self.player.call(
            self.clip,
            "MoveRadarPing",
            vec![
                ping.clone(),
                mint_sage_apt_player::Value::Number(f64::from(pixel.0)),
                mint_sage_apt_player::Value::Number(f64::from(pixel.1)),
            ],
        );
    }

    fn fade(&mut self, ping: &Self::Handle) {
        println!("radar ping: FadeOutRadarPing");
        let _ = self
            .player
            .call(self.clip, "FadeOutRadarPing", vec![ping.clone()]);
    }
}

#[cfg(test)]
mod help_box_state_tests {
    use super::{help_box_state, Arc, HelpBoxState, HoverKey, PreparedHelp};

    fn prepared(key: HoverKey) -> PreparedHelp {
        (key, 42.0, Arc::new(Vec::new()))
    }

    #[test]
    fn prepared_lines_show_when_their_key_is_shown() {
        let text = prepared(HoverKey::Hero(1));
        assert!(matches!(
            help_box_state(Some(&text), Some(HoverKey::Hero(1))),
            HelpBoxState::Shown { height, .. } if height == 42.0
        ));
    }

    #[test]
    fn anything_else_is_hidden() {
        let text = prepared(HoverKey::Hero(1));
        assert_eq!(help_box_state(Some(&text), None), HelpBoxState::Hidden);
        assert_eq!(
            help_box_state(None, Some(HoverKey::Hero(1))),
            HelpBoxState::Hidden
        );
        assert_eq!(
            help_box_state(Some(&text), Some(HoverKey::Ring(0))),
            HelpBoxState::Hidden
        );
    }
}

#[cfg(test)]
mod hero_number_tests {
    use super::hero_number;

    #[test]
    fn hero_n_parses_case_insensitively_within_range() {
        assert_eq!(hero_number("Hero1"), Some(1));
        assert_eq!(hero_number("hero16"), Some(16));
        assert_eq!(hero_number("HERO7"), Some(7));
        assert_eq!(hero_number("Hero0"), None, "no zeroth hero");
        assert_eq!(hero_number("Hero17"), None, "past the shipped 16 slots");
        assert_eq!(hero_number("HeroSelectUI"), None, "not a digit suffix");
        assert_eq!(hero_number("Options"), None);
        assert_eq!(hero_number("Her"), None, "too short to carry the prefix");
    }
}

#[cfg(test)]
mod minimap_tests {
    use super::{minimap_uv_of, minimap_world_at, wall_cells};
    use mint_sage_map::HeightMapData;

    fn square_bounds(side: f32) -> mint_render::Aabb {
        mint_render::Aabb::from_corners(
            mint_render::Vec3::new(0.0, 0.0, 0.0),
            mint_render::Vec3::new(side, side, 1.0),
        )
    }

    fn map(width: u32, height: u32, border: u32) -> HeightMapData {
        HeightMapData {
            width,
            height,
            border_size: border,
            boundaries: Vec::new(),
            samples: Vec::new(),
        }
    }

    #[test]
    fn rotating_the_wall_changes_its_footprint() {
        let (half_major, half_minor, r) = (2.0, 1.0, 10.0);
        let aligned = wall_cells(0.0, half_major, half_minor, r, 1.0);
        let rotated = wall_cells(std::f32::consts::FRAC_PI_4, half_major, half_minor, r, 1.0);
        assert_ne!(aligned, rotated);
    }

    #[test]
    fn wall_cells_follow_the_distance_alpha_rule() {
        let (half_major, half_minor) = (2.0, 1.0);
        let cells = wall_cells(0.0, half_major, half_minor, 40.0, 1.0);
        let at = |dx: f32, dy: f32| {
            cells
                .iter()
                .find(|&&(x, y, _)| x == dx && y == dy)
                .map(|&(_, _, a)| a)
        };
        assert_eq!(at(2.0, 1.0), Some(1.0), "inside the box itself");
        assert_eq!(at(7.0, 1.0), Some(1.0));
        assert_eq!(at(8.0, 1.0), Some(128.0 / 255.0), "6 past - the fringe");
        assert_eq!(at(32.0, 1.0), Some(128.0 / 255.0));
        assert_eq!(at(33.0, 1.0), None, "31 past - untouched");
        assert!(
            cells
                .iter()
                .all(|&(dx, dy, _)| dx.abs() <= 40.0 && dy.abs() <= 40.0),
            "nothing past the clamp radius"
        );
        assert!(
            cells.len() > 2,
            "a footprint, not a two-cell blip: {}",
            cells.len()
        );
    }

    #[test]
    fn the_centre_cell_is_the_centre_of_both_the_map_and_the_minimap() {
        let m = square_bounds(1_000.0);
        assert_eq!(minimap_uv_of(&m, 500.0, 500.0), Some([0.5, 0.5]));
        assert_eq!(minimap_world_at(&m, 0.5, 0.5), Some((500.0, 500.0)));
    }

    #[test]
    fn world_plus_y_is_toward_the_top_of_the_minimap_image() {
        let m = square_bounds(1_000.0);
        let uv = minimap_uv_of(&m, 500.0, 600.0).unwrap();
        assert!(
            uv[1] < 0.5,
            "moving +y must decrease v (toward the top): {uv:?}"
        );
    }

    #[test]
    fn the_radar_draws_the_art_rows_reversed_and_faint() {
        let (uv, tint) = super::as_radar_draws([0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        assert_eq!(uv, [0.0, 1.0, 1.0, -1.0]);
        assert_eq!(tint, [1.0, 1.0, 1.0, super::RADAR_ART_ALPHA]);
        let (half, _) = super::as_radar_draws([0.0, 0.5, 1.0, 0.5], [1.0; 4]);
        assert_eq!(half, [0.0, 1.0, 1.0, -0.5], "a sub-rect flips in place");
    }

    #[test]
    fn the_view_box_is_the_windows_corners_on_the_ground() {
        let camera = |eye: mint_render::Vec3| mint_render::Camera {
            eye,
            target: mint_render::Vec3::new(0.0, 0.0, 10.0),
            up: mint_render::Vec3::Z,
            fov_y: std::f32::consts::FRAC_PI_2,
            near: 1.0,
            far: 10_000.0,
        };
        let down = camera(mint_render::Vec3::new(0.0, -0.001, 110.0));
        let [tl, tr, br, bl] = super::view_box_world(&down, (200, 100), 10.0).unwrap();
        assert!((tr[0] - tl[0] - 400.0).abs() < 1.0, "{tl:?} {tr:?}");
        assert!((tl[1] - bl[1] - 200.0).abs() < 1.0, "{tl:?} {bl:?}");
        assert!((br[0] - bl[0] - 400.0).abs() < 1.0);
        let tilted = camera(mint_render::Vec3::new(0.0, -100.0, 110.0));
        let [tl, tr, br, bl] = super::view_box_world(&tilted, (200, 100), 10.0).unwrap();
        assert!(tr[0] - tl[0] > br[0] - bl[0], "far edge wider");
        assert!(tl[1] > bl[1], "far edge further along +y");
        let grazing = camera(mint_render::Vec3::new(0.0, -1000.0, 11.0));
        let [tl, ..] = super::view_box_world(&grazing, (200, 100), 10.0).unwrap();
        assert!(tl[0].is_finite() && tl[1].is_finite());
        assert!(tl[1] < 10_000.0, "{tl:?}");
    }

    #[test]
    fn the_view_box_draws_its_four_edges_over_the_minimap() {
        let uv = [[0.25, 0.25], [0.75, 0.25], [0.6, 0.5], [0.4, 0.5]];
        let quads = super::view_box_quads(uv, [100.0, 200.0, 300.0, 400.0], 2.0);
        assert_eq!(quads.len(), 4);
        let far = quads[0];
        assert_eq!(far.affine, [[100.0, 0.0], [0.0, 2.0]]);
        assert_eq!(far.translation, [150.0, 249.0]);
        assert_eq!(quads[2].affine[0][0], -40.0);
        for q in &quads {
            assert_eq!(q.clip, [100.0, 200.0, 300.0, 400.0]);
        }
        let mut flat = map(2, 1, 0);
        flat.samples = vec![10, 30];
        assert_eq!(super::average_ground_z(&flat, 0.5), 10.0);
        assert_eq!(super::average_ground_z(&map(0, 0, 0), 1.0), 0.0);
    }

    #[test]
    fn a_ring_is_a_closed_one_cell_circle_of_its_radius() {
        for radius in [1, 4, 13] {
            let mut ring = std::collections::BTreeSet::new();
            super::ring_cells(radius, |x, y| {
                ring.insert((x, y));
            });
            for &(x, y) in &ring {
                #[allow(clippy::cast_precision_loss)]
                let distance = ((x * x + y * y) as f32).sqrt();
                #[allow(clippy::cast_precision_loss)]
                let off = (distance - radius as f32).abs();
                assert!(off < 0.75, "radius {radius}: ({x}, {y}) is {distance}");
                let neighbours = ring
                    .iter()
                    .filter(|&&(nx, ny)| {
                        (nx, ny) != (x, y) && (nx - x).abs() <= 1 && (ny - y).abs() <= 1
                    })
                    .count();
                assert!(neighbours >= 2, "radius {radius}: ({x}, {y}) is an end");
            }
        }
        let mut centre = Vec::new();
        super::ring_cells(0, |x, y| centre.push((x, y)));
        assert!(centre.iter().all(|&cell| cell == (0, 0)));
    }

    #[test]
    fn a_zero_sized_map_answers_none_not_a_panic() {
        let m = mint_render::Aabb::EMPTY;
        assert_eq!(minimap_uv_of(&m, 0.0, 0.0), None);
        assert_eq!(minimap_world_at(&m, 0.5, 0.5), None);
    }

    #[test]
    fn the_minimap_spans_the_boundary_rectangle() {
        use mint_render::{Aabb, Vec3};
        let b = Aabb::from_corners(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3_000.0, 2_000.0, 9.0));
        assert_eq!(minimap_world_at(&b, 0.0, 1.0), Some((0.0, 0.0)));
        assert_eq!(minimap_world_at(&b, 1.0, 0.0), Some((3_000.0, 2_000.0)));
        assert_eq!(minimap_uv_of(&b, 1_500.0, 500.0), Some([0.5, 0.75]));
        let outside = minimap_uv_of(&b, -100.0, 500.0).unwrap_or_default();
        assert!(outside[0] < 0.0);
    }

    #[test]
    fn hero_halo_and_dot_alpha_grids_run_outward_in() {
        use super::{mirrored_alpha, HERO_DOT_QUADRANT, HERO_HALO_QUADRANT};
        let grid = |quadrant: &[[u8; 3]; 3]| -> Vec<Vec<u8>> {
            (0..6)
                .map(|dy| {
                    (0..6)
                        .map(|dx| mirrored_alpha(quadrant, dx, dy, 3))
                        .collect()
                })
                .collect()
        };
        assert_eq!(
            grid(&HERO_HALO_QUADRANT),
            vec![
                vec![0x03, 0x24, 0x45, 0x45, 0x24, 0x03],
                vec![0x24, 0x4F, 0x4F, 0x4F, 0x4F, 0x24],
                vec![0x45, 0x4F, 0x4F, 0x4F, 0x4F, 0x45],
                vec![0x45, 0x4F, 0x4F, 0x4F, 0x4F, 0x45],
                vec![0x24, 0x4F, 0x4F, 0x4F, 0x4F, 0x24],
                vec![0x03, 0x24, 0x45, 0x45, 0x24, 0x03],
            ]
        );
        let dot: Vec<Vec<u8>> = (0..4)
            .map(|dy| {
                (0..4)
                    .map(|dx| mirrored_alpha(&HERO_DOT_QUADRANT, dx, dy, 2))
                    .collect()
            })
            .collect();
        assert_eq!(
            dot,
            vec![
                vec![0x00, 0x4A, 0x4A, 0x00],
                vec![0x4A, 0xFF, 0xFF, 0x4A],
                vec![0x4A, 0xFF, 0xFF, 0x4A],
                vec![0x00, 0x4A, 0x4A, 0x00],
            ]
        );
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn a_window_rect_goes_to_stage_units_per_axis() {
        assert_eq!(
            to_stage([100.0, 60.0, 50.0, 30.0], 2.0, 1.5),
            [50.0, 40.0, 25.0, 20.0]
        );
        assert_eq!(to_stage([1.0; 4], 0.0, 1.0), [0.0; 4]);
    }

    fn targets(rects: &[[f32; 4]]) -> Vec<HitTarget> {
        rects
            .iter()
            .enumerate()
            .map(|(index, &rect)| target(index as u64, rect))
            .collect()
    }

    fn claimed_slot(
        router: &mut InputRouter,
        targets: &[HitTarget],
        position: [f32; 2],
        pressed: bool,
    ) -> Option<u64> {
        let routing =
            route_pointer_button(router, targets, position, PointerButton::Primary, pressed);
        slot_widget_id(routing, pressed).map(|id| id.0)
    }

    fn reached_world(router: &mut InputRouter, targets: &[HitTarget], position: [f32; 2]) -> bool {
        let press = route_pointer_button(router, targets, position, PointerButton::Primary, true);
        let release =
            route_pointer_button(router, targets, position, PointerButton::Primary, false);
        matches!(press, PointerRouting::World) || matches!(release, PointerRouting::World)
    }

    #[test]
    fn a_pointer_release_goes_to_the_movie_that_took_its_press() {
        let mut held = None;
        assert_eq!(
            pointer_button_target(true, false, &mut held),
            PointerTarget::Palantir
        );
        assert_eq!(
            pointer_button_target(false, true, &mut held),
            PointerTarget::Palantir
        );
        assert_eq!(
            pointer_button_target(true, true, &mut held),
            PointerTarget::SpellStore
        );
        assert_eq!(
            pointer_button_target(false, false, &mut held),
            PointerTarget::SpellStore
        );
        assert_eq!(
            pointer_button_target(false, true, &mut held),
            PointerTarget::SpellStore
        );
        assert_eq!(
            pointer_button_target(false, false, &mut held),
            PointerTarget::Palantir
        );
    }

    #[test]
    fn a_click_inside_a_slot_never_reaches_the_world_and_one_outside_does() {
        let targets = targets(&[[0.0, 0.0, 56.0, 56.0], [60.0, 0.0, 116.0, 56.0]]);
        let mut router = InputRouter::new();
        assert!(!reached_world(&mut router, &targets, [4.0, 4.0]));
        assert_eq!(
            claimed_slot(&mut router, &targets, [4.0, 4.0], false),
            Some(0)
        );
        assert!(reached_world(&mut router, &targets, [500.0, 500.0]));
        assert_eq!(
            claimed_slot(&mut router, &targets, [500.0, 500.0], false),
            None
        );
    }

    #[test]
    fn a_press_alone_never_reports_a_slot() {
        let targets = targets(&[[0.0, 0.0, 56.0, 56.0]]);
        let mut router = InputRouter::new();
        assert_eq!(claimed_slot(&mut router, &targets, [1.0, 1.0], true), None);
    }

    #[test]
    fn the_boundary_between_a_slot_and_the_world_is_exact() {
        let targets = targets(&[[0.0, 0.0, 100.0, 100.0]]);
        let mut router = InputRouter::new();
        assert!(!reached_world(&mut router, &targets, [99.0, 99.0]));
        assert_eq!(
            claimed_slot(&mut router, &targets, [99.0, 99.0], false),
            Some(0)
        );
        assert!(reached_world(&mut router, &targets, [100.0, 100.0]));
        assert_eq!(
            claimed_slot(&mut router, &targets, [100.0, 100.0], false),
            None
        );
    }

    #[test]
    fn no_slots_claims_nothing() {
        let mut router = InputRouter::new();
        assert!(reached_world(&mut router, &[], [500.0, 700.0]));
        assert_eq!(claimed_slot(&mut router, &[], [500.0, 700.0], false), None);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod map_art_fallback_tests {
    use mint_render::{Error as RenderError, Renderer};
    use mint_sage_ini::{CommandButtonIndex, MappedImageIndex};
    use mint_vfs::{Mount, Result as VfsResult, Vfs, VfsPath};

    use super::{ImageResolver, ImageSource};

    fn one_pixel_tga() -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[2] = 2;
        bytes[12] = 1;
        bytes[14] = 1;
        bytes[16] = 32;
        bytes[17] = 0x20;
        bytes.extend_from_slice(&[0, 0, 255, 255]);
        bytes
    }

    struct FakeMount(Vec<(VfsPath, Vec<u8>)>);

    impl FakeMount {
        fn new(files: &[(&str, Vec<u8>)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(name, body)| (VfsPath::new(name), body.clone()))
                    .collect(),
            )
        }
    }

    impl Mount for FakeMount {
        fn label(&self) -> &str {
            "test"
        }

        fn entries(&self) -> Vec<VfsPath> {
            self.0.iter().map(|(path, _)| path.clone()).collect()
        }

        fn read(&self, path: &VfsPath) -> VfsResult<Vec<u8>> {
            self.0
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, body)| body.clone())
                .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })
        }
    }

    fn renderer() -> Option<Renderer> {
        match Renderer::offscreen(4, 4) {
            Ok(renderer) => Some(renderer),
            Err(RenderError::NoAdapter) => {
                eprintln!(
                    "SKIPPED: no graphics adapter on this machine, so \
                     map_art_fallback_tests was not actually verified"
                );
                None
            }
            Err(error) => panic!("creating an offscreen renderer failed: {error}"),
        }
    }

    fn resolver(files: &[(&str, Vec<u8>)]) -> ImageResolver {
        let mut vfs = Vfs::new();
        vfs.mount(std::sync::Arc::new(FakeMount::new(files)));
        ImageResolver::new(
            std::sync::Arc::new(vfs),
            std::sync::Arc::new(MappedImageIndex::new()),
            std::sync::Arc::new(CommandButtonIndex::new()),
        )
    }

    #[test]
    fn the_bare_stem_resolves_when_the_art_variant_is_absent() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut images = resolver(&[("maps/x/x.tga", one_pixel_tga())]);
        let content = images.resolve(
            &mut renderer,
            &ImageSource::MapArt("maps/x/x_art.tga".to_owned()),
        );
        assert!(content.is_some(), "the bare .tga is the fallback");
    }

    #[test]
    fn neither_candidate_present_resolves_to_nothing() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut images = resolver(&[]);
        let content = images.resolve(
            &mut renderer,
            &ImageSource::MapArt("maps/x/x_art.tga".to_owned()),
        );
        assert!(content.is_none());
    }
}
