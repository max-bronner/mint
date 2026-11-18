use std::collections::BTreeMap;

use crate::arena::Handle;
use crate::avm::ObjectId;
use crate::geom::{ColorTransform, Matrix};
use crate::library::{CharacterRef, MovieId, TimelineId};

pub type InstanceId = Handle<Instance>;

#[derive(Debug, Clone, PartialEq)]
pub enum InstanceKind {
    Clip(ClipState),
    Button(ButtonVisual),
    Shape,
    Morph,
    EditText { text: String },
    StaticText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVisual {
    #[default]
    Up,
    Over,
    Down,
}

impl ButtonVisual {
    pub(crate) fn to_apt(self) -> mint_sage_apt::ButtonState {
        match self {
            Self::Up => mint_sage_apt::ButtonState::Up,
            Self::Over => mint_sage_apt::ButtonState::Over,
            Self::Down => mint_sage_apt::ButtonState::Down,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipState {
    pub timeline: TimelineId,
    pub current_frame: u16,
    pub playing: bool,
    pub initialized: bool,
    pub clip_actions: Option<(MovieId, u32)>,
}

impl ClipState {
    #[must_use]
    pub fn frame_index(&self) -> u16 {
        self.current_frame.saturating_sub(1)
    }
}

pub(crate) mod flags {
    pub(crate) const TRANSFORMED_BY_SCRIPT: u8 = 1 << 0;
    pub(crate) const SCALE_ROTATION_CACHED: u8 = 1 << 1;
    pub(crate) const INSTANTIATED_BY_TIMELINE: u8 = 1 << 2;
    pub(crate) const PLACED_BY_SCRIPT: u8 = 1 << 3;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub(crate) parent: Option<InstanceId>,
    pub(crate) depth: i32,
    pub(crate) tree_depth: u16,
    pub(crate) placed_id: u32,
    pub(crate) character: CharacterRef,
    pub(crate) kind: InstanceKind,
    pub(crate) matrix: Matrix,
    pub(crate) color: ColorTransform,
    pub(crate) ratio: f32,
    pub(crate) name: Option<String>,
    pub(crate) default_name: Option<u32>,
    pub(crate) clip_depth: i32,
    pub(crate) visible: bool,
    pub(crate) place_frame: u16,
    pub(crate) flags: u8,
    pub(crate) rotation: f32,
    pub(crate) scale_x: f32,
    pub(crate) scale_y: f32,
    pub(crate) skew: f32,
    pub(crate) children: BTreeMap<i32, InstanceId>,
    pub(crate) script: Option<ObjectId>,
    pub(crate) removed: bool,
}

impl Instance {
    pub(crate) fn new(
        parent: Option<InstanceId>,
        depth: i32,
        tree_depth: u16,
        placed_id: u32,
        character: CharacterRef,
        kind: InstanceKind,
    ) -> Self {
        Self {
            parent,
            depth,
            tree_depth,
            placed_id,
            character,
            kind,
            matrix: Matrix::IDENTITY,
            color: ColorTransform::IDENTITY,
            ratio: 0.0,
            name: None,
            default_name: None,
            clip_depth: 0,
            visible: true,
            place_frame: 0,
            flags: 0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            skew: 0.0,
            children: BTreeMap::new(),
            script: None,
            removed: false,
        }
    }

    #[must_use]
    pub fn script(&self) -> Option<ObjectId> {
        self.script
    }

    #[must_use]
    pub fn pending_removal(&self) -> bool {
        self.removed
    }

    #[must_use]
    pub fn placed_by_script(&self) -> bool {
        self.flags & flags::PLACED_BY_SCRIPT != 0
    }

    #[must_use]
    pub fn parent(&self) -> Option<InstanceId> {
        self.parent
    }

    #[must_use]
    pub fn depth(&self) -> i32 {
        self.depth
    }

    #[must_use]
    pub fn tree_depth(&self) -> u16 {
        self.tree_depth
    }

    #[must_use]
    pub fn placed_id(&self) -> u32 {
        self.placed_id
    }

    #[must_use]
    pub fn character(&self) -> CharacterRef {
        self.character
    }

    #[must_use]
    pub fn kind(&self) -> &InstanceKind {
        &self.kind
    }

    #[must_use]
    pub fn clip(&self) -> Option<&ClipState> {
        match &self.kind {
            InstanceKind::Clip(clip) => Some(clip),
            _ => None,
        }
    }

    pub(crate) fn clip_mut(&mut self) -> Option<&mut ClipState> {
        match &mut self.kind {
            InstanceKind::Clip(clip) => Some(clip),
            _ => None,
        }
    }

    pub fn children(&self) -> impl Iterator<Item = (i32, InstanceId)> + '_ {
        self.children.iter().map(|(d, h)| (*d, *h))
    }

    #[must_use]
    pub fn child_at(&self, depth: i32) -> Option<InstanceId> {
        self.children.get(&depth).copied()
    }

    #[must_use]
    pub fn matrix(&self) -> Matrix {
        self.matrix
    }

    #[must_use]
    pub fn color(&self) -> ColorTransform {
        self.color
    }

    #[must_use]
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    #[must_use]
    pub fn explicit_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub fn name(&self) -> String {
        match (&self.name, self.default_name) {
            (Some(name), _) => name.clone(),
            (None, Some(n)) => format!("instance{n}"),
            (None, None) => String::new(),
        }
    }

    #[must_use]
    pub fn clip_depth(&self) -> i32 {
        self.clip_depth
    }

    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    #[must_use]
    pub fn place_frame(&self) -> u16 {
        self.place_frame
    }

    #[must_use]
    pub fn transformed_by_script(&self) -> bool {
        self.flags & flags::TRANSFORMED_BY_SCRIPT != 0
    }

    #[must_use]
    pub fn instantiated_by_timeline(&self) -> bool {
        self.flags & flags::INSTANTIATED_BY_TIMELINE != 0
    }

    pub(crate) fn set_matrix(&mut self, matrix: Matrix) {
        self.matrix = matrix;
        self.flags &= !flags::SCALE_ROTATION_CACHED;
    }

    fn mark_script(&mut self) {
        self.flags |= flags::TRANSFORMED_BY_SCRIPT;
    }

    fn cache_scale_rotation(&mut self) {
        if self.flags & flags::SCALE_ROTATION_CACHED != 0 {
            return;
        }
        let Matrix { a, b, c, d, .. } = self.matrix;
        let rotation_x = mint_math::f32::atan2(b, a);
        let rotation_y = mint_math::f32::atan2(-c, d);
        self.rotation = rotation_x;
        self.scale_x = (a * a + b * b).sqrt();
        self.scale_y = (c * c + d * d).sqrt();
        self.skew = rotation_y - rotation_x;
        self.flags |= flags::SCALE_ROTATION_CACHED;
    }

    #[must_use]
    pub fn x(&self) -> f32 {
        self.matrix.tx
    }

    #[must_use]
    pub fn y(&self) -> f32 {
        self.matrix.ty
    }

    pub fn set_x(&mut self, x: f32) {
        self.matrix.tx = x;
        self.mark_script();
    }

    pub fn set_y(&mut self, y: f32) {
        self.matrix.ty = y;
        self.mark_script();
    }

    pub fn rotation(&mut self) -> f32 {
        self.cache_scale_rotation();
        self.rotation.to_degrees()
    }

    pub fn x_scale(&mut self) -> f32 {
        self.cache_scale_rotation();
        self.scale_x * 100.0
    }

    pub fn y_scale(&mut self) -> f32 {
        self.cache_scale_rotation();
        self.scale_y * 100.0
    }

    pub fn set_rotation(&mut self, degrees: f32) {
        self.mark_script();
        self.cache_scale_rotation();
        let radians = degrees.to_radians();
        self.rotation = radians;
        if radians.is_nan() {
            return;
        }
        let (sin_x, cos_x) = mint_math::f32::sin_cos(radians);
        let (sin_y, cos_y) = mint_math::f32::sin_cos(radians + self.skew);
        self.matrix.a = self.scale_x * cos_x;
        self.matrix.b = self.scale_x * sin_x;
        self.matrix.c = self.scale_y * -sin_y;
        self.matrix.d = self.scale_y * cos_y;
    }

    pub fn set_x_scale(&mut self, percent: f32) {
        self.mark_script();
        self.cache_scale_rotation();
        self.scale_x = percent / 100.0;
        let unit = if percent.is_nan() {
            0.0
        } else {
            percent / 100.0
        };
        let rotation = if self.rotation.is_nan() {
            0.0
        } else {
            self.rotation
        };
        let (sin, cos) = mint_math::f32::sin_cos(rotation);
        self.matrix.a = cos * unit;
        self.matrix.b = sin * unit;
    }

    pub fn set_y_scale(&mut self, percent: f32) {
        self.mark_script();
        self.cache_scale_rotation();
        self.scale_y = percent / 100.0;
        let unit = if percent.is_nan() {
            0.0
        } else {
            percent / 100.0
        };
        let rotation = if self.rotation.is_nan() {
            0.0
        } else {
            self.rotation
        };
        let (sin, cos) = mint_math::f32::sin_cos(rotation + self.skew);
        self.matrix.c = -sin * unit;
        self.matrix.d = cos * unit;
    }

    #[must_use]
    pub fn alpha(&self) -> f32 {
        self.color.mult[3] * 100.0
    }

    pub fn set_alpha(&mut self, percent: f32) {
        self.mark_script();
        self.color.mult[3] = percent / 100.0;
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    pub fn set_color(&mut self, color: ColorTransform) {
        self.mark_script();
        self.color = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CharacterRef, MovieId};

    fn shape() -> Instance {
        Instance::new(
            None,
            1,
            1,
            1,
            CharacterRef {
                movie: MovieId(0),
                id: 1,
            },
            InstanceKind::Shape,
        )
    }

    #[test]
    fn scale_and_rotation_derive_from_the_matrix_and_round_trip() {
        let mut i = shape();
        i.set_matrix(Matrix {
            a: 0.0,
            b: 2.0,
            c: -3.0,
            d: 0.0,
            tx: 5.0,
            ty: 6.0,
        });
        assert!((i.rotation() - 90.0).abs() < 1e-3);
        assert!((i.x_scale() - 200.0).abs() < 1e-3);
        assert!((i.y_scale() - 300.0).abs() < 1e-3);
        i.set_x_scale(50.0);
        assert!(
            (i.matrix.b - 0.5).abs() < 1e-5,
            "rotation kept, scale changed"
        );
        assert!(i.transformed_by_script());
    }

    #[test]
    fn a_negative_scale_survives_once_cached() {
        let mut i = shape();
        i.set_x_scale(-100.0);
        assert!(
            (i.x_scale() + 100.0).abs() < 1e-4,
            "the cached value, not |a|"
        );
    }

    #[test]
    fn unnamed_default_names_are_instance_n() {
        let mut i = shape();
        assert_eq!(i.name(), "");
        i.default_name = Some(3);
        assert_eq!(i.name(), "instance3");
        i.set_name("Open".to_owned());
        assert_eq!(i.name(), "Open");
    }
}
