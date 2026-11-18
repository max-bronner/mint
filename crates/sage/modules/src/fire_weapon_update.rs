use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};

pub const MAX_FIRE_NUGGETS: usize = 64;

const MAX_MILLISECONDS: f32 = 86_400_000.0;

pub(crate) fn milliseconds(fields: &dyn ModuleFieldSource, keyword: &str) -> LogicFrames {
    match fields.field_numeric(keyword) {
        Some(ms) if ms.is_finite() && ms > 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(MAX_MILLISECONDS) as u32;
            LogicFrames::from_milliseconds(whole)
        }
        _ => LogicFrames::ZERO,
    }
}

pub(crate) fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<bool> {
    fields
        .field(keyword)
        .and_then(mint_sage_ini::value::parse_bool)
}

fn tagged(args: &[&str], tag: &str) -> f32 {
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let Some((found, value)) = token.split_once(':') else {
            continue;
        };
        if !found.eq_ignore_ascii_case(tag) {
            continue;
        }
        let raw = if value.is_empty() {
            tokens.next().copied().unwrap_or("")
        } else {
            value
        };
        return raw
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= 10_000.0)
            .unwrap_or(0.0);
    }
    0.0
}

#[derive(Debug, Clone, PartialEq)]
struct FireNugget {
    weapon: Name,
    delay: LogicFrames,
    one_shot: bool,
    offset: (f32, f32),
    gated: bool,
}

#[derive(Debug, PartialEq)]
struct FireData {
    nuggets: Vec<FireNugget>,
    alive_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FireWeaponUpdate {
    data: Arc<FireData>,
    born: Frame,
    fired: u64,
}

impl FireWeaponUpdate {
    #[must_use]
    pub fn nugget_count(&self) -> usize {
        self.data.nuggets.len()
    }

    pub fn weapons(&self) -> impl Iterator<Item = &str> {
        self.data.nuggets.iter().map(|nugget| &*nugget.weapon)
    }
}

impl Component for FireWeaponUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.born.hash_into(hasher);
        hasher.write_u64(self.fired);
    }
}

impl ModuleKind for FireWeaponUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn on_attach(&mut self, now: Frame) {
        self.born = now;
        self.fired = 0;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let now = ctx.now;
        let entity = ctx.entity;
        let mut next: Option<Frame> = None;
        let mut repeating = false;
        for (index, nugget) in self.data.nuggets.iter().enumerate() {
            if nugget.gated {
                continue;
            }
            let due = self
                .born
                .checked_add(nugget.delay)
                .unwrap_or(Frame::FOREVER);
            if now < due {
                next = Some(next.map_or(due, |held: Frame| held.min(due)));
                continue;
            }
            let bit = 1u64.checked_shl(u32::try_from(index).unwrap_or(u32::MAX));
            let repeat = if nugget.one_shot {
                let Some(bit) = bit else { continue };
                if self.fired & bit != 0 {
                    continue;
                }
                self.fired |= bit;
                None
            } else {
                repeating = true;
                u8::try_from(index).ok()
            };
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    entity,
                    Effect::FireWeapon {
                        weapon: Name::clone(&nugget.weapon),
                        at: PowerPlace::Source,
                        offset: nugget.offset,
                        repeat,
                        alive_only: self.data.alive_only,
                        reload_slot: None,
                    },
                );
            }
        }
        if repeating {
            return Sleep::NextFrame;
        }
        match next.and_then(|due| due.since(now)) {
            Some(wait) => Sleep::Frames(wait),
            None => Sleep::Forever,
        }
    }
}

pub fn fire_weapon_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FireWeaponUpdate, FieldError> {
    let mut nuggets = Vec::new();
    if let Some(weapon) = fields.field("Weapon") {
        nuggets.push(FireNugget {
            weapon: Name::from(weapon),
            delay: milliseconds(fields, "InitialDelay"),
            one_shot: false,
            offset: (0.0, 0.0),
            gated: false,
        });
    }
    fields.nested("FireWeaponNugget", &mut |nugget| {
        if nuggets.len() >= MAX_FIRE_NUGGETS {
            return;
        }
        let Some(weapon) = nugget.field("WeaponName") else {
            return;
        };
        let offset = {
            let args = nugget.field_args("Offset");
            (tagged(&args, "X"), tagged(&args, "Y"))
        };
        let gated = nugget.field("ChargingModeTrigger").is_some()
            || nugget.field("HeroModeTrigger").is_some();
        nuggets.push(FireNugget {
            weapon: Name::from(weapon),
            delay: milliseconds(nugget, "FireDelay"),
            one_shot: flag(nugget, "OneShot").unwrap_or(false),
            offset,
            gated,
        });
    });
    if nuggets.is_empty() {
        return Err(FieldError::new("FireWeaponUpdate names no weapon"));
    }
    Ok(FireWeaponUpdate {
        data: Arc::new(FireData {
            nuggets,
            alive_only: flag(fields, "AliveOnly").unwrap_or(false),
        }),
        born: Frame::ZERO,
        fired: 0,
    })
}

