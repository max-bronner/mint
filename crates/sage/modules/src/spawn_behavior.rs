use std::sync::Arc;

use mint_sim::module::{
    Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot, Sleep, UpdateContext,
    UpgradeMask,
};
use mint_sim::{
    Component, Entity, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher,
    UpgradeContext,
};

use crate::effects::{Effect, EffectLog, Name, Names};
use crate::fire_weapon_update::milliseconds;
use crate::upgrade_mux::{sooner, UpgradeMuxData};
use crate::world_reads::{NeutralControlled, UnderConstruction};

const SPAWN_UPDATE_RATE: LogicFrames = LogicFrames::from_frames(15);

pub const MAX_SPAWN_NUMBER: u16 = 1024;

#[derive(Debug, PartialEq)]
struct SpawnData {
    number: u32,
    initial_burst: u32,
    replace_delay: LogicFrames,
    templates: Names,
    one_shot: bool,
    can_reclaim_orphans: bool,
    aggregate_health: bool,
    exit_by_budding: bool,
    spawned_require_spawner: bool,
    propagate_damage_types: Names,
    respect_command_limit: bool,
    fade_in_time: LogicFrames,
    spawn_inside_building: bool,
    kill_spawns_on_condition: bool,
    share_upgrades: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnBehavior {
    data: Arc<SpawnData>,
    mux: UpgradeMuxData,
    active: bool,
    started: bool,
    cursor: u32,
    one_shot_left: u32,
    created: u32,
    due: Vec<Frame>,
    spawns: Vec<Entity>,
}

impl SpawnBehavior {
    #[must_use]
    pub fn spawns(&self) -> &[Entity] {
        &self.spawns
    }

    #[must_use]
    pub fn spawn_number(&self) -> u32 {
        self.data.number
    }

    #[must_use]
    pub fn replace_delay(&self) -> LogicFrames {
        self.data.replace_delay
    }

    pub fn templates(&self) -> impl Iterator<Item = &str> {
        self.data.templates.iter().map(|name| &**name)
    }

    #[must_use]
    pub fn can_reclaim_orphans(&self) -> bool {
        self.data.can_reclaim_orphans
    }

    #[must_use]
    pub fn pending(&self) -> &[Frame] {
        &self.due
    }

    #[must_use]
    pub fn parsed_only(&self) -> Vec<(&'static str, String)> {
        let d = &self.data;
        let mut out = Vec::new();
        let mut flag = |name: &'static str, value: bool| {
            if value {
                out.push((name, String::from("Yes")));
            }
        };
        flag("AggregateHealth", d.aggregate_health);
        flag("ExitByBudding", d.exit_by_budding);
        flag("RespectCommandLimit", d.respect_command_limit);
        flag("SpawnInsideBuilding", d.spawn_inside_building);
        flag(
            "KillSpawnsBasedOnModelConditionState",
            d.kill_spawns_on_condition,
        );
        if !d.fade_in_time.is_zero() {
            out.push(("FadeInTime", format!("{} frames", d.fade_in_time.get())));
        }
        if !d.propagate_damage_types.is_empty() {
            out.push((
                "PropagateDamageTypesToSlavesWhenExisting",
                d.propagate_damage_types.join(" "),
            ));
        }
        out
    }

    fn should_try(&self, ctx: &UpdateContext<'_>) -> bool {
        self.active
            && !ctx
                .reads
                .get::<NeutralControlled>()
                .is_some_and(|neutral| neutral.contains(ctx.entity))
            && !ctx
                .reads
                .get::<UnderConstruction>()
                .is_some_and(|building| building.contains(ctx.entity))
    }

    fn create(&mut self, ctx: &mut UpdateContext<'_>) {
        let names = &self.data.templates;
        let Some(name) = names.get(usize::try_from(self.cursor).unwrap_or(usize::MAX)) else {
            return;
        };
        let name = Name::clone(name);
        let count = u32::try_from(names.len()).unwrap_or(u32::MAX);
        self.cursor = self.cursor.saturating_add(1);
        if self.cursor >= count {
            self.cursor = 0;
        }
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::Spawn {
                    name,
                    requester: ctx.requester,
                    ordinal: self.created,
                },
            );
        }
        self.created = self.created.saturating_add(1);
        if self.data.one_shot {
            self.one_shot_left = self.one_shot_left.saturating_sub(1);
        }
    }
}

impl Component for SpawnBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_bool(self.active);
        hasher.write_bool(self.started);
        hasher.write_u32(self.cursor);
        hasher.write_u32(self.one_shot_left);
        hasher.write_u32(self.created);
        hasher.write_usize(self.due.len());
        for frame in &self.due {
            frame.hash_into(hasher);
        }
        hasher.write_usize(self.spawns.len());
        for spawn in &self.spawns {
            spawn.hash_into(hasher);
        }
    }
}

