use mint_movement::Vec2;
use mint_sage_combat::{DeathTypes, SlowDeathOption};
use mint_sim::{Component, Frame, LogicFrames, StateHasher};

use mint_sage_modules::{DieMuxData, UpgradeBits};
use mint_sim::module::Death;

use crate::combat::DeathNaming;
use crate::death_phase::{DeathPhase, PhaseLists};
use crate::templates::KindTable;
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lifetime {
    pub(crate) dies_at: Frame,
    pub(crate) death: DeathNaming,
    pub(crate) set_at: Option<Frame>,
}

impl Lifetime {
    pub(crate) fn span(&self) -> Option<(Frame, Frame)> {
        Some((self.set_at?, self.dies_at))
    }
}

pub(crate) fn remaining_share((from, until): (Frame, Frame), now: Frame) -> Option<f32> {
    let span = until
        .get()
        .checked_sub(from.get())
        .filter(|span| *span > 0)?;
    let left = until.get().saturating_sub(now.get()).min(span);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let fraction = (left as f64 / span as f64) as f32;
    Some(fraction)
}

impl Component for Lifetime {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.dies_at.hash_into(hasher);
        self.death.hash_into(hasher);
        self.set_at.unwrap_or(Frame::FOREVER).hash_into(hasher);
    }
}

