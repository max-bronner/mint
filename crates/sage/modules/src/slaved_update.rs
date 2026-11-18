use core::f32::consts::TAU;
use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{names, Effect, EffectLog};
use crate::fire_weapon_update::milliseconds;
use crate::world_reads::{MasterView, SlaveView, Slaves};

const SLAVED_UPDATE_RATE: LogicFrames = LogicFrames::from_frames(7);

const CLOSE_ENOUGH: f32 = 15.0;

const STRAY_MULTIPLIER: f32 = 1.0;

const MAX_RANGE: f32 = 100_000.0;

const UNUSED_ZERO_HOUR_FIELDS: [&str; 14] = [
    "AttackWanderRange",
    "ScoutRange",
    "ScoutWanderRange",
    "RepairRange",
    "RepairMinAltitude",
    "RepairMaxAltitude",
    "DistToTargetToGrantRangeBonus",
    "RepairRatePerSecond",
    "RepairWhenBelowHealth%",
    "RepairMinReadyTime",
    "RepairMaxReadyTime",
    "RepairMinWeldTime",
    "RepairMaxWeldTime",
    "StayOnSameLayerAsMaster",
];

#[derive(Debug, Clone, PartialEq)]
struct SlavedData {
    guard_max_range: f32,
    guard_wander_range: f32,
    attack_range: f32,
    leash_range: f32,
    guard_offset: (f32, f32, f32),
    die_on_masters_death: bool,
    fade_out_range: f32,
    fade_time: LogicFrames,
    mark_unselectable: Option<bool>,
    eva_through_slaver: Option<bool>,
    unused: Vec<(&'static str, String)>,
}

impl SlavedData {
    fn guards(&self) -> bool {
        self.guard_max_range > 0.0 || self.guard_wander_range > 0.0 || self.leash_range > 0.0
    }

