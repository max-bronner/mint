use super::map_scripts::Relation;
use super::*;
use crate::diag::diag;
use crate::skirmish_ai::forfeit::{
    advance_forfeit, forfeit_score, SideSums, FORFEIT_EXCLUDED_FACTORIES,
};
use mint_sage_modules::ObjectRecord;

const SKIPPED_STRUCTURE_KINDS: [&str; 5] = [
    "LINKED_TO_FLAG",
    "UNATTACKABLE",
    "ECONOMY_STRUCTURE",
    "SUPPLY_GATHERING_CENTER",
    "SUMMONED",
];
const LEVEL_VALUE_STEP: f32 = 0.25;
const COMMAND_SET_SLOTS: usize = 33;
const RING_UPGRADE: &str = "Upgrade_RingHero";

#[derive(Debug, Default)]
pub(super) struct ForfeitSides {
    pub(super) own: SideSums,
    pub(super) enemy: SideSums,
    pub(super) constant: f32,
    pub(super) ring: bool,
}

fn is_army_unit(record: &ObjectRecord) -> bool {
    !record.has_kind("UNATTACKABLE")
        && !record.has_kind("IMMOBILE")
        && !record.has_kind("STRUCTURE")
        && !record.has_kind("SHIP")
        && (record.has_kind("CAN_ATTACK") || record.has_kind("HERO") || record.has_kind("SUPPORT"))
}

fn is_economy_structure(record: &ObjectRecord) -> bool {
    record.has_kind("STRUCTURE")
        && !record.has_kind("LINKED_TO_FLAG")
        && !record.has_kind("UNATTACKABLE")
        && (record.has_kind("ECONOMY_STRUCTURE") || record.has_kind("SUPPLY_GATHERING_CENTER"))
}

fn is_base_structure(record: &ObjectRecord) -> bool {
    record.has_kind("STRUCTURE")
        && !record.has_kind("UNATTACKABLE")
        && !record.has_kind("EXPANSION_PAD")
        && (!record.has_kind("NOT_AUTOACQUIRABLE") || record.has_kind("REBUILD_HOLE"))
        && !record.has_kind("ECONOMY_STRUCTURE")
        && !record.has_kind("LINKED_TO_FLAG")
        && !record.has_kind("BASE_SITE")
}

impl RotwkRules {
    pub(super) fn step_ai_forfeits(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(tuning) = self.forfeit else {
            return Ok(());
        };
        let per_frame = u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        if tick.frame().get() % per_frame != 0 {
            return Ok(());
        }
        let now = i64::try_from(tick.frame().get() / per_frame).unwrap_or(i64::MAX);
        let local = self.forfeit_local_player();
        for slot in 0..self.ai_seeds.len() {
            let Some(player) = self.ai_seeds.get(slot).map(|seed| seed.player) else {
                continue;
            };
            let Some(entity) =
                crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
            else {
                continue;
            };
            let Some(mut attack) = tick.state().get(ids.ai_attack_progress, entity).cloned() else {
                continue;
            };
            let Some(mut progress) = attack.forfeit else {
                continue;
            };
            let world = tick.state();
            let step = advance_forfeit(
                &mut progress,
                &tuning,
                now,
                || self.forfeit_offered(world, ids, player, local),
                || self.forfeit_check(world, ids, player, &tuning, now),
            );
            if step.warn {
                progress.warned_at = Some(tick.frame().get());
                diag!(
                    "forfeit: player {} warns at frame {} (retail frame {now})",
                    player.get(),
                    tick.frame().get()
                );
            }
            attack.forfeit = Some(progress);
            tick.state_mut()
                .insert(ids.ai_attack_progress, entity, attack)?;
            if step.self_destruct {
                diag!(
                    "forfeit: player {} self-destructs at frame {} (retail frame {now})",
                    player.get(),
                    tick.frame().get()
                );
                self.forfeit_self_destruct(tick, ids, player)?;
            }
        }
        Ok(())
    }

    fn forfeit_local_player(&self) -> Option<PlayerId> {
        (0..self.players)
            .map(PlayerId::new)
            .find(|player| !self.ai_seeds.iter().any(|seed| seed.player == *player))
    }

    fn regards(&self, world: &World, ids: Ids, a: PlayerId, b: PlayerId) -> Relation {
        let base = self.alliances.stance(a, b).into();
        self.script_state(world, ids).map_or(base, |state| {
            state.rt.relations.player_to_player(a, b, base)
        })
    }

    fn forfeit_offered(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        local: Option<PlayerId>,
    ) -> bool {
        let above_easy = self
            .ai_power_difficulty
            .iter()
            .find(|(seat, _)| *seat == player)
            .is_some_and(|(_, difficulty)| *difficulty != crate::lobby::Difficulty::Easy);
        above_easy
            && local.is_some_and(|local| {
                local != player && self.regards(world, ids, player, local) == Relation::Enemies
            })
    }

