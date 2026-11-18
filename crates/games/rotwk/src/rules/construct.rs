use super::*;
use crate::diag::diag;

const WORKER_SPOT_RANGE: f32 = 100.0;
const WORKER_SPOT_RISE: f32 = 10.0;

const DEPLOYED_REACH: f32 = 1.5;

fn wall_hub_max_buildout_distance(record: &mint_sage_modules::ObjectRecord) -> Option<f32> {
    let distance = record
        .declared_modules
        .iter()
        .find(|module| module.name.eq_ignore_ascii_case("WallHubBehavior"))?
        .field("MaxBuildoutDistance")?
        .numeric?;
    (distance > 0.0 && distance != super::wall::MAX_BUILDOUT_UNSET).then_some(distance)
}

impl RotwkRules {
    pub(super) fn is_disallowed(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> bool {
        let economy = Self::economy_ids(ids);
        crate::economy::player_entity(world, economy, player)
            .and_then(|entity| world.get(ids.disallowed_buildings, entity))
            .is_some_and(|set| set.0.contains(&kind))
            || self.buildable_status_refuses(world, ids, player, kind)
    }

    pub(crate) fn allowed_to_build(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        is_structure: bool,
    ) -> bool {
        if self.is_disallowed(world, ids, player, kind) {
            return false;
        }
        let economy = Self::economy_ids(ids);
        crate::economy::player_entity(world, economy, player)
            .and_then(|entity| world.get(ids.build_switches, entity))
            .is_none_or(|switches| {
                if is_structure {
                    switches.can_build_base
                } else {
                    switches.can_build_units
                }
            })
    }

    pub(super) fn construct(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        x: f32,
        y: f32,
        angle: f32,
    ) -> Result<()> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(());
        };
        let record = &entry.record;
        if self.is_disallowed(tick.state(), ids, player, kind) {
            diag!(
                "construct: {} - {}",
                record.name,
                Reason::Disallowed.message()
            );
            return Ok(());
        }
        let Some((cost, frames)) = self.price_for(tick.state(), ids, player, record) else {
            diag!(
                "construct: {} has no usable BuildCost/BuildTime - refused",
                record.name
            );
            return Ok(());
        };
        let footprint = footprint_of(record);
        if footprint.is_empty() {
            diag!(
                "construct: {} - {}",
                record.name,
                Reason::NoFootprint.message()
            );
            return Ok(());
        }
        let centre = Vec2::new(x, y);
        let economy = Self::economy_ids(ids);
        for porter in self.selection(tick, ids, player) {
            let owned = tick
                .state()
                .get(ids.owner, porter)
                .is_some_and(|owner| owner.0 == player);
            if !owned || !tick.state().is_live(porter) {
                continue;
            }
            let is_dozer = tick
                .state()
                .get(ids.shape, porter)
                .is_some_and(|shape| shape.kind_of.contains(KindOf::DOZER));
            if !is_dozer {
                continue;
            }
            let offered = tick
                .state()
                .get(ids.kind, porter)
                .copied()
                .and_then(|porter_kind| {
                    self.construct_menu_of
                        .get(usize::from(porter_kind.0))
                        .copied()
                        .flatten()
                })
                .and_then(|menu| self.construct_menus.get(usize::from(menu)))
                .is_some_and(|menu| menu.contains(&kind));
            if !offered {
                diag!(
                    "construct: {} refused at entity {} - {}",
                    record.name,
                    porter.index(),
                    Reason::NotOffered.message()
                );
                continue;
            }
            let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
            else {
                return Ok(());
            };
            let purse = tick
                .state()
                .get(ids.purse, player_entity)
                .map_or(mint_sage_economy::Money::new(0), |held| held.0);
            if !purse.can_afford(cost) {
                diag!(
                    "construct: {} refused - {}",
                    record.name,
                    Reason::NoMoney.message()
                );
                continue;
            }
            let Some(from) = tick
                .state()
                .get(ids.body, porter)
                .map(|body| body.0.position)
            else {
                continue;
            };
            if let Err(reason) = self.validate_placement(
                tick.state(),
                ids,
                player,
                record,
                &footprint,
                centre,
                angle,
                from,
            ) {
                diag!(
                    "construct: {} refused at ({x:.0}, {y:.0}) - {}",
                    record.name,
                    reason.message()
                );
                continue;
            }
            self.start_construction(
                tick,
                ids,
                (porter, player, from),
                (kind, centre, angle),
                &footprint,
                (cost, false),
            )?;
            diag!(
                "construct: {} ordered at ({x:.0}, {y:.0}) by entity {} - {cost} charged,                  {frames} frame(s) to build",
                record.name,
                porter.index()
            );
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn script_construct(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        builder: Entity,
        kind: crate::unit::Kind,
        (centre, angle): (Vec2, f32),
    ) -> Result<bool> {
        let Some(owner) = tick.state().get(ids.owner, builder).map(|owner| owner.0) else {
            return Ok(false);
        };
        if !self.computer_controlled(owner) {
            return Ok(false);
        }
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(true);
        };
        let is_dozer = tick
            .state()
            .get(ids.shape, builder)
            .is_some_and(|shape| shape.kind_of.contains(KindOf::DOZER));
        let Some(from) = tick
            .state()
            .get(ids.body, builder)
            .map(|body| body.0.position)
        else {
            return Ok(true);
        };
        if !is_dozer {
            return Ok(true);
        }
        let footprint = footprint_of(&entry.record);
        let cost = self
            .price_for(tick.state(), ids, owner, &entry.record)
            .map_or(0, |(cost, _)| cost);
        self.start_construction(
            tick,
            ids,
            (builder, owner, from),
            (kind, centre, angle),
            &footprint,
            (cost, true),
        )?;
        diag!(
            "construct: {} ordered by script at ({:.0}, {:.0}) for entity {}",
            entry.record.name,
            centre.x,
            centre.y,
            builder.index()
        );
        Ok(true)
    }

