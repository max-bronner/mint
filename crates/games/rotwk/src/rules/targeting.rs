use mint_movement::{Cell, MoverClass, Vec2};
use mint_sim::{Entity, PlayerId, Tick, World};

use super::map_scripts::{ATTITUDE_NORMAL, ATTITUDE_SLEEP};
use super::{Ids, RotwkRules};
use crate::combat::priority::{acquire_ranked, PriorityRanking, TargetPriority};
use crate::combat::{acquire_target, in_range, Foe};

const APPROACH_REACH: f32 = 150.0;

impl RotwkRules {
    fn priority_ranking<'a>(
        &'a self,
        world: &'a World,
        ids: Ids,
        scanner: Entity,
    ) -> Option<PriorityRanking<'a>> {
        let set = self.priority_set_of(world, ids, scanner)?;
        let chain = self
            .ai_kind_of_of(world, ids, scanner)
            .map(|kind| (kind, &self.combat_chains));
        let owner = Self::computer_owner(world, ids, scanner);
        Some(PriorityRanking {
            set,
            distance_modifier: self.attack_priorities.distance_modifier,
            chain,
            target_player: owner.and_then(|owner| self.ai_target_player(world, ids, owner)),
            computer: owner.is_some_and(|owner| self.computer_controlled(owner)),
        })
    }

    fn target_priority<'a>(
        &'a self,
        world: &'a World,
        ids: Ids,
        entity: Entity,
    ) -> TargetPriority<'a> {
        let template = |at: Entity| {
            world
                .get(ids.kind, at)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| (entry.record.name.as_str(), entry.record.ai_kind_of))
        };
        let own = template(entity);
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let kind = |bit: &str| self.has_kind_bit(world, ids, entity, bit);
        let open_gate = || {
            kind("BLOCKING_GATE")
                && world
                    .get(ids.gate_cycle, entity)
                    .is_some_and(|cycle| cycle.fully_open())
        };
        let horde = world.get(ids.member, entity).map(|member| member.horde);
        TargetPriority {
            template: own.map(|(name, _)| name),
            owner,
            horde: horde
                .and_then(template)
                .map(|(name, kind)| (Some(name), kind)),
            horde_contained: horde.is_some_and(|horde| world.contains(ids.contained, horde)),
            contents: world
                .get(ids.passengers, entity)
                .map(|held| {
                    held.0
                        .iter()
                        .map(|&inside| {
                            let entry = template(inside);
                            (
                                entry.map(|(name, _)| name),
                                entry.and_then(|(_, kind)| kind),
                                world.get(ids.owner, inside).map(|owner| owner.0),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            creep: owner.is_some_and(|owner| self.alliances.is_creep_player(owner))
                || kind("CREEP"),
            unpickable: self.has_status_bit(world, entity, "DO_NOT_PICK_ME")
                || open_gate()
                || (kind("WALL_SEGMENT") && !kind("CAN_ATTACK")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn acquire_for(
        &self,
        world: &World,
        ids: Ids,
        scanner: Entity,
        from: Vec2,
        radius: f32,
        foes: &[Foe],
        min_range: f32,
        max_range: f32,
    ) -> Option<Entity> {
        match self.priority_ranking(world, ids, scanner) {
            Some(ranking) => {
                let mover = self.mover_class_in(world, ids, scanner);
                let start = world
                    .get(ids.body, scanner)
                    .and_then(|body| self.passability.cell_at(body.0.position));
                let reach = self.approach_reach(world, ids, scanner);
                let blast = self.splash_radius(world, ids, scanner);
                acquire_ranked(
                    from,
                    radius,
                    foes,
                    min_range,
                    max_range,
                    &ranking,
                    |e| self.target_priority(world, ids, e),
                    |foe| self.can_approach(mover, start, foe.position, reach),
                    |foe| {
                        blast.is_some_and(|radius| {
                            self.too_much_friendly_fire(world, ids, scanner, foe, radius)
                        })
                    },
                )
            }
            None => acquire_target(from, radius, foes, min_range, max_range),
        }
    }

    pub(super) fn off_map(&self, world: &World, ids: Ids, at: Vec2) -> bool {
        outside(self.active_high(world, ids), at)
    }

    pub(super) fn scanner_off_map(&self, world: &World, ids: Ids, scanner: Entity) -> bool {
        world
            .get(ids.body, scanner)
            .is_some_and(|body| self.off_map(world, ids, body.0.position))
    }

    fn splash_radius(&self, world: &World, ids: Ids, scanner: Entity) -> Option<f32> {
        let weapon = world.get(ids.armed, scanner)?.weapon;
        let template = self.combat.weapons.get(weapon)?;
        let held = self.effective_upgrades_in(world, ids, scanner);
        let passes = |weapon, index| self.combat.nugget_passes(weapon, index, &held);
        let radius = template
            .nuggets
            .iter()
            .enumerate()
            .filter(|&(index, _)| passes(weapon, index))
            .find_map(|(_, nugget)| {
                let mint_sage_combat::Nugget::Projectile(projectile) = nugget else {
                    return None;
                };
                let warhead = self.combat.weapons.id(&projectile.warhead)?;
                let template = self.combat.weapons.get(warhead)?;
                if !template.radius_affects.affects_allies() {
                    return None;
                }
                template
                    .nuggets
                    .iter()
                    .enumerate()
                    .filter(|&(index, _)| passes(warhead, index))
                    .find_map(|(_, nugget)| match nugget {
                        mint_sage_combat::Nugget::Damage(damage) => Some(damage.radius),
                        _ => None,
                    })
            })?;
        (radius > 0.0).then_some(radius)
    }

    fn too_much_friendly_fire(
        &self,
        world: &World,
        ids: Ids,
        scanner: Entity,
        foe: &Foe,
        radius: f32,
    ) -> bool {
        let max_share = self.attack_priorities.max_friendly_fire;
        let own = world
            .get(ids.health, foe.entity)
            .map_or(0.0, |health| health.0.max());
        if max_share >= 1.0 && own >= 1.0 {
            return false;
        }
        let Some(health) = world.store(ids.health) else {
            return false;
        };
        let (mut allied, mut other) = (0.0_f32, 0.0_f32);
        for (entity, pool) in health.iter() {
            if pool.0.is_dead() {
                continue;
            }
            let (Some(body), Some(shape)) =
                (world.get(ids.body, entity), world.get(ids.shape, entity))
            else {
                continue;
            };
            let reach = radius + shape.radius;
            let (dx, dy) = (
                body.0.position.x - foe.position.x,
                body.0.position.y - foe.position.y,
            );
            if dx * dx + dy * dy > reach * reach {
                continue;
            }
            if self.object_allied(world, scanner, entity) {
                allied += pool.0.max();
            } else {
                other += pool.0.max();
            }
        }
        crate::combat::priority::friendly_fire_skips(allied, other, max_share)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn attackable_from_here(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        scanner: Entity,
        at: Vec2,
        radius: f32,
        foe: &Foe,
        no_walk: bool,
    ) -> bool {
        self.reach_from_here(tick, ids, scanner)
            .attackable(at, radius, foe, no_walk)
    }

    pub(super) fn reach_from_here(&self, tick: &Tick<'_>, ids: Ids, scanner: Entity) -> HereReach {
        let world = tick.state();
        let base = world
            .get(ids.kind, scanner)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .filter(|vision| vision.is_finite() && *vision > 0.0)
            .unwrap_or(0.0);
        let bonus = world
            .get(ids.bonuses, scanner)
            .map_or(0.0, |bonuses| bonuses.0.vision);
        let vision = base * (1.0 + bonus).max(0.0);
        if let Some(horde) = world.get(ids.horde, scanner) {
            let reach = self
                .horde_rangefinder(horde)
                .map(|range| {
                    world
                        .get(ids.bonuses, scanner)
                        .map_or(range, |bonuses| bonuses.0.range(range))
                })
                .or_else(|| self.horde_max_range(tick, ids, horde));
            return HereReach {
                vision,
                horde: Some((reach, self.melee_rangefinder(horde).is_some())),
                arms: Vec::new(),
            };
        }
        let weapons: Vec<mint_sage_combat::WeaponId> = match self.selected_set(tick, ids, scanner) {
            Some((_, slots)) => slots.arms.iter().flatten().map(|arm| arm.weapon).collect(),
            None => world
                .get(ids.armed, scanner)
                .map(|armed| armed.weapon)
                .into_iter()
                .collect(),
        };
        let arms = weapons
            .into_iter()
            .filter_map(|weapon| self.combat.weapons.get(weapon))
            .map(|template| {
                (
                    template.minimum_attack_range,
                    self.attack_range_of(world, ids, scanner, template),
                    template.melee,
                )
            })
            .collect();
        HereReach {
            vision,
            horde: None,
            arms,
        }
    }

    fn has_status_bit(&self, world: &World, entity: Entity, bit: &str) -> bool {
        self.modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .is_some_and(|state| {
                state
                    .status_bits
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(bit))
            })
    }

    pub(super) fn base_priority(
        &self,
        world: &World,
        ids: Ids,
        scanner: Entity,
        target: Entity,
    ) -> i32 {
        let Some(set) = self.priority_set_of(world, ids, scanner) else {
            return 0;
        };
        self.target_priority(world, ids, target)
            .template
            .and_then(|name| set.templates.get(&name.to_ascii_lowercase()))
            .copied()
            .unwrap_or(set.default)
    }

    fn computer_owner(world: &World, ids: Ids, entity: Entity) -> Option<PlayerId> {
        Self::commanded_owner(world, ids, Self::commanded_unit(world, ids, entity))
    }

    fn approach_reach(&self, world: &World, ids: Ids, scanner: Entity) -> f32 {
        world
            .get(ids.armed, scanner)
            .and_then(|armed| self.combat.weapons.get(armed.weapon))
            .map_or(APPROACH_REACH, |template| {
                self.attack_range_of(world, ids, scanner, template)
                    .max(APPROACH_REACH)
            })
    }

    fn can_approach(
        &self,
        mover: MoverClass,
        start: Option<Cell>,
        target: Vec2,
        reach: f32,
    ) -> bool {
        let Some(start) = start else {
            return true;
        };
        (-1_i8..=1)
            .flat_map(|dx| (-1_i8..=1).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| dx != 0 || dy != 0)
            .any(|(dx, dy)| {
                let at = Vec2::new(
                    target.x + f32::from(dx) * reach,
                    target.y + f32::from(dy) * reach,
                );
                self.passability
                    .cell_at(at)
                    .is_some_and(|cell| self.reachable(mover, start, cell))
            })
    }

    pub(super) fn mood_attitude(&self, world: &World, ids: Ids, entity: Entity) -> i8 {
        let unit = Self::commanded_unit(world, ids, entity);
        let Some(owner) = Self::commanded_owner(world, ids, unit) else {
            return ATTITUDE_NORMAL;
        };
        if !self.computer_controlled(owner) {
            return ATTITUDE_NORMAL;
        }
        self.stored_attitude(world, ids, unit)
    }

    pub(super) fn stored_attitude(&self, world: &World, ids: Ids, entity: Entity) -> i8 {
        let (Some(rt), Some(engine)) = (
            world
                .store(ids.map_script_state)
                .and_then(|store| store.iter().next())
                .map(|(_, state)| &state.rt),
            self.map_scripts.as_ref(),
        ) else {
            return ATTITUDE_NORMAL;
        };
        rt.attitudes
            .effective(entity, &rt.teams, engine.team_prototypes())
    }

    pub(super) fn mood_asleep(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.mood_attitude(world, ids, entity) == ATTITUDE_SLEEP
    }

    pub(super) fn last_damager(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        living: &[(Foe, PlayerId)],
    ) -> Option<Foe> {
        let unit = Self::commanded_unit(world, ids, entity);
        let source = world.get(ids.last_attacker, unit)?.source;
        let at = living
            .binary_search_by_key(&source, |(foe, _)| foe.entity)
            .ok()?;
        let (foe, _) = living.get(at)?;
        self.object_hostile(world, entity, foe.entity)
            .then_some(*foe)
    }
}

pub(super) fn outside(high: Vec2, at: Vec2) -> bool {
    !(at.x > 0.0 && at.x < high.x && at.y > 0.0 && at.y < high.y)
}

pub(super) struct HereReach {
    vision: f32,
    horde: Option<(Option<f32>, bool)>,
    arms: Vec<(f32, f32, bool)>,
}

impl HereReach {
    pub(super) fn attackable(&self, at: Vec2, radius: f32, foe: &Foe, no_walk: bool) -> bool {
        let sighted = || in_range(at, radius, foe.position, foe.radius, 0.0, self.vision);
        if let Some((reach, melee)) = self.horde {
            let Some(reach) = reach else {
                return false;
            };
            if in_range(at, radius, foe.position, foe.radius, 0.0, reach) {
                return true;
            }
            return melee && !no_walk && sighted();
        }
        for &(minimum, reach, melee) in &self.arms {
            if in_range(at, radius, foe.position, foe.radius, minimum, reach) {
                return true;
            }
            if melee {
                if no_walk {
                    return false;
                }
                if sighted() {
                    return true;
                }
            }
        }
        false
    }
}
