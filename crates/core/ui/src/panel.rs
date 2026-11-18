use mint_render::UiQuad;

use crate::input::Rect;

#[derive(Debug, Clone, Copy)]
pub struct Panel {
    pub rect: Rect,
    pub color: [f32; 4],
}

impl Panel {
    #[must_use]
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            color: [0.12, 0.12, 0.14, 1.0],
        }
    }

    #[must_use]
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    #[must_use]
    pub fn quad(&self) -> UiQuad {
        let [x, y] = self.rect.min;
        let [max_x, max_y] = self.rect.max;
        UiQuad::new(x, y, (max_x - x).max(0.0), (max_y - y).max(0.0))
            .with_color_transform(self.color, [0.0, 0.0, 0.0, 0.0])
    }

    #[must_use]
    pub fn content_clip(&self) -> [f32; 4] {
        let [x, y] = self.rect.min;
        let [max_x, max_y] = self.rect.max;
        [x, y, max_x, max_y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_draws_its_own_rect_and_clips_to_it() {
        let panel = Panel::new(Rect {
            min: [4.0, 5.0],
            max: [20.0, 40.0],
        });
        let quad = panel.quad();
        assert_eq!(quad.translation, [4.0, 5.0]);
        assert_eq!(quad.affine, [[16.0, 0.0], [0.0, 35.0]]);
        assert_eq!(panel.content_clip(), [4.0, 5.0, 20.0, 40.0]);
    }
}
