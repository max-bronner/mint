use mint_sim::{Entity, PlayerId, Result, Tick, UpgradeMask, World};

use super::{Ids, RotwkRules};
use crate::create_a_hero::{HeroPlan, CREATE_A_HERO_KIND};
use crate::diag::diag;
use crate::experience::LevelTable;
use crate::unit::Kind;

impl RotwkRules {
    pub(super) fn is_cah_kind(&self, kind: Kind) -> bool {
        self.kinds
            .get(kind)
            .is_some_and(|entry| entry.record.has_kind(CREATE_A_HERO_KIND))
    }

    pub(crate) fn cah_plan_of(&self, player: PlayerId) -> Option<&HeroPlan> {
        self.cah_plans.iter().find(|plan| plan.player == player)
    }

    pub(crate) fn cah_colours(&self, kind: Kind, owner: PlayerId) -> Option<[[f32; 4]; 3]> {
        if !self.is_cah_kind(kind) {
            return None;
        }
        let plan = self.cah_plan_of(owner)?;
        Some(plan.record.colors.map(|packed| {
            let [_, r, g, b] = packed.to_be_bytes();
            [
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                1.0,
            ]
        }))
    }

    pub(super) fn build_hero_tables(&self) -> Vec<(PlayerId, Kind, LevelTable)> {
        self.cah_plans
            .iter()
            .flat_map(|plan| {
                plan.ladders.iter().filter_map(|(kind, unlocks)| {
                    let base = self.level_tables.get(*kind)?;
                    Some((plan.player, *kind, base.with_unlocks(unlocks)))
                })
            })
            .collect()
    }

    pub(super) fn hero_table(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&LevelTable> {
        if self.hero_tables.is_empty() {
            return None;
        }
        let owner = world.get(ids.owner, entity)?.0;
        let kind = *world.get(ids.kind, entity)?;
        self.hero_tables
            .iter()
            .find(|(player, own, _)| *player == owner && *own == kind)
            .map(|(_, _, table)| table)
    }

    pub(crate) fn cah_command_set(&self, world: &World, entity: Entity) -> Option<&str> {
        let ids = self.ids?;
        let table = self.hero_table(world, ids, entity)?;
        let owner = world.get(ids.owner, entity)?.0;
        let (index, _) = self.experience_of(world, ids, entity)?;
        let rank = table.level(index)?.rank;
        Some(self.cah_plan_of(owner)?.set_at(rank)?.name.as_str())
    }

    pub(crate) fn cah_set_named(&self, name: &str) -> Option<&mint_sage_ini::CommandSetDef> {
        self.cah_plans
            .iter()
            .flat_map(|plan| plan.sets.iter())
            .find(|set| set.name == name)
            .map(|set| &set.def)
    }

    pub(crate) fn cah_display_name(&self, world: &World, entity: Entity) -> Option<String> {
        let ids = self.ids?;
        self.hero_table(world, ids, entity)?;
        let owner = world.get(ids.owner, entity)?.0;
        Some(self.cah_plan_of(owner)?.name.clone())
    }

    pub(crate) fn cah_power_cost(&self, player: PlayerId, kind: Kind) -> u32 {
        if !self.is_cah_kind(kind) {
            return 0;
        }
        self.cah_plan_of(player).map_or(0, |plan| plan.power_cost)
    }

    pub(super) fn bind_create_a_hero(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        hero: Entity,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, hero).copied() else {
            return Ok(());
        };
        if !self.is_cah_kind(kind) {
            return Ok(());
        }
        let Some(plan) = self.cah_plan_of(owner) else {
            return Ok(());
        };
        let mut removed = UpgradeMask::EMPTY;
        for bit in &plan.removes {
            removed.set(*bit);
        }
        if let Some(held) = tick.state_mut().get_mut(ids.object_upgrades, hero) {
            held.0.clear_all(&removed);
        }
        for bit in &plan.grants {
            self.grant_object_upgrade(tick, ids, hero, *bit)?;
        }
        if let Some(bit) = self.cah_can_build {
            let mut mask = UpgradeMask::EMPTY;
            mask.set(bit);
            self.remove_player_upgrade(tick, ids, owner, &mask)?;
        }
        diag!(
            "cah: entity {} bound to player {} hero {:?}: {} upgrade(s)",
            hero.index(),
            owner.get(),
            plan.name,
            plan.grants.len()
        );
        Ok(())
    }

    pub(super) fn cah_hero_died(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player_entity: Entity,
    ) -> Result<()> {
        let (Some(bit), Some(player)) = (
            self.cah_can_build,
            tick.state().get(ids.slot, player_entity).map(|slot| slot.0),
        ) else {
            return Ok(());
        };
        if self.cah_plan_of(player).is_none() {
            return Ok(());
        }
        self.grant_player_upgrade(tick, ids, player, bit)
    }
}