    fn stray_limit(&self) -> f32 {
        if self.leash_range > 0.0 {
            self.leash_range
        } else {
            STRAY_MULTIPLIER * self.guard_max_range
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlavedUpdate {
    data: Arc<SlavedData>,
    released: bool,
    marked: bool,
    leashed_returning: bool,
}

impl SlavedUpdate {
    #[must_use]
    pub fn guard_max_range(&self) -> f32 {
        self.data.guard_max_range
    }

    #[must_use]
    pub fn guard_wander_range(&self) -> f32 {
        self.data.guard_wander_range
    }

    #[must_use]
    pub fn leash_range(&self) -> f32 {
        self.data.leash_range
    }

    #[must_use]
    pub fn attack_range(&self) -> f32 {
        self.data.attack_range
    }

    #[must_use]
    pub fn guard_offset(&self) -> (f32, f32, f32) {
        self.data.guard_offset
    }

    #[must_use]
    pub fn dies_with_master(&self) -> bool {
        self.data.die_on_masters_death
    }

    #[must_use]
    pub fn parsed_only(&self) -> Vec<(&'static str, String)> {
        let d = &self.data;
        let yes_no = |value: bool| String::from(if value { "Yes" } else { "No" });
        let mut out = Vec::new();
        if d.fade_out_range > 0.0 {
            out.push(("FadeOutRange", format!("{}", d.fade_out_range)));
        }
        if !d.fade_time.is_zero() {
            out.push(("FadeTime", format!("{} frames", d.fade_time.get())));
        }
        if let Some(value) = d.eva_through_slaver {
            out.push(("UseSlaverAsControlForEvaObjectSightedEvents", yes_no(value)));
        }
        out.extend(d.unused.iter().cloned());
        out
    }

    fn pinned(&self, master: &MasterView) -> (f32, f32) {
        let (ox, oy, _) = self.data.guard_offset;
        let (sin, cos) = mint_math::f32::sin_cos(master.facing);
        (
            master.at.0 + cos * ox - sin * oy,
            master.at.1 + sin * ox + cos * oy,
        )
    }

    fn wander_point(&self, pinned: (f32, f32), ctx: &mut UpdateContext<'_>) -> (f32, f32) {
        let range = self.data.guard_wander_range;
        if range <= 0.0 {
            return pinned;
        }
        let (sin, cos) = mint_math::f32::sin_cos(ctx.rng.next_f32() * TAU);
        (pinned.0 + cos * range, pinned.1 + sin * range)
    }

    fn attack(
        &self,
        me: &SlaveView,
        master: &MasterView,
        victim: (mint_sim::Entity, (f32, f32)),
    ) -> Effect {
        let (_, at) = victim;
        let range = self.data.attack_range;
        let (sx, sy) = (at.0 - me.at.0, at.1 - me.at.1);
        if sx * sx + sy * sy <= range * range {
            return Effect::AiMoveTo { at, acquires: true };
        }
        let (mx, my) = (at.0 - master.at.0, at.1 - master.at.1);
        let distance = (mx * mx + my * my).sqrt();
        let at = if distance > 0.0 {
            let scale = range / distance;
            (master.at.0 + mx * scale, master.at.1 + my * scale)
        } else {
            master.at
        };
        Effect::AiMoveTo { at, acquires: true }
    }

    fn guard(&mut self, me: &SlaveView, master: &MasterView, ctx: &mut UpdateContext<'_>) {
        let pinned = self.pinned(master);
        let away_from_pin = distance_squared(me.at, pinned) > CLOSE_ENOUGH * CLOSE_ENOUGH;
        let limit = self.data.stray_limit();
        let strayed = limit > 0.0 && distance_squared(me.at, master.at) > limit * limit;
        let returning = me.ordered_goal.is_some_and(|goal| {
            let home = self.data.guard_wander_range + CLOSE_ENOUGH;
            distance_squared(goal, pinned) <= home * home
        });
        if returning {
            return;
        }
        if me.idle && self.leashed_returning {
            self.leashed_returning = false;
            for effect in leashed_returning_effects(false) {
                push(ctx, effect);
            }
        }
        let acquires = if me.idle {
            if !away_from_pin && !strayed {
                return;
            }
            true
        } else {
            if !strayed {
                return;
            }
            if self.data.leash_range > 0.0 && !self.leashed_returning {
                self.leashed_returning = true;
                for effect in leashed_returning_effects(true) {
                    push(ctx, effect);
                }
            }
            false
        };
        let at = self.wander_point(pinned, ctx);
        push(ctx, Effect::AiMoveTo { at, acquires });
    }
}

fn leashed_returning_effects(on: bool) -> [Effect; 2] {
    let one = names(vec!["LEASHED_RETURNING".to_owned()]);
    let none = names(Vec::new());
    if on {
        [
            Effect::StatusBits {
                set: one.clone(),
                clear: none.clone(),
            },
            Effect::ModelConditions {
                add: one,
                remove: none,
            },
        ]
    } else {
        [
            Effect::StatusBits {
                set: none.clone(),
                clear: one.clone(),
            },
            Effect::ModelConditions {
                add: none,
                remove: one,
            },
        ]
    }
}

fn distance_squared(a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dy * dy
}

fn push(ctx: &mut UpdateContext<'_>, effect: Effect) {
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(ctx.entity, effect);
    }
}

impl Component for SlavedUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.released);
        hasher.write_bool(self.marked);
        hasher.write_bool(self.leashed_returning);
    }
}

impl ModuleKind for SlavedUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn on_attach(&mut self, _now: Frame) {
        self.released = false;
        self.marked = false;
        self.leashed_returning = false;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let Some(me) = ctx
            .reads
            .get::<Slaves>()
            .and_then(|slaves| slaves.get(ctx.entity))
            .copied()
        else {
            return Sleep::Frames(SLAVED_UPDATE_RATE);
        };
        let Some(master) = me.master else {
            if !self.released {
                if self.data.die_on_masters_death {
                    push(ctx, Effect::Die);
                } else if self.marked {
                    push(
                        ctx,
                        Effect::StatusBits {
                            set: names(Vec::new()),
                            clear: names(vec!["UNSELECTABLE".to_owned()]),
                        },
                    );
                }
                self.released = true;
                self.marked = false;
            }
            return Sleep::Frames(SLAVED_UPDATE_RATE);
        };
        if self.released {
            self.released = false;
            self.leashed_returning = false;
        }
        if !self.marked && self.data.mark_unselectable == Some(true) {
            self.marked = true;
            push(
                ctx,
                Effect::StatusBits {
                    set: names(vec!["UNSELECTABLE".to_owned()]),
                    clear: names(Vec::new()),
                },
            );
        }
        if !me.allied {
            push(ctx, Effect::JoinMaster { to: master.owner });
            self.released = true;
            return Sleep::Frames(SLAVED_UPDATE_RATE);
        }
        if self.data.attack_range > 0.0 {
            if let Some(victim) = master.victim {
                let effect = self.attack(&me, &master, victim);
                push(ctx, effect);
                return Sleep::Frames(SLAVED_UPDATE_RATE);
            }
        }
        if self.data.guards() {
            self.guard(&me, &master, ctx);
        }
        Sleep::Frames(SLAVED_UPDATE_RATE)
    }
}

