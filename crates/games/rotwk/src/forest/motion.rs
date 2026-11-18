use mint_movement::Vec2;
use mint_sim::StateHasher;

const ANGLE_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - std::f32::consts::PI / 64.0;
const BOUNCE_STOP_VELOCITY: f32 = 0.01;
const BOUNCE_FX_VELOCITY: f32 = 0.03;
const BOUNCE_FX_HEIGHT: f32 = 27.0;

pub(crate) const NO_SPEED_OVERRIDE: f32 = 0.0;
pub(crate) const DEVASTATION_TOPPLE_SPEED: f32 = 0.1;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToppleMotion {
    initial_velocity: f32,
    initial_accel: f32,
    bounce_velocity: f32,
    minimum_speed: f32,
    kill_when_finished: bool,
    sink_ticks: u32,
    sink_step: f32,
    pub(crate) topple_fx: Option<String>,
    pub(crate) bounce_fx: Option<String>,
}

impl ToppleMotion {
    pub(crate) fn from_draw(draw: &mint_sage_ini::TreeTopple) -> Self {
        let sink_ticks = crate::game_data::retail_duration(draw.sink_time_ms).get();
        let per_retail_frame = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
        let retail_frames = sink_ticks / per_retail_frame.max(1);
        #[allow(clippy::cast_precision_loss)]
        let sink_step = if retail_frames == 0 {
            draw.sink_distance
        } else {
            draw.sink_distance / retail_frames as f32
        };
        Self {
            initial_velocity: draw.initial_velocity,
            initial_accel: draw.initial_accel,
            bounce_velocity: draw.bounce_velocity,
            minimum_speed: draw.minimum_speed,
            kill_when_finished: draw.kill_when_finished,
            sink_ticks,
            sink_step,
            topple_fx: draw.topple_fx.clone(),
            bounce_fx: draw.bounce_fx.clone(),
        }
    }
}

