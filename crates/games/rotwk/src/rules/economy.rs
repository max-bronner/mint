use super::*;
use crate::claims::{ClaimLayer, Holder, Registered};
use crate::diag::diag;
use mint_sage_passability::claims::{Claim, ClaimGrid, ClaimGround, Claimer, Viewer};

const REVIVAL_COOLDOWN: mint_sim::LogicFrames =
    mint_sim::LogicFrames::from_engine_milliseconds(4_000);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IncomeView {
    pub(crate) structure: Entity,
    pub(crate) owner: PlayerId,
    pub(crate) due_in: u32,
    pub(crate) interval: u32,
    pub(crate) amount: i64,
}

impl RotwkRules {
    pub(crate) fn brutal_cheats_for(
        &self,
        player: PlayerId,
    ) -> mint_sage_ini::game_data::BrutalCheats {
        let brutal_ai = self.difficulty.multiplayer
            && self.ai_power_difficulty.iter().any(|(ai, difficulty)| {
                *ai == player && *difficulty == crate::lobby::Difficulty::Brutal
            });
        if brutal_ai {
            self.brutal_cheats
        } else {
            mint_sage_ini::game_data::BrutalCheats::NONE
        }
    }

    pub(super) fn cost_modifiers_for_record(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        record: &mint_sage_modules::ObjectRecord,
    ) -> mint_sage_economy::CostModifiers {
        mint_sage_economy::CostModifiers {
            brutal_reduction: self.brutal_cheats_for(player).build_cost_reduction,
            ..crate::economy::cost_modifiers_for(
                world,
                Self::economy_ids(ids),
                &self.kinds,
                &self.upgrade_bits,
                player,
                &crate::economy::PriceTarget::Object {
                    kind_of: &record.kind_of,
                    name: &record.name,
                },
                self.production_cost_change_percent(player),
            )
        }
    }

    pub(crate) fn time_modifiers_for(&self, player: PlayerId) -> mint_sage_economy::TimeModifiers {
        mint_sage_economy::TimeModifiers {
            handicap: self.handicap_of(Some(player)).build_time,
            brutal_reduction: self.brutal_cheats_for(player).build_time_reduction,
            ..mint_sage_economy::TimeModifiers::default()
        }
    }

    pub(crate) fn unit_time_modifiers(&self, player: PlayerId) -> mint_sage_economy::TimeModifiers {
        mint_sage_economy::TimeModifiers {
            speed_multiplier: self.multiplay.unit_speed(self.players),
            ..self.time_modifiers_for(player)
        }
    }

    pub(crate) fn price_for(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        record: &mint_sage_modules::ObjectRecord,
    ) -> Option<(i64, u32)> {
        price_of(
            record,
            &self.cost_modifiers_for_record(world, ids, player, record),
            &self.time_modifiers_for(player),
        )
    }

