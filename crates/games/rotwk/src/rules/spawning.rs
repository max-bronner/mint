use mint_movement::{Facing, MoverState, Route, Vec2};
use mint_sage_modules::{
    Effect, MasterSlaves, MasterView, NeutralControlled, SlaveView, SlavedTo, Slaves,
};
use mint_sim::module::OwnerChange;
use mint_sim::{Entity, PlayerId, Requester, Result, Tick, World};

use super::upgrades::ModuleWiring;
use super::{Ids, PathBudgets, RotwkRules};
use crate::combat::AttackOrder;
use crate::diag::diag;
use crate::economy::exit_point;
use crate::horde::HordeSpawn;
use crate::summons::Lifetime;
use crate::templates::{exit_rally, rotate};
use crate::unit::{Kind, Path, PathKind};

#[derive(Clone, Copy, Debug)]
pub(super) enum Placement {
    Standable(Vec2),
    Exact(Vec2),
}

impl RotwkRules {
    pub(super) fn spawn_for_module(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        (source, owner): (Entity, PlayerId),
        name: &str,
        requester: Requester,
        ordinal: u32,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(kind) = self.kinds.kind(name) else {
            diag!("spawn: {name} is not in this match's templates");
            return Ok(());
        };
        let Some(body) = tick.state().get(ids.body, source).map(|body| body.0) else {
            return Ok(());
        };
        if self.reclaim_orphan_spawn(tick, ids, wiring, source, owner, requester)? {
            return Ok(());
        }
        if tick.state().contains(wiring.spawn_point_state, source) {
            return self.exit_on_spawn_point(tick, ids, wiring, (source, owner), kind, requester);
        }
        let record = tick
            .state()
            .get(ids.kind, source)
            .and_then(|spawner| self.kinds.get(*spawner))
            .map(|entry| &entry.record);
        let rally = record.map_or(body.position, |record| {
            exit_rally(record, body.position, body.facing)
        });
        let (at, walk) = self.production_exit(tick, ids, source, body, rally, ordinal);
        let facing = self.production_exit_facing(tick, ids, source, body.facing);
        let Some(product) = self.spawn_kind_at(tick, ids, kind, at, facing, owner)? else {
            diag!("spawn: {name} not created for entity {}", source.index());
            return Ok(());
        };
        let now = tick.frame();
        wiring.adopt_created(tick.state_mut(), source, requester, product, now)?;
        self.route_out(tick, ids, product, source, walk, None)?;
        let slaved = self.enslave_if_slaved(tick, wiring, kind, source, product, false)?;
        diag!(
            "spawn: entity {} made {name} (entity {}{}) for player {} at ({:.0}, {:.0}), \
             {:.0} from it, exits to ({:.0}, {:.0}), frame {}",
            source.index(),
            product.index(),
            if slaved { ", slaved" } else { "" },
            owner.get(),
            at.x,
            at.y,
            at.minus(body.position).length(),
            walk.unwrap_or(rally).x,
            walk.unwrap_or(rally).y,
            now.get()
        );
        Ok(())
    }

    fn reclaim_orphan_spawn(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        source: Entity,
        owner: PlayerId,
        requester: Requester,
    ) -> Result<bool> {
        let Some(module) = wiring.reclaimable_spawner(tick.state(), source) else {
            return Ok(false);
        };
        let templates: Vec<Kind> = module
            .templates()
            .filter_map(|name| self.kinds.kind(name))
            .collect();
        if templates.is_empty() {
            return Ok(false);
        }
        let Some(orphan) = self.find_orphan(tick, ids, wiring, source, owner, &templates) else {
            return Ok(false);
        };
        let now = tick.frame();
        wiring.adopt_created(tick.state_mut(), source, requester, orphan, now)?;
        if let Some(orphan_kind) = tick.state().get(ids.kind, orphan).copied() {
            self.enslave_if_slaved(tick, wiring, orphan_kind, source, orphan, false)?;
        }
        diag!(
            "spawn: entity {} reclaims orphan entity {} rather than creating a new one, frame {}",
            source.index(),
            orphan.index(),
            now.get()
        );
        Ok(true)
    }

