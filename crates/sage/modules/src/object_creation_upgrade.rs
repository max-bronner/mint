use std::sync::Arc;

use mint_sim::module::{
    Capability, CapabilityMask, Sleep, UpdateContext, UpgradeContext, UpgradeMask,
};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, ModuleKind, StateHasher,
};

use crate::effects::{CreateLocation, Effect, EffectLog, Name, PowerPlace};
use crate::fire_weapon_update::milliseconds;
use crate::upgrade_mux::{sooner, UpgradeMuxData};

const MAX_OFFSET: f32 = 100_000.0;

const POLL_RATE: LogicFrames = LogicFrames::from_frames(7);

#[derive(Debug, PartialEq)]
struct Data {
    delay: LogicFrames,
    offset: (f32, f32, f32),
    thing_to_spawn: Option<Name>,
    fade_in_time: LogicFrames,
    remove_upgrade: UpgradeMask,
    grant_upgrade: Option<Name>,
    destroy_when_sold: bool,
    death_anim_and_duration: Vec<String>,
    use_building_production: bool,
    upgrade_object: Option<Name>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectCreationUpgrade {
    data: Arc<Data>,
    mux: UpgradeMuxData,
    due: Option<Frame>,
}

impl ObjectCreationUpgrade {
    #[must_use]
    pub fn thing_to_spawn(&self) -> Option<&str> {
        self.data.thing_to_spawn.as_deref()
    }

    #[must_use]
    pub fn destroy_when_sold(&self) -> bool {
        self.data.destroy_when_sold
    }

    #[must_use]
    pub fn delay(&self) -> LogicFrames {
        self.data.delay
    }

    #[must_use]
    pub fn parsed_only(&self) -> Vec<(&'static str, String)> {
        let d = &self.data;
        let mut out = Vec::new();
        if !d.fade_in_time.is_zero() {
            out.push(("FadeInTime", format!("{} frames", d.fade_in_time.get())));
        }
        if !d.death_anim_and_duration.is_empty() {
            out.push(("DeathAnimAndDuration", d.death_anim_and_duration.join(" ")));
        }
        out
    }

    fn fire_effects(&self, requester: mint_sim::Requester, out: &mut Vec<Effect>) {
        let d = &self.data;
        if let Some(name) = &d.thing_to_spawn {
            if d.use_building_production {
                out.push(Effect::Spawn {
                    name: Name::clone(name),
                    requester,
                    ordinal: 0,
                });
            } else {
                out.push(Effect::CreateSlave {
                    name: Name::clone(name),
                    offset: d.offset,
                    destroy_when_sold: d.destroy_when_sold,
                });
            }
        }
        if let Some(name) = &d.upgrade_object {
            out.push(Effect::CreateObjects {
                list: Name::clone(name),
                location: CreateLocation::UseOwnerObject,
                at: PowerPlace::Source,
                from: crate::effects::OclFrom::Location,
            });
        }
        if let Some(name) = &d.grant_upgrade {
            out.push(Effect::GrantUpgrade(Name::clone(name)));
        }
        let removes = d.remove_upgrade.union(self.mux.removes_upgrades());
        if !removes.is_empty() {
            out.push(Effect::RemoveUpgrades(removes, false));
        }
    }
}

impl Component for ObjectCreationUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_bool(self.due.is_some());
        if let Some(due) = self.due {
            due.hash_into(hasher);
        }
    }
}

impl ModuleKind for ObjectCreationUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn on_attach(&mut self, _now: Frame) {
        self.due = None;
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if self.mux.fire(active, ctx) {
            self.due = Some(
                ctx.now
                    .checked_add(self.data.delay)
                    .unwrap_or(Frame::FOREVER),
            );
        }
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let anim = self.mux.update(ctx);
        sooner(anim, self.own_update(ctx))
    }
}

impl ObjectCreationUpgrade {
    fn own_update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let Some(due) = self.due else {
            return Sleep::Forever;
        };
        if ctx.now < due {
            return Sleep::Frames(due.since(ctx.now).unwrap_or(POLL_RATE));
        }
        let mut out = Vec::new();
        self.fire_effects(ctx.requester, &mut out);
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            for effect in out {
                log.push(ctx.entity, effect);
            }
        }
        self.due = None;
        Sleep::Forever
    }
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
            .filter(|v| v.is_finite() && v.abs() <= MAX_OFFSET)
            .ok_or_else(bad)?;
        match found.get_mut(slot) {
            Some(cell @ None) => *cell = Some(number),
            _ => return Err(bad()),
        }
    }
    let [x, y, z] = found;
    Ok((x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)))
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