pub(crate) fn lifetime_delay(
    min: LogicFrames,
    max: LogicFrames,
    draw: impl FnOnce(u32) -> u32,
) -> LogicFrames {
    let (low, high) = if min.get() <= max.get() {
        (min.get(), max.get())
    } else {
        (max.get(), min.get())
    };
    let span = high.saturating_sub(low);
    let extra = if span == 0 { 0 } else { draw(span).min(span) };
    LogicFrames::from_frames(low.saturating_add(extra).max(1))
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Lifetimes(Vec<Option<(LogicFrames, LogicFrames, DeathNaming)>>);

impl Lifetimes {
    pub(crate) fn build(kinds: &KindTable, death_types: &mut DeathTypes) -> Self {
        let mut out = Vec::with_capacity(kinds.len());
        for index in 0..kinds.len() {
            let range = u16::try_from(index)
                .ok()
                .and_then(|k| kinds.get(Kind(k)))
                .and_then(|entry| {
                    let module = entry
                        .record
                        .declared_modules
                        .iter()
                        .find(|m| m.name.eq_ignore_ascii_case("LifetimeUpdate"))?;
                    let ms = |keyword: &str| {
                        module
                            .field(keyword)
                            .and_then(|field| field.numeric)
                            .filter(|ms| ms.is_finite() && *ms >= 0.0 && *ms <= 86_400_000.0)
                    };
                    let (min, max) = match (ms("MinLifetime"), ms("MaxLifetime")) {
                        (Some(min), Some(max)) => (min, max),
                        (Some(one), None) | (None, Some(one)) => (one, one),
                        (None, None) => return None,
                    };
                    let frames = |ms: f32| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let whole = ms as u32;
                        LogicFrames::from_milliseconds(whole)
                    };
                    let death = DeathNaming::of(
                        module
                            .field("DeathType")
                            .and_then(|field| field.args.first())
                            .and_then(|name| death_types.intern(name)),
                    );
                    Some((frames(min), frames(max), death))
                });
            out.push(range);
        }
        Self(out)
    }

    pub(crate) fn get(&self, kind: Kind) -> Option<(LogicFrames, LogicFrames, DeathNaming)> {
        self.0.get(usize::from(kind.0)).copied().flatten()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeathOption {
    pub(crate) option: SlowDeathOption,
    pub(crate) fx: PhaseLists,
    pub(crate) ocl: PhaseLists,
    pub(crate) weapon: PhaseLists,
    pub(crate) sound: PhaseLists,
    pub(crate) death_flags: Vec<String>,
    pub(crate) mux: DieMuxData,
}

impl DeathOption {
    pub(crate) fn has_presentation(&self, phase: DeathPhase) -> bool {
        has(&self.fx, phase) || has(&self.sound, phase)
    }

    pub(crate) fn has_lists(&self, phase: DeathPhase) -> bool {
        self.has_presentation(phase) || has(&self.ocl, phase) || has(&self.weapon, phase)
    }
}

fn has(lists: &PhaseLists, phase: DeathPhase) -> bool {
    lists
        .get(phase.index())
        .is_some_and(|list| !list.is_empty())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DeathPlans {
    plans: Vec<Vec<DeathOption>>,
    destroy: Vec<Vec<DieMuxData>>,
    pub(crate) report: (usize, usize, usize, usize),
    destroyed_outright: usize,
}

impl DeathPlans {
    pub(crate) fn build(kinds: &KindTable) -> Self {
        let mut out = Self::default();
        let bits = UpgradeBits::default();
        let normal = Death {
            kind: crate::rules::death::NORMAL_DEATH,
            ..Death::NONE
        };
        for index in 0..kinds.len() {
            let mut plan = Vec::new();
            let mut destroy = Vec::new();
            if let Some(entry) = u16::try_from(index).ok().and_then(|k| kinds.get(Kind(k))) {
                for module in entry
                    .record
                    .declared_modules
                    .iter()
                    .filter(|m| m.name.eq_ignore_ascii_case("DestroyDie"))
                {
                    match DieMuxData::from_fields(&module.source(&bits)) {
                        Ok(mux) => destroy.push(mux),
                        Err(_) => out.report.3 = out.report.3.saturating_add(1),
                    }
                }
                if destroy.iter().any(|mux| mux.admits(&normal)) {
                    out.destroyed_outright = out.destroyed_outright.saturating_add(1);
                }
                for module in entry
                    .record
                    .declared_modules
                    .iter()
                    .filter(|m| m.name.eq_ignore_ascii_case("SlowDeathBehavior"))
                {
                    out.report.0 = out.report.0.saturating_add(1);
                    let source = module.source(&bits);
                    let Ok(mux) = DieMuxData::from_fields(&source) else {
                        out.report.3 = out.report.3.saturating_add(1);
                        continue;
                    };
                    if mux.admits(&normal) {
                        out.report.1 = out.report.1.saturating_add(1);
                    }
                    let (Ok(sink_rate), Ok(sink_delay), Ok(sink_delay_variance)) = (
                        mint_sage_modules::sink_rate_from_fields(&source),
                        mint_sage_modules::sink_delay_from_fields(&source, "SinkDelay"),
                        mint_sage_modules::sink_delay_from_fields(&source, "SinkDelayVariance"),
                    ) else {
                        out.report.3 = out.report.3.saturating_add(1);
                        continue;
                    };
                    let int = |keyword: &str, default: i32| {
                        module
                            .field(keyword)
                            .and_then(|f| f.numeric)
                            .filter(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                            .map_or(default, |v| {
                                #[allow(clippy::cast_possible_truncation)]
                                let whole = v as i32;
                                whole
                            })
                    };
                    let delay_ms = int("DestructionDelay", 0).max(0);
                    let fade_delay = module.field("FadeDelay").map(|_| {
                        LogicFrames::from_milliseconds(
                            u32::try_from(int("FadeDelay", 0).max(0)).unwrap_or(0),
                        )
                    });
                    let mut option = DeathOption {
                        option: SlowDeathOption {
                            probability_modifier: int("ProbabilityModifier", 10),
                            modifier_bonus_per_overkill_percent: int(
                                "ModifierBonusPerOverkillPercent",
                                0,
                            ),
                            destruction_delay: LogicFrames::from_milliseconds(
                                u32::try_from(delay_ms).unwrap_or(0),
                            ),
                            sink_rate,
                            sink_delay,
                            sink_delay_variance,
                            fade_delay,
                        },
                        fx: PhaseLists::default(),
                        ocl: PhaseLists::default(),
                        weapon: PhaseLists::default(),
                        sound: PhaseLists::default(),
                        death_flags: module
                            .field("DeathFlags")
                            .map(|f| f.args.clone())
                            .unwrap_or_default(),
                        mux,
                    };
                    for field in &module.fields {
                        let keyword = field.keyword.to_ascii_uppercase();
                        let lists = match keyword.as_str() {
                            "FX" => &mut option.fx,
                            "OCL" => &mut option.ocl,
                            "WEAPON" => &mut option.weapon,
                            "SOUND" => &mut option.sound,
                            _ => continue,
                        };
                        let Some((phase, names)) = field.args.split_first() else {
                            continue;
                        };
                        if let Some(list) =
                            DeathPhase::parse(phase).and_then(|phase| lists.get_mut(phase.index()))
                        {
                            list.extend(names.iter().cloned());
                        }
                    }
                    plan.push(option);
                }
            }
            if !plan.is_empty() {
                out.report.2 = out.report.2.saturating_add(1);
            }
            out.plans.push(plan);
            out.destroy.push(destroy);
        }
        out
    }

    pub(crate) fn destroys(&self, kind: Kind, death: &Death<'_>) -> bool {
        self.destroy
            .get(usize::from(kind.0))
            .is_some_and(|muxes| muxes.iter().any(|mux| mux.admits(death)))
    }

    pub(crate) fn get(&self, kind: Kind) -> Option<&[DeathOption]> {
        self.plans
            .get(usize::from(kind.0))
            .map(Vec::as_slice)
            .filter(|plan| !plan.is_empty())
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "deaths: {} SlowDeathBehavior declaration(s) over the match's kinds, {} apply to a \
             NORMAL death, {} kind(s) roll their own, {} a DestroyDie removes at once, {} \
             refused for an unreadable filter",
            self.report.0, self.report.1, self.report.2, self.destroyed_outright, self.report.3
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub(crate) offset: Vec2,
    pub(crate) facing_degrees: Option<f32>,
}

pub(crate) fn placements(
    entry: &mint_sage_ini::CreationEntry,
    count: u32,
    caller_facing_degrees: f32,
    logic_hz: f32,
) -> Vec<Placement> {
    let has = |name: &str| {
        entry
            .disposition
            .iter()
            .any(|token| token.eq_ignore_ascii_case(name))
    };
    let base = Vec2::new(entry.offset[0], entry.offset[1]);
    let arc = entry
        .disposition_angle
        .filter(|a| a.is_finite())
        .unwrap_or(0.0);
    let radius = entry
        .disposition_intensity
        .filter(|r| r.is_finite() && *r >= 0.0 && *r <= 10_000.0)
        .unwrap_or(0.0)
        / logic_hz.max(1.0);
    let like = has("LIKE_EXISTING");
    let absolute = !like && has("ABSOLUTE_ANGLE");
    let relative = !like && !absolute && has("RELATIVE_ANGLE") && caller_facing_degrees.is_finite();
    let around = !like && !absolute && !relative && has("SPAWN_AROUND");
    let facing_degrees = if absolute {
        Some(arc)
    } else if relative {
        Some(caller_facing_degrees + arc)
    } else {
        None
    };
    (0..count)
        .map(|at| {
            if around {
                #[allow(clippy::cast_precision_loss)]
                let degrees = arc * at as f32;
                let radians = degrees.to_radians();
                Placement {
                    offset: Vec2::new(
                        base.x + radius * mint_math::f32::cos(radians),
                        base.y + radius * mint_math::f32::sin(radians),
                    ),
                    facing_degrees: Some(degrees),
                }
            } else {
                Placement {
                    offset: base,
                    facing_degrees,
                }
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Throw {
    Flying {
        intensity: f32,
    },
    Up {
        intensity: f32,
    },
    RandomForce {
        magnitude: [f32; 2],
        pitch_radians: [f32; 2],
    },
}

impl Throw {
    pub(crate) fn of(entry: &mint_sage_ini::CreationEntry, logic_hz: f32) -> Option<Self> {
        let has = |name: &str| {
            entry
                .disposition
                .iter()
                .any(|token| token.eq_ignore_ascii_case(name))
        };
        let sane = |value: Option<f32>| {
            value
                .filter(|v| v.is_finite() && v.abs() <= 10_000.0)
                .unwrap_or(0.0)
        };
        let intensity = sane(entry.disposition_intensity) / logic_hz;
        if has("SEND_IT_FLYING") {
            Some(Self::Flying { intensity })
        } else if has("SEND_IT_UP") {
            Some(Self::Up { intensity })
        } else if has("RANDOM_FORCE") {
            let [low, high] = entry.force_magnitude;
            let [min_pitch, max_pitch] = entry.force_pitch_degrees;
            Some(Self::RandomForce {
                magnitude: [sane(low), sane(high)],
                pitch_radians: [sane(min_pitch).to_radians(), sane(max_pitch).to_radians()],
            })
        } else {
            None
        }
    }

    pub(crate) fn velocity(self, mut unit: impl FnMut() -> f32) -> [f32; 3] {
        let mut between = |low: f32, high: f32| low + (high - low) * unit();
        match self {
            Self::Flying { intensity } | Self::Up { intensity } => {
                let (out, up, floor) = match self {
                    Self::Flying { .. } => (4.0 * intensity, 3.0 * intensity, 0.33),
                    _ => (2.0 * intensity, 4.0 * intensity, 0.75),
                };
                let x = between(-out, out);
                let y = between(-out, out);
                [x, y, between(up * floor, up)]
            }
            Self::RandomForce {
                magnitude,
                pitch_radians,
            } => {
                let bearing = between(0.0, core::f32::consts::TAU);
                let pitch = between(pitch_radians[0], pitch_radians[1]);
                let force = between(magnitude[0], magnitude[1]);
                let level = force * mint_math::f32::cos(pitch);
                [
                    level * mint_math::f32::cos(bearing),
                    level * mint_math::f32::sin(bearing),
                    force * mint_math::f32::sin(pitch),
                ]
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_lifetime_reads_the_share_of_its_span_still_to_run() {
        let timed = |set_at: Option<u64>| Lifetime {
            dies_at: Frame::new(400),
            death: DeathNaming::Normal,
            set_at: set_at.map(Frame::new),
        };
        let share =
            |lifetime: Lifetime, now: u64| remaining_share(lifetime.span()?, Frame::new(now));
        let left = |now: u64| share(timed(Some(100)), now);
        assert_eq!(left(100), Some(1.0));
        assert_eq!(left(175), Some(0.75));
        assert_eq!(left(400), Some(0.0));
        assert_eq!(left(900), Some(0.0), "past its death");
        assert_eq!(left(40), Some(1.0), "before its span");
        assert_eq!(share(timed(None), 175), None, "no span");
        assert_eq!(share(timed(Some(400)), 175), None, "empty");
        assert_eq!(share(timed(Some(500)), 175), None, "inverted");
    }

    #[test]
    fn a_lifetime_is_its_range_drawn_and_never_zero() {
        let ms = LogicFrames::from_milliseconds;
        let never = |_: u32| -> u32 { panic!("a single value draws nothing") };
        assert_eq!(
            lifetime_delay(ms(0), ms(0), never).get(),
            1,
            "the egg: one frame"
        );
        assert_eq!(lifetime_delay(ms(180_000), ms(180_000), never), ms(180_000));
        let drawn = lifetime_delay(
            LogicFrames::from_frames(10),
            LogicFrames::from_frames(20),
            |span| {
                assert_eq!(span, 10);
                4
            },
        );
        assert_eq!(drawn.get(), 14);
        let clamped = lifetime_delay(
            LogicFrames::from_frames(10),
            LogicFrames::from_frames(20),
            |_| 99,
        );
        assert_eq!(clamped.get(), 20, "a draw past the span is clamped");
    }

    fn entry(
        disposition: &str,
        angle: Option<f32>,
        intensity: Option<f32>,
    ) -> mint_sage_ini::CreationEntry {
        mint_sage_ini::CreationEntry {
            names: vec![String::from("X")],
            offset: [1.0, 2.0, 0.0],
            count: Some(1),
            disposition: tokens(disposition),
            disposition_angle: angle,
            disposition_intensity: intensity,
            ..mint_sage_ini::CreationEntry::default()
        }
    }

    #[test]
    fn spawn_around_rings_the_objects_at_the_intensity_per_frame() {
        let spots = placements(&entry("SPAWN_AROUND", Some(60.0), Some(500.0)), 6, 0.0, 5.0);
        assert_eq!(spots.len(), 6);
        for spot in &spots {
            let (dx, dy) = (spot.offset.x - 1.0, spot.offset.y - 2.0);
            assert!(((dx * dx + dy * dy).sqrt() - 100.0).abs() < 0.01);
        }
        assert!(
            (spots[0].offset.x - 101.0).abs() < 0.01,
            "the first at angle 0"
        );
        assert!(spots[1].offset.y > 2.0, "the next counter-clockwise");
        assert_eq!(spots[0].facing_degrees, Some(0.0));
        assert_eq!(spots[2].facing_degrees, Some(120.0));
    }

    #[test]
    fn an_earlier_disposition_bit_wins_over_spawn_around() {
        let absolute = placements(
            &entry("SPAWN_AROUND ABSOLUTE_ANGLE", Some(90.0), Some(500.0)),
            2,
            0.0,
            5.0,
        );
        assert!(absolute
            .iter()
            .all(|p| p.offset == Vec2::new(1.0, 2.0) && p.facing_degrees == Some(90.0)));
        let like = placements(
            &entry("SPAWN_AROUND LIKE_EXISTING", Some(90.0), Some(500.0)),
            2,
            0.0,
            5.0,
        );
        assert!(like
            .iter()
            .all(|p| p.offset == Vec2::new(1.0, 2.0) && p.facing_degrees.is_none()));
    }

    #[test]
    fn absolute_angle_faces_the_object_at_its_offset_and_the_rest_stand_at_it() {
        let spots = placements(&entry("ABSOLUTE_ANGLE", Some(300.0), None), 1, 45.0, 5.0);
        assert_eq!(spots[0].offset, Vec2::new(1.0, 2.0));
        assert_eq!(spots[0].facing_degrees, Some(300.0));
        let like = placements(
            &entry("LIKE_EXISTING", Some(300.0), Some(50.0)),
            2,
            0.0,
            5.0,
        );
        assert!(like
            .iter()
            .all(|p| p.offset == Vec2::new(1.0, 2.0) && p.facing_degrees.is_none()));
        let hostile = placements(
            &entry("SPAWN_AROUND", Some(f32::NAN), Some(f32::INFINITY)),
            2,
            0.0,
            5.0,
        );
        assert!(hostile
            .iter()
            .all(|p| p.offset.x.is_finite() && p.offset.y.is_finite()));
    }

    #[test]
    fn a_flying_disposition_throws_at_eas_force_and_the_rest_do_not() {
        let at = |disposition: &str, draw: f32| {
            Throw::of(&entry(disposition, None, Some(30.0)), 5.0)
                .map(|throw| throw.velocity(|| draw))
        };
        let cases = [
            ("SEND_IT_FLYING LIKE_EXISTING", 0.0, [-24.0, -24.0, 5.94]),
            ("LIKE_EXISTING SEND_IT_FLYING", 1.0, [24.0, 24.0, 18.0]),
            ("SEND_IT_UP", 0.0, [-12.0, -12.0, 18.0]),
            ("SEND_IT_UP RANDOM_FORCE", 1.0, [12.0, 12.0, 24.0]),
        ];
        for (disposition, draw, want) in cases {
            let got = at(disposition, draw).unwrap();
            for axis in 0..3 {
                assert!(
                    (got[axis] - want[axis]).abs() < 1e-4,
                    "{disposition} at {draw}: {got:?}"
                );
            }
        }
        for placed in [
            "LIKE_EXISTING",
            "SPAWN_AROUND",
            "ABSOLUTE_ANGLE ON_GROUND_ALIGNED",
        ] {
            assert_eq!(at(placed, 0.5), None, "{placed}");
        }
    }

    #[test]
    fn a_random_force_stays_within_its_magnitude_and_pitch() {
        let mut nugget = entry("RANDOM_FORCE", None, None);
        nugget.force_magnitude = [Some(8.0), Some(16.0)];
        nugget.force_pitch_degrees = [Some(60.0), Some(70.0)];
        let throw = Throw::of(&nugget, 5.0).unwrap();
        let mut rng = mint_math::rng::Pcg32::new(7);
        let mut bearings = (f32::MAX, f32::MIN);
        for _ in 0..500 {
            let [x, y, z] = throw.velocity(|| rng.next_f32());
            let level = (x * x + y * y).sqrt();
            let force = (level * level + z * z).sqrt();
            assert!((8.0 - 1e-3..=16.0 + 1e-3).contains(&force), "{force}");
            let pitch = mint_math::f32::atan2(z, level).to_degrees();
            assert!((60.0 - 1e-2..=70.0 + 1e-2).contains(&pitch), "{pitch}");
            let bearing = mint_math::f32::atan2(y, x);
            bearings = (bearings.0.min(bearing), bearings.1.max(bearing));
        }
        assert!(
            bearings.0 < -3.0 && bearings.1 > 3.0,
            "every bearing: {bearings:?}"
        );
    }

    #[test]
    fn relative_angle_adds_the_arc_to_the_callers_facing() {
        let spots = placements(&entry("RELATIVE_ANGLE", Some(315.0), None), 1, 45.0, 5.0);
        assert_eq!(spots[0].facing_degrees, Some(360.0));
        let no_caller_facing = placements(&entry("RELATIVE_ANGLE", Some(10.0), None), 1, 0.0, 5.0);
        assert_eq!(no_caller_facing[0].facing_degrees, Some(10.0));
    }
}