impl ModuleKind for SpawnBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Die))
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.active = self.mux.triggered_by().is_empty();
        self.started = false;
        self.cursor = 0;
        self.one_shot_left = if self.data.one_shot {
            self.data.number
        } else {
            0
        };
        self.created = 0;
        self.due.clear();
        self.spawns.clear();
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if self.mux.fire(active, ctx) {
            self.active = true;
        }
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let anim = self.mux.update(ctx);
        sooner(anim, self.own_update(ctx))
    }

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        for slot in instances {
            if slot.state.data.spawned_require_spawner {
                if let Some(log) = ctx.effects.get::<EffectLog>() {
                    for spawn in &slot.state.spawns {
                        log.push(*spawn, Effect::Die);
                    }
                }
            }
            slot.state.spawns.clear();
        }
    }

    fn on_created(&mut self, product: Entity, _now: Frame) {
        self.spawns.push(product);
    }

    fn on_product_died(&mut self, product: Entity, now: Frame) {
        let Some(at) = self.spawns.iter().position(|spawn| *spawn == product) else {
            return;
        };
        self.spawns.remove(at);
        self.due.push(
            now.checked_add(self.data.replace_delay)
                .unwrap_or(Frame::FOREVER),
        );
    }
}

impl SpawnBehavior {
    fn own_update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if self.data.share_upgrades {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(ctx.entity, Effect::ShareUpgradesWithSlaves);
            }
        }
        if !self.started {
            self.started = true;
            let burst = self.data.initial_burst > 0;
            self.due.extend(
                (0..self.data.number).map(|index| Frame::new(if burst { 0 } else { index.into() })),
            );
        }
        if self.should_try(ctx) {
            let now = ctx.now;
            let queued = core::mem::take(&mut self.due);
            let mut kept = Vec::with_capacity(queued.len());
            for at in queued {
                if now > at {
                    self.create(ctx);
                } else {
                    kept.push(at);
                }
            }
            self.due = kept;
            if self.data.one_shot && self.one_shot_left == 0 {
                self.active = false;
            }
        }
        Sleep::Frames(SPAWN_UPDATE_RATE)
    }
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn count(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<u32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0);
    };
    let value = fields
        .field_numeric(keyword)
        .filter(|value| value.is_finite() && value.fract() == 0.0)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a whole number")))?;
    if value > f32::from(MAX_SPAWN_NUMBER) {
        return Err(FieldError::new(format!(
            "{keyword} {value} is above {MAX_SPAWN_NUMBER}"
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value.max(0.0) as u32)
}

pub fn spawn_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpawnBehavior, FieldError> {
    let templates: Vec<String> = fields
        .field_args_each("SpawnTemplateName")
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .collect();
    if templates.is_empty() {
        return Err(FieldError::new("SpawnBehavior names no SpawnTemplateName"));
    }
    let data = SpawnData {
        number: count(fields, "SpawnNumber")?,
        initial_burst: count(fields, "InitialBurst")?,
        replace_delay: milliseconds(fields, "SpawnReplaceDelay"),
        templates: crate::effects::names(templates),
        one_shot: boolean(fields, "OneShot")?,
        can_reclaim_orphans: boolean(fields, "CanReclaimOrphans")?,
        aggregate_health: boolean(fields, "AggregateHealth")?,
        exit_by_budding: boolean(fields, "ExitByBudding")?,
        spawned_require_spawner: boolean(fields, "SpawnedRequireSpawner")?,
        propagate_damage_types: crate::effects::names(
            fields
                .field_args("PropagateDamageTypesToSlavesWhenExisting")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        respect_command_limit: boolean(fields, "RespectCommandLimit")?,
        fade_in_time: milliseconds(fields, "FadeInTime"),
        spawn_inside_building: boolean(fields, "SpawnInsideBuilding")?,
        kill_spawns_on_condition: boolean(fields, "KillSpawnsBasedOnModelConditionState")?,
        share_upgrades: boolean(fields, "ShareUpgrades")?,
    };
    Ok(SpawnBehavior {
        data: Arc::new(data),
        mux: UpgradeMuxData::from_fields(fields)?,
        active: true,
        started: false,
        cursor: 0,
        one_shot_left: 0,
        created: 0,
        due: Vec::new(),
        spawns: Vec::new(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlavedTo {
    pub master: Entity,
    pub destroy_when_sold: bool,
}

impl Component for SlavedTo {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.master.hash_into(hasher);
        hasher.write_bool(self.destroy_when_sold);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchedSlave(pub Entity);

impl Component for WatchedSlave {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[must_use]
pub fn watches_slaves(record: &crate::ObjectRecord) -> bool {
    record
        .declared_modules
        .iter()
        .any(|module| module.name == "SlaveWatcherBehavior")
}

#[must_use]
pub fn is_slaved(record: &crate::ObjectRecord) -> bool {
    record
        .declared_modules
        .iter()
        .any(|module| module.name == "SlavedUpdate")
}

#[cfg(test)]
mod tests;