pub fn object_creation_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ObjectCreationUpgrade, FieldError> {
    let data = Data {
        delay: milliseconds(fields, "Delay"),
        offset: offset(fields, "Offset")?,
        thing_to_spawn: fields.field("ThingToSpawn").map(Name::from),
        fade_in_time: milliseconds(fields, "FadeInTime"),
        remove_upgrade: fields.field_upgrade_mask("RemoveUpgrade"),
        grant_upgrade: fields.field("GrantUpgrade").map(Name::from),
        destroy_when_sold: boolean(fields, "DestroyWhenSold")?,
        death_anim_and_duration: fields
            .field_args("DeathAnimAndDuration")
            .into_iter()
            .map(str::to_owned)
            .collect(),
        use_building_production: boolean(fields, "UseBuildingProduction")?,
        upgrade_object: fields.field("UpgradeObject").map(Name::from),
    };
    Ok(ObjectCreationUpgrade {
        data: Arc::new(data),
        mux: UpgradeMuxData::from_fields(fields)?,
        due: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, UpgradeBit};
    use mint_sim::{Effects, Pcg32, World};
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(n) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(n));
                }
            }
            mask
        }
    }

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    fn fire(module: ObjectCreationUpgrade, frames: u64) -> Vec<(u64, Vec<Effect>)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ObjectCreationUpgrade>(&mut world, "ObjectCreationUpgrade");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut trigger = UpgradeMask::EMPTY;
        trigger.set(UpgradeBit::new(1));
        let mut rng = Pcg32::new(1);
        let mut out = Vec::new();
        for frame in 0..frames {
            let now = Frame::new(frame);
            if frame == 0 {
                registry.broadcast_upgrade(&mut world, entity, &trigger, now);
            }
            let mut log = EffectLog::new();
            registry.update_into(
                &mut world,
                now,
                &mut rng,
                &mut DestroyRequests::new(),
                Effects::into_sink(&mut log),
            );
            let effects: Vec<Effect> = log.drain().into_iter().map(|(_, e)| e).collect();
            if !effects.is_empty() {
                out.push((frame, effects));
            }
        }
        out
    }

    #[test]
    fn a_zero_delay_trigger_fires_the_same_frame() {
        let module = object_creation_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", &["1"]),
                ("ThingToSpawn", &["TestSpawn"]),
                ("GrantUpgrade", &["Upgrade_Test"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let reports = fire(module, 3);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, 0);
        assert_eq!(reports[0].1.len(), 2);
        assert!(matches!(reports[0].1[0], Effect::CreateSlave { .. }));
        assert!(matches!(reports[0].1[1], Effect::GrantUpgrade(_)));
    }

    #[test]
    fn a_delayed_trigger_waits_the_full_delay_before_firing() {
        let module = object_creation_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", &["1"]),
                ("ThingToSpawn", &["TestSpawn"]),
                ("Delay", &["1000"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let reports = fire(module, 35);
        assert_eq!(reports.len(), 1, "fires exactly once");
        assert_eq!(reports[0].0, 30, "1000 ms is 30 frames at 30 Hz");
    }

    #[test]
    fn an_offset_z_is_carried_into_the_create_slave_effect() {
        let module = object_creation_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", &["1"]),
                ("ThingToSpawn", &["TestSpawn"]),
                ("Offset", &["X:12.0", "Y:0.0", "Z:51.0"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let reports = fire(module, 3);
        assert_eq!(reports.len(), 1);
        let Effect::CreateSlave { offset, .. } = &reports[0].1[0] else {
            panic!("expected a CreateSlave effect");
        };
        assert_eq!(*offset, (12.0, 0.0, 51.0));
    }

    #[test]
    fn no_trigger_never_fires() {
        let module = object_creation_upgrade_from_fields(
            &fields(&[("ThingToSpawn", &["TestSpawn"])]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(fire(module, 5).is_empty());
    }
}