#[derive(Debug, PartialEq)]
struct DeletionData {
    min: LogicFrames,
    max: LogicFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeletionUpdate {
    data: Arc<DeletionData>,
    destroy_at: Frame,
}

impl DeletionUpdate {
    #[must_use]
    pub fn destroy_at(&self) -> Option<Frame> {
        (self.destroy_at != Frame::FOREVER).then_some(self.destroy_at)
    }
}

impl Component for DeletionUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.destroy_at.hash_into(hasher);
    }
}

impl ModuleKind for DeletionUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn on_attach(&mut self, _now: Frame) {
        self.destroy_at = Frame::FOREVER;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if self.destroy_at == Frame::FOREVER {
            let (low, high) = if self.data.min.get() <= self.data.max.get() {
                (self.data.min.get(), self.data.max.get())
            } else {
                (self.data.max.get(), self.data.min.get())
            };
            let span = high.saturating_sub(low);
            let extra = if span == 0 {
                0
            } else {
                ctx.rng.below(span.saturating_add(1)).min(span)
            };
            let delay = LogicFrames::from_frames(low.saturating_add(extra).max(1));
            self.destroy_at = ctx.now.checked_add(delay).unwrap_or(Frame::FOREVER);
            return Sleep::Frames(delay);
        }
        if ctx.now >= self.destroy_at {
            ctx.destroy.request(ctx.entity);
            return Sleep::Forever;
        }
        match self.destroy_at.since(ctx.now) {
            Some(wait) => Sleep::Frames(wait),
            None => Sleep::Forever,
        }
    }
}

fn unsigned_milliseconds(fields: &dyn ModuleFieldSource, keyword: &str) -> LogicFrames {
    match fields.field_numeric(keyword) {
        Some(ms) if ms.is_finite() && ms < 0.0 && ms >= -(u32::MAX as f32) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wrapped = (ms as i64) as u32;
            LogicFrames::from_milliseconds(wrapped)
        }
        _ => milliseconds(fields, keyword),
    }
}