    fn start_construction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        (porter, player, from): (Entity, PlayerId, Vec2),
        (kind, centre, angle): (crate::unit::Kind, Vec2, f32),
        footprint: &[FootprintRecord],
        (cost, unchecked): (i64, bool),
    ) -> Result<()> {
        let economy = Self::economy_ids(ids);
        if let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player) {
            let mut purse = tick
                .state()
                .get(ids.purse, player_entity)
                .map_or(mint_sage_economy::Money::new(0), |held| held.0);
            purse.withdraw(cost);
            tick.state_mut()
                .insert(ids.purse, player_entity, Purse(purse))?;
            crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
                stats.spend(crate::match_stats::SpendKind::Structures, cost);
            });
        }
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return Ok(());
        };
        self.cancel_build_order(tick, ids, porter)?;
        tick.state_mut().insert(
            ids.build_task,
            porter,
            BuildTask {
                kind,
                centre,
                angle,
                unchecked,
            },
        )?;
        let reach = Obstacle {
            id: 0,
            centre,
            angle,
            records: footprint.to_vec(),
        }
        .reach();
        let spot = self.ai_table_of(tick, ids, porter).and_then(|ai| {
            super::contact::build_spot(
                &record.contact_points,
                &ai.special_contact_points,
                (
                    centre,
                    Facing::from_radians(angle),
                    record.geometry.major_radius.unwrap_or(0.0),
                ),
                from,
            )
        });
        match spot {
            Some(point) => self.order_builder_to_site(tick, ids, porter, point, 0.0)?,
            None => self.order_builder_to_site(tick, ids, porter, centre, reach)?,
        }
        if !tick.state().contains(ids.path, porter) {
            self.begin_building(tick, ids, porter)?;
        }
        if !self.is_computer(player) {
            self.push_misc_sound(tick, ids, MiscSound::BuildingPlacement, player, from);
        }
        Ok(())
    }

    pub(super) fn spawn_structure(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        x: f32,
        y: f32,
        angle: f32,
    ) -> Result<()> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(());
        };
        let record = entry.record.clone();
        let footprint = footprint_of(&record);
        if footprint.is_empty() {
            diag!(
                "spawn-buildings: {} - {}",
                record.name,
                Reason::NoFootprint.message()
            );
            return Ok(());
        }
        let centre = Vec2::new(x, y);
        if let Err(reason) = self.validate_placement(
            tick.state(),
            ids,
            player,
            &record,
            &footprint,
            centre,
            angle,
            centre,
        ) {
            diag!(
                "spawn-buildings: {} refused at ({x:.0}, {y:.0}) - {}",
                record.name,
                reason.message()
            );
            return Ok(());
        }
        if let Some(entity) =
            self.place_structure_complete(tick, ids, player, kind, centre, angle)?
        {
            diag!(
                "spawn-buildings: {} at ({x:.0}, {y:.0}) - entity {}",
                record.name,
                entity.index()
            );
        }
        Ok(())
    }

    pub(super) fn place_structure_complete(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
        centre: Vec2,
        angle: f32,
    ) -> Result<Option<Entity>> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(None);
        };
        let record = entry.record.clone();
        let footprint = footprint_of(&record);
        let facing = mint_movement::Facing::from_radians(angle);
        self.clear_for_construction(tick, ids, kind, centre, angle, None);
        let Some(spawn) = self.kinds.spawn(&self.combat, kind, centre, facing, player) else {
            return Ok(None);
        };
        let entity = tick.state_mut().spawn()?;
        Self::insert_spawn(tick, ids, entity, &spawn, &self.combat)?;
        self.stamp_footprint(tick, ids, entity, centre, angle, footprint)?;
        let created = tick.frame();
        tick.state_mut().insert(
            ids.construction,
            entity,
            Construction {
                builder: None,
                elapsed: 0,
                rebuild: None,
                worker: None,
                delay: 0,
                created,
            },
        )?;
        self.complete_construction(tick, ids, entity, kind, &record)?;
        Ok(Some(entity))
    }

    pub(super) fn stamp_footprint(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        centre: Vec2,
        angle: f32,
        records: Vec<FootprintRecord>,
    ) -> Result<()> {
        let kind = tick.state().get(ids.kind, site).copied();
        let ground = self
            .passability
            .heights
            .height_at_point(centre)
            .unwrap_or(0.0);
        let bounds = kind.and_then(|kind| self.wall_bounds_of(kind));
        if let Some(bounds) = bounds {
            let mut polygon = bounds.placed(centre, angle);
            polygon.height += ground;
            self.stamp_wall(tick, ids, site, polygon)?;
        }
        let ramps = kind.map_or(&[][..], |kind| self.ramp_meshes_of(kind));
        if !ramps.is_empty() {
            self.stamp_ramps(tick, ids, site, centre, angle, ground, ramps, &records)?;
        }
        if bounds.is_some() {
            return Ok(());
        }
        if kind.is_some_and(|kind| self.is_walk_on_top_of_wall(kind))
            && !kind.is_some_and(|kind| self.has_raised_wall_mesh_of(kind))
        {
            return Ok(());
        }
        let Some(clock_entity) = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.obstacles, clock_entity).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        if let Err(message) = layer.stamp(
            &self.passability.grid,
            geometry,
            extent,
            site,
            centre,
            angle,
            records,
            kind.is_some_and(|kind| self.scaleable_wall_geometry_of(kind).is_some()),
        ) {
            diag!("obstacles: {message}");
        }
        tick.state_mut()
            .insert(ids.obstacles, clock_entity, layer)?;
        if let Some((major_radius, minor_radius, height)) =
            kind.and_then(|kind| self.scaleable_wall_geometry_of(kind))
        {
            if let Some(mut walls) = tick.state().get(ids.scaleable_walls, clock_entity).cloned() {
                walls.insert(
                    site,
                    mint_sage_passability::WallGeometry {
                        id: 0,
                        centre,
                        angle,
                        major_radius,
                        minor_radius,
                        height,
                    },
                );
                tick.state_mut()
                    .insert(ids.scaleable_walls, clock_entity, walls)?;
            }
        }
        Ok(())
    }

    fn stamp_wall(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        polygon: crate::wall_layer::Polygon,
    ) -> Result<()> {
        let Some(clock_entity) = tick
            .state()
            .store(ids.wall_layer)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.wall_layer, clock_entity).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let counts = crate::obstacles::counts_of(tick.state(), ids.obstacles);
        layer.stamp(geometry, extent, site, polygon, counts);
        self.rebuild_reachability(
            &layer,
            crate::obstacles::scaleable_of(tick.state(), ids.obstacles),
        );
        tick.state_mut()
            .insert(ids.wall_layer, clock_entity, layer)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn stamp_ramps(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        centre: Vec2,
        angle: f32,
        ground: f32,
        ramps: &[crate::wall_meshes::RampMesh],
        records: &[FootprintRecord],
    ) -> Result<()> {
        let Some(clock_entity) = tick
            .state()
            .store(ids.wall_layer)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.wall_layer, clock_entity).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let placed: Vec<crate::wall_layer::Ramp> = ramps
            .iter()
            .map(|ramp_mesh| ramp_mesh.placed(centre, angle, ground))
            .collect();
        let released = if records.is_empty() {
            Vec::new()
        } else {
            mint_sage_passability::covered_cells(
                geometry,
                extent,
                &Obstacle {
                    id: 0,
                    centre,
                    angle,
                    records: records.to_vec(),
                },
            )
        };
        for message in layer.stamp_resolved_ramps(
            geometry,
            extent,
            site,
            &self.passability.heights,
            &placed,
            &released,
        ) {
            diag!("{message}");
        }
        self.rebuild_reachability(
            &layer,
            crate::obstacles::scaleable_of(tick.state(), ids.obstacles),
        );
        tick.state_mut()
            .insert(ids.wall_layer, clock_entity, layer)?;
        Ok(())
    }

    pub(super) fn unstamp_footprint(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
    ) -> Result<()> {
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        if let Some(clock_entity) = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
        {
            if let Some(mut layer) = tick.state().get(ids.obstacles, clock_entity).cloned() {
                layer.unstamp(&self.passability.grid, geometry, extent, site);
                tick.state_mut()
                    .insert(ids.obstacles, clock_entity, layer)?;
            }
        }
        self.drop_wall_occupants(tick, ids, site);
        if let Some(clock_entity) = tick
            .state()
            .store(ids.wall_layer)
            .and_then(|store| store.entities().first().copied())
        {
            if let Some(mut layer) = tick.state().get(ids.wall_layer, clock_entity).cloned() {
                layer.unstamp(geometry, extent, site);
                self.rebuild_reachability(
                    &layer,
                    crate::obstacles::scaleable_of(tick.state(), ids.obstacles),
                );
                tick.state_mut()
                    .insert(ids.wall_layer, clock_entity, layer)?;
            }
        }
        Ok(())
    }

    pub(super) fn drop_wall_occupants(&self, tick: &Tick<'_>, ids: Ids, wall: Entity) {
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let Some(cells) = tick
            .state()
            .store(ids.wall_layer)
            .and_then(|store| store.entities().first().copied())
            .and_then(|clock_entity| tick.state().get(ids.wall_layer, clock_entity))
            .and_then(|layer| layer.entity_cells(geometry, extent, wall))
        else {
            return;
        };
        if cells.is_empty() {
            return;
        }
        let Some(store) = tick.state().store(ids.body) else {
            return;
        };
        for (entity, body) in store.iter() {
            if entity == wall {
                continue;
            }
            let Some(cell) = geometry.cell_at(body.0.position, 0, extent) else {
                continue;
            };
            if cells.contains(&cell) {
                diag!(
                    "wall: entity {} stood on wall {} and drops to the ground unharmed",
                    entity.index(),
                    wall.index()
                );
            }
        }
    }

    pub(super) fn order_builder_to_site(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        builder: Entity,
        centre: Vec2,
        reach: f32,
    ) -> Result<()> {
        tick.state_mut().remove(ids.attack_order, builder);
        let Some(from) = tick
            .state()
            .get(ids.body, builder)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let modes = self.modes(tick, ids);
        let mut pipeline = Pipeline::new(modes.config());
        let geometry = self.passability.geometry;
        let mover = self.mover_class(tick, ids, builder);
        if self.give_off_map_route(tick, ids, builder, (from, centre), PathKind::Ordered)? {
            return Ok(());
        }
        let Some(start) = self.start_cell_toward(tick, ids, from, centre, mover) else {
            return Ok(());
        };
        let near_side = (reach > 0.0)
            .then(|| from.minus(centre).normalised())
            .flatten()
            .map(|direction| centre.plus(direction.scale(reach)));
        let Some(goal_cell) = near_side
            .and_then(|point| self.passable_cell_for(tick, ids, point, mover))
            .or_else(|| self.passable_cell_near(tick, ids, centre, reach, mover))
        else {
            return Ok(());
        };
        let prune = !self.reachable(mover, start, goal_cell);
        let request = PathRequest {
            start,
            goal: goal_cell,
            mover,
            goal_id: GoalId(u64::from(builder.index())),
            origin: Some(from),
            corner_radius: self.mover_limits(tick, ids, builder).corner_radius,
            departure: self.departure(tick, ids, builder),
            approach: None,
            prune,
            final_goal: true,
        };
        let mut budget = NodeBudget::new(PATH_NODE_BUDGET);
        let view = self.dynamic_view(tick, ids);
        let outcome = pipeline
            .plan(&view, geometry, &request, &mut budget)
            .map_err(|e| mint_sim::Error::Rules {
                frame: tick.frame().get(),
                reason: format!(
                    "planning a porter's approach for entity {}: {e}",
                    builder.index()
                ),
            })?;
        if let PlanOutcome::Ready(plan) = outcome {
            if let Some(mut body) = tick.state().get(ids.body, builder).copied() {
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, builder, body)?;
            }
            tick.state_mut().insert(
                ids.path,
                builder,
                Path {
                    route: plan.route,
                    goal: self.passability.geometry.centre_of(goal_cell),
                    kind: PathKind::Ordered,
                    facing: None,
                },
            )?;
        }
        Ok(())
    }

    pub(crate) const fn with_building_height_variation(mut self, variation: f32) -> Self {
        self.building_height_variation = variation;
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_placement(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        record: &mint_sage_modules::ObjectRecord,
        footprint: &[FootprintRecord],
        centre: Vec2,
        angle: f32,
        from: Vec2,
    ) -> core::result::Result<(), Reason> {
        self.validate_placement_ignoring(
            world,
            ids,
            player,
            record,
            footprint,
            centre,
            angle,
            from,
            &[],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_placement_ignoring(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        record: &mint_sage_modules::ObjectRecord,
        footprint: &[FootprintRecord],
        centre: Vec2,
        angle: f32,
        from: Vec2,
        ignore: &[Entity],
    ) -> core::result::Result<(), Reason> {
        if self.passability.cell_at(centre).is_none() {
            return Err(Reason::OffMap);
        }
        if record.has_kind("WALL_HUB") {
            if let Some(max_distance) = wall_hub_max_buildout_distance(record) {
                let centres = self.command_centers_of(world, ids, player);
                let reachable = centres
                    .iter()
                    .any(|centre_of| centre_of.minus(centre).length() <= max_distance);
                if !reachable {
                    return Err(Reason::WallHubTooFar);
                }
            }
        }
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let obstacle = Obstacle {
            id: 0,
            centre,
            angle,
            records: footprint.to_vec(),
        };
        let cells = mint_sage_passability::covered_cells(geometry, extent, &obstacle);
        if cells.is_empty() {
            return Err(Reason::NoFootprint);
        }
        let counts = crate::obstacles::counts_of(world, ids.obstacles);
        let view = DynamicView {
            grid: &self.passability.grid,
            counts,
            rubble: crate::obstacles::rubble_of(world, ids.obstacles),
            wall_rim: crate::wall_layer::rim_of(world, ids.wall_layer),
            tags: crate::wall_layer::tags_of(world, ids.wall_layer),
            connect: crate::wall_layer::connect_of(world, ids.wall_layer),
            released: crate::wall_layer::released_of(world, ids.wall_layer),
            closed: crate::obstacles::closed_of(world, ids.obstacles),
            obstacle_pinched: crate::obstacles::pinched_of(world, ids.obstacles),
            ramp_side: crate::wall_layer::side_of(world, ids.wall_layer),
            scaleable: crate::obstacles::scaleable_of(world, ids.obstacles),
            barred: super::border::barred_of(world, ids.human_barred),
        };
        let ignored: Vec<_> = ignore
            .iter()
            .flat_map(|entity| self.covered_cells_of(world, ids, *entity))
            .collect();
        let ignored_view = self.static_view();
        for cell in &cells {
            let flags = if ignored.contains(cell) {
                ignored_view.flags(*cell)
            } else {
                view.flags(*cell)
            };
            if flags.intersects(GROUND.blocked_by) {
                return Err(Reason::Blocked);
            }
            if flags.intersects(mint_sage_passability::FORD) {
                return Err(Reason::Ford);
            }
        }
        if !record.has_kind("WALL_HUB")
            && !record.has_kind("WALL_SEGMENT")
            && crate::construction::height_variation(footprint, centre, angle, |x, y| {
                self.passability.heights.height_at(x, y)
            }) > self.building_height_variation
        {
            return Err(Reason::NotFlat);
        }
        if self.meets_a_plan(world, ids, player, &cells) {
            return Err(Reason::Planned);
        }
        let reach = obstacle.reach();
        if let Some(bodies) = world.store(ids.body) {
            for (candidate, body) in bodies.iter() {
                let Some(owner) = world.get(ids.owner, candidate) else {
                    continue;
                };
                if !self.alliances.hostile(player, owner.0) {
                    continue;
                }
                if world
                    .get(ids.health, candidate)
                    .is_some_and(|health| health.0.is_dead())
                {
                    continue;
                }
                if self.view_relation(world, player, candidate) != map_scripts::Relation::Enemies
                    || ["MINE", "INERT"]
                        .iter()
                        .any(|bit| self.has_kind_bit(world, ids, candidate, bit))
                {
                    continue;
                }
                let radius = world
                    .get(ids.shape, candidate)
                    .map_or(0.0, |shape| shape.radius);
                let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                let limit = reach + radius;
                if dx * dx + dy * dy < limit * limit {
                    return Err(Reason::EnemyUnit);
                }
            }
        }
        let static_view = self.static_view();
        let Some(from_cell) = self.passability.cell_at(from) else {
            return Err(Reason::Unreachable);
        };
        let Some(start_cell) =
            mint_sage_passability::nearest_passable_over(&static_view, from_cell, GROUND)
        else {
            return Err(Reason::Unreachable);
        };
        let Some(rect) = self.passability.covering_rect(centre, reach) else {
            return Err(Reason::Unreachable);
        };
        let Some(goal_cell) =
            mint_sage_passability::nearest_passable_around_over(&static_view, rect, GROUND)
        else {
            return Err(Reason::Unreachable);
        };
        if !self
            .reachability
            .borrow()
            .same_component(start_cell, goal_cell)
        {
            return Err(Reason::Unreachable);
        }
        let mut pipeline = Pipeline::new(PipelineConfig {
            search: SearchMode::AStar,
            smoothing: SmoothingMode::Off,
            avoidance: AvoidanceMode::Off,
        });
        let request = PathRequest {
            start: start_cell,
            goal: goal_cell,
            mover: GROUND,
            goal_id: GoalId(u64::MAX),
            origin: None,
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
        };
        let mut budget = NodeBudget::new(PATH_NODE_BUDGET);
        let outcome = pipeline
            .plan(&static_view, geometry, &request, &mut budget)
            .map_err(|_| Reason::Unreachable)?;
        if !matches!(outcome, PlanOutcome::Ready(_)) {
            return Err(Reason::Unreachable);
        }
        Ok(())
    }

    pub(super) fn covered_cells_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Vec<mint_movement::Cell> {
        let Some(record) = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return Vec::new();
        };
        let Some(body) = world.get(ids.body, entity) else {
            return Vec::new();
        };
        let obstacle = Obstacle {
            id: 0,
            centre: body.0.position,
            angle: body.0.facing.radians(),
            records: footprint_of(record),
        };
        mint_sage_passability::covered_cells(
            self.passability.geometry,
            self.passability.grid.extent(),
            &obstacle,
        )
    }

    pub(crate) fn shown_build_orders(
        &self,
        world: &World,
        ids: Ids,
        viewer: PlayerId,
    ) -> Vec<(Entity, PlayerId, BuildTask)> {
        let builder_selected = world.store(ids.selected).is_some_and(|selected| {
            selected.iter().any(|(entity, mask)| {
                mask.holds(viewer)
                    && world.is_live(entity)
                    && world.get(ids.owner, entity).is_some_and(|o| o.0 == viewer)
                    && world
                        .get(ids.shape, entity)
                        .is_some_and(|shape| shape.kind_of.contains(KindOf::DOZER))
            })
        });
        if !builder_selected {
            return Vec::new();
        }
        world.store(ids.build_task).map_or_else(Vec::new, |tasks| {
            tasks
                .iter()
                .filter_map(|(builder, task)| {
                    let owner = world.get(ids.owner, builder)?.0;
                    (self.view_relation(world, viewer, builder)
                        == super::map_scripts::Relation::Allies)
                        .then_some((builder, owner, *task))
                })
                .collect()
        })
    }

    fn meets_a_plan(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        cells: &[mint_movement::Cell],
    ) -> bool {
        let Some(tasks) = world.store(ids.build_task) else {
            return false;
        };
        tasks.iter().any(|(builder, task)| {
            let ours = world.contains(ids.owner, builder)
                && self.view_relation(world, player, builder)
                    == super::map_scripts::Relation::Allies;
            ours && self.kinds.get(task.kind).is_some_and(|entry| {
                let planned = Obstacle {
                    id: 0,
                    centre: task.centre,
                    angle: task.angle,
                    records: footprint_of(&entry.record),
                };
                mint_sage_passability::covered_cells(
                    self.passability.geometry,
                    self.passability.grid.extent(),
                    &planned,
                )
                .iter()
                .any(|cell| cells.contains(cell))
            })
        })
    }

    pub(crate) fn cancel_refusal(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        player: PlayerId,
        site: Entity,
    ) -> Option<&'static str> {
        if world.get(ids.owner, site).map(|owner| owner.0) != Some(player) {
            return Some("not the builder's");
        }
        if !world.is_live(site) || world.contains(ids.doomed, site) {
            return Some("dead");
        }
        let Some(construction) = world.get(ids.construction, site) else {
            return Some("not under construction");
        };
        if construction.rebuild.is_some() {
            return Some("a rebuild, not a paid construction");
        }
        None
    }

    pub(super) fn cancel_construction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        for site in self.selection(tick, ids, player) {
            if let Some(why) = self.cancel_refusal(tick.state(), ids, player, site) {
                diag!("construction: site {} cancel refused - {why}", site.index());
                continue;
            }
            let Some(construction) = tick.state().get(ids.construction, site).copied() else {
                continue;
            };
            self.cancel_building_site(tick, ids, site, construction)?;
        }
        Ok(())
    }

    pub(super) fn cancel_building_site(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        construction: Construction,
    ) -> Result<()> {
        let refund = self.refund_site(tick, ids, site)?;
        if let Some((owner, kind)) = tick
            .state()
            .get(ids.owner, site)
            .map(|owner| owner.0)
            .zip(tick.state().get(ids.kind, site).copied())
        {
            self.note_structure_cancelled(tick, ids, owner, kind);
        }
        self.release_site_worker(tick, ids, construction.worker)?;
        if let Some(builder) = construction.builder {
            self.release_builder(tick, ids, site, builder)?;
        }
        let mut fired = Vec::new();
        if let Some(wiring) = self.modules.as_ref() {
            let now = tick.frame();
            if let Some(kind) = tick.state().get(ids.kind, site).copied() {
                wiring.attach_for_death(tick.state_mut(), site, kind, now)?;
            }
            let mut scratch = mint_sim::module::DestroyRequests::new();
            let mut log = mint_sage_modules::EffectLog::new();
            let (world, rng) = tick.state_and_rng_mut();
            wiring.broadcast_die(
                world,
                site,
                now,
                rng,
                &mut scratch,
                Effects::into_sink(&mut log),
                mint_sim::module::Death::NONE,
            );
            fired = log.drain();
        }
        let names: Vec<String> = fired
            .iter()
            .filter_map(|(_, effect)| match effect {
                mint_sage_modules::Effect::PlayFx { list, .. } => Some(list.to_string()),
                mint_sage_modules::Effect::CreateObjects { list, .. } => {
                    Some(format!("OCL {list}"))
                }
                _ => None,
            })
            .collect();
        self.apply_effects(tick, ids, fired)?;
        diag!(
            "construction: site {} cancelled {} frame(s) into its build - {refund} refunded, \
             fired [{}]",
            site.index(),
            construction.elapsed,
            names.join(", ")
        );
        self.unstamp_footprint(tick, ids, site)?;
        tick.state_mut().despawn(site);
        super::map_scripts::note_object_count_changed(tick, ids);
        Ok(())
    }

    pub(super) fn release_builder(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        builder: Entity,
    ) -> Result<()> {
        let Some(body) = tick.state().get(ids.body, site).copied() else {
            tick.state_mut().remove(ids.concealed, builder);
            return Ok(());
        };
        let reach = self.site_reach(tick, ids, site);
        self.reappear_builder(tick, ids, builder, body.0.position, reach)
    }

    fn site_reach(&self, tick: &Tick<'_>, ids: Ids, site: Entity) -> f32 {
        let Some(body) = tick.state().get(ids.body, site) else {
            return 0.0;
        };
        tick.state()
            .get(ids.kind, site)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .map_or(0.0, |entry| {
                Obstacle {
                    id: 0,
                    centre: body.0.position,
                    angle: body.0.facing.radians(),
                    records: footprint_of(&entry.record),
                }
                .reach()
            })
    }

    pub(super) fn refund_site(&self, tick: &mut Tick<'_>, ids: Ids, site: Entity) -> Result<i64> {
        let Some((owner, kind)) = tick
            .state()
            .get(ids.owner, site)
            .map(|owner| owner.0)
            .zip(tick.state().get(ids.kind, site).copied())
        else {
            return Ok(0);
        };
        self.refund_kind(
            tick,
            ids,
            owner,
            kind,
            self.construction_cancel_refund_percent,
        )
    }

    fn refund_kind(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
        percent: f32,
    ) -> Result<i64> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(0);
        };
        let Some((cost, _)) = self.price_for(tick.state(), ids, owner, &entry.record) else {
            return Ok(0);
        };
        let refund = crate::construction::scale_refund(cost, percent);
        if refund <= 0 {
            return Ok(0);
        }
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(0);
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        purse.deposit(refund);
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        let category = if self.has_kind_of(kind, "HERO") {
            crate::match_stats::SpendKind::Heroes
        } else if self.has_kind_of(kind, "STRUCTURE") {
            crate::match_stats::SpendKind::Structures
        } else {
            crate::match_stats::SpendKind::Units
        };
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.refund(category, refund);
        });
        Ok(refund)
    }

    pub(super) fn cancel_build_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        porter: Entity,
    ) -> Result<()> {
        let Some(task) = tick.state().get(ids.build_task, porter).copied() else {
            return Ok(());
        };
        tick.state_mut().remove(ids.build_task, porter);
        let Some(owner) = tick.state().get(ids.owner, porter).map(|owner| owner.0) else {
            return Ok(());
        };
        let refund = self.refund_kind(
            tick,
            ids,
            owner,
            task.kind,
            self.construction_cancel_refund_percent,
        )?;
        diag!(
            "construct: entity {} dropped its pending order - {refund} refunded",
            porter.index()
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn place_site(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
        centre: Vec2,
        angle: f32,
        builder: Option<Entity>,
        delay: u32,
    ) -> Result<Option<Entity>> {
        let footprint = self
            .kinds
            .get(kind)
            .map(|entry| footprint_of(&entry.record))
            .unwrap_or_default();
        let facing = mint_movement::Facing::from_radians(angle);
        self.clear_for_construction(tick, ids, kind, centre, angle, builder);
        let Some(mut spawn) = self.kinds.spawn(&self.combat, kind, centre, facing, owner) else {
            return Ok(None);
        };
        spawn.producer = None;
        spawn.income = None;
        spawn.upkeep = None;
        spawn.weapon = None;
        spawn.vision = Some(spawn.shape.radius.max(1.0));
        let site = tick.state_mut().spawn()?;
        Self::insert_spawn(tick, ids, site, &spawn, &self.combat)?;
        self.apply_weather_to_new_object(tick, ids, site)?;
        if let Some(mut health) = tick.state().get(ids.health, site).copied() {
            health.0.set_current(1.0);
            tick.state_mut().insert(ids.health, site, health)?;
        }
        self.stamp_footprint(tick, ids, site, centre, angle, footprint)?;
        self.clear_trees_under(tick, ids, site, centre)?;
        let worker_kind = self
            .kinds
            .get(kind)
            .filter(|_| builder.is_none())
            .and_then(|entry| self.getting_built_worker(&entry.record, owner));
        let worker = self.spawn_site_worker(tick, ids, worker_kind, site, owner)?;
        let created = tick.frame();
        tick.state_mut().insert(
            ids.construction,
            site,
            Construction {
                builder,
                elapsed: 0,
                rebuild: None,
                worker,
                delay,
                created,
            },
        )?;
        if let Some((cost, _)) = self
            .kinds
            .get(kind)
            .and_then(|entry| self.price_for(tick.state(), ids, owner, &entry.record))
        {
            tick.state_mut().insert(
                ids.object_value,
                site,
                crate::economy::ObjectValue {
                    paid_cost: i32::try_from(cost).unwrap_or(i32::MAX),
                    upgrade_spend: 0,
                },
            )?;
        }
        self.note_structure_placed(tick, ids, owner, kind);
        Ok(Some(site))
    }

    const MIN_ACTION_TOLERANCE: f32 = 70.0;
    const ARRIVAL_SLOP: f32 = 15.0;

    const FACING_TOLERANCE: f32 = 0.0175;

    pub(super) fn check_builder_arrivals(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let porters: Vec<Entity> = tick
            .state()
            .store(ids.build_task)
            .map_or_else(Vec::new, |store| store.entities().to_vec());
        for porter in porters {
            if Self::builder_dying(tick, ids, porter) {
                self.cancel_build_order(tick, ids, porter)?;
                continue;
            }
            if tick.state().contains(ids.path, porter) {
                self.try_early_builder_arrival(tick, ids, porter)?;
            }
            if !tick.state().contains(ids.path, porter) {
                self.begin_building(tick, ids, porter)?;
            }
        }
        Ok(())
    }

    fn builder_dying(tick: &Tick<'_>, ids: Ids, porter: Entity) -> bool {
        let world = tick.state();
        !world.is_live(porter)
            || world.contains(ids.doomed, porter)
            || world
                .get(ids.health, porter)
                .is_some_and(|health| health.0.is_dead())
    }

    fn try_early_builder_arrival(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        porter: Entity,
    ) -> Result<()> {
        let Some(path) = tick.state().get(ids.path, porter).cloned() else {
            return Ok(());
        };
        if path.kind != PathKind::Ordered {
            return Ok(());
        }
        let Some(body) = tick.state().get(ids.body, porter) else {
            return Ok(());
        };
        let radius = self
            .record_of(tick, ids, porter)
            .and_then(|record| record.geometry.radius)
            .unwrap_or(0.0);
        let tolerance = Self::MIN_ACTION_TOLERANCE.max(radius + Self::ARRIVAL_SLOP);
        if body.0.position.minus(path.goal).length() <= radius + tolerance {
            tick.state_mut().remove(ids.path, porter);
        }
        Ok(())
    }

    pub(super) fn begin_building(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        porter: Entity,
    ) -> Result<()> {
        let Some(task) = tick.state().get(ids.build_task, porter).copied() else {
            return Ok(());
        };
        if Self::builder_dying(tick, ids, porter) {
            return self.cancel_build_order(tick, ids, porter);
        }
        if let Some(body) = tick.state().get(ids.body, porter).copied() {
            if let Some(direction) = task.centre.minus(body.0.position).normalised() {
                let desired = Facing::from_vector(direction).unwrap_or(body.0.facing);
                if body.0.facing.delta_to(desired).abs() > Self::FACING_TOLERANCE {
                    let limits = self.mover_limits(tick, ids, porter);
                    let turned = body
                        .0
                        .facing
                        .turned_towards(desired, limits.turn_rate * Self::tick_seconds());
                    let mut next = body;
                    next.0.facing = turned;
                    tick.state_mut().insert(ids.body, porter, next)?;
                    return Ok(());
                }
            }
        }
        let Some(task) = tick.state_mut().remove(ids.build_task, porter) else {
            return Ok(());
        };
        let Some(owner) = tick.state().get(ids.owner, porter).map(|owner| owner.0) else {
            return Ok(());
        };
        let entry = self.kinds.get(task.kind);
        let footprint = entry.map_or_else(Vec::new, |entry| footprint_of(&entry.record));
        let from = tick
            .state()
            .get(ids.body, porter)
            .map_or(task.centre, |body| body.0.position);
        let refused = footprint
            .is_empty()
            .then_some(Reason::NoFootprint)
            .or_else(|| {
                entry.filter(|_| !task.unchecked).and_then(|entry| {
                    self.validate_placement(
                        tick.state(),
                        ids,
                        owner,
                        &entry.record,
                        &footprint,
                        task.centre,
                        task.angle,
                        from,
                    )
                    .err()
                })
            });
        if let Some(reason) = refused {
            let refund = self.refund_kind(tick, ids, owner, task.kind, 1.0)?;
            diag!(
                "construct: entity {} arrived but cannot build - {} - {refund} refunded",
                porter.index(),
                reason.message()
            );
            return Ok(());
        }
        let Some(site) = self.place_site(
            tick,
            ids,
            owner,
            task.kind,
            task.centre,
            task.angle,
            Some(porter),
            0,
        )?
        else {
            return Ok(());
        };
        tick.state_mut().insert(ids.concealed, porter, Concealed)?;
        tick.state_mut().remove(ids.selected, porter);
        tick.state_mut().remove(ids.attack_order, porter);
        diag!(
            "construct: entity {} started building site {}",
            porter.index(),
            site.index()
        );
        Ok(())
    }

    pub(super) fn reappear_builder(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        builder: Entity,
        centre: Vec2,
        reach: f32,
    ) -> Result<()> {
        tick.state_mut().remove(ids.concealed, builder);
        let mover = self.mover_class(tick, ids, builder);
        if let Some(cell) = self.passable_cell_near(tick, ids, centre, reach, mover) {
            let point = self.passability.geometry.centre_of(cell);
            if let Some(mut body) = tick.state().get(ids.body, builder).copied() {
                body.0.position = point;
                body.0.speed = 0.0;
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, builder, body)?;
            }
        }
        Ok(())
    }

    pub(super) fn is_evil_owner(&self, player: PlayerId) -> bool {
        self.alignments
            .get(usize::from(player.get()))
            .copied()
            .unwrap_or_else(|| crate::economy::alignment_of(player))
            == mint_sage_economy::Alignment::Evil
    }

    pub(super) fn getting_built_worker(
        &self,
        record: &mint_sage_modules::ObjectRecord,
        owner: PlayerId,
    ) -> Option<crate::unit::Kind> {
        let name = if record.test_faction && self.is_evil_owner(owner) {
            record.evil_worker_name.as_deref()
        } else {
            record.worker_name.as_deref()
        };
        name.and_then(|name| self.kinds.kind(name))
    }

    pub(super) fn spawn_site_worker(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: Option<crate::unit::Kind>,
        site: Entity,
        owner: PlayerId,
    ) -> Result<Option<Entity>> {
        let (Some(kind), Some(body)) = (kind, tick.state().get(ids.body, site).copied()) else {
            return Ok(None);
        };
        let (centre, facing) = (body.0.position, body.0.facing);
        let Some(worker) = self.spawn_kind_placed(
            tick,
            ids,
            kind,
            super::spawning::Placement::Exact(centre),
            facing,
            owner,
        )?
        else {
            return Ok(None);
        };
        tick.state_mut().insert(
            ids.site_worker,
            worker,
            SiteWorker {
                site,
                stage: WorkerStage::Approaching,
                deployed: false,
            },
        )?;
        let spot = match self.worker_contact_spot(tick, ids, kind, site, (centre, facing)) {
            Some(spot) => Some(spot),
            None => self.worker_free_spot(tick, ids, kind, centre),
        };
        self.route_out(tick, ids, worker, site, spot, None)?;
        Ok(Some(worker))
    }

    fn worker_contact_spot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        kind: crate::unit::Kind,
        site: Entity,
        (centre, facing): (Vec2, Facing),
    ) -> Option<Vec2> {
        let labels = &self.kinds.get(kind)?.record.ai.special_contact_points;
        let record = &self.kinds.get(*tick.state().get(ids.kind, site)?)?.record;
        super::contact::build_spot(
            &record.contact_points,
            labels,
            (centre, facing, record.geometry.major_radius.unwrap_or(0.0)),
            centre,
        )
    }

    fn worker_free_spot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: crate::unit::Kind,
        centre: Vec2,
    ) -> Option<Vec2> {
        let mover = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.mover)
            .unwrap_or(GROUND);
        let start_angle = tick.rng().next_f32() * core::f32::consts::TAU;
        let ground = self.ground_height(centre.x, centre.y);
        mint_sage_passability::find_position_around(
            &self.dynamic_view(tick, ids),
            self.passability.geometry,
            centre,
            (0.0, WORKER_SPOT_RANGE, start_angle),
            mover,
            |at| (self.ground_height(at.x, at.y) - ground).abs() <= WORKER_SPOT_RISE,
        )
    }

    pub(super) fn release_site_worker(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        worker: Option<Entity>,
    ) -> Result<()> {
        let Some(worker) = worker else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.site_worker, worker).copied() else {
            return Ok(());
        };
        state.stage = WorkerStage::Released;
        tick.state_mut().insert(ids.site_worker, worker, state)?;
        tick.state_mut().remove(ids.path, worker);
        tick.state_mut().remove(ids.ignored_obstacle, worker);
        Ok(())
    }

    pub(super) fn tend_site_workers(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let workers: Vec<(Entity, SiteWorker)> = tick
            .state()
            .store(ids.site_worker)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, state)| (entity, *state))
                    .collect()
            })
            .unwrap_or_default();
        for (worker, mut state) in workers {
            if state.stage == WorkerStage::Released
                || tick.state().contains(ids.doomed, worker)
                || tick.state().contains(ids.path, worker)
            {
                continue;
            }
            if !tick.state().contains(ids.body, state.site) {
                self.release_site_worker(tick, ids, Some(worker))?;
                continue;
            }
            let owners = tick
                .state()
                .get(ids.owner, worker)
                .zip(tick.state().get(ids.owner, state.site));
            let (Some(site_body), Some((_, _))) =
                (tick.state().get(ids.body, state.site).copied(), owners)
            else {
                continue;
            };
            if !self.object_allied(tick.state(), worker, state.site) {
                self.release_site_worker(tick, ids, Some(worker))?;
                continue;
            }
            let centre = site_body.0.position;
            self.turn_towards(tick, ids, worker, centre)?;
            let Some(body) = tick.state().get(ids.body, worker).copied() else {
                continue;
            };
            let faced = Facing::from_vector(centre.minus(body.0.position))
                .is_none_or(|desired| body.0.facing.delta_to(desired) == 0.0);
            if state.stage != WorkerStage::Constructing && faced {
                state.stage = WorkerStage::Constructing;
            } else if state.stage == WorkerStage::Approaching {
                state.stage = WorkerStage::Facing;
            }
            let radius = |entity| {
                tick.state()
                    .get(ids.shape, entity)
                    .map_or(0.0, |shape| shape.radius)
            };
            let border =
                body.0.position.minus(centre).length() - radius(state.site) - radius(worker);
            state.deployed |= border > DEPLOYED_REACH * radius(worker);
            tick.state_mut().insert(ids.site_worker, worker, state)?;
        }
        Ok(())
    }

    pub(super) fn fade_released_workers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
    ) -> Result<Vec<(Entity, f32)>> {
        let released: Vec<Entity> = tick
            .state()
            .store(ids.site_worker)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, state)| state.stage == WorkerStage::Released)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let mut killed = Vec::new();
        for worker in released {
            killed.extend(self.kill(tick, ids, worker, None, crate::combat::DeathNaming::Faded)?);
        }
        Ok(killed)
    }

    pub(super) fn advance_construction(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let sites: Vec<Entity> = tick
            .state()
            .store(ids.construction)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for site in sites {
            if tick.state().contains(ids.doomed, site) {
                continue;
            }
            let Some(mut construction) = tick.state().get(ids.construction, site).copied() else {
                continue;
            };
            if construction.delay > 0 {
                construction.delay = construction.delay.saturating_sub(1);
                tick.state_mut()
                    .insert(ids.construction, site, construction)?;
                continue;
            }
            let Some(kind) = tick.state().get(ids.kind, site).copied() else {
                continue;
            };
            let Some(entry) = self.kinds.get(kind) else {
                continue;
            };
            let owner = tick.state().get(ids.owner, site).map(|owner| owner.0);
            if let Some(worker) = construction.worker {
                let world = tick.state();
                let hammering = world.get(ids.site_worker, worker).is_some_and(|state| {
                    state.site == site && state.stage == WorkerStage::Constructing
                });
                let gone = !world.is_live(worker) || world.contains(ids.doomed, worker);
                if !hammering {
                    if let Some(owner) =
                        owner.filter(|_| gone && super::repair::is_wake(tick.frame()))
                    {
                        let replacement = self.getting_built_worker(&entry.record, owner);
                        construction.worker =
                            self.spawn_site_worker(tick, ids, replacement, site, owner)?;
                        tick.state_mut()
                            .insert(ids.construction, site, construction)?;
                    }
                    continue;
                }
            }
            let Some(frames) = construction
                .rebuild
                .map(|frames| frames.max(1))
                .or_else(|| self.build_frames(&entry.record, owner))
            else {
                continue;
            };
            construction.elapsed = construction.elapsed.saturating_add(1);
            let max_health = entry
                .record
                .max_health
                .filter(|health| *health > 0.0)
                .unwrap_or(1.0);
            if let Some(mut health) = tick.state().get(ids.health, site).copied() {
                #[allow(clippy::cast_precision_loss)]
                let per_frame = max_health / frames as f32;
                health.0.set_current(health.0.current() + per_frame);
                tick.state_mut().insert(ids.health, site, health)?;
            }
            if construction.elapsed >= frames {
                let record = entry.record.clone();
                self.complete_construction(tick, ids, site, kind, &record)?;
            } else {
                tick.state_mut()
                    .insert(ids.construction, site, construction)?;
            }
        }
        Ok(())
    }

    pub(super) fn build_frames(
        &self,
        record: &mint_sage_modules::ObjectRecord,
        owner: Option<PlayerId>,
    ) -> Option<u32> {
        let time = owner.map_or_else(mint_sage_economy::TimeModifiers::default, |owner| {
            self.time_modifiers_for(owner)
        });
        let time = mint_sage_economy::TimeModifiers {
            speed_multiplier: self.multiplay.building_speed(self.players),
            ..time
        };
        let seconds = record
            .build_time
            .filter(|seconds| (0.0..=crate::templates::MAX_BUILD_SECONDS).contains(seconds))?;
        Some(
            mint_sage_economy::time_to_build(seconds, &time)
                .get()
                .max(1),
        )
    }

    pub(super) fn complete_construction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        kind: crate::unit::Kind,
        record: &mint_sage_modules::ObjectRecord,
    ) -> Result<()> {
        let Some(construction) = tick.state().get(ids.construction, site).copied() else {
            return Ok(());
        };
        tick.state_mut().remove(ids.construction, site);
        super::map_scripts::note_object_count_changed(tick, ids);
        if let Some(owner) = tick.state().get(ids.owner, site).map(|owner| owner.0) {
            self.note_structure_completed(tick, ids, owner, kind);
        }
        self.release_site_worker(tick, ids, construction.worker)?;
        self.emit_voice(tick, ids, site, crate::voice::VoiceKind::FullyCreated, None);
        if let Some(range) = crate::templates::vision(record) {
            tick.state_mut().insert(
                ids.vision,
                site,
                Vision {
                    clearing_range: range,
                },
            )?;
        }
        let (position, facing) = tick
            .state()
            .get(ids.body, site)
            .map_or((Vec2::ZERO, mint_movement::Facing::EAST), |body| {
                (body.0.position, body.0.facing)
            });
        let rebuilt = construction.rebuild.is_some();
        if let Some(producer) = self
            .kinds
            .producer(&self.production, kind, position, facing)
            .filter(|_| !rebuilt)
        {
            tick.state_mut().insert(
                ids.producer,
                site,
                Producer {
                    queue: mint_sage_economy::ProductionQueue::new(producer.depth),
                    menu: Some(producer.menu),
                    object_menu: None,
                    player_menu: None,
                    rally: producer.rally,
                    player_rally: None,
                },
            )?;
        }
        if let Some((timing, amount)) = crate::templates::income(record).filter(|_| !rebuilt) {
            tick.state_mut().insert(
                ids.income,
                site,
                Income(mint_sage_economy::AutoDeposit::new(timing, amount)),
            )?;
        }
        if let Some(points) = self.kinds.upkeep(kind).filter(|_| !rebuilt) {
            tick.state_mut().insert(ids.upkeep, site, Upkeep(points))?;
        }
        if let Some(weapon_name) = record.weapon.as_deref() {
            if let Some(weapon) = self.combat.weapons.id(weapon_name) {
                if let Some(template) = self.combat.weapons.get(weapon) {
                    let next_acquire = staggered_first_acquire(site, tick.frame());
                    let now = tick.frame();
                    let state = WeaponState::activated(template, now, 1.0, tick.rng());
                    tick.state_mut().insert(
                        ids.armed,
                        site,
                        Armed {
                            weapon,
                            state,
                            target: None,
                            winding: false,
                            shots_at_target: 0,
                            last_shot_at: None,
                            next_acquire,
                            reacquire_ready: None,
                            squish_reacquire: false,
                        },
                    )?;
                }
            }
        }
        let reach = footprint_of(record).first().map_or(0.0, |_| {
            Obstacle {
                id: 0,
                centre: position,
                angle: facing.radians(),
                records: footprint_of(record),
            }
            .reach()
        });
        if !rebuilt {
            self.attach_upgrade_state(tick, ids, site, Some(kind))?;
            self.inherit_castle_upgrades(tick, ids, site)?;
            self.castle_build_complete(tick, ids, site, construction.created)?;
        }
        match construction.builder {
            Some(builder) => self.reappear_builder(tick, ids, builder, position, reach),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mint_sage_modules::{CapturedField, CapturedModule};

    use super::wall_hub_max_buildout_distance;

    fn record_with_max_buildout(numeric: Option<f32>) -> mint_sage_modules::ObjectRecord {
        mint_sage_modules::ObjectRecord {
            declared_modules: vec![CapturedModule {
                name: "WallHubBehavior".to_owned(),
                tag: None,
                args: Vec::new(),
                fields: vec![CapturedField {
                    keyword: "MaxBuildoutDistance".to_owned(),
                    args: Vec::new(),
                    numeric,
                    percent: None,
                    file: Arc::from("test.ini"),
                    line: 1,
                }],
                blocks: Vec::new(),
            }],
            ..mint_sage_modules::ObjectRecord::default()
        }
    }

    #[test]
    fn wall_hub_max_buildout_distance_treats_the_sentinel_as_unlimited() {
        assert_eq!(
            wall_hub_max_buildout_distance(&record_with_max_buildout(Some(900.0))),
            Some(900.0)
        );
        assert_eq!(
            wall_hub_max_buildout_distance(&record_with_max_buildout(Some(
                super::super::wall::MAX_BUILDOUT_UNSET
            ))),
            None
        );
        assert_eq!(
            wall_hub_max_buildout_distance(&record_with_max_buildout(Some(0.0))),
            None
        );
        assert_eq!(
            wall_hub_max_buildout_distance(&mint_sage_modules::ObjectRecord::default()),
            None
        );
    }
}
