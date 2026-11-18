use std::sync::Arc;

use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, Entity, FieldError, Frame, ModuleFieldSource, StateHasher};

pub const MAX_SPAWN_POINTS: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnPointProductionExitUpdate {
    bone_name: Arc<str>,
    occupiers: Vec<Option<Entity>>,
}

impl SpawnPointProductionExitUpdate {
    #[must_use]
    pub fn bone_name(&self) -> &str {
        &self.bone_name
    }

    pub fn free_point(&mut self, points: usize, is_live: impl Fn(Entity) -> bool) -> Option<usize> {
        self.occupiers.resize(points.min(MAX_SPAWN_POINTS), None);
        for slot in &mut self.occupiers {
            if slot.is_some_and(|occupier| !is_live(occupier)) {
                *slot = None;
            }
        }
        self.occupiers.iter().position(Option::is_none)
    }

    pub fn occupy(&mut self, point: usize, object: Entity) {
        if let Some(slot) = self.occupiers.get_mut(point) {
            *slot = Some(object);
        }
    }
}

impl Component for SpawnPointProductionExitUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.occupiers.len());
        for slot in &self.occupiers {
            hasher.write_bool(slot.is_some());
            if let Some(occupier) = slot {
                occupier.hash_into(hasher);
            }
        }
    }
}

impl ModuleKind for SpawnPointProductionExitUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;

    fn on_attach(&mut self, _now: Frame) {
        self.occupiers.clear();
    }
}

pub fn spawn_point_production_exit_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpawnPointProductionExitUpdate, FieldError> {
    const KEYWORD: &str = "SpawnPointBoneName";
    let bone_name = match (fields.field(KEYWORD), fields.field_args(KEYWORD).as_slice()) {
        (None, _) => "",
        (Some(_), [name]) => name,
        (Some(raw), _) => {
            return Err(FieldError::new(format!(
                "{KEYWORD} '{raw}' is not one bone name"
            )))
        }
    };
    Ok(SpawnPointProductionExitUpdate {
        bone_name: Arc::from(bone_name),
        occupiers: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fields(Option<&'static str>);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.filter(|_| keyword == "SpawnPointBoneName")
        }

        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|raw| raw.split_whitespace().collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn a_bone_name_is_one_token_and_anything_else_is_refused() {
        let read = |raw| spawn_point_production_exit_update_from_fields(&Fields(raw), Frame::ZERO);
        assert_eq!(read(Some("PERCH_")).unwrap().bone_name(), "PERCH_");
        assert_eq!(read(None).unwrap().bone_name(), "");
        assert!(read(Some("")).is_err());
        assert!(read(Some("PERCH_ EXTRA")).is_err());
    }

    #[test]
    fn the_first_point_without_a_live_occupier_is_the_one_used() {
        let mut exit =
            spawn_point_production_exit_update_from_fields(&Fields(Some("PERCH_")), Frame::ZERO)
                .unwrap();
        let mut world = mint_sim::World::new();
        let (a, b) = (world.spawn().unwrap(), world.spawn().unwrap());
        assert_eq!(exit.free_point(2, |_| true), Some(0));
        exit.occupy(0, a);
        assert_eq!(exit.free_point(2, |_| true), Some(1));
        exit.occupy(1, b);
        assert_eq!(exit.free_point(2, |_| true), None);
        assert_eq!(exit.free_point(2, |entity| entity != a), Some(0));
        exit.occupy(0, a);
        assert_eq!(exit.free_point(MAX_SPAWN_POINTS + 5, |_| true), Some(2));
        assert_eq!(exit.occupiers.len(), MAX_SPAWN_POINTS);
    }
}
