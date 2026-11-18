use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ToppleDirections(BTreeMap<String, [u32; 3]>);

impl ToppleDirections {
    #[allow(dead_code)]
    pub(crate) fn direction(&self, name: &str, at: [f32; 3]) -> Option<[f32; 2]> {
        let target = self.0.get(name)?.map(f32::from_bits);
        let delta = [target[0] - at[0], target[1] - at[1], target[2] - at[2]];
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        (length.is_finite() && length > 0.0).then(|| [delta[0] / length, delta[1] / length])
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (name, point) in &self.0 {
            hasher.write_bytes(name.as_bytes());
            hasher.write_u8(0);
            for value in point {
                hasher.write_u32(*value);
            }
        }
    }
}

struct ActionSetTopple {
    name: String,
    point: [f32; 3],
}

impl ActionVerb for ActionSetTopple {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if !self.name.is_empty() {
            state
                .rt
                .topple
                .0
                .insert(self.name.clone(), self.point.map(f32::to_bits));
        }
        Ok(())
    }
}

pub(super) fn parse_set_topple_direction(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let Some(&Parameter::Coord3D { x, y, z }) = params.get(1) else {
        return None;
    };
    Some(Arc::new(ActionSetTopple {
        name: parameter_string(params, 0)?,
        point: [x, y, z],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_direction_points_from_the_object_to_the_stored_point_and_is_normalised_in_3d() {
        let mut directions = ToppleDirections::default();
        directions
            .0
            .insert("Tower".to_owned(), [30.0, 40.0, 0.0].map(f32::to_bits));
        let [x, y] = directions.direction("Tower", [0.0, 0.0, 0.0]).unwrap();
        assert!((x - 0.6).abs() < 1e-6 && (y - 0.8).abs() < 1e-6);
        let [x, y] = directions.direction("Tower", [0.0, 0.0, -50.0]).unwrap();
        assert!(
            x < 0.6 && y < 0.8,
            "a height gap shortens the horizontal share"
        );
        assert!(directions.direction("Other", [0.0, 0.0, 0.0]).is_none());
    }
}
