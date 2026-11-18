use mint_math::f32::atan2;
use mint_movement::Passability as _;
use mint_sage_modules::WallHubBehavior;
use mint_sage_passability::{Obstacle, BOUNDARY, CLIFF};
use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::construction::footprint_of;
use crate::diag::diag;
use mint_movement::Vec2;

const MAX_PIECES: usize = 128;

const QUARTER_TURN: f32 = core::f32::consts::FRAC_PI_2;

pub(super) const MAX_BUILDOUT_UNSET: f32 = 54321.0;

const HUB_SNAP_RADIUS: f32 = 20.0;

const HUB_SNAP_MIN_RADIUS_FACTOR: f32 = 0.666;

const SPOT_FRACTION: f32 = 0.25;

const STANDING_ON_SLOT: f32 = 1.1;

const STANDING_RADIUS_FACTOR: f32 = 1.1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpanPiece {
    pub(crate) kind: crate::unit::Kind,
    pub(crate) centre: Vec2,
    pub(crate) angle: f32,
    pub(crate) code: u8,
    pub(crate) rubble: Option<Entity>,
}

#[derive(Debug, Clone)]
pub(crate) struct SpanPlan {
    pub(crate) hub: Entity,
    pub(crate) step: f32,
    pub(crate) stagger: u32,
    pub(crate) pieces: Vec<SpanPiece>,
    pub(crate) target: Option<Entity>,
    pub(crate) rebuild: bool,
    pub(crate) money_short: bool,
}

impl SpanPlan {
    pub(crate) fn code(&self) -> u8 {
        if self.rebuild {
            return if self.money_short { 7 } else { 10 };
        }
        let highest = self
            .pieces
            .iter()
            .map(|piece| piece.code)
            .max()
            .unwrap_or(0);
        if self.money_short {
            highest.max(7)
        } else {
            highest
        }
    }

    pub(crate) fn drawn(&self) -> &[SpanPiece] {
        if self.target.is_some() && !self.rebuild {
            self.pieces.split_last().map_or(&[], |(_, rest)| rest)
        } else {
            &self.pieces
        }
    }
}

impl RotwkRules {
    fn span_step(&self, kind: crate::unit::Kind) -> Option<f32> {
        let lateral = self.geometry_radius(kind)?;
        let step = (lateral - 1.0) * 2.0;
        (step > 0.0).then_some(step)
    }

    fn geometry_radius(&self, kind: crate::unit::Kind) -> Option<f32> {
        let geometry = &self.kinds.get(kind)?.record.geometry;
        geometry
            .minor_radius
            .or(geometry.major_radius)
            .or(geometry.radius)
    }

