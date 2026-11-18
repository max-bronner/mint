use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Capability, CapabilityMask, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, ModuleKind, StateHasher,
    UpgradeContext,
};

use crate::effects::{Effect, EffectLog};
use crate::object_filter::ObjectFilter;
use crate::upgrade_mux::UpgradeMuxData;

const MAX_INDEX: u32 = 4096;

const MAX_ENTRIES: usize = 256;
const MAX_VIA: usize = 64;

const MAX_COORD: f32 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WayPointType {
    PreClimb,
    Climb,
    Walk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WayPoint {
    pub index: u32,
    pub kind: WayPointType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub from: u32,
    pub via: Vec<u32>,
    pub to: u32,
}

#[derive(Debug, PartialEq)]
struct Data {
    generate_now: bool,
    allow_enemies: bool,
    filter: ObjectFilter,
    bone_prefix: Option<String>,
    number_of_bones: u32,
    waypoints: Vec<WayPoint>,
    links: Vec<Link>,
    activation_delay: LogicFrames,
    above_wall: Option<u32>,
    top_attack_pos: Option<(f32, f32, f32)>,
    top_attack_radius: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicPortalBehaviour {
    data: Arc<Data>,
    mux: UpgradeMuxData,
    activated_at: Option<Frame>,
}

impl DynamicPortalBehaviour {
    #[must_use]
    pub fn allow_enemies(&self) -> bool {
        self.data.allow_enemies
    }

    #[must_use]
    pub fn filter(&self) -> &ObjectFilter {
        &self.data.filter
    }

    #[must_use]
    pub fn bone_prefix(&self) -> Option<&str> {
        self.data.bone_prefix.as_deref()
    }

    #[must_use]
    pub fn number_of_bones(&self) -> u32 {
        self.data.number_of_bones
    }

    #[must_use]
    pub fn waypoints(&self) -> &[WayPoint] {
        &self.data.waypoints
    }

    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.data.links
    }

    #[must_use]
    pub fn above_wall(&self) -> Option<u32> {
        self.data.above_wall
    }

    #[must_use]
    pub fn top_attack_pos(&self) -> Option<(f32, f32, f32)> {
        self.data.top_attack_pos
    }

    #[must_use]
    pub fn top_attack_radius(&self) -> Option<f32> {
        self.data.top_attack_radius
    }

    #[must_use]
    pub fn is_active(&self, now: Frame) -> bool {
        if self.data.generate_now {
            return true;
        }
        self.activated_at.is_some_and(|at| {
            at.checked_add(self.data.activation_delay)
                .is_none_or(|ready| now >= ready)
        })
    }
}

impl Component for DynamicPortalBehaviour {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_bool(self.activated_at.is_some());
        if let Some(at) = self.activated_at {
            at.hash_into(hasher);
        }
    }
}

impl ModuleKind for DynamicPortalBehaviour {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if !self.mux.fire(active, ctx) {
            return;
        }
        self.activated_at = Some(ctx.now);
        let removes = self.mux.removes_upgrades();
        if !removes.is_empty() {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(ctx.entity, Effect::RemoveUpgrades(removes.clone(), false));
            }
        }
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        self.mux.update(ctx)
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        let was_fired = self.mux.has_fired();
        self.mux.reset_if_triggered_by(removed);
        if was_fired && !self.mux.has_fired() {
            self.activated_at = None;
        }
    }
}

pub fn dynamic_portal_behaviour_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DynamicPortalBehaviour, FieldError> {
    Ok(DynamicPortalBehaviour {
        data: Arc::new(Data {
            generate_now: flag(fields, "GenerateNow")?,
            allow_enemies: flag(fields, "AllowEnemies")?,
            filter: ObjectFilter::parse(&owned(fields.field_args("ObjectFilter"))),
            bone_prefix: fields.field("BonePrefix").map(str::to_owned),
            number_of_bones: bounded_count(fields, "NumberOfBones")?,
            waypoints: way_points(fields)?,
            links: links(fields)?,
            activation_delay: seconds(fields, "ActivationDelaySeconds")?,
            above_wall: bounded_index(fields, "AboveWall")?,
            top_attack_pos: xyz(fields, "TopAttackPos")?,
            top_attack_radius: non_negative(fields, "TopAttackRadius")?,
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
        activated_at: None,
    })
}

fn owned(args: Vec<&str>) -> Vec<String> {
    args.into_iter().map(str::to_owned).collect()
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn bounded_count(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<u32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0);
    };
    #[allow(clippy::cast_precision_loss)]
    let max = MAX_INDEX as f32;
    let value = fields
        .field_numeric(keyword)
        .filter(|v| v.is_finite() && v.fract() == 0.0 && (0.0..=max).contains(v))
        .ok_or_else(|| {
            FieldError::new(format!(
                "{keyword} '{raw}' is not a whole number in 0..={MAX_INDEX}"
            ))
        })?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u32)
}

