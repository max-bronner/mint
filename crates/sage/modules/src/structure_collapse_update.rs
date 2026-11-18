use mint_sage_ini::value::parse_bool;
use mint_sim::module::{
    Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot, Sleep, UpdateContext,
};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, Pcg32, StateHasher};

use crate::effects::{CreateLocation, Effect, EffectLog, Name, PowerPlace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollapsePhase {
    Initial,
    Delay,
    Burst,
    AlmostFinal,
    Final,
}

impl CollapsePhase {
    const NAMES: [(&'static str, Self); 5] = [
        ("INITIAL", Self::Initial),
        ("DELAY", Self::Delay),
        ("BURST", Self::Burst),
        ("ALMOST_FINAL", Self::AlmostFinal),
        ("FINAL", Self::Final),
    ];

    fn from_token(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, phase)| *phase)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseState {
    Standing,
    Waiting,
    Collapsing,
    Done,
}

#[derive(Debug, Clone, PartialEq)]
struct PhaseList {
    phase: CollapsePhase,
    is_ocl: bool,
    names: Vec<Name>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureCollapseUpdate {
    min_collapse_delay: LogicFrames,
    max_collapse_delay: LogicFrames,
    min_burst_delay: LogicFrames,
    max_burst_delay: LogicFrames,
    collapse_damping: f32,
    big_burst_frequency: u32,
    collapse_height: f32,
    max_shudder: f32,
    destroy_when_done: bool,
    lists: Vec<PhaseList>,
    state: CollapseState,
    collapse_frame: Frame,
    burst_frame: Frame,
    gravity: f32,
    depth: f32,
    velocity: f32,
    height: f32,
}

impl StructureCollapseUpdate {
    #[must_use]
    pub const fn new(min_collapse_delay: LogicFrames, max_collapse_delay: LogicFrames) -> Self {
        Self {
            min_collapse_delay,
            max_collapse_delay,
            min_burst_delay: LogicFrames::ZERO,
            max_burst_delay: LogicFrames::ZERO,
            collapse_damping: 0.0,
            big_burst_frequency: 0,
            collapse_height: 0.0,
            max_shudder: 0.0,
            destroy_when_done: false,
            lists: Vec::new(),
            state: CollapseState::Standing,
            collapse_frame: Frame::FOREVER,
            burst_frame: Frame::FOREVER,
            gravity: 0.0,
            depth: 0.0,
            velocity: 0.0,
            height: 0.0,
        }
    }

    pub fn set_physics(&mut self, gravity_per_frame_squared: f32, geometry_height: f32) {
        self.gravity = gravity_per_frame_squared;
        self.depth = if geometry_height < self.collapse_height {
            self.collapse_height
        } else {
            geometry_height
        };
    }

    #[must_use]
    pub const fn state(&self) -> CollapseState {
        self.state
    }

    #[must_use]
    pub const fn is_collapsing(&self) -> bool {
        !matches!(self.state, CollapseState::Standing)
    }

    #[must_use]
    pub fn sink(&self) -> f32 {
        match self.state {
            CollapseState::Collapsing | CollapseState::Done => -self.height,
            CollapseState::Standing | CollapseState::Waiting => 0.0,
        }
    }

    #[must_use]
    pub fn max_shudder(&self) -> f32 {
        match self.state {
            CollapseState::Waiting | CollapseState::Collapsing => self.max_shudder,
            CollapseState::Standing | CollapseState::Done => 0.0,
        }
    }

    fn do_phase(
        &self,
        phase: CollapsePhase,
        entity: mint_sim::Entity,
        ctx_rng: &mut Pcg32,
        log: Option<&mut EffectLog>,
    ) {
        let mut log = log;
        for list in self.lists.iter().filter(|list| list.phase == phase) {
            let span = u32::try_from(list.names.len()).unwrap_or(u32::MAX);
            if span == 0 {
                continue;
            }
            let pick = usize::try_from(ctx_rng.below(span)).unwrap_or(0);
            let (Some(name), Some(log)) = (list.names.get(pick), log.as_deref_mut()) else {
                continue;
            };
            let effect = if list.is_ocl {
                Effect::CreateObjects {
                    list: name.clone(),
                    location: CreateLocation::AtLocation,
                    at: PowerPlace::Source,
                    from: crate::effects::OclFrom::Location,
                }
            } else {
                Effect::PlayFx {
                    list: name.clone(),
                    at: PowerPlace::Source,
                    on_object: false,
                    secondary: None,
                }
            };
            log.push(entity, effect);
        }
    }
}

impl Component for StructureCollapseUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.min_collapse_delay.get());
        hasher.write_u32(self.max_collapse_delay.get());
        hasher.write_u8(self.state as u8);
        self.collapse_frame.hash_into(hasher);
        self.burst_frame.hash_into(hasher);
        hasher.write_f32(self.gravity);
        hasher.write_f32(self.depth);
        hasher.write_f32(self.velocity);
        hasher.write_f32(self.height);
    }
}