fn range(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0.0);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| (0.0..=MAX_RANGE).contains(value))
        .ok_or_else(|| {
            FieldError::new(format!(
                "{keyword} '{raw}' is not a number from 0 to {MAX_RANGE}"
            ))
        })
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    fields
        .field(keyword)
        .map(|raw| {
            mint_sage_ini::value::parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No")))
        })
        .transpose()
}

fn offset(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<(f32, f32, f32), FieldError> {
    let args = fields.field_args(keyword);
    if args.is_empty() {
        return Ok((0.0, 0.0, 0.0));
    }
    let bad = || {
        FieldError::new(format!(
            "{keyword} '{}' is not X:.. Y:.. Z:..",
            args.join(" ")
        ))
    };
    let mut found = [None; 3];
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        let slot = match tag.to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => return Err(bad()),
        };
        let raw = if value.is_empty() {
            tokens.next().copied().ok_or_else(bad)?
        } else {
            value
        };
        let number = raw
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= MAX_RANGE)
            .ok_or_else(bad)?;
        match found.get_mut(slot) {
            Some(cell @ None) => *cell = Some(number),
            _ => return Err(bad()),
        }
    }
    let [x, y, z] = found;
    Ok((x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)))
}

pub fn slaved_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SlavedUpdate, FieldError> {
    range(fields, "FadeTime")?;
    let unused = UNUSED_ZERO_HOUR_FIELDS
        .iter()
        .filter_map(|keyword| fields.field(keyword).map(|raw| (*keyword, raw.to_owned())))
        .collect();
    let data = SlavedData {
        guard_max_range: range(fields, "GuardMaxRange")?,
        guard_wander_range: range(fields, "GuardWanderRange")?,
        attack_range: range(fields, "AttackRange")?,
        leash_range: range(fields, "LeashRange")?,
        guard_offset: offset(fields, "GuardPositionOffset")?,
        die_on_masters_death: boolean(fields, "DieOnMastersDeath")?.unwrap_or(false),
        fade_out_range: range(fields, "FadeOutRange")?,
        fade_time: milliseconds(fields, "FadeTime"),
        mark_unselectable: boolean(fields, "MarkUnselectable")?,
        eva_through_slaver: boolean(fields, "UseSlaverAsControlForEvaObjectSightedEvents")?,
        unused,
    };
    Ok(SlavedUpdate {
        data: Arc::new(data),
        released: false,
        marked: false,
        leashed_returning: false,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct TestFields(BTreeMap<&'static str, &'static str>);

    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    fn with_attack_range(range: f32) -> SlavedUpdate {
        let fields = TestFields(BTreeMap::from([("GuardMaxRange", "0")]));
        let mut update = slaved_update_from_fields(&fields, Frame::ZERO).unwrap();
        Arc::make_mut(&mut update.data).attack_range = range;
        update
    }

    fn entity() -> mint_sim::Entity {
        mint_sim::World::new().spawn().unwrap()
    }

    #[test]
    fn slave_attack_is_always_a_move_never_an_order() {
        let slaved = with_attack_range(50.0);
        let me = SlaveView {
            at: (0.0, 0.0),
            idle: false,
            ordered_goal: None,
            master: None,
            allied: true,
        };
        let victim = entity();
        let master = MasterView {
            at: (100.0, 0.0),
            facing: 0.0,
            victim: Some((victim, (30.0, 0.0))),
            owner: mint_sim::PlayerId::new(0),
        };
        assert_eq!(
            slaved.attack(&me, &master, (victim, (30.0, 0.0))),
            Effect::AiMoveTo {
                at: (30.0, 0.0),
                acquires: true,
            },
            "the victim is within AttackRange of the slave: moves straight to it"
        );
        let far = MasterView {
            victim: Some((victim, (120.0, 0.0))),
            ..master
        };
        assert_eq!(
            far.victim
                .map(|(_, at)| slaved.attack(&me, &far, (victim, at))),
            Some(Effect::AiMoveTo {
                at: (150.0, 0.0),
                acquires: true,
            }),
            "too far from the slave: clips to AttackRange from the master, toward the victim"
        );
    }
}
