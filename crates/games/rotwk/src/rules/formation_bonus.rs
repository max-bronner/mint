use mint_sage_combat::ModifierId;
use mint_sim::{Component, Entity, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};

const LEVEL_CATEGORY: &str = "LEVEL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormationLists(pub(crate) Vec<ModifierId>);

impl Component for FormationLists {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for id in &self.0 {
            hasher.write_u16(id.index());
        }
    }
}

impl RotwkRules {
    fn formation_lists_of(&self, formation: u16) -> Vec<ModifierId> {
        let Some(table) = self.formations.get(usize::from(formation)) else {
            return Vec::new();
        };
        let mut found: Vec<ModifierId> = Vec::new();
        for name in &table.settings().attribute_modifiers {
            if let Some(id) = self.combat.modifiers.id(name) {
                if !found.contains(&id) {
                    found.push(id);
                }
            }
        }
        found
    }

    pub(super) fn step_formation_lists(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let mut hordes: Vec<Entity> = tick
            .state()
            .store(ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        hordes.sort_unstable_by_key(|entity| entity.index());
        hordes.dedup();
        for horde_entity in hordes {
            let Some(horde) = tick.state().get(ids.horde, horde_entity) else {
                continue;
            };
            let formation = horde.formation;
            let members = horde.members.clone();
            let lists = self.formation_lists_of(formation);
            let member_lists: Vec<ModifierId> = lists
                .iter()
                .copied()
                .filter(|id| {
                    !self.combat.modifiers.get(*id).is_some_and(|list| {
                        list.category
                            .as_deref()
                            .is_some_and(|name| name.eq_ignore_ascii_case(LEVEL_CATEGORY))
                    })
                })
                .collect();
            self.hold_formation_lists(tick, ids, horde_entity, &lists)?;
            for member in members {
                if tick.state().is_live(member) {
                    self.hold_formation_lists(tick, ids, member, &member_lists)?;
                }
            }
        }
        Ok(())
    }

    fn hold_formation_lists(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        wanted: &[ModifierId],
    ) -> Result<()> {
        let held = tick
            .state()
            .get(ids.formation_lists, entity)
            .map_or(&[][..], |held| held.0.as_slice());
        if held == wanted {
            return Ok(());
        }
        if wanted.is_empty() {
            tick.state_mut().remove(ids.formation_lists, entity);
        } else {
            tick.state_mut().insert(
                ids.formation_lists,
                entity,
                FormationLists(wanted.to_vec()),
            )?;
        }
        self.recompute_bonuses(tick, ids, entity)
    }
}
