use mint_sage_modules::effects::{Effect, Names};
use mint_sim::StateHasher;

use super::*;

const MODULE: &str = "MonitorConditionUpdate";
const ATTACKING_POSITION: &str = "ATTACKING_POSITION";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorSpec {
    conditions: Vec<String>,
    condition_set: Option<String>,
    weapon_flags: Vec<String>,
    weapon_set: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandMonitor {
    spec: std::sync::Arc<MonitorSpec>,
    saved: Option<Option<std::sync::Arc<str>>>,
}

impl Component for CommandMonitor {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.saved {
            None => hasher.write_u8(0),
            Some(None) => hasher.write_u8(1),
            Some(Some(name)) => {
                hasher.write_u8(2);
                hasher.write_bytes(name.as_bytes());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttackingPositionRaised;

impl Component for AttackingPositionRaised {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

fn same(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => true,
        _ => false,
    }
}

fn condition(name: &str) -> Names {
    vec![std::sync::Arc::<str>::from(name)].into()
}

impl RotwkRules {
    pub(super) fn attach_command_monitor(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::unit::Kind,
    ) -> Result<()> {
        let Some(module) = self.kinds.get(kind).and_then(|entry| {
            entry
                .record
                .declared_modules
                .iter()
                .find(|module| module.name.eq_ignore_ascii_case(MODULE))
        }) else {
            return Ok(());
        };
        let list = |keyword: &str| -> Vec<String> {
            module
                .field(keyword)
                .map(|field| field.args.clone())
                .unwrap_or_default()
        };
        let first = |keyword: &str| list(keyword).into_iter().next();
        let spec = MonitorSpec {
            conditions: list("ModelConditionFlags"),
            condition_set: first("ModelConditionCommandSet"),
            weapon_flags: list("WeaponSetFlags"),
            weapon_set: first("WeaponToggleCommandSet"),
        };
        tick.state_mut().insert(
            ids.command_monitor,
            entity,
            CommandMonitor {
                spec: std::sync::Arc::new(spec),
                saved: None,
            },
        )?;
        Ok(())
    }

    pub(super) fn step_command_monitors(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        self.sync_attacking_position(tick, ids)?;
        let watchers: Vec<Entity> = tick
            .state()
            .store(ids.command_monitor)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in watchers {
            let (Some(monitor), Some(held)) = (
                tick.state().get(ids.command_monitor, entity).cloned(),
                tick.state().get(wiring.state, entity),
            ) else {
                continue;
            };
            let names = held.model_condition_names();
            let holds = |wanted: &[String]| {
                wanted
                    .iter()
                    .any(|name| names.iter().any(|held| held.eq_ignore_ascii_case(name)))
            };
            let current = held.command_set.clone();
            let spec = &monitor.spec;
            let mut saved = monitor.saved.clone();
            let swap_to = if holds(&spec.conditions) {
                spec.condition_set
                    .as_deref()
                    .filter(|set| !same(current.as_deref(), Some(set)))
                    .map(|set| (set, spec.weapon_set.as_deref()))
            } else if holds(&spec.weapon_flags) {
                spec.weapon_set
                    .as_deref()
                    .filter(|set| !same(current.as_deref(), Some(set)))
                    .map(|set| (set, spec.condition_set.as_deref()))
            } else {
                None
            };
            let next = if let Some((set, other)) = swap_to {
                if !other.is_some_and(|other| same(current.as_deref(), Some(other))) {
                    saved = Some(current.clone());
                }
                Some(Some(std::sync::Arc::<str>::from(set)))
            } else if holds(&spec.conditions) || holds(&spec.weapon_flags) {
                None
            } else if let Some(original) = saved.clone() {
                if same(current.as_deref(), original.as_deref()) {
                    None
                } else {
                    saved = None;
                    Some(original)
                }
            } else {
                None
            };
            if let Some(set) = next {
                if let Some(state) = tick.state_mut().get_mut(wiring.state, entity) {
                    state.command_set = set;
                }
            }
            if saved != monitor.saved {
                tick.state_mut().insert(
                    ids.command_monitor,
                    entity,
                    CommandMonitor {
                        spec: monitor.spec,
                        saved,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn sync_attacking_position(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let ordered: Vec<Entity> = world
            .store(ids.ground_order)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let raised: Vec<Entity> = world
            .store(ids.position_raised)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for entity in &ordered {
            if !raised.contains(entity) {
                effects.push((
                    *entity,
                    Effect::ModelConditions {
                        add: condition(ATTACKING_POSITION),
                        remove: Vec::new().into(),
                    },
                ));
            }
        }
        for entity in &raised {
            if !ordered.contains(entity) {
                effects.push((
                    *entity,
                    Effect::ModelConditions {
                        add: Vec::new().into(),
                        remove: condition(ATTACKING_POSITION),
                    },
                ));
            }
        }
        for (entity, _) in &effects {
            if ordered.contains(entity) {
                tick.state_mut()
                    .insert(ids.position_raised, *entity, AttackingPositionRaised)?;
            } else {
                tick.state_mut().remove(ids.position_raised, *entity);
            }
        }
        self.apply_effects(tick, ids, effects)
    }
}
