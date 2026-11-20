use super::*;
use mint_sage_combat::{DamageFxTypeId, DamageTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DamageFxThrottle {
    last: (bool, u16),
    next: Frame,
}

impl Component for DamageFxThrottle {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.last.0);
        hasher.write_u16(self.last.1);
        self.next.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowKey(bool, u16);

impl RowKey {
    fn of(fx_type: Option<DamageFxTypeId>, damage_type: DamageTypeId) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        match fx_type {
            Some(id) => Self(true, id.index() as u16),
            None => Self(false, damage_type.index() as u16),
        }
    }
}

impl RotwkRules {
    pub(super) fn play_damage_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        event: &DamageEvent,
        dealt: f32,
    ) {
        let Some(name) = self.damage_fx_name(tick, ids, event.victim) else {
            return;
        };
        let Some(def) = self.combat.damage_fx.get(&name) else {
            return;
        };
        let key = RowKey::of(event.fx_type, event.damage_type);
        let type_name = match event.fx_type {
            Some(id) => self.combat.damage_fx_type_names.name(id),
            None => self.combat.damage_types.name(event.damage_type),
        }
        .unwrap_or_default()
        .to_owned();
        if dealt == 0.0 {
            diag!(
                "damagefx: entity {} zero, no fx (type {type_name})",
                event.victim.index()
            );
            return;
        }
        let now = tick.frame();
        let held = tick
            .state()
            .get(ids.damage_fx_throttle, event.victim)
            .copied();
        if let Some(held) = held {
            if held.last == (key.0, key.1) && held.next > now {
                diag!(
                    "damagefx: entity {} throttled until {}",
                    event.victim.index(),
                    held.next.get()
                );
                return;
            }
        }
        let level = self.veterancy_level(tick, ids, event.attacker);
        let row = def.row(&type_name, level);
        let major = dealt >= row.amount_for_major_fx;
        let list = if major {
            row.major_fx.clone()
        } else {
            row.minor_fx.clone()
        };
        let throttle = row
            .throttle
            .map_or(LogicFrames::ZERO, |duration| frames_of(duration.raw()));
        let next = now.checked_add(throttle).unwrap_or(now);
        let _ = tick.state_mut().insert(
            ids.damage_fx_throttle,
            event.victim,
            DamageFxThrottle {
                last: (key.0, key.1),
                next,
            },
        );
        diag!(
            "damagefx: entity {} def={name} type={type_name} amount={dealt:.1} {} fx={}",
            event.victim.index(),
            if major { "major" } else { "minor" },
            list.as_deref().unwrap_or("none")
        );
        let Some(list) = list else {
            return;
        };
        let Some(at) = Self::fx_point_of(tick.state(), ids, event.victim) else {
            return;
        };
        self.emit_fx_object_named(
            tick,
            ids,
            &list,
            at,
            crate::fx::FxSource::Weapon,
            event.victim,
            Some(event.attacker),
        );
    }

    fn damage_fx_name(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> Option<String> {
        let kind = tick.state().get(ids.kind, victim).copied()?;
        let record = &self.kinds.get(kind)?.record;
        let armour = tick.state().get(ids.armour, victim).copied();
        let matching = record.armor_sets.iter().find(|set| {
            set.damage_fx.is_some()
                && match (armour, set.value.as_deref()) {
                    (Some(worn), Some(name)) => self.combat.armors.id(name) == Some(worn.0),
                    (None, None) => true,
                    _ => false,
                }
        });
        matching
            .or_else(|| record.armor_sets.iter().find(|set| set.damage_fx.is_some()))
            .and_then(|set| set.damage_fx.clone())
    }

    fn veterancy_level(&self, tick: &Tick<'_>, ids: Ids, attacker: Entity) -> usize {
        tick.state()
            .get(ids.experience, attacker)
            .map_or(0, |experience| usize::from(experience.level))
    }
}

fn frames_of(milliseconds: f32) -> LogicFrames {
    mint_sage_modules::retail_duration_ticks_f32(milliseconds)
}