    fn wall_hub_for_option<'a>(
        &self,
        world: &'a World,
        entity: Entity,
        option: u8,
    ) -> Option<&'a WallHubBehavior> {
        let slots = self.modules.as_ref()?.wall_hubs(world, entity);
        slots
            .iter()
            .find(|slot| slot.state.option().index() == option)
            .or_else(|| slots.get(usize::from(option)))
            .map(|slot| &slot.state)
    }

    pub(crate) fn span_plan(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        option: u8,
        x: f32,
        y: f32,
    ) -> Option<SpanPlan> {
        let (hub, module) = self.selected_wall_hub(world, ids, player, option)?;
        let hub_position = world.get(ids.body, hub).map(|body| body.0.position)?;
        diag!(
            "wall: hub {} at ({:.0}, {:.0}) answers option {option}",
            hub.index(),
            hub_position.x,
            hub_position.y
        );
        let cursor = Vec2::new(x, y);
        let hub_cap_radius = module
            .hub_cap()
            .and_then(|name| self.kinds.kind(name))
            .and_then(|kind| self.geometry_radius(kind));
        let snap_hub = hub_cap_radius.and_then(|radius| {
            self.own_wall_hub_near(
                world,
                ids,
                player,
                cursor,
                hub,
                HUB_SNAP_RADIUS,
                radius * HUB_SNAP_MIN_RADIUS_FACTOR,
            )
        });
        let end = snap_hub
            .and_then(|entity| world.get(ids.body, entity).map(|body| body.0.position))
            .unwrap_or(cursor);
        let aim = end.minus(hub_position);
        let aim_length = aim.length();
        if !aim_length.is_finite() || aim_length <= 0.0 {
            diag!("wall: hub and drag end coincide - no span");
            return None;
        }
        let aim_direction = aim.scale(1.0 / aim_length);
        let builder_radius = self.resolved_builder_radius(world, ids, hub, &module);
        let origin = hub_position.plus(aim_direction.scale(builder_radius));
        let to_end = end.minus(origin);
        let length = to_end.length();
        let first = module
            .segment_at(0)
            .and_then(|name| self.kinds.kind(name))?;
        let step = self.span_step(first)?;
        if !length.is_finite() || length <= 0.0 || length < step {
            diag!("wall: drag of {length:.0} is shorter than one piece ({step:.0}) - no span");
            return None;
        }
        let direction = to_end.scale(1.0 / length);
        let angle = atan2(direction.y, direction.x) - QUARTER_TURN;
        let max_distance = module.max_buildout_distance();
        let unlimited = max_distance <= 0.0 || max_distance == MAX_BUILDOUT_UNSET;
        let centres = self.command_centers_of(world, ids, player);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = ((length / step) as usize).min(MAX_PIECES);
        let mut pieces: Vec<SpanPiece> = Vec::with_capacity(count);
        for index in 1..=count {
            let name = if index == count && snap_hub.is_none() {
                module.hub_cap().or_else(|| module.segment_at(index - 1))
            } else {
                module.segment_at(index - 1)
            };
            let Some(kind) = name.and_then(|name| self.kinds.kind(name)) else {
                break;
            };
            #[allow(clippy::cast_precision_loss)]
            let centre = origin.plus(direction.scale(step * (index as f32 - 0.5)));
            let Some(piece_entry) = self.kinds.get(kind) else {
                break;
            };
            let footprint = footprint_of(&piece_entry.record);
            if footprint.is_empty() {
                break;
            }
            if self.terrain_restricts(&footprint, centre, angle) {
                let Some(cliff) = module.cliff_cap().and_then(|name| self.kinds.kind(name)) else {
                    diag!("wall: piece {index} on uneven ground - no CliffCapTemplateName, span ends short");
                    break;
                };
                if let (Some(last), Some(hub_cap)) = (
                    pieces.last_mut(),
                    module.hub_cap().and_then(|name| self.kinds.kind(name)),
                ) {
                    last.kind = hub_cap;
                }
                pieces.push(SpanPiece {
                    kind: cliff,
                    centre,
                    angle,
                    code: 0,
                    rubble: None,
                });
                diag!(
                    "wall: piece {index} on uneven ground at ({:.0}, {:.0}) - cliff cap, span ends there",
                    centre.x,
                    centre.y
                );
                break;
            }
            let too_far = !unlimited
                && !centres
                    .iter()
                    .any(|centre_of| centre_of.minus(centre).length() <= max_distance);
            let legality = self.validate_placement_ignoring(
                world,
                ids,
                player,
                &piece_entry.record,
                &footprint,
                centre,
                angle,
                origin,
                &[hub],
            );
            let code = match legality {
                _ if too_far => 9,
                Err(reason) => {
                    diag!(
                        "wall: piece {index} refused at ({:.0}, {:.0}) - {}",
                        centre.x,
                        centre.y,
                        reason.message()
                    );
                    reason.code()
                }
                Ok(()) => 0,
            };
            pieces.push(SpanPiece {
                kind,
                centre,
                angle,
                code,
                rubble: None,
            });
        }
        if pieces.is_empty() {
            return None;
        }
        if let Some(target) = snap_hub {
            let blocked_early = pieces
                .split_last()
                .is_some_and(|(_, rest)| rest.iter().any(|piece| piece.code == 8));
            if blocked_early && self.hubs_linked(world, ids, hub, target) {
                return self.rebuild_plan(world, ids, player, hub, &module, target, false);
            }
        }
        let money_short = self.mark_unaffordable(world, ids, player, &mut pieces);
        if snap_hub.is_some() {
            if let Some(last) = pieces.last_mut() {
                last.code = 0;
            }
        }
        Some(SpanPlan {
            hub,
            step,
            stagger: stagger_frames(module.staggered_build_factor()),
            pieces,
            target: snap_hub,
            rebuild: false,
            money_short,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn rebuild_plan(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        hub: Entity,
        module: &WallHubBehavior,
        target: Entity,
        reversed: bool,
    ) -> Option<SpanPlan> {
        let (start, end) = if reversed {
            (target, hub)
        } else {
            (hub, target)
        };
        let from = world.get(ids.body, start).map(|body| body.0.position)?;
        let to = world.get(ids.body, end).map(|body| body.0.position)?;
        let aim = to.minus(from);
        let total = aim.length();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let direction = aim.scale(1.0 / total);
        let first = module
            .segment_at(0)
            .and_then(|name| self.kinds.kind(name))?;
        let step = self.span_step(first)?;
        let half = step * 0.5;
        let radius = self.resolved_builder_radius(world, ids, start, module);
        let angle = atan2(direction.y, direction.x) - QUARTER_TURN;
        let bodies = world.store(ids.body)?;
        let mut pieces: Vec<SpanPiece> = Vec::new();
        for slot in 1..=MAX_PIECES {
            #[allow(clippy::cast_precision_loss)]
            let reach = step * slot as f32;
            if radius + reach > total {
                break;
            }
            let centre = from.plus(direction.scale(radius + reach - half));
            let Some(kind) = module
                .segment_at(pieces.len())
                .and_then(|name| self.kinds.kind(name))
            else {
                break;
            };
            let nearest = bodies
                .iter()
                .map(|(entity, body)| (entity, body.0.position.minus(centre).length()))
                .filter(|(_, distance)| *distance <= SPOT_FRACTION * half)
                .min_by(|(_, a), (_, b)| a.total_cmp(b));
            let mut piece = SpanPiece {
                kind,
                centre,
                angle,
                code: 0,
                rubble: None,
            };
            if let Some((object, distance)) = nearest {
                let object_kind = world.get(ids.kind, object).copied();
                let equivalent =
                    object_kind.is_some_and(|other| self.kinds.equivalent(other, kind));
                if self.is_rubble(world, ids, object) {
                    if equivalent {
                        piece.code = 10;
                        piece.rubble = Some(object);
                    }
                } else if distance <= STANDING_ON_SLOT {
                    continue;
                } else if equivalent
                    && object_kind
                        .and_then(|other| self.geometry_radius(other))
                        .is_some_and(|extent| extent <= STANDING_RADIUS_FACTOR * half)
                {
                    if reversed {
                        return None;
                    }
                    return self.rebuild_plan(world, ids, player, hub, module, target, true);
                } else {
                    continue;
                }
            }
            pieces.push(piece);
        }
        if pieces.is_empty() {
            return None;
        }
        let money_short = self.mark_unaffordable(world, ids, player, &mut pieces);
        Some(SpanPlan {
            hub,
            step,
            stagger: stagger_frames(module.staggered_build_factor()),
            pieces,
            target: Some(target),
            rebuild: true,
            money_short,
        })
    }

    fn terrain_restricts(
        &self,
        footprint: &[mint_sage_passability::FootprintRecord],
        centre: Vec2,
        angle: f32,
    ) -> bool {
        if self.passability.cell_at(centre).is_none() {
            return true;
        }
        let obstacle = Obstacle {
            id: 0,
            centre,
            angle,
            records: footprint.to_vec(),
        };
        let cells = mint_sage_passability::covered_cells(
            self.passability.geometry,
            self.passability.grid.extent(),
            &obstacle,
        );
        if cells.is_empty() {
            return true;
        }
        let terrain = CLIFF.union(BOUNDARY);
        let view = self.static_view();
        cells
            .iter()
            .any(|cell| view.flags(*cell).intersects(terrain))
    }

    pub(super) fn command_centers_of(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> Vec<Vec2> {
        let Some(store) = world.store(ids.kind) else {
            return Vec::new();
        };
        store
            .iter()
            .filter(|(entity, _)| {
                world.get(ids.owner, *entity).map(|owner| owner.0) == Some(player)
            })
            .filter(|(_, kind)| {
                self.kinds
                    .get(**kind)
                    .is_some_and(|entry| entry.record.has_kind("COMMANDCENTER"))
            })
            .filter_map(|(entity, _)| world.get(ids.body, entity).map(|body| body.0.position))
            .collect()
    }

    fn own_wall_hub_near(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        point: Vec2,
        exclude: Entity,
        radius: f32,
        min_radius: f32,
    ) -> Option<Entity> {
        let store = world.store(ids.kind)?;
        store
            .iter()
            .filter_map(|(entity, kind)| {
                if entity == exclude {
                    return None;
                }
                if world.get(ids.owner, entity).map(|owner| owner.0) != Some(player) {
                    return None;
                }
                if !self
                    .kinds
                    .get(*kind)
                    .is_some_and(|entry| entry.record.has_kind("WALL_HUB"))
                {
                    return None;
                }
                if self.geometry_radius(*kind)? < min_radius {
                    return None;
                }
                let body = world.get(ids.body, entity)?;
                let distance = body.0.position.minus(point).length();
                (distance <= radius).then_some((entity, distance))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(entity, _)| entity)
    }

    fn resolved_builder_radius(
        &self,
        world: &World,
        ids: Ids,
        hub: Entity,
        module: &WallHubBehavior,
    ) -> f32 {
        let radius = module.builder_radius();
        if (radius - mint_sage_modules::BUILDER_RADIUS_USES_HUB).abs() < 0.001 {
            world
                .get(ids.kind, hub)
                .and_then(|kind| self.geometry_radius(*kind))
                .unwrap_or(0.0)
        } else {
            radius
        }
    }

    fn mark_unaffordable(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        pieces: &mut [SpanPiece],
    ) -> bool {
        let economy = Self::economy_ids(ids);
        let purse = crate::economy::player_entity(world, economy, player)
            .and_then(|player_entity| world.get(ids.purse, player_entity))
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        let mut total = 0i64;
        let mut short = false;
        for piece in pieces {
            let cost = self
                .kinds
                .get(piece.kind)
                .and_then(|entry| self.price_for(world, ids, player, &entry.record))
                .map_or(i64::MAX, |(cost, _)| cost);
            total = total.saturating_add(cost);
            if !purse.can_afford(total) {
                piece.code = 7;
                short = true;
            }
        }
        short
    }

    fn selected_wall_hub(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        option: u8,
    ) -> Option<(Entity, WallHubBehavior)> {
        world
            .store(ids.selected)?
            .entities()
            .iter()
            .copied()
            .find_map(|entity| {
                let owned = world
                    .get(ids.owner, entity)
                    .is_some_and(|owner| owner.0 == player);
                if !owned || !world.is_live(entity) {
                    return None;
                }
                self.wall_hub_for_option(world, entity, option)
                    .cloned()
                    .map(|module| (entity, module))
            })
    }

    pub(super) fn wall_span(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        option: u8,
        x: f32,
        y: f32,
    ) -> Result<()> {
        let Some(plan) = self.span_plan(tick.state(), ids, player, option, x, y) else {
            diag!("wall: no span from the selection for option {option} towards ({x:.0}, {y:.0})");
            return Ok(());
        };
        let code = plan.code();
        if code != 0 && code != 10 {
            diag!("wall: span towards ({x:.0}, {y:.0}) refused, code {code}");
            return Ok(());
        }
        if let Some(hub) = tick
            .state()
            .get(ids.body, plan.hub)
            .map(|body| body.0.position)
        {
            self.push_misc_sound(tick, ids, super::MiscSound::WallPlacement, player, hub);
        }
        let count = plan.pieces.len();
        let mut placed = 0usize;
        let mut created: Vec<Entity> = Vec::new();
        for (index, piece) in plan.pieces.iter().enumerate() {
            let delay = if plan.rebuild {
                0
            } else {
                u32::try_from(index)
                    .unwrap_or(0)
                    .saturating_mul(plan.stagger)
            };
            if let Some(rubble) = piece.rubble {
                if !self.revive_span_piece(tick, ids, player, piece.kind, rubble)? {
                    break;
                }
            } else {
                let Some(site) = self.place_span_piece(
                    tick,
                    ids,
                    player,
                    piece.kind,
                    piece.centre,
                    piece.angle,
                    delay,
                )?
                else {
                    break;
                };
                let mut links = tick
                    .state()
                    .get(ids.wall_links, site)
                    .cloned()
                    .unwrap_or_default();
                links.producer = Some(plan.hub);
                tick.state_mut().insert(ids.wall_links, site, links)?;
                created.push(site);
            }
            placed = placed.saturating_add(1);
        }
        self.write_span_links(tick, ids, plan.hub, plan.target, &created)?;
        diag!(
            "wall: hub {} laid {placed} piece(s) of {count} at {:.0} apart, option {option},              stagger {} frame(s)",
            plan.hub.index(),
            plan.step,
            plan.stagger
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn place_span_piece(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        centre: Vec2,
        angle: f32,
        delay: u32,
    ) -> Result<Option<Entity>> {
        if !self.charge_span_piece(tick, ids, player, kind)? {
            return Ok(None);
        }
        self.place_site(tick, ids, player, kind, centre, angle, None, delay)
    }

    fn revive_span_piece(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        rubble: Entity,
    ) -> Result<bool> {
        let revive = self.self_repair(tick.state(), ids, player, rubble, tick.frame())
            == super::rebuild::SelfRepair::Rebuild
            && {
                let cost = self.self_repair_cost(tick.state(), ids, player, rubble);
                self.charge_structure_cost(tick, ids, player, cost)?
            };
        if !self.charge_span_piece(tick, ids, player, kind)? {
            return Ok(false);
        }
        if !revive {
            return Ok(true);
        }
        let Some(record) = self
            .kinds
            .get(kind)
            .map(|entry| entry.record.clone())
            .filter(|record| record.rebuild_seconds.is_some())
        else {
            return Ok(true);
        };
        let seconds = record.rebuild_seconds.unwrap_or(0.0);
        let frames = mint_sim::LogicFrames::from_seconds(seconds).get().max(1);
        self.rebuild_rubble(tick, ids, rubble, &record, player, frames)?;
        Ok(true)
    }

    fn charge_span_piece(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> Result<bool> {
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return Ok(false);
        };
        let Some((cost, _)) = self.price_for(tick.state(), ids, player, record) else {
            return Ok(false);
        };
        let name = record.name.clone();
        let paid = self.charge_structure_cost(tick, ids, player, cost)?;
        if !paid {
            diag!("wall: {name} refused - no money");
        }
        Ok(paid)
    }
}

fn stagger_frames(factor: f32) -> u32 {
    if !factor.is_finite() || factor <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = factor as u32;
    frames.min(600)
}
