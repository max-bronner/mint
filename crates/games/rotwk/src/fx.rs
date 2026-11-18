use mint_sim::{Component, Entity, Frame, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FxId(pub(crate) u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FxSource {
    Power,
    Weapon,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FxPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: Option<f32>,
}

impl From<mint_movement::Vec2> for FxPoint {
    fn from(at: mint_movement::Vec2) -> Self {
        Self {
            x: at.x,
            y: at.y,
            z: None,
        }
    }
}

impl From<mint_sage_combat::Point3> for FxPoint {
    fn from(at: mint_sage_combat::Point3) -> Self {
        Self {
            x: at.x,
            y: at.y,
            z: Some(at.z),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FxEvent {
    pub(crate) list: FxId,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: Option<f32>,
    pub(crate) source: FxSource,
    pub(crate) caller: Entity,
    pub(crate) target: Option<(f32, f32)>,
    pub(crate) victim: Option<Entity>,
    pub(crate) on_object: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct FxEvents {
    pub(crate) frame: Frame,
    pub(crate) events: Vec<FxEvent>,
}

impl FxEvents {
    pub(crate) fn push(&mut self, now: Frame, event: FxEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    pub(crate) fn of(&self, now: Frame) -> &[FxEvent] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for FxEvents {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            hasher.write_u16(event.list.0);
            hasher.write_f32(event.x);
            hasher.write_f32(event.y);
            hasher.write_bool(event.z.is_some());
            if let Some(z) = event.z {
                hasher.write_f32(z);
            }
            hasher.write_bool(event.source == FxSource::Power);
            event.caller.hash_into(hasher);
            hasher.write_bool(event.target.is_some());
            if let Some((x, y)) = event.target {
                hasher.write_f32(x);
                hasher.write_f32(y);
            }
            hasher.write_bool(event.on_object);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FxNames {
    sorted: Vec<(String, String)>,
}

impl FxNames {
    pub(crate) fn build<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut sorted: Vec<(String, String)> = names
            .into_iter()
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
            .map(|name| (name.to_ascii_lowercase(), name.to_owned()))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted.dedup_by(|a, b| a.0 == b.0);
        sorted.truncate(usize::from(u16::MAX));
        Self { sorted }
    }

    pub(crate) fn id(&self, name: &str) -> Option<FxId> {
        let key = name.trim().bytes().map(|b| b.to_ascii_lowercase());
        self.sorted
            .binary_search_by(|(held, _)| held.bytes().cmp(key.clone()))
            .ok()
            .and_then(|at| u16::try_from(at).ok())
            .map(FxId)
    }

    pub(crate) fn name(&self, id: FxId) -> Option<&str> {
        self.sorted
            .get(usize::from(id.0))
            .map(|(_, name)| name.as_str())
    }

    pub(crate) fn len(&self) -> usize {
        self.sorted.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sim::World;

    fn caller() -> Entity {
        let mut world = World::new();
        world.spawn().unwrap()
    }

    fn event(list: u16) -> FxEvent {
        FxEvent {
            list: FxId(list),
            x: 1.0,
            y: 2.0,
            z: None,
            source: FxSource::Power,
            caller: caller(),
            target: None,
            victim: None,
            on_object: false,
        }
    }

    #[test]
    fn a_new_frame_replaces_the_last_frames_events() {
        let mut fx = FxEvents::default();
        fx.push(Frame::new(3), event(1));
        fx.push(Frame::new(3), event(2));
        assert_eq!(fx.of(Frame::new(3)).len(), 2);
        assert!(fx.of(Frame::new(4)).is_empty(), "frame 4 fired nothing");
        let capacity = fx.events.capacity();
        fx.push(Frame::new(5), event(7));
        assert_eq!(fx.of(Frame::new(5)), &[event(7)]);
        assert_eq!(fx.events.capacity(), capacity, "cleared, not reallocated");
    }

    #[test]
    fn names_are_sorted_deduplicated_and_found_case_insensitively() {
        let names = FxNames::build(["FX_B", "fx_a", "FX_A", "NONE", " ", "FX_C"]);
        assert_eq!(names.len(), 3);
        let a = names.id("FX_a").unwrap();
        assert_eq!(names.name(a), Some("fx_a"), "the first spelling is kept");
        assert_eq!(names.id(" fx_A\t"), Some(a), "trimmed as the table was");
        assert!(names.id("FX_Missing").is_none());
        assert!(names.id("none").is_none());
        assert!(names.id("FX_B").unwrap() < names.id("fx_c").unwrap());
    }

    #[test]
    fn every_field_changes_the_hash() {
        let hash = |fx: &FxEvents| {
            let mut h = StateHasher::new();
            fx.hash_into(&mut h);
            h.finish()
        };
        let mut base = FxEvents::default();
        base.push(Frame::new(1), event(1));
        let mut other_frame = base.clone();
        other_frame.frame = Frame::new(2);
        let mut other_list = base.clone();
        other_list.events[0].list = FxId(2);
        let mut other_place = base.clone();
        other_place.events[0].x = 9.0;
        let mut other_source = base.clone();
        other_source.events[0].source = FxSource::Weapon;
        let mut other_caller = base.clone();
        let mut world = World::new();
        world.spawn().unwrap();
        other_caller.events[0].caller = world.spawn().unwrap();
        let mut other_target = base.clone();
        other_target.events[0].target = Some((3.0, 4.0));
        let mut other_on_object = base.clone();
        other_on_object.events[0].on_object = true;
        for changed in [
            other_frame,
            other_list,
            other_place,
            other_source,
            other_caller,
            other_target,
            other_on_object,
        ] {
            assert_ne!(hash(&base), hash(&changed));
        }
    }
}