impl ModuleKind for StructureCollapseUpdate {
    const CAPABILITIES: CapabilityMask =
        CapabilityMask::single(Capability::Update).union(CapabilityMask::single(Capability::Die));

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if self.state == CollapseState::Waiting {
            if ctx.now < self.collapse_frame {
                return self
                    .collapse_frame
                    .since(ctx.now)
                    .map_or(Sleep::NextFrame, Sleep::Frames);
            }
            self.state = CollapseState::Collapsing;
            self.do_phase(
                CollapsePhase::Burst,
                ctx.entity,
                ctx.rng,
                ctx.effects.get::<EffectLog>(),
            );
            let delay = draw_delay(ctx.rng, self.min_burst_delay, self.max_burst_delay);
            self.burst_frame = ctx.now.checked_add(delay).unwrap_or(Frame::FOREVER);
        }
        if self.state != CollapseState::Collapsing {
            return Sleep::Forever;
        }
        self.height -= self.velocity;
        self.velocity -= self.gravity * (1.0 - self.collapse_damping);
        if ctx.now >= self.burst_frame {
            let big = ctx.rng.below(self.big_burst_frequency.max(1)) == 0;
            let phase = if big {
                CollapsePhase::Burst
            } else {
                CollapsePhase::Delay
            };
            self.do_phase(phase, ctx.entity, ctx.rng, ctx.effects.get::<EffectLog>());
            let delay = draw_delay(ctx.rng, self.min_burst_delay, self.max_burst_delay);
            self.burst_frame = self
                .burst_frame
                .checked_add(delay)
                .unwrap_or(Frame::FOREVER);
        }
        if self.height + self.depth <= 0.0 {
            self.state = CollapseState::Done;
            self.do_phase(
                CollapsePhase::Final,
                ctx.entity,
                ctx.rng,
                ctx.effects.get::<EffectLog>(),
            );
            if self.destroy_when_done {
                ctx.destroy.request(ctx.entity);
            }
            return Sleep::Forever;
        }
        if self.height + 0.7 * self.depth <= 0.0 {
            self.do_phase(
                CollapsePhase::AlmostFinal,
                ctx.entity,
                ctx.rng,
                ctx.effects.get::<EffectLog>(),
            );
        }
        Sleep::NextFrame
    }

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        for slot in instances {
            if slot.state.is_collapsing() {
                continue;
            }
            let delay = draw_delay(
                ctx.rng,
                slot.state.min_collapse_delay,
                slot.state.max_collapse_delay,
            );
            slot.state.collapse_frame = ctx.now.checked_add(delay).unwrap_or(Frame::FOREVER);
            slot.state.do_phase(
                CollapsePhase::Initial,
                ctx.entity,
                ctx.rng,
                ctx.effects.get::<EffectLog>(),
            );
            slot.state.state = CollapseState::Waiting;
            slot.state.height = 0.0;
            slot.state.velocity = 0.0;
            slot.wake = slot.state.collapse_frame;
        }
    }
}

fn draw_delay(rng: &mut Pcg32, min: LogicFrames, max: LogicFrames) -> LogicFrames {
    let (low, high) = if min.get() <= max.get() {
        (min.get(), max.get())
    } else {
        (max.get(), min.get())
    };
    let span = high.saturating_sub(low).saturating_add(1);
    LogicFrames::from_frames(low.saturating_add(rng.below(span)))
}

pub fn structure_collapse_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StructureCollapseUpdate, FieldError> {
    let mut module = StructureCollapseUpdate::new(
        optional_duration_ms(fields, "MinCollapseDelay")?,
        optional_duration_ms(fields, "MaxCollapseDelay")?,
    );
    module.min_burst_delay = optional_duration_ms(fields, "MinBurstDelay")?;
    module.max_burst_delay = optional_duration_ms(fields, "MaxBurstDelay")?;
    module.collapse_damping = optional_numeric(fields, "CollapseDamping")?.unwrap_or(0.0);
    module.collapse_height = optional_numeric(fields, "CollapseHeight")?.unwrap_or(0.0);
    if module.collapse_height < 0.0 {
        return Err(FieldError::new("CollapseHeight is negative"));
    }
    module.max_shudder = optional_numeric(fields, "MaxShudder")?.unwrap_or(0.0);
    if module.max_shudder < 0.0 {
        return Err(FieldError::new("MaxShudder is negative"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        module.big_burst_frequency = optional_numeric(fields, "BigBurstFrequency")?
            .filter(|value| *value >= 0.0)
            .map_or(0, |value| value as u32);
    }
    module.destroy_when_done = match fields.field("DestroyObjectWhenDone") {
        None => false,
        Some(raw) => parse_bool(raw).ok_or_else(|| {
            FieldError::new(format!("DestroyObjectWhenDone '{raw}' is not Yes/No"))
        })?,
    };
    for (keyword, is_ocl) in [("FXList", false), ("OCL", true)] {
        for args in fields.field_args_each(keyword) {
            let Some((phase, names)) = args.split_first() else {
                return Err(FieldError::new(format!("{keyword} names no phase")));
            };
            let phase = CollapsePhase::from_token(phase)
                .ok_or_else(|| FieldError::new(format!("{keyword} phase '{phase}' is unknown")))?;
            module.lists.push(PhaseList {
                phase,
                is_ocl,
                names: names.iter().map(|name| Name::from(*name)).collect(),
            });
        }
    }
    Ok(module)
}

fn optional_numeric(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => fields.field_numeric(keyword).map(Some).ok_or_else(|| {
            FieldError::new(format!("{keyword} '{raw}' is not a number/known macro"))
        }),
    }
}

fn optional_duration_ms(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<LogicFrames, FieldError> {
    let Some(ms) = optional_numeric(fields, keyword)? else {
        return Ok(LogicFrames::ZERO);
    };
    if ms < 0.0 {
        return Err(FieldError::new(format!("{keyword} is negative")));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(crate::special_ability_update::retail_duration_ticks(
        ms as u32,
    ))
}

#[cfg(test)]
mod tests;
