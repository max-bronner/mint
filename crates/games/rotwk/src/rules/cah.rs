use mint_sim::{Entity, Frame, PlayerId, Result, Tick, UpgradeMask, World};

use super::{Ids, RotwkRules};
use crate::create_a_hero::{HeroPlan, CREATE_A_HERO_KIND};
use crate::diag::diag;
use crate::experience::LevelTable;
use crate::heroes::HeroState;
use crate::kill_credit::KillCredit;
use crate::unit::Kind;

const PASS_EXPERIENCE_KIND: &str = "PASS_EXPERIENCE_TO_PRODUCER";
const PRODUCER_WALK_LIMIT: usize = 16;

impl RotwkRules {
    pub(super) fn is_cah_kind(&self, kind: Kind) -> bool {
        self.kinds
            .get(kind)
            .is_some_and(|entry| entry.record.has_kind(CREATE_A_HERO_KIND))
    }

    pub(crate) fn cah_plan_of(&self, player: PlayerId) -> Option<&HeroPlan> {
        self.cah_plans.iter().find(|plan| plan.player == player)
    }

    fn bound_plan(&self, world: &World, ids: Ids, entity: Entity) -> Option<&HeroPlan> {
        self.cah_plans.iter().find(|plan| {
            self.roster_of(world, ids, plan.player)
                .is_some_and(|roster| {
                    roster.0.iter().any(|entry| {
                        entry.state == HeroState::Alive(entity)
                            && entry.kind.is_some_and(|kind| self.is_cah_kind(kind))
                    })
                })
        })
    }

    pub(crate) fn cah_colours(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<[[f32; 4]; 3]> {
        let plan = self.bound_plan(world, ids, entity)?;
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
        let kind = *world.get(ids.kind, entity)?;
        let plan = self.bound_plan(world, ids, entity)?;
        self.hero_tables
            .iter()
            .find(|(player, own, _)| *player == plan.player && *own == kind)
            .map(|(_, _, table)| table)
    }

    pub(crate) fn cah_command_set(&self, world: &World, entity: Entity) -> Option<&str> {
        let ids = self.ids?;
        let table = self.hero_table(world, ids, entity)?;
        let (index, _) = self.experience_of(world, ids, entity)?;
        let rank = table.level(index)?.rank;
        Some(
            self.bound_plan(world, ids, entity)?
                .set_at(rank)?
                .name
                .as_str(),
        )
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
        Some(self.bound_plan(world, ids, entity)?.name.clone())
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

    pub(crate) fn cah_bound_player(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<PlayerId> {
        self.bound_plan(world, ids, entity).map(|plan| plan.player)
    }

    pub(super) fn credit_hero_kill(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killer: Entity,
        victim: Entity,
    ) {
        let world = tick.state();
        let Some(victim_kind) = world.get(ids.kind, victim).copied() else {
            return;
        };
        let kind_has = |entity: Entity, token: &str| {
            world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| entry.record.has_kind(token))
        };
        let mut credited = killer;
        for _ in 0..PRODUCER_WALK_LIMIT {
            if !kind_has(credited, PASS_EXPERIENCE_KIND) {
                break;
            }
            let Some(link) = self
                .modules
                .as_ref()
                .and_then(|modules| modules.created_by(world, credited))
            else {
                break;
            };
            if link.creator == credited || !world.is_live(link.creator) {
                break;
            }
            credited = link.creator;
        }
        if !kind_has(credited, CREATE_A_HERO_KIND) {
            return;
        }
        let now = tick.frame();
        if let Some((_, credits)) = tick
            .state_mut()
            .store_mut(ids.kill_credits)
            .and_then(|store| store.iter_mut().next())
        {
            credits.push(
                now,
                KillCredit {
                    killer: credited,
                    victim: victim_kind,
                },
            );
        }
    }

    pub(crate) fn kill_credits<'w>(&self, world: &'w World, frame: Frame) -> &'w [KillCredit] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.kill_credits)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, credits)| credits.of(frame))
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