    fn forfeit_check(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        tuning: &mint_sage_ini::game_data::RetreatTuning,
        now: i64,
    ) -> i32 {
        let sides = self.forfeit_sides(world, ids, player);
        let score = forfeit_score(&sides.own, &sides.enemy, tuning, sides.ring, sides.constant);
        diag!(
            "forfeit: player {} scores {score} at retail frame {now} (own q {:.0} n {} h {} m {} s {:.0}; enemy q {:.0} n {} h {} m {} s {:.0}; ring {} c {})",
            player.get(),
            sides.own.quality,
            sides.own.quantity,
            sides.own.hero_level,
            sides.own.map_control,
            sides.own.base,
            sides.enemy.quality,
            sides.enemy.quantity,
            sides.enemy.hero_level,
            sides.enemy.map_control,
            sides.enemy.base,
            sides.ring,
            sides.constant
        );
        score
    }

    fn allied_or_self(&self, world: &World, ids: Ids, a: PlayerId, b: PlayerId) -> bool {
        a == b || self.regards(world, ids, a, b) == Relation::Allies
    }

    fn forfeit_value(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        record: &ObjectRecord,
        owner: PlayerId,
        counts: &crate::economy::PlayerCostModifierCounts,
    ) -> i64 {
        let own_cost = record
            .build_cost
            .and_then(crate::templates::whole)
            .unwrap_or(0);
        let modifiers = mint_sage_economy::CostModifiers {
            brutal_reduction: self.brutal_cheats_for(owner).build_cost_reduction,
            ..crate::economy::cost_modifiers_from_counts(
                counts,
                &self.kinds,
                &self.upgrade_bits,
                &crate::economy::PriceTarget::Object {
                    kind_of: &record.kind_of,
                    name: &record.name,
                },
                self.production_cost_change_percent(owner),
            )
        };
        let mut value = mint_sage_economy::cost_to_build(own_cost, &modifiers);
        let Some(producer) = world.get(ids.producer, entity) else {
            return value;
        };
        let horde = world.get(ids.horde, entity);
        if horde.is_some_and(|horde| {
            self.horde_overloaded
                .get(usize::from(horde.formation))
                .copied()
                .unwrap_or(false)
        }) {
            return value;
        }
        let holders: Vec<Entity> = std::iter::once(entity)
            .chain(horde.map(|horde| horde.members.clone()).unwrap_or_default())
            .collect();
        let holds = |bit| {
            holders.iter().any(|holder| {
                world
                    .get(ids.object_upgrades, *holder)
                    .is_some_and(|mask| mask.0.contains(bit))
            })
        };
        for (menus, menu) in [
            (&self.object_upgrade_menus, producer.object_menu),
            (&self.player_upgrade_menus, producer.player_menu),
        ] {
            let Some(slots) = menu.and_then(|menu| menus.get(usize::from(menu))) else {
                continue;
            };
            for index in slots.iter().take(COMMAND_SET_SLOTS) {
                if let Some(upgrade) = self.upgrade_buildables.get(usize::from(*index)) {
                    if holds(upgrade.bit) {
                        value = value.saturating_add(upgrade.cost);
                    }
                }
            }
        }
        value
    }