pub fn deletion_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DeletionUpdate, FieldError> {
    let (min, max) = match (
        fields
            .field("MinLifetime")
            .map(|_| unsigned_milliseconds(fields, "MinLifetime")),
        fields
            .field("MaxLifetime")
            .map(|_| unsigned_milliseconds(fields, "MaxLifetime")),
    ) {
        (Some(min), Some(max)) => (min, max),
        (Some(one), None) | (None, Some(one)) => (one, one),
        (None, None) => {
            return Err(FieldError::new(
                "DeletionUpdate writes neither MinLifetime nor MaxLifetime",
            ))
        }
    };
    Ok(DeletionUpdate {
        data: Arc::new(DeletionData { min, max }),
        destroy_at: Frame::FOREVER,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry};
    use mint_sim::{Effects, World};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fields {
        fields: BTreeMap<&'static str, &'static str>,
        nuggets: Vec<Fields>,
    }

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.fields
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|v| v.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
            if keyword.eq_ignore_ascii_case("FireWeaponNugget") {
                for nugget in &self.nuggets {
                    visit(nugget);
                }
            }
        }
    }

    fn nugget(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields {
            fields: pairs.iter().copied().collect(),
            nuggets: Vec::new(),
        }
    }

    fn run<M: ModuleKind + Clone>(
        module: M,
        name: &'static str,
        frames: u64,
    ) -> (Vec<(u64, Effect)>, Vec<(u64, mint_sim::Entity)>) {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<M>(&mut world, name);
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut rng = mint_sim::Pcg32::new(7);
        let mut out = Vec::new();
        let mut gone = Vec::new();
        for frame in 0..frames {
            let mut log = EffectLog::new();
            let mut destroy = DestroyRequests::new();
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
            );
            out.extend(log.drain().into_iter().map(|(_, e)| (frame, e)));
            gone.extend(destroy.drain_sorted().into_iter().map(|e| (frame, e)));
        }
        (out, gone)
    }

    #[test]
    fn one_shot_nuggets_fire_once_each_at_their_delay() {
        let fields = Fields {
            fields: BTreeMap::new(),
            nuggets: vec![
                nugget(&[
                    ("WeaponName", "RainOfFirePhaseInitialWeapon"),
                    ("FireDelay", "0"),
                    ("OneShot", "Yes"),
                ]),
                nugget(&[
                    ("WeaponName", "RainOfFirePhaseOneWeapon"),
                    ("FireDelay", "2500"),
                    ("OneShot", "Yes"),
                ]),
            ],
        };
        let module = fire_weapon_update_from_fields(&fields, Frame::ZERO).unwrap();
        let (fired, _) = run(module, "FireWeaponUpdate", 200);
        let weapons: Vec<(u64, &str)> = fired
            .iter()
            .map(|(frame, effect)| match effect {
                Effect::FireWeapon { weapon, repeat, .. } => {
                    assert!(repeat.is_none());
                    (*frame, &**weapon)
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            weapons,
            vec![
                (0, "RainOfFirePhaseInitialWeapon"),
                (75, "RainOfFirePhaseOneWeapon")
            ],
            "2500 ms is 75 frames"
        );
    }

    #[test]
    fn a_repeating_nugget_is_reported_every_frame_after_its_delay() {
        let fields = Fields {
            fields: BTreeMap::new(),
            nuggets: vec![nugget(&[
                ("WeaponName", "SpikeMoatRadiusWeapon"),
                ("FireDelay", "100"),
                ("OneShot", "No"),
                ("Offset", "X:-25 Y:0 Z:0"),
            ])],
        };
        let module = fire_weapon_update_from_fields(&fields, Frame::ZERO).unwrap();
        let (fired, _) = run(module, "FireWeaponUpdate", 10);
        let frames: Vec<u64> = fired.iter().map(|(frame, _)| *frame).collect();
        assert_eq!(frames, vec![3, 4, 5, 6, 7, 8, 9], "100 ms is 3 frames");
        let Effect::FireWeapon { repeat, offset, .. } = &fired[0].1 else {
            panic!();
        };
        assert_eq!(*repeat, Some(0));
        assert_eq!(*offset, (-25.0, 0.0));
    }

    #[test]
    fn a_mode_gated_nugget_never_fires_and_a_weaponless_module_is_refused() {
        let fields = Fields {
            fields: BTreeMap::new(),
            nuggets: vec![nugget(&[
                ("WeaponName", "ChargeWeapon"),
                ("OneShot", "Yes"),
                ("ChargingModeTrigger", "Yes"),
            ])],
        };
        let module = fire_weapon_update_from_fields(&fields, Frame::ZERO).unwrap();
        let (fired, _) = run(module, "FireWeaponUpdate", 30);
        assert!(fired.is_empty());
        let empty = Fields {
            fields: BTreeMap::new(),
            nuggets: vec![nugget(&[("FireDelay", "0")])],
        };
        assert!(fire_weapon_update_from_fields(&empty, Frame::ZERO).is_err());
    }

    #[test]
    fn zero_hours_weapon_field_reads_as_one_repeating_nugget() {
        let fields = nugget(&[("Weapon", "PoisonField"), ("InitialDelay", "1000")]);
        let module = fire_weapon_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.nugget_count(), 1);
        let (fired, _) = run(module, "FireWeaponUpdate", 32);
        assert_eq!(fired.first().map(|(frame, _)| *frame), Some(30));
        assert_eq!(fired.len(), 2);
    }

    #[test]
    fn a_deletion_update_destroys_its_object_once_its_delay_is_up() {
        let module = deletion_update_from_fields(
            &nugget(&[("MinLifetime", "10000"), ("MaxLifetime", "10000")]),
            Frame::ZERO,
        )
        .unwrap();
        let (_, gone) = run(module, "DeletionUpdate", 400);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].0, 300);
        assert!(deletion_update_from_fields(&nugget(&[]), Frame::ZERO).is_err());
        let one =
            deletion_update_from_fields(&nugget(&[("MaxLifetime", "0")]), Frame::ZERO).unwrap();
        let (_, gone) = run(one, "DeletionUpdate", 5);
        assert_eq!(gone[0].0, 1, "at least one frame");
    }

    #[test]
    fn a_negative_lifetime_wraps_to_the_longest_one() {
        let module = deletion_update_from_fields(
            &nugget(&[("MinLifetime", "-1"), ("MaxLifetime", "-1")]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.data.min, LogicFrames::from_frames(128_849_019));
        let (_, gone) = run(module, "DeletionUpdate", 60);
        assert!(gone.is_empty(), "{gone:?}");
    }

    #[test]
    fn a_deletion_range_draws_inside_it_and_repeats() {
        let build = || {
            deletion_update_from_fields(
                &nugget(&[("MinLifetime", "1000"), ("MaxLifetime", "2000")]),
                Frame::ZERO,
            )
            .unwrap()
        };
        let (_, first) = run(build(), "DeletionUpdate", 100);
        let (_, second) = run(build(), "DeletionUpdate", 100);
        assert_eq!(first, second);
        assert!((30..=60).contains(&first[0].0), "{first:?}");
    }

    #[test]
    fn the_hash_covers_the_creation_frame_and_the_latch() {
        let fields = Fields {
            fields: BTreeMap::new(),
            nuggets: vec![nugget(&[("WeaponName", "W"), ("OneShot", "Yes")])],
        };
        let base = fire_weapon_update_from_fields(&fields, Frame::ZERO).unwrap();
        let hash = |m: &FireWeaponUpdate| {
            let mut h = StateHasher::new();
            m.hash_into(&mut h);
            h.finish()
        };
        let mut later = base.clone();
        later.born = Frame::new(5);
        let mut fired = base.clone();
        fired.fired = 1;
        assert_ne!(hash(&base), hash(&later));
        assert_ne!(hash(&base), hash(&fired));
    }
}