    pub(crate) fn cost_modifier_counts(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> crate::economy::PlayerCostModifierCounts {
        crate::economy::player_cost_modifier_counts(world, Self::economy_ids(ids), player)
    }

    pub(crate) fn build_price_from_counts(
        &self,
        counts: &crate::economy::PlayerCostModifierCounts,
        player: PlayerId,
        target: crate::unit::Kind,
    ) -> Option<i64> {
        let record = &self.kinds.get(target)?.record;
        let modifiers = mint_sage_economy::CostModifiers {
            brutal_reduction: self.brutal_cheats_for(player).build_cost_reduction,
            ..crate::economy::cost_modifiers_from_counts(
                counts,
                &self.kinds,
                &self.upgrade_bits,
                &crate::economy::PriceTarget::Object {
                    kind_of: &record.kind_of,
                    name: &record.name,
                },
                self.production_cost_change_percent(player),
            )
        };
        crate::construction::price_of(record, &modifiers, &self.time_modifiers_for(player))
            .map(|(cost, _)| cost)
    }

    pub(crate) fn claim_grid(&self) -> Option<ClaimGrid> {
        ClaimGrid::new(
            self.claim_cell_size?,
            self.passability.geometry,
            self.passability.grid.extent(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn with_claim_ground<R>(
        &self,
        world: &World,
        ids: Ids,
        held: &mint_sage_passability::claims::ClaimLayer,
        claimer: Claimer,
        own: &[Cell],
        viewer: Option<PlayerId>,
        f: impl Fn(&ClaimGround<'_>) -> R,
    ) -> Option<R> {
        let grid = self.claim_grid()?;
        let ground = DynamicView {
            grid: &self.passability.grid,
            counts: crate::obstacles::counts_of(world, ids.obstacles),
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
        let run = |viewer: Option<Viewer<'_>>| {
            f(&ClaimGround {
                grid: &grid,
                ground: &ground,
                ground_geometry: self.passability.geometry,
                layer: held,
                claimer,
                own,
                viewer,
            })
        };
        let Some(player) = viewer else {
            return Some(run(None));
        };
        let registered = crate::claims::layer_of(world, ids.claims);
        let holder = |key: u32| {
            registered
                .and_then(|layer| layer.object_of(key))
                .filter(|entity| world.is_live(*entity))
        };
        let shrouded = crate::shroud::with_grid(world, Self::shroud_ids(ids), |shroud| {
            let state = |at: Vec2| {
                let (x, y) = shroud.world_to_cell(at.x, at.y);
                shroud.cell_state(player, x, y)
            };
            let hidden = |key: u32| {
                holder(key).is_none_or(|entity| {
                    world.get(ids.seen, entity).is_none_or(|seen| {
                        matches!(
                            seen.visibility(player),
                            mint_sage_shroud::ObjectVisibility::Fogged
                                | mint_sage_shroud::ObjectVisibility::Shrouded
                        )
                    })
                })
            };
            run(Some(Viewer {
                unexplored: &|at| state(at) == mint_sage_shroud::CellVisibility::Unexplored,
                fogged: &|at| state(at) == mint_sage_shroud::CellVisibility::Explored,
                hidden: &hidden,
            }))
        });
        Some(shrouded.unwrap_or_else(|| {
            run(Some(Viewer {
                unexplored: &|_| false,
                fogged: &|_| false,
                hidden: &|key| holder(key).is_none(),
            }))
        }))
    }

    pub(crate) fn preview_claim(
        &self,
        world: &World,
        ids: Ids,
        claimant: &crate::claims::Claimant,
        centre: Vec2,
        viewer: PlayerId,
    ) -> Option<Claim> {
        let held = crate::claims::layer_of(world, ids.claims)?;
        self.with_claim_ground(
            world,
            ids,
            held.cells(),
            Self::preview_claimer(viewer),
            &[],
            Some(viewer),
            |ground| mint_sage_passability::claims::assess(ground, centre, claimant.radius),
        )?
    }

    pub(crate) fn claim_fade(
        &self,
        world: &World,
        ids: Ids,
        claimant: &crate::claims::Claimant,
        centre: Vec2,
        viewer: PlayerId,
    ) -> Option<mint_sage_passability::claims::Fade> {
        let held = crate::claims::layer_of(world, ids.claims)?;
        self.with_claim_ground(
            world,
            ids,
            held.cells(),
            Self::preview_claimer(viewer),
            &[],
            Some(viewer),
            |ground| mint_sage_passability::claims::fade(ground, centre, claimant.radius),
        )?
    }

    fn preview_claimer(viewer: PlayerId) -> Claimer {
        Claimer {
            key: 0,
            player: u32::from(viewer.get()),
            finished: false,
        }
    }

    fn claim_again(&self, world: &World, ids: Ids, layer: &mut ClaimLayer, at: usize, why: &str) {
        let Some(claimant) = layer.registered().get(at).copied() else {
            return;
        };
        let own = match claimant.holder {
            Holder::Object(entity) => self.covered_cells_of(world, ids, entity),
            Holder::Plan { .. } => Vec::new(),
        };
        let claim = self
            .with_claim_ground(
                world,
                ids,
                layer.cells(),
                claimant.claimer(),
                &own,
                None,
                |ground| {
                    mint_sage_passability::claims::assess(ground, claimant.centre, claimant.radius)
                },
            )
            .flatten();
        let (cells, wanted) = claim.map_or((Vec::new(), 0), |claim| (claim.cells, claim.wanted));
        let Some((taken, share)) = layer.take(at, &cells, wanted) else {
            return;
        };
        let (what, entity) = match claimant.holder {
            Holder::Object(entity) => ("entity", entity),
            Holder::Plan { builder, .. } => ("plan of entity", builder),
        };
        diag!(
            "claims: {what} {} at ({:.0}, {:.0}) radius {:.0} {why} {taken} of {wanted} cell(s), \
             {:.0}%",
            entity.index(),
            claimant.centre.x,
            claimant.centre.y,
            claimant.radius,
            share * 100.0
        );
    }

    fn claim_round(
        &self,
        world: &World,
        ids: Ids,
        layer: &mut ClaimLayer,
        centre: Vec2,
        reach: f32,
    ) {
        for at in 0..layer.registered().len() {
            let near = layer.registered().get(at).is_some_and(|claimant| {
                claimant.centre.minus(centre).length_squared() <= reach * reach
            });
            if near {
                self.claim_again(world, ids, layer, at, "re-claims");
            }
        }
    }

    pub(super) fn settle_claims(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.claims)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let world = tick.state();
        let pending: Vec<(Entity, crate::claims::Claimant)> = world
            .store(ids.claimant)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, claimant)| claimant.share.is_none())
                    .map(|(entity, claimant)| (entity, *claimant))
                    .collect()
            })
            .unwrap_or_default();
        let Some(layer) = world.get(ids.claims, clock) else {
            return Ok(());
        };
        let plans: Vec<(Entity, BuildTask, crate::claims::Claimant)> = world
            .store(ids.build_task)
            .map(|store| {
                store
                    .iter()
                    .filter(|(builder, task)| !layer.holds_plan(*builder, task))
                    .filter_map(|(builder, task)| {
                        let claimant =
                            crate::templates::claimant(&self.kinds.get(task.kind)?.record)?;
                        Some((builder, *task, claimant))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let built = |claimant: &Registered| match claimant.holder {
            Holder::Object(entity) => {
                !claimant.finished
                    && world.is_live(entity)
                    && !world.contains(ids.construction, entity)
            }
            Holder::Plan { .. } => false,
        };
        let changed = layer
            .registered()
            .iter()
            .any(|claimant| !claimant.is_live(world, ids.build_task) || built(claimant));
        if pending.is_empty() && plans.is_empty() && !changed {
            return Ok(());
        }
        let mut layer = layer.clone();

        let mut fresh = Vec::with_capacity(pending.len());
        for (entity, claimant) in pending {
            let planned = world
                .get(ids.construction, entity)
                .and_then(|site| site.builder)
                .zip(world.get(ids.kind, entity).copied())
                .and_then(|(builder, kind)| layer.hand_over(builder, kind, entity));
            match planned {
                Some(share) => diag!(
                    "claims: entity {} keeps its plan's claim, {:.0}%",
                    entity.index(),
                    share * 100.0
                ),
                None => fresh.push((entity, claimant)),
            }
        }

        while let Some(at) = layer
            .registered()
            .iter()
            .position(|claimant| !claimant.is_live(world, ids.build_task))
        {
            let Some(gone) = layer.remove(at) else {
                break;
            };
            let (what, entity) = match gone.holder {
                Holder::Object(entity) => ("entity", entity),
                Holder::Plan { builder, .. } => ("plan of entity", builder),
            };
            diag!(
                "claims: {what} {} at ({:.0}, {:.0}) leaves its ground",
                entity.index(),
                gone.centre.x,
                gone.centre.y
            );
            let reach = gone.radius + layer.max_radius();
            self.claim_round(world, ids, &mut layer, gone.centre, reach);
        }

        for at in 0..layer.registered().len() {
            if layer.registered().get(at).is_some_and(built) {
                layer.finish(at);
                self.claim_again(world, ids, &mut layer, at, "is built and takes");
            }
        }

        let objects = fresh.into_iter().filter_map(|(entity, claimant)| {
            let centre = world.get(ids.body, entity)?.0.position;
            let owner = world.get(ids.owner, entity)?.0;
            Some((Holder::Object(entity), owner, centre, claimant))
        });
        let planned = plans.into_iter().filter_map(|(builder, task, claimant)| {
            let owner = world.get(ids.owner, builder)?.0;
            Some((Holder::Plan { builder, task }, owner, task.centre, claimant))
        });
        for (holder, owner, centre, claimant) in
            objects.collect::<Vec<_>>().into_iter().chain(planned)
        {
            let at = layer.register(
                Registered {
                    holder,
                    owner,
                    centre,
                    radius: claimant.radius,
                    finished: false,
                    share: 0.0,
                },
                claimant.high_priority,
            );
            self.claim_again(world, ids, &mut layer, at, "takes");
            if claimant.high_priority {
                let reach = claimant.radius + layer.max_radius();
                self.claim_round(world, ids, &mut layer, centre, reach);
            }
            let finished = match holder {
                Holder::Object(entity) => !world.contains(ids.construction, entity),
                Holder::Plan { .. } => false,
            };
            if finished {
                if let Some(at) = layer
                    .registered()
                    .iter()
                    .position(|registered| registered.holder == holder)
                {
                    layer.finish(at);
                    self.claim_again(world, ids, &mut layer, at, "is built and takes");
                }
            }
        }

        let shares: Vec<(Entity, f32)> = layer
            .registered()
            .iter()
            .filter_map(|claimant| match claimant.holder {
                Holder::Object(entity) => Some((entity, claimant.share)),
                Holder::Plan { .. } => None,
            })
            .collect();
        for (entity, share) in shares {
            let Some(mut claimant) = tick.state().get(ids.claimant, entity).copied() else {
                continue;
            };
            if claimant.share != Some(share) {
                claimant.share = Some(share);
                tick.state_mut().insert(ids.claimant, entity, claimant)?;
            }
        }
        tick.state_mut().insert(ids.claims, clock, layer)?;
        Ok(())
    }

    pub(crate) fn shown_plan_claims(
        &self,
        world: &World,
        viewer: PlayerId,
    ) -> Vec<(PlayerId, Vec2, f32)> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(layer) = crate::claims::layer_of(world, ids.claims) else {
            return Vec::new();
        };
        layer
            .registered()
            .iter()
            .filter_map(|claimant| {
                let Holder::Plan { task, .. } = claimant.holder else {
                    return None;
                };
                let visible = self.kinds.get(task.kind)?.record.claim_decal_visible;
                (visible && self.allied(viewer, claimant.owner)).then_some((
                    claimant.owner,
                    claimant.centre,
                    claimant.radius,
                ))
            })
            .collect()
    }

    pub(crate) fn income_views(&self, world: &World) -> Vec<IncomeView> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.income) else {
            return Vec::new();
        };
        let money = self.multiplay.money(self.players);
        store
            .iter()
            .filter_map(|(structure, income)| {
                let owner = world.get(ids.owner, structure)?.0;
                let production = world
                    .get(ids.bonuses, structure)
                    .map_or(1.0, |bonuses| bonuses.0.production);
                Some(IncomeView {
                    structure,
                    owner,
                    due_in: income.0.frames_until_payment(),
                    interval: income.0.timing().get(),
                    amount: crate::economy::deposited(income.0.payment(), money, production),
                })
            })
            .collect()
    }

    pub(super) fn advance_economy(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let economy = Self::economy_ids(ids);
        let deposits =
            crate::economy::advance_income(tick, economy, self.multiplay.money(self.players))?;
        self.award_income(tick, ids, &deposits)?;
        self.step_production_doors(tick, ids)?;
        let now = tick.frame();
        crate::economy::advance_production(
            tick,
            economy,
            |world, owner, item| {
                let Some(kind) = u16::try_from(item)
                    .ok()
                    .and_then(|item| self.buildable_at(item))
                    .map(|buildable| buildable.kind)
                else {
                    return 1;
                };
                crate::economy::player_entity(world, economy, owner)
                    .and_then(|player| world.get(ids.production_bonuses, player))
                    .map_or(1, |bonuses| bonuses.frames_for(kind, now))
            },
            |world, structure, item| {
                if usize::try_from(item).is_ok_and(|item| item >= self.buildables.len()) {
                    return None;
                }
                let buildable = u16::try_from(item)
                    .ok()
                    .and_then(|item| self.buildable_at(item))?;
                let owner = world.get(ids.owner, structure)?.0;
                Some(
                    crate::economy::unit_frames_to_produce(
                        world,
                        economy,
                        structure,
                        buildable.build_seconds,
                        &self.unit_time_modifiers(owner),
                    )
                    .get(),
                )
            },
            |world, owner, item| {
                let Some(buildable) = u16::try_from(item)
                    .ok()
                    .and_then(|item| self.buildable_at(item))
                else {
                    return true;
                };
                let is_structure = self
                    .kinds
                    .get(buildable.kind)
                    .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
                self.allowed_to_build(world, ids, owner, buildable.kind, is_structure)
            },
            |world, owner, item| {
                let Some(buildable) = u16::try_from(item)
                    .ok()
                    .and_then(|item| self.buildable_at(item))
                else {
                    return true;
                };
                let army_of_dead = self
                    .kinds
                    .get(buildable.kind)
                    .is_some_and(|entry| entry.record.has_kind("ARMY_OF_DEAD"));
                let free = crate::economy::points_of(world, economy, owner).free();
                crate::heroes::command_points_fit(buildable.command_points, free, army_of_dead)
            },
            |tick, buildable, rally, ordinal, owner, structure| {
                let Some(template) = self.buildable_at(buildable) else {
                    return Ok(None);
                };
                if !self.through_production_door(tick, ids, structure)? {
                    return Ok(None);
                }
                let is_hero = usize::from(buildable) >= self.buildables.len();
                let exit_structure = if is_hero {
                    self.revive_exit_structure(tick.state(), ids, owner, structure, template.kind)
                } else {
                    structure
                };
                let exit_rally = tick
                    .state()
                    .get(ids.producer, exit_structure)
                    .map_or(rally, |producer| producer.rally);
                let body = tick
                    .state()
                    .get(ids.body, exit_structure)
                    .map(|body| body.0);
                let facing = body.map_or(mint_movement::Facing::EAST, |body| {
                    self.production_exit_facing(tick, ids, exit_structure, body.facing)
                });
                let (at, walk) = body.map_or_else(
                    || (crate::economy::exit_point(exit_rally, ordinal), None),
                    |body| {
                        self.production_exit(tick, ids, exit_structure, body, exit_rally, ordinal)
                    },
                );
                let player_rally = tick
                    .state()
                    .get(ids.producer, exit_structure)
                    .and_then(|producer| producer.player_rally);
                let no_exit_path = tick
                    .state()
                    .get(ids.kind, exit_structure)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| entry.record.no_exit_path);
                match &template.output {
                    crate::economy::Output::Unit(spawn) => {
                        let resolved = self.resolve_build_variation(tick, template.kind);
                        let spawn = if resolved == template.kind {
                            *spawn
                        } else {
                            self.kinds
                                .spawn(&self.combat, resolved, at, facing, owner)
                                .map(|fresh| Spawn {
                                    upkeep: self.kinds.upkeep(resolved),
                                    ..fresh
                                })
                                .unwrap_or(*spawn)
                        };
                        let entity = tick.state_mut().spawn()?;
                        Self::insert_spawn(
                            tick,
                            ids,
                            entity,
                            &Spawn {
                                position: at,
                                facing,
                                owner,
                                ..spawn
                            },
                            &self.combat,
                        )?;
                        self.attach_upgrade_state(tick, ids, entity, spawn.kind)?;
                        let no_walk = no_exit_path && player_rally.is_none();
                        let loose_walk = if no_walk { None } else { walk };
                        if !no_walk {
                            self.begin_factory_exit(tick, ids, entity)?;
                        }
                        self.route_out(
                            tick,
                            ids,
                            entity,
                            exit_structure,
                            loose_walk,
                            player_rally,
                        )?;
                        if !tick.state().contains(ids.path, entity) {
                            self.end_factory_exit(tick, ids, entity)?;
                        }
                        if is_hero {
                            self.hero_delivered(tick, ids, structure, owner, entity)?;
                            if let Some(producer) =
                                tick.state_mut().get_mut(ids.producer, structure)
                            {
                                producer.queue.set_revival_cooldown(REVIVAL_COOLDOWN.get());
                            }
                        }
                        self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::Created, None);
                        self.note_produced(tick, ids, owner, template.kind, template.kind, 1);
                        self.team_builder_unit_produced(
                            tick,
                            ids,
                            owner,
                            structure,
                            entity,
                            template.kind,
                        )?;
                        self.inherit_army(tick, ids, structure, entity);
                        Ok(Some(entity))
                    }
                    crate::economy::Output::Horde {
                        formation,
                        member,
                        count,
                        vision,
                    } => {
                        let member_spawn = match member.kind {
                            Some(kind) => {
                                let resolved = self.resolve_build_variation(tick, kind);
                                if resolved == kind {
                                    Spawn { owner, ..*member }
                                } else {
                                    self.kinds
                                        .spawn(&self.combat, resolved, at, facing, owner)
                                        .unwrap_or(Spawn { owner, ..*member })
                                }
                            }
                            None => Spawn { owner, ..*member },
                        };
                        let rally = walk.unwrap_or(at);
                        let create = at;
                        let midpoint = create.plus(rally.minus(create).scale(0.5));
                        let Some(table) = self
                            .formations
                            .get(usize::from(*formation))
                            .filter(|table| !table.is_empty())
                        else {
                            return Ok(None);
                        };
                        let banner_offset = self
                            .banners
                            .get(usize::from(*formation))
                            .copied()
                            .flatten()
                            .map(|plan| plan.offset);
                        let horde = crate::horde::spawn_horde_shell(
                            tick,
                            Self::horde_ids(ids),
                            *formation,
                            rally,
                            facing,
                            table,
                            banner_offset,
                            |name| self.is_machine(name),
                        )?;
                        tick.state_mut().insert(ids.kind, horde, template.kind)?;
                        self.attach_upgrade_state(tick, ids, horde, Some(template.kind))?;
                        if !no_exit_path {
                            self.begin_factory_exit(tick, ids, horde)?;
                        }
                        tick.state_mut().insert(ids.owner, horde, Owner(owner))?;
                        tick.state_mut().insert(
                            ids.vision,
                            horde,
                            Vision {
                                clearing_range: *vision,
                            },
                        )?;
                        self.move_allies_from_destination(
                            tick,
                            ids,
                            horde,
                            None,
                            create,
                            &[rally],
                            false,
                        )?;
                        if *count > 0 {
                            let delay = self.production_exit_delay(tick, ids, exit_structure);
                            let next_at = tick.frame().checked_add(delay).unwrap_or(Frame::FOREVER);
                            tick.state_mut().insert(
                                ids.production_stream,
                                horde,
                                crate::horde::ProductionStream {
                                    member: member_spawn,
                                    remaining: *count,
                                    next_at,
                                    delay,
                                    create,
                                    midpoint,
                                    facing,
                                    source: exit_structure,
                                },
                            )?;
                        }
                        self.emit_voice(tick, ids, horde, crate::voice::VoiceKind::Created, None);
                        self.note_produced(
                            tick,
                            ids,
                            owner,
                            template.kind,
                            member_spawn.kind.unwrap_or(template.kind),
                            *count,
                        );
                        self.team_builder_unit_produced(
                            tick,
                            ids,
                            owner,
                            structure,
                            horde,
                            template.kind,
                        )?;
                        self.inherit_army(tick, ids, structure, horde);
                        Ok(Some(horde))
                    }
                }
            },
            |world, structure, item| {
                let upgrade = usize::try_from(item)
                    .ok()
                    .and_then(|item| self.upgrade_buildables.get(item))?;
                Some(
                    crate::economy::frames_to_produce(
                        world,
                        economy,
                        structure,
                        upgrade.build_time,
                    )
                    .get(),
                )
            },
            |tick, structure, owner, item, paid| {
                let Some(upgrade) = usize::try_from(item)
                    .ok()
                    .and_then(|item| self.upgrade_buildables.get(item))
                    .cloned()
                else {
                    return Ok(());
                };
                self.complete_upgrade(tick, ids, structure, owner, upgrade.bit, upgrade.is_player)?;
                super::map_scripts::note_upgrade_completed(
                    self,
                    tick,
                    ids,
                    owner,
                    upgrade.bit,
                    structure,
                );
                let gives_no_xp = tick
                    .state()
                    .get(ids.kind, structure)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| entry.record.production_gives_no_xp);
                if !gives_no_xp {
                    let world = tick.state();
                    if let Some(receiver) = self.experience_receiver(world, ids, structure) {
                        let scalar = self.xp_scalar(world, ids, receiver)
                            * self.level_scalar(world, ids, receiver);
                        let paid = u32::try_from(paid).unwrap_or(0);
                        let gain = crate::experience::scaled(paid, scalar);
                        if gain > 0 {
                            self.add_experience(tick, ids, receiver, gain)?;
                        }
                    }
                }
                Ok(())
            },
        )?;
        self.advance_production_streams(tick, ids)?;
        self.settle_factory_exits(tick, ids)?;
        crate::economy::recount_command_points(tick, economy)?;
        self.recount_point_limits(tick, ids)
    }

    fn advance_production_streams(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let due: Vec<(Entity, crate::horde::ProductionStream)> = tick
            .state()
            .store(ids.production_stream)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, stream)| (entity, *stream))
                    .collect()
            })
            .unwrap_or_default();
        for (horde, stream) in due {
            if tick.frame() < stream.next_at {
                continue;
            }
            let spawn_member = |tick: &mut Tick<'_>, position, facing| {
                let entity = tick.state_mut().spawn()?;
                Self::insert_spawn(
                    tick,
                    ids,
                    entity,
                    &Spawn {
                        position,
                        facing,
                        ..stream.member
                    },
                    &self.combat,
                )?;
                self.attach_upgrade_state(tick, ids, entity, stream.member.kind)?;
                Ok(entity)
            };
            if !tick.state().is_live(horde) {
                let rally = stream
                    .create
                    .plus(stream.midpoint.minus(stream.create).scale(2.0));
                let player_rally = tick
                    .state()
                    .get(ids.producer, stream.source)
                    .and_then(|producer| producer.player_rally);
                for _ in 0..stream.remaining {
                    let member = spawn_member(tick, stream.create, stream.facing)?;
                    self.begin_factory_exit(tick, ids, member)?;
                    self.route_out(tick, ids, member, stream.source, Some(rally), player_rally)?;
                    if !tick.state().contains(ids.path, member) {
                        self.end_factory_exit(tick, ids, member)?;
                    }
                    self.emit_voice(tick, ids, member, crate::voice::VoiceKind::Created, None);
                }
                tick.state_mut().remove(ids.production_stream, horde);
                continue;
            }
            let formation_table = tick
                .state()
                .get(ids.horde, horde)
                .and_then(|def| self.formations.get(usize::from(def.formation)));
            let slot_accepts = |unit_type: Option<u16>| -> bool {
                let Some(member_kind) = stream.member.kind else {
                    return true;
                };
                let Some(table) = formation_table else {
                    return false;
                };
                let Some(name) = unit_type.and_then(|at| table.unit_types().get(usize::from(at)))
                else {
                    return false;
                };
                self.kinds
                    .kind(name)
                    .is_some_and(|slot_kind| self.kinds.equivalent(member_kind, slot_kind))
            };
            let created = crate::horde::exit_one_member(
                tick,
                Self::horde_ids(ids),
                horde,
                stream.create,
                stream.facing,
                slot_accepts,
                spawn_member,
            )?;
            let (member, slot, joined) = match created {
                Some(member) => {
                    let slot =
                        crate::horde::member_slot_pose(tick.state(), Self::horde_ids(ids), member)
                            .unwrap_or(stream.midpoint);
                    (member, slot, true)
                }
                None => {
                    let slot =
                        crate::horde::horde_slot_pose(tick.state(), Self::horde_ids(ids), horde, 0)
                            .unwrap_or(stream.midpoint);
                    (
                        spawn_member(tick, stream.create, stream.facing)?,
                        slot,
                        false,
                    )
                }
            };
            self.settle_stream_member(tick, ids, &stream, member, slot)?;
            if joined
                && self.route_crosses_structure(tick, ids, stream.source, &[stream.midpoint, slot])
            {
                self.move_allies_from_destination(
                    tick,
                    ids,
                    member,
                    Some(horde),
                    stream.create,
                    &[stream.midpoint, slot],
                    true,
                )?;
            }
            let remaining = stream.remaining.saturating_sub(1);
            if remaining == 0 {
                self.finish_production_burst(tick, ids, horde, stream.source)?;
            } else {
                let next_at = tick
                    .frame()
                    .checked_add(stream.delay)
                    .unwrap_or(Frame::FOREVER);
                tick.state_mut().insert(
                    ids.production_stream,
                    horde,
                    crate::horde::ProductionStream {
                        remaining,
                        next_at,
                        ..stream
                    },
                )?;
            }
        }
        Ok(())
    }

    fn settle_stream_member(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        stream: &crate::horde::ProductionStream,
        member: Entity,
        slot: Vec2,
    ) -> Result<()> {
        let no_exit_path = tick
            .state()
            .get(ids.kind, stream.source)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.no_exit_path);
        if no_exit_path {
            tick.state_mut().insert(
                ids.ignored_obstacle,
                member,
                IgnoredObstacle(stream.source),
            )?;
            self.emit_voice(tick, ids, member, crate::voice::VoiceKind::Created, None);
            return Ok(());
        }
        self.begin_factory_exit(tick, ids, member)?;
        Self::ai_move_with(tick, ids, (member, member), |tick| {
            crate::unit::give_path(
                tick.state_mut(),
                (ids.body, ids.path),
                member,
                Path {
                    route: Route {
                        points: vec![stream.midpoint, slot],
                        extra_distance: 0.0,
                        unsearched: false,
                        portals: Vec::new(),
                    },
                    goal: slot,
                    kind: PathKind::Ordered,
                    facing: None,
                },
            )
        })?;
        tick.state_mut()
            .insert(ids.ignored_obstacle, member, IgnoredObstacle(stream.source))?;
        self.emit_voice(tick, ids, member, crate::voice::VoiceKind::Created, None);
        Ok(())
    }

    pub(super) fn finish_production_burst(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        source: Entity,
    ) -> Result<()> {
        tick.state_mut().remove(ids.production_stream, horde);
        let reform = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .is_none_or(|entry| entry.record.use_return_to_formation);
        let mut gather = crate::horde::BurstGather::KeepRoutes;
        if reform {
            self.reform_horde(tick, ids, horde)?;
            let cavalry = tick
                .state()
                .get(ids.kind, horde)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| {
                    entry
                        .record
                        .kind_of
                        .iter()
                        .any(|kind| kind.eq_ignore_ascii_case("CAVALRY"))
                });
            if !cavalry {
                if let Some(producer) = tick
                    .state()
                    .get(ids.body, source)
                    .map(|body| body.0.position)
                {
                    let clear_radius = tick
                        .state()
                        .get(ids.kind, source)
                        .and_then(|kind| self.kinds.get(*kind))
                        .map_or(0.0, |entry| crate::templates::clear_radius(&entry.record));
                    gather = crate::horde::BurstGather::ClearOfBuilding {
                        producer,
                        clear_radius,
                    };
                }
            }
        }
        let destination = tick
            .state()
            .get(ids.producer, source)
            .and_then(|producer| producer.player_rally);
        crate::horde::finish_burst(
            tick,
            Self::horde_ids(ids),
            horde,
            gather,
            &self.kinds,
            destination,
        )?;
        if destination.is_some() {
            self.end_factory_exit(tick, ids, horde)?;
        }
        Ok(())
    }

    pub(super) fn complete_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        owner: PlayerId,
        bit: UpgradeBit,
        is_player: bool,
    ) -> Result<()> {
        let mut granted = UpgradeMask::EMPTY;
        granted.set(bit);
        let granted = granted.union(&self.upgrade_bits.sub_mask(bit));
        if is_player {
            let Some(player_entity) =
                crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
            else {
                return Ok(());
            };
            let mask = tick
                .state()
                .get(ids.player_upgrades, player_entity)
                .map_or_else(UpgradeMask::default, |held| held.0.clone())
                .union(&granted);
            tick.state_mut()
                .insert(ids.player_upgrades, player_entity, PlayerUpgrades(mask))?;
            if let Some(mut in_progress) = tick
                .state()
                .get(ids.player_upgrades_in_progress, player_entity)
                .cloned()
            {
                in_progress.0.retain(|held| *held != bit);
                tick.state_mut().insert(
                    ids.player_upgrades_in_progress,
                    player_entity,
                    in_progress,
                )?;
            }
            diag!(
                "upgrade: player {} completed research (bit {})",
                owner.get(),
                bit.index()
            );
            let owned: Vec<Entity> = tick
                .state()
                .store(ids.kind)
                .map(|store| {
                    store
                        .iter()
                        .filter(|(entity, _)| {
                            tick.state()
                                .get(ids.owner, *entity)
                                .is_some_and(|held| held.0 == owner)
                        })
                        .map(|(entity, _)| entity)
                        .collect()
                })
                .unwrap_or_default();
            for entity in owned {
                self.offer_upgrades(tick, ids, entity)?;
            }
        } else {
            let mask = tick
                .state()
                .get(ids.object_upgrades, structure)
                .map_or_else(UpgradeMask::default, |held| held.0.clone())
                .union(&granted);
            tick.state_mut()
                .insert(ids.object_upgrades, structure, ObjectUpgrades(mask))?;
            diag!(
                "upgrade: entity {} completed research (bit {})",
                structure.index(),
                bit.index()
            );
            self.object_upgrade_completed(tick, ids, structure, &granted)?;
        }
        Ok(())
    }
}
