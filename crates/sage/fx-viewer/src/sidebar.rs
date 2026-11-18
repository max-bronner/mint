use mint_render::{Renderer, TextureData, TextureHandle, UiQuad};
use mint_ui::{Font, FontId, GlyphAtlas, GlyphAtlasConfig, HitTarget, List, Rect, WidgetId};

use crate::tree::NodeKind;

pub const WIDTH: f32 = 500.0;
pub const ROW_HEIGHT: f32 = 24.0;
pub const TEXT_PX: u32 = 14;
const FILTER_HEIGHT: f32 = 32.0;
const INDENT: f32 = 14.0;
const EYE_X: f32 = WIDTH - 54.0;
const SOLO_X: f32 = WIDTH - 30.0;
const TOGGLE_W: f32 = 22.0;
const DISC: f32 = 12.0;
const CHEVRON_X: f32 = 12.0;
const LABEL_GAP: f32 = 15.0;
pub const TEXT_RIGHT: f32 = EYE_X - 6.0;

pub mod color {
    pub const PANEL: [f32; 4] = [0.098, 0.106, 0.125, 0.98];
    pub const EDGE: [f32; 4] = [1.0, 1.0, 1.0, 0.09];
    pub const FIELD: [f32; 4] = [0.155, 0.168, 0.200, 1.0];
    pub const FIELD_FOCUS: [f32; 4] = [0.185, 0.215, 0.280, 1.0];
    pub const HEADER: [f32; 4] = [1.0, 1.0, 1.0, 0.045];
    pub const HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.060];
    pub const SELECTED: [f32; 4] = [0.230, 0.420, 0.760, 0.420];
    pub const ACCENT: [f32; 4] = [0.380, 0.640, 1.0, 1.0];
    pub const GUIDE: [f32; 4] = [1.0, 1.0, 1.0, 0.070];
    pub const DIM: [f32; 4] = [0.560, 0.600, 0.670, 1.0];
    pub const TEXT: [f32; 4] = [0.880, 0.900, 0.930, 1.0];
    pub const OFF: [f32; 4] = [1.0, 1.0, 1.0, 0.140];
    pub const SOLO: [f32; 4] = [1.0, 0.780, 0.300, 1.0];
    pub const HIDDEN: [f32; 4] = [0.950, 0.420, 0.400, 1.0];
    pub const OVERLAY: [f32; 4] = [0.070, 0.075, 0.090, 0.860];
}

pub const FILTER_ID: WidgetId = WidgetId(1);
pub const LIST_ID: WidgetId = WidgetId(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowHit {
    Eye,
    Solo,
    Name,
}

#[must_use]
pub fn row_hit(x: f32) -> RowHit {
    if (EYE_X..EYE_X + TOGGLE_W).contains(&x) {
        RowHit::Eye
    } else if (SOLO_X..SOLO_X + TOGGLE_W).contains(&x) {
        RowHit::Solo
    } else {
        RowHit::Name
    }
}

pub struct Text {
    font: Option<Font>,
    atlas: GlyphAtlas,
    texture: TextureHandle,
}

impl Text {
    pub fn new(renderer: &mut Renderer) -> crate::Result<Self> {
        let font = system_sans().or_else(|| {
            mint_ui::bundled_substitute_font("Tinos")
                .and_then(|bytes| Font::from_bytes(bytes.to_vec()).ok())
        });
        let atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config())?;
        let white = TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
            .ok_or(crate::Error::NoGlyphTexture)?;
        let texture = renderer.upload_texture(&white)?;
        Ok(Self {
            font,
            atlas,
            texture,
        })
    }

    pub fn begin(&mut self) {
        self.atlas.begin_frame();
    }

    pub fn run(&mut self, text: &str, origin: [f32; 2], color: [f32; 4]) -> (Vec<UiQuad>, f32) {
        let quads = self.line(text, origin, color);
        let end = quads
            .iter()
            .map(|quad| quad.translation[0] + quad.affine[0][0])
            .fold(origin[0], f32::max);
        (quads, end)
    }

    pub fn line(&mut self, text: &str, origin: [f32; 2], color: [f32; 4]) -> Vec<UiQuad> {
        mint_ui::layout_text(
            text,
            origin,
            color,
            FontId(0),
            self.font.as_ref(),
            &mut self.atlas,
            TEXT_PX,
            self.texture,
        )
    }

    pub fn flush(&mut self, renderer: &mut Renderer, quads: &mut [UiQuad]) {
        if !self.atlas.take_dirty() {
            return;
        }
        let (width, height) = self.atlas.dimensions();
        let Some(data) = TextureData::rgba8(width, height, self.atlas.pixels().to_vec()) else {
            return;
        };
        let Ok(handle) = renderer.upload_texture(&data) else {
            return;
        };
        let old = self.texture;
        self.texture = handle;
        for quad in quads.iter_mut() {
            if quad.texture == Some(old) {
                quad.texture = Some(handle);
            }
        }
    }
}

