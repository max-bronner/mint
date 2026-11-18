use crate::camera::WorldCamera;

const EDGE_PIXELS: f32 = 3.0;
const SCROLL_AMOUNT: f32 = 350.0;
const TURN_PER_PIXEL: f32 = 0.005;
const KEYPAD_TURN: f32 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollFactors {
    pub horizontal: f32,
    pub vertical: f32,
    pub keyboard: f32,
    pub edge: f32,
    pub move_anchor: bool,
    pub edge_scroll: bool,
    pub rotate: f32,
}

impl Default for ScrollFactors {
    fn default() -> Self {
        Self {
            horizontal: 1.0,
            vertical: 1.0,
            keyboard: 1.0,
            edge: 1.0,
            move_anchor: true,
            edge_scroll: true,
            rotate: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollKey {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKey {
    TurnLeft,
    TurnRight,
    ZoomIn,
    ZoomOut,
    Reset,
}

impl PadKey {
    #[must_use]
    pub fn from_digit(digit: u8) -> Option<Self> {
        Some(match digit {
            4 => Self::TurnLeft,
            6 => Self::TurnRight,
            8 => Self::ZoomIn,
            2 => Self::ZoomOut,
            5 => Self::Reset,
            _ => return None,
        })
    }
}

#[must_use]
pub fn click_point(press: [f32; 2], release: [f32; 2], tolerance: f32) -> Option<[f32; 2]> {
    let under = |a: f32, b: f32| (a - b).abs() < tolerance;
    (under(press[0], release[0]) && under(press[1], release[1]))
        .then(|| [press[0].min(release[0]), press[1].min(release[1])])
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScrollControls {
    pub factors: ScrollFactors,
    viewport: [f32; 2],
    pointer: Option<[f32; 2]>,
    anchor: Option<[f32; 2]>,
    turn_from: Option<[f32; 2]>,
    keys: [bool; 4],
    pad: [bool; 4],
    reset: bool,
    outside: bool,
}

impl ScrollControls {
    #[must_use]
    pub fn new(factors: ScrollFactors) -> Self {
        Self {
            factors,
            ..Self::default()
        }
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = [width, height];
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer = Some([x, y]);
        self.outside = false;
    }

    pub fn pointer_left(&mut self) {
        self.outside = true;
    }

    pub fn right_button(&mut self, pressed: bool) {
        self.anchor = if pressed { self.pointer } else { None };
    }

    pub fn middle_button(&mut self, pressed: bool) {
        self.turn_from = if pressed { self.pointer } else { None };
    }

    pub fn middle_double_click(&mut self) {
        self.turn_from = None;
        self.reset = true;
    }

    pub fn key(&mut self, key: ScrollKey, pressed: bool) {
        let index = match key {
            ScrollKey::Up => 0,
            ScrollKey::Down => 1,
            ScrollKey::Left => 2,
            ScrollKey::Right => 3,
        };
        if let Some(held) = self.keys.get_mut(index) {
            *held = pressed;
        }
    }

    pub fn pad_key(&mut self, key: PadKey, pressed: bool) {
        let index = match key {
            PadKey::TurnLeft => 0,
            PadKey::TurnRight => 1,
            PadKey::ZoomIn => 2,
            PadKey::ZoomOut => 3,
            PadKey::Reset => {
                self.reset = true;
                return;
            }
        };
        if let Some(held) = self.pad.get_mut(index) {
            *held = pressed;
        }
    }

    #[must_use]
    pub fn right_scrolling(&self) -> bool {
        self.anchor.is_some()
    }

    #[must_use]
    pub fn offset(&mut self) -> ([f32; 2], f32) {
        let f = self.factors;
        let mut offset = [0.0f32; 2];
        if let (Some(anchor), Some(at)) = (self.anchor.as_mut(), self.pointer) {
            if f.move_anchor {
                let reach = [
                    (self.viewport[0] * 0.5).floor(),
                    (self.viewport[1] * 0.5).floor(),
                ];
                for ((value, at), reach) in anchor.iter_mut().zip(at).zip(reach) {
                    *value = value.clamp(at - reach, at + reach);
                }
            }
            let pull = [
                (at[0] - anchor[0]) * f.horizontal,
                (at[1] - anchor[1]) * f.vertical,
            ];
            let length = (pull[0] * pull[0] + pull[1] * pull[1]).sqrt();
            let unit = if length > 0.0 {
                [pull[0] / length, pull[1] / length]
            } else {
                [0.0, 0.0]
            };
            let floor = f.keyboard * f.keyboard;
            offset[0] += pull[0] + f.horizontal * unit[0] * floor;
            offset[1] += pull[1] + f.vertical * unit[1] * floor;
        }
        let [up, down, left, right] = self.keys;
        let step = [
            SCROLL_AMOUNT * f.keyboard * f.horizontal,
            SCROLL_AMOUNT * f.keyboard * f.vertical,
        ];
        offset[1] += f32::from(u8::from(down)) * step[1] - f32::from(u8::from(up)) * step[1];
        offset[0] += f32::from(u8::from(right)) * step[0] - f32::from(u8::from(left)) * step[0];
        if let Some(at) = self.edge_pointer() {
            let [w, h] = self.viewport;
            let edge = [step[0] * f.edge, step[1] * f.edge];
            if at[1] < EDGE_PIXELS {
                offset[1] -= edge[1];
            }
            if at[1] >= h - EDGE_PIXELS {
                offset[1] += edge[1];
            }
            if at[0] < EDGE_PIXELS {
                offset[0] -= edge[0];
            }
            if at[0] >= w - EDGE_PIXELS {
                offset[0] += edge[0];
            }
        }
        let turn = match (self.turn_from, self.pointer) {
            (Some(from), Some(at)) => (at[0] - from[0]) * TURN_PER_PIXEL,
            _ => 0.0,
        };
        (offset, turn)
    }

    fn edge_pointer(&self) -> Option<[f32; 2]> {
        self.pointer
            .filter(|_| self.factors.edge_scroll && !self.outside)
    }

    fn scrolling(&self) -> bool {
        let at_edge = self.edge_pointer().is_some_and(|[x, y]| {
            let [w, h] = self.viewport;
            x < EDGE_PIXELS || y < EDGE_PIXELS || x >= w - EDGE_PIXELS || y >= h - EDGE_PIXELS
        });
        self.anchor.is_some() || self.turn_from.is_some() || self.keys.contains(&true) || at_edge
    }

    pub fn frame(&mut self, camera: &mut WorldCamera) {
        if camera.flying() {
            self.anchor = None;
        }
        if std::mem::take(&mut self.reset) {
            camera.reset_view();
        }
        if self.viewport[0] <= 0.0 || self.viewport[1] <= 0.0 {
            return;
        }
        if self.scrolling() {
            let (offset, turn) = self.offset();
            camera.scroll_by(offset, turn);
            return;
        }
        let turn = KEYPAD_TURN * self.factors.rotate;
        match self.pad {
            [true, ..] => camera.scroll_by([0.0, 0.0], -turn),
            [_, true, ..] => camera.scroll_by([0.0, 0.0], turn),
            [_, _, true, _] => camera.wheel(1.0),
            [.., true] => camera.wheel(-1.0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
