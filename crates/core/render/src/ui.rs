use crate::renderer::TextureHandle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiQuad {
    pub affine: [[f32; 2]; 2],
    pub translation: [f32; 2],
    pub texture: Option<TextureHandle>,
    pub uv_rect: [f32; 4],
    pub color_multiply: [f32; 4],
    pub color_add: [f32; 4],
    pub clip: [f32; 4],
    pub premultiplied: bool,
    pub greyscale: bool,
    pub opaque: bool,
    pub additive: bool,
    pub disc: bool,
    pub wedge: f32,
    pub hole: f32,
    pub uv_rotated_cw: bool,
}

impl UiQuad {
    pub const UNCLIPPED: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];

    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            affine: [[width, 0.0], [0.0, height]],
            translation: [x, y],
            texture: None,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color_multiply: [1.0, 1.0, 1.0, 1.0],
            color_add: [0.0, 0.0, 0.0, 0.0],
            clip: Self::UNCLIPPED,
            premultiplied: false,
            greyscale: false,
            opaque: false,
            additive: false,
            disc: false,
            wedge: 1.0,
            hole: 0.0,
            uv_rotated_cw: false,
        }
    }

    #[must_use]
    pub fn with_texture(mut self, texture: TextureHandle, uv_rect: [f32; 4]) -> Self {
        self.texture = Some(texture);
        self.uv_rect = uv_rect;
        self
    }

    #[must_use]
    pub fn with_color_transform(mut self, multiply: [f32; 4], add: [f32; 4]) -> Self {
        self.color_multiply = multiply;
        self.color_add = add;
        self
    }

    #[must_use]
    pub fn with_clip(mut self, clip: [f32; 4]) -> Self {
        self.clip = clip;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiGeometry<'a> {
    pub affine: [[f32; 2]; 2],
    pub translation: [f32; 2],
    pub triangles: &'a [[f32; 2]],
    pub lines: &'a [[f32; 2]],
    pub color: [f32; 4],
    pub color_add: [f32; 4],
    pub clip: [f32; 4],
    pub texture: Option<TextureHandle>,
    pub texture_transform: [[f32; 2]; 2],
    pub texture_translation: [f32; 2],
}

impl<'a> UiGeometry<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            affine: [[1.0, 0.0], [0.0, 1.0]],
            translation: [0.0, 0.0],
            triangles: &[],
            lines: &[],
            color: [1.0, 1.0, 1.0, 1.0],
            color_add: [0.0, 0.0, 0.0, 0.0],
            clip: UiQuad::UNCLIPPED,
            texture: None,
            texture_transform: [[1.0, 0.0], [0.0, 1.0]],
            texture_translation: [0.0, 0.0],
        }
    }

    #[must_use]
    pub fn with_transform(mut self, affine: [[f32; 2]; 2], translation: [f32; 2]) -> Self {
        self.affine = affine;
        self.translation = translation;
        self
    }

    #[must_use]
    pub fn with_triangles(mut self, triangles: &'a [[f32; 2]]) -> Self {
        self.triangles = triangles;
        self
    }

    #[must_use]
    pub fn with_lines(mut self, lines: &'a [[f32; 2]]) -> Self {
        self.lines = lines;
        self
    }

    #[must_use]
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    #[must_use]
    pub fn with_color_add(mut self, add: [f32; 4]) -> Self {
        self.color_add = add;
        self
    }

    #[must_use]
    pub fn with_clip(mut self, clip: [f32; 4]) -> Self {
        self.clip = clip;
        self
    }

    #[must_use]
    pub fn with_texture(
        mut self,
        texture: TextureHandle,
        transform: [[f32; 2]; 2],
        translation: [f32; 2],
    ) -> Self {
        self.texture = Some(texture);
        self.texture_transform = transform;
        self.texture_translation = translation;
        self
    }
}

impl<'a> Default for UiGeometry<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScreenDraw<'a> {
    Quad(UiQuad),
    Geometry(UiGeometry<'a>),
    PushMask(MaskShape<'a>),
    PopMask(MaskShape<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskShape<'a> {
    pub affine: [[f32; 2]; 2],
    pub translation: [f32; 2],
    pub triangles: &'a [[f32; 2]],
}

impl<'a> MaskShape<'a> {
    #[must_use]
    pub fn new(triangles: &'a [[f32; 2]]) -> Self {
        Self {
            affine: [[1.0, 0.0], [0.0, 1.0]],
            translation: [0.0, 0.0],
            triangles,
        }
    }

    #[must_use]
    pub fn with_transform(mut self, affine: [[f32; 2]; 2], translation: [f32; 2]) -> Self {
        self.affine = affine;
        self.translation = translation;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_new_is_identity_empty_white_and_unclipped() {
        let geometry = UiGeometry::new();
        assert_eq!(geometry.affine, [[1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(geometry.translation, [0.0, 0.0]);
        assert!(geometry.triangles.is_empty());
        assert!(geometry.lines.is_empty());
        assert_eq!(geometry.color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(geometry.color_add, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(geometry.clip, UiQuad::UNCLIPPED);
        assert_eq!(geometry.texture, None);
        assert_eq!(geometry.texture_transform, [[1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(geometry.texture_translation, [0.0, 0.0]);
    }

    fn dummy_texture() -> TextureHandle {
        let mut renderer = crate::Renderer::offscreen(4, 4).unwrap();
        let data = crate::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).unwrap();
        renderer.upload_texture(&data).unwrap()
    }

    #[test]
    fn with_texture_sets_texture_and_leaves_triangles_and_color_alone() {
        let texture = dummy_texture();
        let geometry = UiGeometry::new()
            .with_color([0.5, 0.5, 0.5, 1.0])
            .with_texture(texture, [[2.0, 0.0], [0.0, 3.0]], [10.0, 20.0]);
        assert_eq!(geometry.texture, Some(texture));
        assert_eq!(geometry.texture_transform, [[2.0, 0.0], [0.0, 3.0]]);
        assert_eq!(geometry.texture_translation, [10.0, 20.0]);
        assert_eq!(geometry.color, [0.5, 0.5, 0.5, 1.0]);
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn new_is_axis_aligned_white_and_unclipped() {
        let quad = UiQuad::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(quad.affine, [[30.0, 0.0], [0.0, 40.0]]);
        assert_eq!(quad.translation, [10.0, 20.0]);
        assert_eq!(quad.texture, None);
        assert_eq!(quad.color_multiply, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(quad.color_add, [0.0, 0.0, 0.0, 0.0]);
        assert!((quad.wedge - 1.0).abs() < f32::EPSILON);
        assert!(quad.hole.abs() < f32::EPSILON);
        assert_eq!(quad.clip, UiQuad::UNCLIPPED);
    }

    #[test]
    fn unclipped_contains_any_finite_screen_position() {
        let [min_x, min_y, max_x, max_y] = UiQuad::UNCLIPPED;
        for (x, y) in [(0.0, 0.0), (-1.0e6, 1.0e6), (50_000.0, -50_000.0)] {
            assert!(x >= min_x && x <= max_x);
            assert!(y >= min_y && y <= max_y);
        }
    }
}