    fn find_orphan(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        source: Entity,
        owner: PlayerId,
        templates: &[Kind],
    ) -> Option<Entity> {
        let world = tick.state();
        let origin = world.get(ids.body, source)?.0.position;
        let store = world.store(ids.kind)?;
        let mut best: Option<(u32, u32, Entity)> = None;
        for (entity, held) in store.iter() {
            if !templates.contains(held)
                || !world.is_live(entity)
                || world.get(ids.owner, entity).map(|owner| owner.0) != Some(owner)
                || self.holds_condition_in(world, entity, "BURNINGDEATH")
            {
                continue;
            }
            if wiring.created_by(world, entity).is_some_and(|link| {
                world.is_live(link.creator) && !world.contains(ids.doomed, link.creator)
            }) {
                continue;
            }
            let Some(position) = world.get(ids.body, entity).map(|body| body.0.position) else {
                continue;
            };
            let distance = position.minus(origin).length_squared().to_bits();
            let key = (distance, entity.index());
            if best.is_none_or(|(bd, bi, _)| key < (bd, bi)) {
                best = Some((distance, entity.index(), entity));
            }
        }
        best.map(|(_, _, entity)| entity)
    }

    pub(super) fn production_exit(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        source: Entity,
        body: MoverState,
        rally: Vec2,
        ordinal: u32,
    ) -> (Vec2, Option<Vec2>) {
        let created = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.unit_create_point)
            .map(|local| rotate(body.position, body.facing, local));
        match created {
            Some(point) => (point, Some(rally)),
            None => (exit_point(rally, ordinal), None),
        }
    }

    pub(super) fn production_exit_facing(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        source: Entity,
        structure: Facing,
    ) -> Facing {
        let degrees = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.production_exit_angle)
            .filter(|degrees| degrees.is_finite())
            .unwrap_or(0.0);
        Facing::from_radians(structure.radians() + degrees.to_radians())
    }

    pub(super) fn production_exit_delay(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        source: Entity,
    ) -> mint_sim::LogicFrames {
        let exit_delay_ms = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.production_exit_delay);
        crate::templates::production_exit_delay(exit_delay_ms)
    }

    pub(super) fn route_out(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        product: Entity,
        source: Entity,
        walk: Option<Vec2>,
        then: Option<Vec2>,
    ) -> Result<()> {
        let points: Vec<Vec2> = [walk, then].into_iter().flatten().collect();
        let Some(&goal) = points.last() else {
            return Ok(());
        };
        if self.move_refused(tick, ids, product).is_some() {
            return Ok(());
        }
        let exit = Path {
            route: Route {
                points: points.clone(),
                extra_distance: 0.0,
                portals: Vec::new(),
            },
            goal,
            kind: PathKind::Ordered,
            facing: None,
        };
        tick.state_mut().insert(ids.path, product, exit)?;
        tick.state_mut().insert(
            ids.ignored_obstacle,
            product,
            super::IgnoredObstacle(source),
        )?;
        let Some(from) = tick.state().get(ids.body, product).map(|b| b.0.position) else {
            return Ok(());
        };
        self.move_allies_from_destination(tick, ids, product, None, from, &points, true)
    }

    fn exit_on_spawn_point(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        (source, owner): (Entity, PlayerId),
        kind: Kind,
        requester: Requester,
    ) -> Result<()> {
        let name = self
            .kinds
            .get(kind)
            .map_or("", |entry| entry.record.name.as_str());
        let world = tick.state();
        let (Some(body), Some(mut exit)) = (
            world.get(ids.body, source).map(|body| body.0),
            world
                .get(wiring.spawn_point_state, source)
                .and_then(|column| column.0.first())
                .map(|slot| slot.state.clone()),
        ) else {
            return Ok(());
        };
        let points = world
            .get(ids.kind, source)
            .and_then(|kind| self.spawn_points.get(usize::from(kind.0)))
            .map_or(&[][..], Vec::as_slice);
        let Some((point, perch)) = exit
            .free_point(points.len(), |occupier| world.is_live(occupier))
            .and_then(|point| points.get(point).map(|perch| (point, *perch)))
        else {
            diag!(
                "spawn: entity {} has no free {} spawn point for {name} ({} on its model)",
                source.index(),
                exit.bone_name(),
                points.len()
            );
            return Ok(());
        };
        let at = rotate(
            body.position,
            body.facing,
            [perch.position[0], perch.position[1]],
        );
        let facing = Facing::from_radians(body.facing.radians() + perch.facing.radians());
        let Some(product) =
            self.spawn_kind_placed(tick, ids, kind, Placement::Exact(at), facing, owner)?
        else {
            diag!("spawn: {name} not created for entity {}", source.index());
            return Ok(());
        };
        let height = perch.position[2];
        if height != 0.0 {
            tick.state_mut().insert(
                ids.ground_offset,
                product,
                crate::unit::GroundOffset(height),
            )?;
        }
        tick.state_mut()
            .insert(ids.held, product, super::Held(None))?;
        exit.occupy(point, product);
        if let Some(slot) = tick
            .state_mut()
            .get_mut(wiring.spawn_point_state, source)
            .and_then(|column| column.0.first_mut())
        {
            slot.state = exit;
        }
        let now = tick.frame();
        wiring.adopt_created(tick.state_mut(), source, requester, product, now)?;
        let slaved = self.enslave_if_slaved(tick, wiring, kind, source, product, false)?;
        diag!(
            "spawn: entity {} made {name} (entity {}{}) for player {} on spawn point {} of {} \
             at ({:.0}, {:.0}), height {:.0}, frame {}",
            source.index(),
            product.index(),
            if slaved { ", slaved" } else { "" },
            owner.get(),
            point.saturating_add(1),
            points.len(),
            at.x,
            at.y,
            height,
            now.get()
        );
        Ok(())
    }

    fn enslave_if_slaved(
        &self,
        tick: &mut Tick<'_>,
        wiring: &ModuleWiring,
        kind: Kind,
        source: Entity,
        product: Entity,
        destroy_when_sold: bool,
    ) -> Result<bool> {
        let slaved = self
            .kinds
            .get(kind)
            .is_some_and(|entry| mint_sage_modules::is_slaved(&entry.record));
        if slaved || destroy_when_sold {
            tick.state_mut().insert(
                wiring.slaved_to,
                product,
                SlavedTo {
                    master: source,
                    destroy_when_sold,
                },
            )?;
        }
        Ok(slaved)
    }

    pub(super) fn spawn_slave_at_offset(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        name: &str,
        offset: (f32, f32, f32),
        destroy_when_sold: bool,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(body) = tick.state().get(ids.body, source).map(|body| body.0) else {
            return Ok(());
        };
        let Some(owner) = tick.state().get(ids.owner, source).map(|owner| owner.0) else {
            return Ok(());
        };
        let Some(kind) = self.kinds.kind(name) else {
            diag!("object-creation: {name} is not in this match's templates");
            return Ok(());
        };
        let (dx, dy, dz) = offset;
        let at = self.offset_in_frame(tick.state(), ids, source, body.position, (dx, dy));
        let Some(product) =
            self.spawn_kind_placed(tick, ids, kind, Placement::Exact(at), body.facing, owner)?
        else {
            diag!(
                "object-creation: {name} not created for entity {}",
                source.index()
            );
            return Ok(());
        };
        if dz != 0.0 {
            tick.state_mut()
                .insert(ids.ground_offset, product, crate::unit::GroundOffset(dz))?;
        }
        let slaved =
            self.enslave_if_slaved(tick, wiring, kind, source, product, destroy_when_sold)?;
        diag!(
            "object-creation: entity {} spawned {name} (entity {}{}) for player {} at ({:.0}, \
             {:.0}), height {:.0}",
            source.index(),
            product.index(),
            if slaved { ", slaved" } else { "" },
            owner.get(),
            at.x,
            at.y,
            dz
        );
        Ok(())
    }

    pub(super) fn slaves_to_destroy_with(
        &self,
        world: &World,
        ids: Ids,
        master: Entity,
    ) -> Vec<Entity> {
        let Some(wiring) = self.modules.as_ref() else {
            return Vec::new();
        };
        let Some(store) = world.store(wiring.slaved_to) else {
            return Vec::new();
        };
        store
            .iter()
            .filter(|(slave, link)| {
                link.master == master
                    && link.destroy_when_sold
                    && world.is_live(*slave)
                    && !world.contains(ids.doomed, *slave)
            })
            .map(|(slave, _)| slave)
            .collect()
    }

    pub(super) fn order_slaves_to_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        master: Entity,
        target: Entity,
    ) -> Result<usize> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(0);
        };
        let Some(store) = tick.state().store(wiring.slaved_to) else {
            return Ok(0);
        };
        let mut slaves: Vec<Entity> = store
            .iter()
            .filter(|(slave, link)| {
                link.master == master
                    && tick.state().is_live(*slave)
                    && !tick.state().contains(ids.doomed, *slave)
                    && tick.state().contains(ids.armed, *slave)
            })
            .map(|(slave, _)| slave)
            .collect();
        slaves.sort_unstable_by_key(|slave| slave.index());
        for slave in &slaves {
            if tick.state().get(ids.attack_order, *slave).map(|o| o.target) == Some(target) {
                continue;
            }
            tick.state_mut().remove(ids.player_order, *slave);
            tick.state_mut().insert(
                ids.attack_order,
                *slave,
                AttackOrder {
                    target,
                    planned: false,
                    horde: None,
                    guard: false,
                    relation_free: false,
                    order_owner: None,
                },
            )?;
        }
        Ok(slaves.len())
    }

    pub(super) fn neutral_spawners(&self, world: &World, ids: Ids) -> NeutralControlled {
        let (Some(wiring), Some(neutral)) = (self.modules.as_ref(), self.neutral_player()) else {
            return NeutralControlled::default();
        };
        NeutralControlled::from_entities(
            wiring
                .spawners(world)
                .into_iter()
                .filter(|spawner| world.get(ids.owner, *spawner).map(|o| o.0) == Some(neutral)),
        )
    }

    pub(super) fn slave_views(&self, world: &World, ids: Ids) -> Slaves {
        let Some(wiring) = self.modules.as_ref() else {
            return Slaves::default();
        };
        let Some(store) = world.store(wiring.slaved_to) else {
            return Slaves::default();
        };
        let alive = |entity: Entity| {
            world.is_live(entity)
                && !world.contains(ids.doomed, entity)
                && world
                    .get(ids.health, entity)
                    .is_none_or(|health| !health.0.is_dead())
        };
        let at = |entity: Entity| {
            world
                .get(ids.body, entity)
                .map(|body| (body.0.position.x, body.0.position.y))
        };
        let victim_of = |entity: Entity| {
            world
                .get(ids.attack_order, entity)
                .map(|order| order.target)
                .or_else(|| world.get(ids.armed, entity).and_then(|armed| armed.target))
        };
        Slaves::from_pairs(store.iter().filter_map(|(slave, link)| {
            if !alive(slave) {
                return None;
            }
            let path = world.get(ids.path, slave);
            let master = Some(link.master).filter(|m| alive(*m)).and_then(|m| {
                let body = world.get(ids.body, m)?;
                let owner = world.get(ids.owner, m)?.0;
                let victim = victim_of(m)
                    .filter(|v| alive(*v))
                    .and_then(|v| Some((v, at(v)?)));
                Some(MasterView {
                    at: (body.0.position.x, body.0.position.y),
                    facing: body.0.facing.radians(),
                    victim,
                    owner,
                })
            });
            let allied = master.is_some() && self.object_allied(world, link.master, slave);
            Some((
                slave,
                SlaveView {
                    at: at(slave)?,
                    idle: path.is_none() && victim_of(slave).is_none(),
                    ordered_goal: path
                        .filter(|path| path.kind == PathKind::Ordered)
                        .map(|path| (path.goal.x, path.goal.y)),
                    master,
                    allied,
                },
            ))
        }))
    }

    pub(super) fn master_slaves(&self, world: &World, ids: Ids) -> MasterSlaves {
        let Some(wiring) = self.modules.as_ref() else {
            return MasterSlaves::default();
        };
        let Some(store) = world.store(wiring.slaved_to) else {
            return MasterSlaves::default();
        };
        let alive = |entity: Entity| {
            world.is_live(entity)
                && !world.contains(ids.doomed, entity)
                && world
                    .get(ids.health, entity)
                    .is_none_or(|health| !health.0.is_dead())
        };
        MasterSlaves::from_pairs(
            store
                .iter()
                .filter(|(slave, _)| alive(*slave))
                .map(|(slave, link)| (link.master, slave)),
        )
    }

    pub(super) fn share_upgrades_with_slaves(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        master: Entity,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let held = tick
            .state()
            .get(ids.object_upgrades, master)
            .map(|held| held.0.clone())
            .unwrap_or_default();
        if held.is_empty() {
            return Ok(());
        }
        let world = tick.state();
        let Some(store) = world.store(wiring.slaved_to) else {
            return Ok(());
        };
        let mut slaves: Vec<Entity> = store
            .iter()
            .filter(|(slave, link)| {
                link.master == master
                    && world.is_live(*slave)
                    && !world.contains(ids.doomed, *slave)
            })
            .map(|(slave, _)| slave)
            .collect();
        slaves.sort_unstable_by_key(|slave| slave.index());
        if !slaves.is_empty() {
            diag!(
                "spawn: entity {} shares its upgrades with {} slave(s)",
                master.index(),
                slaves.len()
            );
        }
        for slave in slaves {
            self.grant_object_upgrades(tick, ids, slave, &held)?;
        }
        Ok(())
    }

    pub(super) fn apply_slave_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        effect: &Effect,
        budgets: &mut PathBudgets,
    ) -> Result<bool> {
        match effect {
            Effect::AiMoveTo { at, acquires } => {
                let mover = self.approach_mover(tick, ids, entity);
                let Some(from) = tick.state().get(ids.body, mover).map(|b| b.0.position) else {
                    return Ok(true);
                };
                let to = Vec2::new(at.0, at.1);
                if std::env::var_os("MINT_ROTWK_SLAVED_DEBUG").is_some() {
                    self.log_slave_walk(tick, ids, entity, from, to);
                }
                tick.state_mut().remove(ids.attack_order, entity);
                tick.state_mut().remove(ids.attack_move_order, entity);
                tick.state_mut().remove(ids.player_order, entity);
                tick.state_mut().remove(ids.path, mover);
                let kind = if *acquires {
                    PathKind::Ability
                } else {
                    PathKind::Ordered
                };
                self.close_on_target(tick, ids, mover, from, to, 0.0, kind, budgets)?;
            }
            Effect::AiAttack { target } => {
                let current = tick.state().get(ids.attack_order, entity).map(|o| o.target);
                if current != Some(*target) {
                    tick.state_mut().remove(ids.player_order, entity);
                    tick.state_mut().insert(
                        ids.attack_order,
                        entity,
                        AttackOrder {
                            target: *target,
                            planned: false,
                            horde: None,
                            guard: false,
                            relation_free: false,
                            order_owner: None,
                        },
                    )?;
                }
            }
            Effect::Die => {
                if !tick.state().contains(ids.doomed, entity) {
                    let dies_at = tick.frame();
                    tick.state_mut().insert(
                        ids.lifetime,
                        entity,
                        Lifetime {
                            dies_at,
                            death: crate::combat::DeathNaming::Normal,
                            set_at: None,
                        },
                    )?;
                }
            }
            Effect::JoinMaster { to } => {
                self.change_owner_for(tick, ids, entity, *to, OwnerChange::Capture)?;
                self.release_slave(tick, entity);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn log_slaves(&self, world: &World, ids: Ids, now: mint_sim::Frame) {
        if now.get() % 30 != 0 || std::env::var_os("MINT_ROTWK_SLAVED_DEBUG").is_none() {
            return;
        }
        let Some(store) = self.modules.as_ref().and_then(|w| world.store(w.slaved_to)) else {
            return;
        };
        for (slave, link) in store.iter() {
            let (Some(me), Some(master)) =
                (world.get(ids.body, slave), world.get(ids.body, link.master))
            else {
                continue;
            };
            if world.contains(ids.doomed, slave) {
                continue;
            }
            let target = world
                .get(ids.attack_order, slave)
                .map(|order| order.target)
                .or_else(|| world.get(ids.armed, slave).and_then(|armed| armed.target));
            let doing = match (target, world.get(ids.path, slave)) {
                (Some(target), _) => format!("fighting entity {}", target.index()),
                (None, Some(path)) => format!("walking ({:?})", path.kind),
                (None, None) => String::from("idle"),
            };
            println!(
                "slaves: frame {} entity {} {:.0} from its master (entity {}), {doing}",
                now.get(),
                slave.index(),
                me.0.position.minus(master.0.position).length(),
                link.master.index()
            );
        }
    }

    fn log_slave_walk(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, from: Vec2, to: Vec2) {
        let master = self
            .modules
            .as_ref()
            .and_then(|wiring| tick.state().get(wiring.slaved_to, entity))
            .and_then(|link| tick.state().get(ids.body, link.master))
            .map(|body| body.0.position);
        let span = |p: Vec2| master.map_or(-1.0, |m| m.minus(p).length());
        println!(
            "slaved: frame {} entity {} at ({:.0}, {:.0}), {:.0} from its master, walks to \
             ({:.0}, {:.0}), {:.0} from it",
            tick.frame().get(),
            entity.index(),
            from.x,
            from.y,
            span(from),
            to.x,
            to.y,
            span(to)
        );
    }

    pub(super) fn clear_trees_under(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        object: Entity,
        position: Vec2,
    ) -> Result<()> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(());
        };
        let radius = tick
            .state()
            .get(ids.shape, object)
            .map_or(0.0, |shape| shape.radius);
        let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
            return Ok(());
        };
        let cleared = forest.clear_within(position, radius);
        if cleared > 0 {
            diag!("forest: {cleared} tree(s) cleared for a new object");
        }
        Ok(())
    }

    pub(super) fn neutral_player(&self) -> Option<PlayerId> {
        let at = self.map_players.iter().position(String::is_empty)?;
        let at = u8::try_from(at).ok()?;
        self.players.checked_add(at).map(PlayerId::new)
    }

    pub(super) fn resolve_build_variation(&self, tick: &mut Tick<'_>, kind: Kind) -> Kind {
        let count = self.kinds.variant_count(kind);
        let Ok(count) = u32::try_from(count) else {
            return kind;
        };
        if count == 0 {
            return kind;
        }
        let roll = tick.rng().below(count);
        self.kinds.resolve_build_variation(kind, roll)
    }

    pub(super) fn spawn_kind_at(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: Kind,
        position: Vec2,
        facing: Facing,
        owner: PlayerId,
    ) -> Result<Option<Entity>> {
        self.spawn_kind_placed(
            tick,
            ids,
            kind,
            Placement::Standable(position),
            facing,
            owner,
        )
    }

    pub(super) fn spawn_kind_placed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: Kind,
        place: Placement,
        facing: Facing,
        owner: PlayerId,
    ) -> Result<Option<Entity>> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(None);
        };
        let position = match place {
            Placement::Exact(position) => position,
            Placement::Standable(position)
                if entry.record.has_kind("STRUCTURE") || entry.record.has_kind("IMMOBILE") =>
            {
                position
            }
            Placement::Standable(position) => {
                let here = self.passability.cell_at(position);
                match self.passable_cell(tick, ids, position) {
                    Some(cell) if Some(cell) != here => self.passability.geometry.centre_of(cell),
                    _ => position,
                }
            }
        };
        if let (Some(formation), Some(member)) = (entry.formation, entry.member) {
            let Some(member_spawn) =
                self.kinds
                    .spawn(&self.combat, member, position, facing, owner)
            else {
                return Ok(None);
            };
            let count = self
                .formations
                .get(usize::from(formation))
                .map_or(0, |table| u32::try_from(table.len()).unwrap_or(0));
            let horde = self.spawn_horde_entity(
                tick,
                ids,
                &HordeSpawn {
                    formation,
                    origin: position,
                    facing,
                    member: member_spawn,
                    count,
                    vision: entry
                        .record
                        .shroud_clearing_range
                        .or(entry.record.vision_range)
                        .filter(|range| (0.0..=2_000.0).contains(range))
                        .unwrap_or(crate::shroud::HORDE_SHROUD_RANGE),
                },
            )?;
            if let Some(horde) = horde {
                tick.state_mut().insert(ids.kind, horde, kind)?;
                self.attach_upgrade_state(tick, ids, horde, Some(kind))?;
            }
            return Ok(horde);
        }
        let kind = self.resolve_build_variation(tick, kind);
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(None);
        };
        let Some(spawn) = self
            .kinds
            .spawn(&self.combat, kind, position, facing, owner)
        else {
            return Ok(None);
        };
        let spawn = crate::unit::Spawn {
            upkeep: self.kinds.upkeep(kind),
            producer: self
                .kinds
                .producer(&self.production, kind, position, facing),
            ..spawn
        };
        let entity = tick.state_mut().spawn()?;
        Self::insert_spawn(tick, ids, entity, &spawn, &self.combat)?;
        self.attach_upgrade_state(tick, ids, entity, Some(kind))?;
        let records = crate::construction::footprint_of(&entry.record);
        if entry.record.has_kind("STRUCTURE") && !records.is_empty() {
            self.stamp_footprint(tick, ids, entity, position, facing.radians(), records)?;
        }
        Ok(Some(entity))
    }
}
