use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mint_render::UiQuad;
use mint_sage_apt::{RuFile, Style};
use mint_sage_apt_player::{CharacterRef, ColorTransform, InstanceId, Matrix, RenderCommand};

use crate::{Letterbox, TextAnchor};

pub type Affine = ([[f32; 2]; 2], [f32; 2]);

#[must_use]
pub fn window_affine(letterbox: &Letterbox, m: &Matrix) -> Affine {
    let [sx, sy] = letterbox.scale;
    let o = letterbox.offset;
    (
        [[sx * m.a, sx * m.c], [sy * m.b, sy * m.d]],
        [sx * m.tx + o[0], sy * m.ty + o[1]],
    )
}

#[must_use]
pub fn apply(a: &Affine, p: [f32; 2]) -> [f32; 2] {
    let (m, t) = a;
    [
        m[0][0] * p[0] + m[0][1] * p[1] + t[0],
        m[1][0] * p[0] + m[1][1] * p[1] + t[1],
    ]
}

#[must_use]
pub fn rect_triangles(r: [f32; 4]) -> [[f32; 2]; 6] {
    [
        [r[0], r[1]],
        [r[2], r[1]],
        [r[0], r[3]],
        [r[2], r[1]],
        [r[2], r[3]],
        [r[0], r[3]],
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextureRef {
    pub key: (String, String),
    pub pixel_transform: [[f32; 2]; 2],
    pub pixel_translate: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeRun {
    pub triangles: Vec<[f32; 2]>,
    pub lines: Vec<[f32; 2]>,
    pub color: [f32; 4],
    pub texture: Option<TextureRef>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapeMesh {
    pub runs: Vec<ShapeRun>,
    pub fill: Vec<[f32; 2]>,
    pub unresolved_textures: u32,
    pub bounds: Option<[f32; 4]>,
    id: u64,
}

static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

impl ShapeMesh {
    #[must_use]
    pub fn retain_key(&self, run: usize) -> Option<u64> {
        let run = u64::try_from(run).ok().filter(|run| *run < 1 << 20)?;
        (self.id != 0).then_some(self.id << 20 | run)
    }

    pub fn from_ru(
        ru: &RuFile,
        mut texture_of: impl FnMut(u32) -> Option<(String, String)>,
    ) -> Self {
        let mut mesh = Self::default();
        for entry in &ru.entries {
            for run in &entry.runs {
                let triangles: Vec<[f32; 2]> = run.triangles.iter().flatten().copied().collect();
                let lines: Vec<[f32; 2]> = run.lines.iter().flatten().copied().collect();
                let (color, texture) = match run.style {
                    Style::Solid { color } | Style::Line { color, .. } => (color, None),
                    Style::Texture {
                        color,
                        character,
                        pixel_transform,
                        pixel_translate,
                    } => {
                        let texture = texture_of(character).map(|key| TextureRef {
                            key,
                            pixel_transform,
                            pixel_translate,
                        });
                        if texture.is_none() {
                            mesh.unresolved_textures = mesh.unresolved_textures.saturating_add(1);
                        }
                        (color, texture)
                    }
                };
                if let Some(b) = aabb(triangles.iter().chain(lines.iter())) {
                    mesh.bounds = Some(match mesh.bounds {
                        Some(m) => [
                            m[0].min(b[0]),
                            m[1].min(b[1]),
                            m[2].max(b[2]),
                            m[3].max(b[3]),
                        ],
                        None => b,
                    });
                }
                mesh.fill.extend_from_slice(&triangles);
                mesh.runs.push(ShapeRun {
                    triangles,
                    lines,
                    color,
                    texture,
                });
            }
        }
        mesh
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskMode {
    Off,
    BoundingBox,
    #[default]
    Stencil,
}

impl MaskMode {
    #[must_use]
    pub fn cycled(self) -> Self {
        match self {
            Self::Off => Self::BoundingBox,
            Self::BoundingBox => Self::Stencil,
            Self::Stencil => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off (mask drawn as content - the old bug)",
            Self::BoundingBox => "bounding box (axis-aligned approximation)",
            Self::Stencil => "stencil (real shape, sprite masks as their whole subtree)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    Shape {
        character: CharacterRef,
        world: Affine,
        color: ColorTransform,
        clip: [f32; 4],
    },
    Text {
        instance: InstanceId,
        character: CharacterRef,
        world: Affine,
        color: ColorTransform,
        clip: [f32; 4],
    },
    Surface {
        instance: InstanceId,
        bounds: [f32; 4],
        world: Affine,
        color: ColorTransform,
        clip: [f32; 4],
    },
    PushMask(usize),
    PopMask(usize),
}

#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    pub masks: Vec<Vec<[f32; 2]>>,
    pub mask_count: u32,
    pub compound_masks: u32,
    pub single_pieces: Vec<InstanceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Defining,
    Active,
    Undoing,
    Nested,
}

struct Open {
    phase: Phase,
    mask: usize,
    clip_before: [f32; 4],
    pieces: u32,
    first: Option<InstanceId>,
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn aabb<'a>(points: impl IntoIterator<Item = &'a [f32; 2]>) -> Option<[f32; 4]> {
    let mut it = points.into_iter();
    let first = it.next()?;
    let mut r = [first[0], first[1], first[0], first[1]];
    for p in it {
        r = [
            r[0].min(p[0]),
            r[1].min(p[1]),
            r[2].max(p[0]),
            r[3].max(p[1]),
        ];
    }
    Some(r)
}

#[must_use]
pub fn can_show(multiply: [f32; 4], add: [f32; 4]) -> bool {
    multiply[3].max(0.0) + add[3] > 0.0
}

const EDGE_SLACK: f32 = 2.0;

#[must_use]
pub fn on_screen(world: &Affine, local: [f32; 4], clip: [f32; 4], size: (f32, f32)) -> bool {
    let corners = [
        apply(world, [local[0], local[1]]),
        apply(world, [local[2], local[1]]),
        apply(world, [local[0], local[3]]),
        apply(world, [local[2], local[3]]),
    ];
    let Some(b) = aabb(corners.iter()) else {
        return false;
    };
    let visible = intersect(clip, [0.0, 0.0, size.0, size.1]);
    b[0] <= visible[2] + EDGE_SLACK
        && b[2] >= visible[0] - EDGE_SLACK
        && b[1] <= visible[3] + EDGE_SLACK
        && b[3] >= visible[1] - EDGE_SLACK
}

pub fn plan(
    commands: &[RenderCommand],
    letterbox_of: &dyn Fn(Option<InstanceId>) -> Letterbox,
    mode: MaskMode,
    mut fill: impl FnMut(&RenderCommand, &Affine, &mut Vec<[f32; 2]>),
    out: &mut Plan,
) {
    out.items.clear();
    for m in &mut out.masks {
        m.clear();
    }
    out.mask_count = 0;
    out.compound_masks = 0;
    out.single_pieces.clear();
    let mut used = 0usize;
    let mut stack: Vec<Open> = Vec::new();
    let mut clip = UiQuad::UNCLIPPED;
    for command in commands {
        match command {
            RenderCommand::PushMask => {
                let defining = stack
                    .iter()
                    .any(|o| matches!(o.phase, Phase::Defining | Phase::Nested));
                let undoing = stack.iter().any(|o| o.phase == Phase::Undoing);
                if defining || undoing {
                    stack.push(Open {
                        phase: if undoing {
                            Phase::Undoing
                        } else {
                            Phase::Nested
                        },
                        mask: usize::MAX,
                        clip_before: clip,
                        pieces: 0,
                        first: None,
                    });
                    continue;
                }
                if out.masks.len() <= used {
                    out.masks.push(Vec::new());
                }
                stack.push(Open {
                    phase: Phase::Defining,
                    mask: used,
                    clip_before: clip,
                    pieces: 0,
                    first: None,
                });
                used = used.saturating_add(1);
                out.mask_count = out.mask_count.saturating_add(1);
            }
            RenderCommand::ActivateMask => {
                let Some(top) = stack.last_mut() else {
                    continue;
                };
                if top.phase != Phase::Defining {
                    continue;
                }
                top.phase = Phase::Active;
                match (top.pieces, top.first) {
                    (1, Some(first)) => out.single_pieces.push(first),
                    _ => out.compound_masks = out.compound_masks.saturating_add(1),
                }
                let index = top.mask;
                match mode {
                    MaskMode::Stencil => out.items.push(Item::PushMask(index)),
                    MaskMode::BoundingBox => {
                        if let Some(r) = out.masks.get(index).and_then(|m| aabb(m.iter())) {
                            clip = intersect(clip, r);
                        }
                    }
                    MaskMode::Off => {}
                }
            }
            RenderCommand::DeactivateMask => {
                let Some(top) = stack.last_mut() else {
                    continue;
                };
                if top.phase != Phase::Active {
                    continue;
                }
                top.phase = Phase::Undoing;
                match mode {
                    MaskMode::Stencil => out.items.push(Item::PopMask(top.mask)),
                    MaskMode::BoundingBox => clip = top.clip_before,
                    MaskMode::Off => {}
                }
            }
            RenderCommand::PopMask => {
                if let Some(open) = stack.pop() {
                    if open.phase == Phase::Active {
                        match mode {
                            MaskMode::Stencil => out.items.push(Item::PopMask(open.mask)),
                            MaskMode::BoundingBox => clip = open.clip_before,
                            MaskMode::Off => {}
                        }
                    }
                }
            }
            drawable => {
                let defining = stack.iter_mut().rev().find(|o| o.phase != Phase::Nested);
                if let Some(open) = defining.filter(|o| o.phase == Phase::Defining) {
                    open.pieces = open.pieces.saturating_add(1);
                    open.first = open.first.or_else(|| command_instance(drawable));
                }
                let defining = stack
                    .iter()
                    .rev()
                    .find(|o| o.phase != Phase::Nested)
                    .map(|o| (o.phase, o.mask));
                match defining {
                    Some((Phase::Undoing, _)) => continue,
                    Some((Phase::Defining, index)) if mode != MaskMode::Off => {
                        if let Some(target) = out.masks.get_mut(index) {
                            let lb = letterbox_of(command_instance(drawable));
                            let world = command_world(drawable, &lb);
                            fill(drawable, &world, target);
                        }
                        continue;
                    }
                    _ => {}
                }
                let lb = letterbox_of(command_instance(drawable));
                if let Some(item) = item_of(drawable, &lb, clip) {
                    out.items.push(item);
                }
            }
        }
    }
    while let Some(open) = stack.pop() {
        if open.phase == Phase::Active && mode == MaskMode::Stencil {
            out.items.push(Item::PopMask(open.mask));
        }
    }
}

fn command_instance(command: &RenderCommand) -> Option<InstanceId> {
    match *command {
        RenderCommand::Shape { instance, .. }
        | RenderCommand::Morph { instance, .. }
        | RenderCommand::EditText { instance, .. }
        | RenderCommand::StaticText { instance, .. }
        | RenderCommand::Surface { instance, .. } => Some(instance),
        _ => None,
    }
}

fn command_world(command: &RenderCommand, letterbox: &Letterbox) -> Affine {
    let m = match command {
        RenderCommand::Shape { transform, .. }
        | RenderCommand::Morph { transform, .. }
        | RenderCommand::EditText { transform, .. }
        | RenderCommand::StaticText { transform, .. }
        | RenderCommand::Surface { transform, .. } => *transform,
        _ => Matrix::IDENTITY,
    };
    window_affine(letterbox, &m)
}

fn item_of(command: &RenderCommand, letterbox: &Letterbox, clip: [f32; 4]) -> Option<Item> {
    match *command {
        RenderCommand::Shape {
            character,
            transform,
            color,
            ..
        } => Some(Item::Shape {
            character,
            world: window_affine(letterbox, &transform),
            color,
            clip,
        }),
        RenderCommand::EditText {
            instance,
            character,
            transform,
            color,
        } => Some(Item::Text {
            instance,
            character,
            world: window_affine(letterbox, &transform),
            color,
            clip,
        }),
        RenderCommand::Surface {
            instance,
            bounds,
            transform,
            color,
        } => Some(Item::Surface {
            instance,
            bounds,
            world: window_affine(letterbox, &transform),
            color,
            clip,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextKey {
    pub text: String,
    pub font: Option<&'static str>,
    pub size_px: u32,
    pub bounds: [u32; 4],
    pub alignment: u32,
    pub anchor: TextAnchor,
    pub scale: u32,
    pub x_scale: u32,
    pub wrap: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLayout {
    pub pen: [f32; 2],
    pub quads: Vec<UiQuad>,
}

#[derive(Debug, Default)]
pub struct TextCache {
    map: HashMap<TextKey, TextLayout>,
    provisional: HashSet<TextKey>,
    pub layouts: u64,
    pub hits: u64,
}

impl TextCache {
    pub fn begin_frame(&mut self) {
        for key in self.provisional.drain() {
            self.map.remove(&key);
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.provisional.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.provisional.is_empty()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get_or_layout(
        &mut self,
        key: &TextKey,
        layout: impl FnOnce(&TextKey) -> (TextLayout, bool),
    ) -> &TextLayout {
        if self.map.contains_key(key) {
            self.hits = self.hits.saturating_add(1);
        } else {
            let (made, provisional) = layout(key);
            self.layouts = self.layouts.saturating_add(1);
            if provisional {
                self.provisional.insert(key.clone());
            }
            self.map.insert(key.clone(), made);
        }
        self.map.entry(key.clone()).or_default()
    }
}

#[derive(Debug, Default)]
pub struct MeshCache {
    map: HashMap<CharacterRef, Option<Arc<ShapeMesh>>>,
    pub built: u64,
}

impl MeshCache {
    pub fn get_or_build(
        &mut self,
        character: CharacterRef,
        build: impl FnOnce() -> Option<ShapeMesh>,
    ) -> Option<Arc<ShapeMesh>> {
        if let Some(found) = self.map.get(&character) {
            return found.clone();
        }
        let mesh = build().map(|mut mesh| {
            mesh.id = NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed);
            Arc::new(mesh)
        });
        self.built = self.built.saturating_add(1);
        self.map.insert(character, mesh.clone());
        mesh
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use mint_sage_apt::parse_ru;
    use mint_sage_apt_player::MovieId;

    fn character(id: u32) -> CharacterRef {
        CharacterRef {
            movie: MovieId(0),
            id,
        }
    }

    fn shape(id: u32) -> RenderCommand {
        RenderCommand::Shape {
            instance: InstanceId::from_parts(id, 0),
            character: character(id),
            transform: Matrix::IDENTITY,
            color: ColorTransform::IDENTITY,
        }
    }

    fn fill(command: &RenderCommand, world: &Affine, out: &mut Vec<[f32; 2]>) {
        if let RenderCommand::Shape { character, .. } = command {
            #[allow(clippy::cast_precision_loss)]
            let x = 10.0 * character.id as f32;
            out.extend(
                rect_triangles([x, 0.0, x + 10.0, 10.0])
                    .iter()
                    .map(|p| apply(world, *p)),
            );
        }
    }

    fn masked() -> Vec<RenderCommand> {
        vec![
            RenderCommand::PushMask,
            shape(1),
            shape(3),
            RenderCommand::ActivateMask,
            shape(5),
            RenderCommand::DeactivateMask,
            shape(1),
            shape(3),
            RenderCommand::PopMask,
            shape(7),
        ]
    }

    fn planned(mode: MaskMode) -> Plan {
        let letterbox = Letterbox::fit(1024.0, 768.0, 1024.0, 768.0);
        let mut out = Plan::default();
        plan(&masked(), &|_| letterbox, mode, fill, &mut out);
        out
    }

    fn drawn(p: &Plan) -> Vec<(u32, [f32; 4])> {
        p.items
            .iter()
            .filter_map(|i| match i {
                Item::Shape {
                    character, clip, ..
                } => Some((character.id, *clip)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_sprite_mask_is_the_union_of_its_pieces() {
        let p = planned(MaskMode::Stencil);
        assert!(matches!(
            p.items.as_slice(),
            [
                Item::PushMask(0),
                Item::Shape { .. },
                Item::PopMask(0),
                Item::Shape { .. }
            ]
        ));
        assert_eq!(
            drawn(&p),
            vec![(5, UiQuad::UNCLIPPED), (7, UiQuad::UNCLIPPED)]
        );
        assert_eq!((p.mask_count, p.compound_masks), (1, 1));
        let silhouette = &p.masks[0];
        assert_eq!(silhouette.len(), 12, "both pieces' triangles");
        assert!(silhouette.contains(&[10.0, 0.0]) && silhouette.contains(&[40.0, 10.0]));
    }

    #[test]
    fn box_mode_clips_to_the_pieces_and_off_draws_them() {
        let p = planned(MaskMode::BoundingBox);
        assert_eq!(
            drawn(&p),
            vec![(5, [10.0, 0.0, 40.0, 10.0]), (7, UiQuad::UNCLIPPED)]
        );
        let p = planned(MaskMode::Off);
        let ids: Vec<u32> = drawn(&p).iter().map(|d| d.0).collect();
        assert_eq!(ids, vec![1, 3, 5, 7]);
    }

    fn key(text: &str) -> TextKey {
        TextKey {
            text: text.to_owned(),
            font: None,
            size_px: 12,
            bounds: [0; 4],
            alignment: 0,
            anchor: TextAnchor::Top,
            scale: 1.0f32.to_bits(),
            x_scale: 1.0f32.to_bits(),
            wrap: 0,
        }
    }

    fn layout_at(x: f32) -> TextLayout {
        TextLayout {
            pen: [x, 0.0],
            quads: Vec::new(),
        }
    }

    #[test]
    fn text_is_laid_out_once_per_change() {
        let mut cache = TextCache::default();
        let mut calls = 0;
        for _ in 0..3 {
            cache.begin_frame();
            let got = cache.get_or_layout(&key("PLAY"), |_| {
                calls += 1;
                (layout_at(1.0), false)
            });
            assert_eq!(got.pen, [1.0, 0.0]);
        }
        assert_eq!(calls, 1);
        cache.get_or_layout(&key("QUIT"), |_| {
            calls += 1;
            (layout_at(2.0), false)
        });
        assert_eq!(calls, 2);

        cache.get_or_layout(&key("NEW"), |_| {
            calls += 1;
            (layout_at(3.0), true)
        });
        cache.get_or_layout(&key("NEW"), |_| panic!("same frame: cached"));
        cache.begin_frame();
        let got = cache.get_or_layout(&key("NEW"), |_| {
            calls += 1;
            (layout_at(4.0), false)
        });
        assert_eq!(got.pen, [4.0, 0.0]);
        assert_eq!(calls, 4);
        cache.begin_frame();
        cache.get_or_layout(&key("NEW"), |_| panic!("final layout kept"));
        assert_eq!(cache.layouts, 4);
    }

    #[test]
    fn is_settled_tracks_whether_a_provisional_layout_is_outstanding() {
        let mut cache = TextCache::default();
        assert!(cache.is_settled(), "an empty cache has nothing outstanding");

        cache.get_or_layout(&key("PARTIAL"), |_| (layout_at(1.0), true));
        assert!(
            !cache.is_settled(),
            "a provisional layout must be reported as unsettled"
        );

        cache.begin_frame();
        cache.get_or_layout(&key("UNRELATED"), |_| (layout_at(9.0), false));
        assert!(
            cache.is_settled(),
            "begin_frame drops the stale provisional entry; nothing outstanding \
             until it is asked for again"
        );

        let mut called = false;
        cache.get_or_layout(&key("UNRELATED"), |_| {
            called = true;
            (layout_at(0.0), true)
        });
        assert!(!called, "an already-cached key is a hit, not a relayout");

        cache.begin_frame();
        assert!(
            cache.is_settled(),
            "dropped again, but nothing has asked for it this frame yet"
        );
        cache.get_or_layout(&key("PARTIAL"), |_| (layout_at(3.0), true));
        assert!(
            !cache.is_settled(),
            "asking for it again and still deferring it must be unsettled"
        );
    }

    #[test]
    fn a_mesh_is_built_once_per_character() {
        let mut cache = MeshCache::default();
        for _ in 0..3 {
            let _ = cache.get_or_build(character(1), || Some(ShapeMesh::default()));
            let _ = cache.get_or_build(character(2), || None);
        }
        assert_eq!(cache.built, 2);
    }

    #[test]
    fn a_mesh_keeps_its_fill_box_and_unresolved_textures() {
        let ru = parse_ru(
            "c\ns s:1:2:3:4\nt 0:0:10:0:0:5\ns l:1:1:1:1:1\nl -3:2:4:20\n\
             s tc:255:255:255:255:12:1:0:0:1:0:0\nt 0:0:1:0:0:1\n",
        )
        .unwrap();
        let mesh = ShapeMesh::from_ru(&ru, |_| None);
        assert_eq!(mesh.runs.len(), 3);
        assert_eq!(mesh.fill.len(), 6);
        assert_eq!(mesh.bounds, Some([-3.0, 0.0, 10.0, 20.0]));
        assert_eq!(mesh.unresolved_textures, 1);
    }

    #[test]
    fn only_content_that_cannot_show_is_culled() {
        assert!(can_show([1.0, 1.0, 1.0, 0.5], [0.0; 4]));
        assert!(!can_show([1.0, 1.0, 1.0, 0.0], [0.0; 4]));
        assert!(can_show([1.0, 1.0, 1.0, 0.0], [0.0, 0.0, 0.0, 0.1]));
        assert!(!can_show([1.0, 1.0, 1.0, 0.5], [0.0, 0.0, 0.0, -0.5]));
        assert!(!can_show([1.0, 1.0, 1.0, -1.0], [0.0; 4]));

        let identity: Affine = ([[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
        let window = (1024.0, 768.0);
        let all = UiQuad::UNCLIPPED;
        assert!(on_screen(
            &identity,
            [1020.0, 0.0, 1030.0, 10.0],
            all,
            window
        ));
        assert!(!on_screen(
            &identity,
            [1030.0, 0.0, 1040.0, 10.0],
            all,
            window
        ));
        assert!(!on_screen(
            &identity,
            [0.0, -40.0, 10.0, -20.0],
            all,
            window
        ));
        assert!(!on_screen(
            &identity,
            [200.0, 0.0, 210.0, 10.0],
            [0.0, 0.0, 100.0, 100.0],
            window
        ));
        let moved: Affine = ([[1.0, 0.0], [0.0, 1.0]], [-2000.0, 0.0]);
        assert!(!on_screen(&moved, [0.0, 0.0, 10.0, 10.0], all, window));
    }
}