pub struct Layout {
    pub panel: Rect,
    pub filter: Rect,
    pub list: List,
}

impl Layout {
    #[must_use]
    pub fn new(height: f32, rows: usize) -> Self {
        let panel = Rect {
            min: [0.0, 0.0],
            max: [WIDTH, height],
        };
        let filter = Rect {
            min: [10.0, 10.0],
            max: [WIDTH - 10.0, 10.0 + FILTER_HEIGHT],
        };
        let list = List::new(
            LIST_ID,
            Rect {
                min: [0.0, filter.max[1] + 10.0],
                max: [WIDTH, (height - 4.0).max(filter.max[1] + 10.0)],
            },
            ROW_HEIGHT,
            rows,
        );
        Self {
            panel,
            filter,
            list,
        }
    }

    #[must_use]
    pub fn targets(&self) -> Vec<HitTarget> {
        vec![
            HitTarget {
                id: FILTER_ID,
                region: self.filter,
                focusable: true,
            },
            HitTarget {
                id: LIST_ID,
                region: self.list.viewport,
                focusable: false,
            },
        ]
    }
}

#[must_use]
pub fn row_color(kind: NodeKind) -> [f32; 4] {
    match kind {
        NodeKind::Category(_) => [0.960, 0.965, 0.980, 1.0],
        NodeKind::Bucket => color::DIM,
        NodeKind::FxList => [0.560, 0.780, 1.0, 1.0],
        NodeKind::System => [0.560, 0.900, 0.680, 1.0],
        NodeKind::Object | NodeKind::Weapon => [0.980, 0.860, 0.560, 1.0],
        NodeKind::ModelState | NodeKind::AnimState => [0.800, 0.720, 1.0, 1.0],
        NodeKind::Dangling => [1.0, 0.500, 0.450, 1.0],
        NodeKind::Stopped => [1.0, 0.750, 0.400, 1.0],
        _ => color::TEXT,
    }
}

#[must_use]
pub fn chevron_x(depth: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let steps = depth.min(24) as f32;
    CHEVRON_X + steps * INDENT
}

#[must_use]
pub fn label_x(depth: u32) -> f32 {
    chevron_x(depth) + LABEL_GAP
}

fn flat(x: f32, y: f32, width: f32, height: f32, fill: [f32; 4]) -> UiQuad {
    UiQuad::new(x, y, width, height).with_color_transform(fill, [0.0; 4])
}

#[must_use]
pub fn fill(rect: Rect, color: [f32; 4]) -> UiQuad {
    flat(
        rect.min[0],
        rect.min[1],
        rect.max[0] - rect.min[0],
        rect.max[1] - rect.min[1],
        color,
    )
}

#[must_use]
pub fn guides(top: f32, depth: u32) -> Vec<UiQuad> {
    (1..depth.min(24))
        .map(|level| flat(chevron_x(level) + 3.0, top, 1.0, ROW_HEIGHT, color::GUIDE))
        .collect()
}

#[must_use]
pub fn chevron(top: f32, depth: u32, open: bool) -> Vec<UiQuad> {
    let x = chevron_x(depth);
    let middle = top + ROW_HEIGHT * 0.5;
    (0..4u8)
        .flat_map(|step| {
            let step = f32::from(step);
            if open {
                vec![flat(
                    x + step,
                    middle - 2.0 + step,
                    8.0 - 2.0 * step,
                    1.0,
                    color::DIM,
                )]
            } else {
                vec![flat(
                    x + 1.0 + step,
                    middle - 4.0 + step,
                    1.0,
                    8.0 - 2.0 * step,
                    color::DIM,
                )]
            }
        })
        .collect()
}

#[must_use]
pub fn toggles(top: f32, hidden: bool, soloed: bool, hovered: bool) -> Vec<UiQuad> {
    let disc = |x: f32, fill: [f32; 4]| {
        let mut quad = flat(
            x + (TOGGLE_W - DISC) * 0.5,
            top + (ROW_HEIGHT - DISC) * 0.5,
            DISC,
            DISC,
            fill,
        );
        quad.disc = true;
        quad
    };
    let mut quads = Vec::new();
    if hidden || hovered {
        quads.push(disc(
            EYE_X,
            if hidden { color::HIDDEN } else { color::ACCENT },
        ));
    }
    if soloed || hovered {
        quads.push(disc(SOLO_X, if soloed { color::SOLO } else { color::OFF }));
    }
    quads
}

fn system_sans() -> Option<Font> {
    let fonts = std::path::PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts");
    ["segoeui.ttf", "arial.ttf", "tahoma.ttf"]
        .iter()
        .find_map(|name| Font::from_bytes(std::fs::read(fonts.join(name)).ok()?).ok())
}
