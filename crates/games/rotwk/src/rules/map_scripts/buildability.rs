use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sage_modules::BuildableStatus;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};
use crate::unit::Kind;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct BuildOverrides(BTreeMap<u16, BuildableStatus>);

impl BuildOverrides {
    fn get(&self, kind: Kind) -> Option<BuildableStatus> {
        self.0.get(&kind.0).copied()
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (kind, status) in &self.0 {
            hasher.write_u32(u32::from(*kind));
            hasher.write_u8(*status as u8);
        }
    }
}

impl RotwkRules {
    pub(in crate::rules) fn buildable_status_refuses(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: Kind,
    ) -> bool {
        let overridden = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .and_then(|(_, state)| state.rt.buildability.get(kind));
        let status = overridden
            .or_else(|| {
                self.kinds
                    .get(kind)
                    .and_then(|entry| entry.record.buildable)
            })
            .unwrap_or(BuildableStatus::Yes);
        match status {
            BuildableStatus::No => true,
            BuildableStatus::OnlyByAi => !self.is_computer(player),
            BuildableStatus::Yes | BuildableStatus::IgnorePrerequisites => false,
        }
    }
}

struct ActionModifyBuildability {
    template: String,
    status: BuildableStatus,
}

impl ActionVerb for ActionModifyBuildability {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if let Some(kind) = rules.kinds.kind(&self.template) {
            state.rt.buildability.0.insert(kind.0, self.status);
        }
        Ok(())
    }
}

pub(super) fn parse_modify_buildability(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let by_name = parameter_string(params, 1).and_then(|name| BuildableStatus::from_name(&name));
    let by_index = parameter_int(params, 1).and_then(|index| match index {
        0 => Some(BuildableStatus::Yes),
        1 => Some(BuildableStatus::IgnorePrerequisites),
        2 => Some(BuildableStatus::No),
        3 => Some(BuildableStatus::OnlyByAi),
        _ => None,
    });
    Some(Arc::new(ActionModifyBuildability {
        template: parameter_string(params, 0)?,
        status: by_name.or(by_index)?,
    }))
}