impl Default for ToppleMotion {
    fn default() -> Self {
        Self::from_draw(&mint_sage_ini::TreeTopple::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Upright,
    Starting {
        speed_override: f32,
    },
    Falling,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Sink {
    #[default]
    None,
    Pending,
    Sinking {
        ticks_left: u32,
        depth: f32,
    },
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct TreeMotion {
    phase: Phase,
    dir: Vec2,
    angle: f32,
    velocity: f32,
    accel: f32,
    sink: Sink,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TreePose {
    pub(crate) toppled: bool,
    pub(crate) dir: Vec2,
    pub(crate) angle: f32,
    pub(crate) depth: f32,
    pub(crate) gone: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TreeFx {
    Topple { at: Vec2 },
    Bounce { at: Vec2, height: f32 },
}

impl TreeMotion {
    pub(crate) fn topple(&mut self, dir: Vec2, speed_override: f32) {
        if self.phase != Phase::Upright || self.sink != Sink::None {
            return;
        }
        let length = dir.length();
        self.dir = if length > 0.0 {
            Vec2::new(dir.x / length, dir.y / length)
        } else {
            Vec2::ZERO
        };
        self.phase = Phase::Starting { speed_override };
    }

    pub(crate) fn sink(&mut self) {
        if self.sink == Sink::None {
            self.sink = Sink::Pending;
        }
    }

    pub(crate) fn active(&self) -> bool {
        matches!(self.phase, Phase::Starting { .. } | Phase::Falling)
            || matches!(self.sink, Sink::Pending | Sink::Sinking { .. })
    }

    pub(crate) fn gone(&self) -> bool {
        self.sink == Sink::Gone
    }

    pub(crate) fn moved(&self) -> bool {
        self.phase != Phase::Upright || self.sink != Sink::None
    }

    pub(crate) fn pose(&self) -> TreePose {
        TreePose {
            toppled: self.phase != Phase::Upright,
            dir: self.dir,
            angle: self.angle,
            depth: match self.sink {
                Sink::Sinking { depth, .. } => depth,
                _ => 0.0,
            },
            gone: self.gone(),
        }
    }

    pub(crate) fn step(&mut self, motion: &ToppleMotion, at: Vec2, fx: &mut Vec<TreeFx>) {
        if self.sink == Sink::Pending {
            self.sink = Sink::Sinking {
                ticks_left: motion.sink_ticks,
                depth: 0.0,
            };
        }
        if let Sink::Sinking { ticks_left, depth } = self.sink {
            if ticks_left == 0 {
                self.sink = Sink::Gone;
                return;
            }
            self.sink = Sink::Sinking {
                ticks_left: ticks_left.saturating_sub(1),
                depth: depth + motion.sink_step,
            };
        }
        if let Phase::Starting { speed_override } = self.phase {
            let speed = if speed_override > 0.0 {
                speed_override
            } else {
                motion.minimum_speed.max(speed_override)
            };
            self.angle = 0.0;
            self.velocity = speed * motion.initial_velocity;
            self.accel = speed * motion.initial_accel;
            self.phase = Phase::Falling;
            fx.push(TreeFx::Topple { at });
        }
        if self.phase == Phase::Falling {
            self.fall(motion, at, fx);
        }
    }

    fn fall(&mut self, motion: &ToppleMotion, at: Vec2, fx: &mut Vec<TreeFx>) {
        let turn = if self.angle + self.velocity > ANGLE_LIMIT {
            ANGLE_LIMIT - self.angle
        } else {
            self.velocity
        };
        self.angle += turn;
        if self.angle < ANGLE_LIMIT || self.velocity <= 0.0 {
            self.velocity += self.accel;
            return;
        }
        self.velocity *= -motion.bounce_velocity;
        let speed = self.velocity.abs();
        if speed < BOUNCE_STOP_VELOCITY {
            self.velocity = 0.0;
            self.phase = Phase::Down;
            if motion.kill_when_finished {
                self.sink();
            }
        } else if speed >= BOUNCE_FX_VELOCITY {
            let (sin, cos) = mint_math::f32::sin_cos(self.angle);
            let reach = BOUNCE_FX_HEIGHT * sin;
            let depth = self.pose().depth;
            fx.push(TreeFx::Bounce {
                at: Vec2::new(at.x + self.dir.x * reach, at.y + self.dir.y * reach),
                height: BOUNCE_FX_HEIGHT * cos - depth,
            });
        }
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        let (phase, speed_override) = match self.phase {
            Phase::Upright => (0, 0.0),
            Phase::Starting { speed_override } => (1, speed_override),
            Phase::Falling => (2, 0.0),
            Phase::Down => (3, 0.0),
        };
        hasher.write_u8(phase);
        hasher.write_f32(speed_override);
        hasher.write_f32(self.dir.x);
        hasher.write_f32(self.dir.y);
        hasher.write_f32(self.angle);
        hasher.write_f32(self.velocity);
        hasher.write_f32(self.accel);
        let (sink, ticks_left, depth) = match self.sink {
            Sink::None => (0, 0, 0.0),
            Sink::Pending => (1, 0, 0.0),
            Sink::Sinking { ticks_left, depth } => (2, ticks_left, depth),
            Sink::Gone => (3, 0, 0.0),
        };
        hasher.write_u8(sink);
        hasher.write_u32(ticks_left);
        hasher.write_f32(depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(kill_when_finished: bool) -> ToppleMotion {
        ToppleMotion::from_draw(&mint_sage_ini::TreeTopple {
            kill_when_finished,
            ..mint_sage_ini::TreeTopple::default()
        })
    }

    fn settle(motion: &mut TreeMotion, data: &ToppleMotion) -> (u32, Vec<TreeFx>) {
        let mut fx = Vec::new();
        let mut ticks = 0;
        while motion.active() && ticks < 10_000 {
            motion.step(data, Vec2::new(100.0, 100.0), &mut fx);
            ticks += 1;
        }
        (ticks, fx)
    }

    #[test]
    fn a_crushed_tree_falls_bounces_twice_and_rests_on_the_ground() {
        let data = draw(false);
        let mut motion = TreeMotion::default();
        motion.topple(Vec2::new(0.0, 5.0), NO_SPEED_OVERRIDE);
        let mut fx = Vec::new();
        let mut first_ground = None;
        for tick in 1..=12 {
            motion.step(&data, Vec2::new(100.0, 100.0), &mut fx);
            if motion.angle >= ANGLE_LIMIT && first_ground.is_none() {
                first_ground = Some(tick);
            }
        }
        assert_eq!(first_ground, Some(12));
        let (rest_ticks, rest) = settle(&mut motion, &data);
        assert_eq!(12 + rest_ticks, 39);
        fx.extend(rest);
        assert_eq!(motion.phase, Phase::Down);
        assert!((motion.angle - ANGLE_LIMIT).abs() < 1e-4);
        assert_eq!(motion.dir, Vec2::new(0.0, 1.0));
        let bounces = fx
            .iter()
            .filter(|fx| matches!(fx, TreeFx::Bounce { .. }))
            .count();
        assert_eq!(
            fx.first(),
            Some(&TreeFx::Topple {
                at: Vec2::new(100.0, 100.0)
            })
        );
        assert_eq!(bounces, 1);
        let pose = motion.pose();
        assert!(pose.toppled && !pose.gone && pose.depth == 0.0);
    }

    #[test]
    fn the_devastation_override_falls_slower_than_the_minimum() {
        let data = draw(false);
        let mut motion = TreeMotion::default();
        motion.topple(Vec2::new(1.0, 0.0), DEVASTATION_TOPPLE_SPEED);
        let mut fx = Vec::new();
        let mut ticks = 0;
        while motion.angle < ANGLE_LIMIT {
            motion.step(&data, Vec2::ZERO, &mut fx);
            ticks += 1;
        }
        assert_eq!(ticks, 40);
    }

    #[test]
    fn a_sink_lasts_sink_time_and_a_killed_tree_sinks_at_rest() {
        let data = ToppleMotion::from_draw(&mint_sage_ini::TreeTopple {
            sink_time_ms: 1000.0,
            sink_distance: 2.0,
            ..mint_sage_ini::TreeTopple::default()
        });
        let mut standing = TreeMotion::default();
        standing.sink();
        let mut fx = Vec::new();
        for _ in 0..30 {
            standing.step(&data, Vec2::ZERO, &mut fx);
        }
        assert!(!standing.gone());
        assert!(
            (standing.pose().depth - 12.0).abs() < 1e-3,
            "{:?}",
            standing.pose()
        );
        standing.step(&data, Vec2::ZERO, &mut fx);
        assert!(standing.gone() && !standing.active());

        let mut killed = TreeMotion::default();
        killed.topple(Vec2::new(1.0, 0.0), NO_SPEED_OVERRIDE);
        settle(&mut killed, &draw(true));
        assert!(killed.gone());
        let mut kept = TreeMotion::default();
        kept.topple(Vec2::new(1.0, 0.0), NO_SPEED_OVERRIDE);
        settle(&mut kept, &draw(false));
        assert!(!kept.gone());
    }
}