fn bounded_index(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<u32>, FieldError> {
    if fields.field(keyword).is_none() {
        return Ok(None);
    }
    bounded_count(fields, keyword).map(Some)
}

fn seconds(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<LogicFrames, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(LogicFrames::ZERO);
    };
    let value = fields
        .field_numeric(keyword)
        .filter(|v| v.is_finite())
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number of seconds")))?;
    Ok(LogicFrames::from_seconds(value))
}

fn non_negative(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_numeric(keyword)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(Some)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number")))
}

fn way_points(fields: &dyn ModuleFieldSource) -> Result<Vec<WayPoint>, FieldError> {
    let occurrences = fields.field_args_each("WayPoint");
    if occurrences.len() > MAX_ENTRIES {
        return Err(FieldError::new(format!(
            "WayPoint declares more than {MAX_ENTRIES} entries"
        )));
    }
    occurrences.iter().map(|args| way_point(args)).collect()
}

fn way_point(args: &[&str]) -> Result<WayPoint, FieldError> {
    let bad = || {
        FieldError::new(format!(
            "WayPoint '{}' is not Index:.. Type:..",
            args.join(" ")
        ))
    };
    let mut index = None;
    let mut kind = None;
    for token in args {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        match tag.to_ascii_uppercase().as_str() {
            "INDEX" if index.is_none() => {
                index = Some(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| *v <= MAX_INDEX)
                        .ok_or_else(bad)?,
                );
            }
            "TYPE" if kind.is_none() => {
                kind = Some(match value.to_ascii_uppercase().as_str() {
                    "PRECLIMB" => WayPointType::PreClimb,
                    "CLIMB" => WayPointType::Climb,
                    "WALK" => WayPointType::Walk,
                    _ => return Err(bad()),
                });
            }
            _ => return Err(bad()),
        }
    }
    Ok(WayPoint {
        index: index.ok_or_else(bad)?,
        kind: kind.ok_or_else(bad)?,
    })
}

fn links(fields: &dyn ModuleFieldSource) -> Result<Vec<Link>, FieldError> {
    let occurrences = fields.field_args_each("Link");
    if occurrences.len() > MAX_ENTRIES {
        return Err(FieldError::new(format!(
            "Link declares more than {MAX_ENTRIES} entries"
        )));
    }
    occurrences.iter().map(|args| link(args)).collect()
}

fn link(args: &[&str]) -> Result<Link, FieldError> {
    let bad = || {
        FieldError::new(format!(
            "Link '{}' is not From:.. Via:.. To:..",
            args.join(" ")
        ))
    };
    let mut from = None;
    let mut to = None;
    let mut via = Vec::new();
    for token in args {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        let n = |value: &str| value.parse::<u32>().ok().filter(|v| *v <= MAX_INDEX);
        match tag.to_ascii_uppercase().as_str() {
            "FROM" if from.is_none() => from = Some(n(value).ok_or_else(bad)?),
            "TO" if to.is_none() => to = Some(n(value).ok_or_else(bad)?),
            "VIA" if via.len() < MAX_VIA => via.push(n(value).ok_or_else(bad)?),
            _ => return Err(bad()),
        }
    }
    Ok(Link {
        from: from.ok_or_else(bad)?,
        via,
        to: to.ok_or_else(bad)?,
    })
}

fn xyz(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<(f32, f32, f32)>, FieldError> {
    let args = fields.field_args(keyword);
    if args.is_empty() {
        return Ok(None);
    }
    let bad = || {
        FieldError::new(format!(
            "{keyword} '{}' is not X:.. Y:.. Z:..",
            args.join(" ")
        ))
    };
    let mut found: [Option<f32>; 3] = [None; 3];
    for token in &args {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        let slot = match tag.to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => return Err(bad()),
        };
        let number = value
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= MAX_COORD)
            .ok_or_else(bad)?;
        match found.get_mut(slot) {
            Some(cell @ None) => *cell = Some(number),
            _ => return Err(bad()),
        }
    }
    let [x, y, z] = found;
    Ok(Some((x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0))))
}

#[cfg(test)]
mod tests;
