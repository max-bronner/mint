use std::collections::BTreeMap;

use mint_sim::{Entity, StateHasher, Tick};

use super::counters::SideKey;
use super::runtime::ScriptRuntime;
use crate::rules::{Ids, RotwkRules};

pub(crate) const THIS_OBJECT: &str = "<This Object>";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ObjectRefs {
    named: BTreeMap<SideKey, Entity>,
    subject: Option<Entity>,
    fallback_subject: Option<Entity>,
    running_side: String,
}

impl ObjectRefs {
    pub(crate) fn set(&mut self, name: &str, side: &str, entity: Entity) {
        self.named.insert(SideKey::qualify(name, side), entity);
    }

    pub(crate) fn bind_subject(&mut self, subject: Option<Entity>, fallback: Option<Entity>) {
        self.subject = subject;
        self.fallback_subject = fallback;
    }

    pub(crate) fn set_condition_object(&mut self, object: Option<Entity>) {
        self.fallback_subject = object;
    }

    pub(crate) fn swap_running_side(&mut self, side: String) -> String {
        std::mem::replace(&mut self.running_side, side)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.named.retain(|_, entity| is_live(*entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.named.len());
        for (key, entity) in &self.named {
            hasher.write_bytes(key.side.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(key.name.as_bytes());
            entity.hash_into(hasher);
        }
        for slot in [self.subject, self.fallback_subject] {
            hasher.write_bool(slot.is_some());
            if let Some(entity) = slot {
                entity.hash_into(hasher);
            }
        }
    }
}

impl ObjectRefs {
    pub(crate) fn resolve(
        &self,
        name: &str,
        side: &str,
        is_live: impl Fn(Entity) -> bool,
        raw_name: impl FnOnce(&str) -> Option<Entity>,
    ) -> Option<Entity> {
        if name.is_empty() {
            return None;
        }
        if name == THIS_OBJECT {
            return self
                .subject
                .or(self.fallback_subject)
                .filter(|&e| is_live(e));
        }
        let side = if side.is_empty() {
            self.running_side.as_str()
        } else {
            side
        };
        let key = SideKey::qualify(name, side);
        if let Some(&entity) = self.named.get(&key) {
            return Some(entity).filter(|&e| is_live(e));
        }
        raw_name(&key.name)
    }
}

impl RotwkRules {
    pub(crate) fn object_named(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        runtime: &ScriptRuntime,
        name: &str,
        side: &str,
    ) -> Option<Entity> {
        runtime.objects.resolve(
            name,
            side,
            |e| tick.state().is_live(e),
            |bare| self.named_entity(tick, ids, bare),
        )
    }

    pub(crate) fn object_named_or_dying(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        runtime: &ScriptRuntime,
        name: &str,
        side: &str,
    ) -> Option<Entity> {
        runtime.objects.resolve(
            name,
            side,
            |e| tick.state().is_live(e),
            |bare| {
                let store = tick.state().store(ids.script_name)?;
                store
                    .iter()
                    .find(|(entity, tag)| tag.0 == bare && tick.state().is_live(*entity))
                    .map(|(entity, _)| entity)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use mint_sim::World;

    use super::*;

    #[test]
    fn lookup_order_is_this_object_then_reference_then_raw_name() {
        let mut world = World::new();
        let (marked, subject, raw) = (
            world.spawn().unwrap(),
            world.spawn().unwrap(),
            world.spawn().unwrap(),
        );
        let mut refs = ObjectRefs::default();
        refs.set("Marker", "SideA", marked);
        refs.bind_subject(Some(subject), None);
        let live = |_: Entity| true;
        let raw_hit = |_: &str| Some(raw);
        assert_eq!(refs.resolve("Marker", "SideA", live, raw_hit), Some(marked));
        assert_eq!(
            refs.resolve("SideA/Marker", "SideB", live, raw_hit),
            Some(marked)
        );
        assert_eq!(refs.resolve("Marker", "SideB", live, raw_hit), Some(raw));
        assert_eq!(
            refs.resolve(THIS_OBJECT, "SideB", live, raw_hit),
            Some(subject)
        );
        assert_eq!(refs.resolve("", "SideA", live, raw_hit), None);
    }

    #[test]
    fn a_lookup_with_no_side_reads_the_running_scripts_side() {
        let mut world = World::new();
        let (marked, raw) = (world.spawn().unwrap(), world.spawn().unwrap());
        let mut refs = ObjectRefs::default();
        refs.set("Marker", "SideA", marked);
        refs.swap_running_side(String::from("SideA"));
        assert_eq!(
            refs.resolve("Marker", "", |_| true, |_| Some(raw)),
            Some(marked)
        );
    }

    #[test]
    fn a_reference_to_a_gone_object_is_nothing_not_the_raw_name() {
        let mut world = World::new();
        let (marked, raw) = (world.spawn().unwrap(), world.spawn().unwrap());
        let mut refs = ObjectRefs::default();
        refs.set("Marker", "SideA", marked);
        let live = |e: Entity| e != marked;
        assert_eq!(refs.resolve("Marker", "SideA", live, |_| Some(raw)), None);
    }

    #[test]
    fn a_qualified_name_with_no_reference_finds_the_object_by_its_bare_name() {
        let mut world = World::new();
        let raw = world.spawn().unwrap();
        let refs = ObjectRefs::default();
        let by_name = |name: &str| (name == "Unit").then_some(raw);
        assert_eq!(
            refs.resolve("SideA/Unit", "SideB", |_| true, by_name),
            Some(raw)
        );
        assert_eq!(refs.resolve("/Unit", "SideB", |_| true, by_name), Some(raw));
    }
}
