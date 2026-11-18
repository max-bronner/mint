use mint_sage_ini::living_world::presentation::{FadeMethod, FadeType, IconObjectBase};

pub const SELECTED: u8 = 1;
pub const HILIGHTED: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    None,
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFade {
    flags: u8,
    shown: bool,
    mode: Mode,
    elapsed: u32,
    duration: u32,
    in_end: u32,
    out_start: u32,
    end: u32,
    hold: f32,
    alpha: f32,
}

#[must_use]
pub fn visible(object: &IconObjectBase, flags: u8) -> bool {
    let set = |field: Option<bool>| field == Some(true);
    let selection = if flags & SELECTED != 0 {
        set(object.hide_when_selected)
    } else {
        set(object.hide_when_unselected)
    };
    let highlight = if flags & HILIGHTED != 0 {
        set(object.hide_when_hilighted)
    } else {
        set(object.hide_when_unhilighted)
    };
    !selection && !highlight
}

fn ratio(numerator: u32, denominator: u32) -> f32 {
    if denominator == 0 {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let r = numerator as f32 / denominator as f32;
    r
}

impl ObjectFade {
    #[must_use]
    pub fn new(object: &IconObjectBase) -> Self {
        Self {
            flags: 0,
            shown: visible(object, 0),
            mode: Mode::None,
            elapsed: 0,
            duration: 0,
            in_end: 0,
            out_start: 0,
            end: 0,
            hold: 0.0,
            alpha: 1.0,
        }
    }

    #[must_use]
    pub fn shown(&self) -> bool {
        self.shown
    }

    #[must_use]
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn set_flags(&mut self, object: &IconObjectBase, flags: u8) {
        for (bit, on, off) in [
            (
                SELECTED,
                object.fade_type_for_selection,
                object.fade_type_for_deselection,
            ),
            (
                HILIGHTED,
                object.fade_type_for_hilighting,
                object.fade_type_for_unhilighting,
            ),
        ] {
            let want = flags & bit;
            if self.flags & bit == want {
                continue;
            }
            let was = visible(object, self.flags);
            self.flags = (self.flags & !bit) | want;
            let now = visible(object, self.flags);
            let kind = if want != 0 { on } else { off };
            self.change(object, was, now, kind.unwrap_or(FadeType::None));
        }
    }

    fn change(&mut self, object: &IconObjectBase, was: bool, now: bool, kind: FadeType) {
        let fade_in = object.fade_in_time.unwrap_or(10);
        let fade_out = object.fade_out_time.unwrap_or(10);
        let running = match kind {
            FadeType::In => Mode::In,
            FadeType::Out => Mode::Out,
            FadeType::InOut => Mode::InOut,
            FadeType::None | FadeType::Stop => Mode::None,
        };
        let mut keep = false;
        let same = running == self.mode && !matches!(kind, FadeType::Stop);
        if !same {
            match kind {
                FadeType::In => {
                    if was {
                    } else if !now {
                        return;
                    } else {
                        self.start(Mode::In, fade_in, 0, 0, 0, 0.0);
                    }
                }
                FadeType::Out => {
                    self.start(Mode::Out, fade_out, 0, 0, 0, 0.0);
                    keep = was;
                }
                FadeType::InOut => {
                    let hold = object.fade_hold_percent.unwrap_or(0.0);
                    let end = fade_in.saturating_add(fade_out);
                    self.start(Mode::InOut, fade_in, fade_in, fade_in, end, hold);
                    keep = was;
                }
                FadeType::Stop => self.stop(now),
                FadeType::None => {}
            }
        }
        if was != now && !keep {
            self.shown = now;
        }
    }

    fn start(
        &mut self,
        mode: Mode,
        duration: u32,
        in_end: u32,
        out_start: u32,
        end: u32,
        hold: f32,
    ) {
        self.mode = mode;
        self.elapsed = 0;
        self.duration = duration;
        self.in_end = in_end;
        self.out_start = out_start;
        self.end = end;
        self.hold = hold;
        self.update();
    }

    fn stop(&mut self, now: bool) {
        if self.mode != Mode::None {
            self.alpha = if now { 1.0 } else { 0.0 };
            self.shown = now;
            self.mode = Mode::None;
        }
    }

    fn finish(&mut self) {
        if self.mode == Mode::Out || (self.mode == Mode::InOut && self.hold == 0.0) {
            self.shown = false;
        }
        self.mode = Mode::None;
    }

    pub fn update(&mut self) {
        let f = match self.mode {
            Mode::None => return,
            Mode::In | Mode::Out => {
                let f = if self.mode == Mode::In {
                    ratio(self.elapsed, self.duration)
                } else {
                    ratio(self.duration.saturating_sub(self.elapsed), self.duration)
                };
                self.elapsed = self.elapsed.saturating_add(1);
                if self.elapsed > self.duration {
                    self.finish();
                }
                f
            }
            Mode::InOut => {
                self.elapsed = self.elapsed.saturating_add(1);
                let e = self.elapsed;
                if e > self.end {
                    self.finish();
                    0.0
                } else if e > self.out_start {
                    let f = ratio(self.end - e, self.end - self.out_start);
                    if self.hold >= f {
                        self.finish();
                        self.hold
                    } else {
                        f
                    }
                } else if e > self.in_end {
                    1.0
                } else {
                    ratio(e, self.in_end)
                }
            }
        };
        self.alpha = f;
    }
}

#[must_use]
pub fn tint(method: Option<FadeMethod>, tint: [f32; 4], alpha: f32) -> [f32; 4] {
    match method.unwrap_or(FadeMethod::Opacity) {
        FadeMethod::Opacity => [tint[0], tint[1], tint[2], tint[3] * alpha],
        FadeMethod::Additive => [tint[0] * alpha, tint[1] * alpha, tint[2] * alpha, tint[3]],
    }
}

#[cfg(test)]
mod tests;