    pub(super) fn forfeit_sides(&self, world: &World, ids: Ids, player: PlayerId) -> ForfeitSides {
        let defeat = Self::defeat_ids(ids);
        let economy = Self::economy_ids(ids);
        let mut sides = ForfeitSides::default();
        let all = || (0..self.players).map(PlayerId::new);
        sides.ring = self.upgrade_bits.bit(RING_UPGRADE).is_some_and(|bit| {
            all().any(|other| {
                self.allied_or_self(world, ids, player, other)
                    && crate::economy::player_entity(world, economy, other)
                        .and_then(|entity| world.get(ids.player_upgrades, entity))
                        .is_some_and(|held| held.0.contains(bit))
            })
        });
        let Some(owners) = world.store(ids.owner) else {
            return sides;
        };
        let live =
            |entity: Entity| world.is_live(entity) && !world.contains(defeat.dying.doomed, entity);
        let record_of = |entity: Entity| {
            world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
        };
        let flag_owners: Vec<PlayerId> = owners
            .iter()
            .filter(|(entity, _)| live(*entity))
            .filter(|(entity, _)| record_of(*entity).is_some_and(|r| r.has_kind("CAPTUREFLAG")))
            .map(|(_, owner)| owner.0)
            .collect();
        for member in all().filter(|member| self.forfeit_playable(*member)) {
            let counts = crate::economy::player_cost_modifier_counts(world, economy, member);
            let allied_to_player = self.allied_or_self(world, ids, player, member);
            let side = if member == player
                || self.regards(world, ids, player, member) != Relation::Enemies
            {
                &mut sides.own
            } else {
                &mut sides.enemy
            };
            let held_flags = flag_owners
                .iter()
                .filter(|flag| self.allied_or_self(world, ids, member, **flag))
                .count();
            side.map_control = side
                .map_control
                .saturating_add(i32::try_from(held_flags).unwrap_or(i32::MAX));
            for (entity, owner) in owners.iter() {
                if owner.0 != member || !live(entity) {
                    continue;
                }
                let Some(record) = record_of(entity) else {
                    continue;
                };
                if is_army_unit(record) {
                    let value = self.forfeit_value(world, ids, entity, record, member, &counts);
                    let level = self
                        .experience_of(world, ids, entity)
                        .map_or(0, |(index, _)| index);
                    if record.has_kind("HERO") {
                        side.hero_level = side
                            .hero_level
                            .max(i32::try_from(level.saturating_add(1)).unwrap_or(i32::MAX));
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        let veteran = 1.0 + LEVEL_VALUE_STEP * level as f32;
                        #[allow(clippy::cast_precision_loss)]
                        let value = value as f32;
                        side.quality += value * veteran;
                    }
                }
                let structure = record.has_kind("STRUCTURE");
                if !structure
                    && !record.has_kind("HORDE")
                    && record.has_kind("SELECTABLE")
                    && !record.command_point_bonus.is_some_and(|bonus| bonus > 0.0)
                {
                    let points = record
                        .command_points
                        .and_then(crate::templates::whole)
                        .and_then(|points| i32::try_from(points).ok())
                        .unwrap_or(0);
                    side.quantity = side.quantity.saturating_add(points);
                }
                if is_economy_structure(record) {
                    side.map_control = side.map_control.saturating_add(1);
                }
                if is_base_structure(record)
                    && !SKIPPED_STRUCTURE_KINDS
                        .iter()
                        .any(|kind| record.has_kind(kind))
                {
                    let value = self.forfeit_value(world, ids, entity, record, member, &counts);
                    #[allow(clippy::cast_precision_loss)]
                    let value = value as f32;
                    let doubled =
                        record.has_kind("FS_FACTORY") && !record.has_kind("DOZER_FACTORY");
                    side.base += if doubled { value * 2.0 } else { value };
                    if record.has_kind("DOZER_FACTORY")
                        && !FORFEIT_EXCLUDED_FACTORIES
                            .iter()
                            .any(|name| record.name.eq_ignore_ascii_case(name))
                    {
                        sides.constant = if sides.constant < 2.0 && allied_to_player {
                            2.0
                        } else {
                            1.0
                        };
                    }
                }
            }
        }
        sides
    }

    fn forfeit_playable(&self, player: PlayerId) -> bool {
        self.difficulty
            .players
            .iter()
            .any(|entry| entry.player == player && entry.playable)
    }

    fn forfeit_self_destruct(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) -> Result<()> {
        self.kill_player(tick, ids, player)?;
        let human_ally_stands = (0..self.players).map(PlayerId::new).any(|other| {
            other != player
                && !self.ai_seeds.iter().any(|seed| seed.player == other)
                && self.script_allied(tick.state(), ids, player, other)
                && !self.player_defeated(tick.state(), ids, other)
        });
        if human_ally_stands {
            return Ok(());
        }
        for other in (0..self.players).map(PlayerId::new) {
            if other != player
                && self.script_allied(tick.state(), ids, player, other)
                && self.script_allied(tick.state(), ids, other, player)
                && !self.player_defeated(tick.state(), ids, other)
            {
                self.kill_player(tick, ids, other)?;
            }
        }
        Ok(())
    }

    fn kill_player(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) -> Result<()> {
        let defeat = Self::defeat_ids(ids);
        crate::defeat::latch_defeat(tick, defeat, player)?;
        if let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        {
            tick.state_mut().insert(
                ids.purse,
                entity,
                crate::economy::Purse(mint_sage_economy::Money::new(0)),
            )?;
        }
        crate::defeat::kill_every_object_of(tick, defeat, player)
    }

    pub(crate) fn forfeit_warnings(&self, world: &World, frame: u64) -> Vec<PlayerId> {
        let Some(ids) = self.ids() else {
            return Vec::new();
        };
        self.ai_seeds
            .iter()
            .map(|seed| seed.player)
            .filter(|&player| {
                crate::economy::player_entity(world, Self::economy_ids(ids), player)
                    .and_then(|entity| world.get(ids.ai_attack_progress, entity))
                    .and_then(|attack| attack.forfeit)
                    .and_then(|progress| progress.warned_at)
                    == Some(frame)
            })
            .collect()
    }
}
