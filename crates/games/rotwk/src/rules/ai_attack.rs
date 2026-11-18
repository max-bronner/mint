use super::*;
use crate::diag::diag;

impl RotwkRules {
    pub(crate) fn ai_target_player(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        player: PlayerId,
    ) -> Option<PlayerId> {
        let economy = Self::economy_ids(ids);
        let entity = crate::economy::player_entity(world, economy, player)?;
        let lead = world.get(ids.ai_attack_progress, entity)?.lead;
        let lead_entity = crate::economy::player_entity(world, economy, lead)?;
        world
            .get(ids.ai_attack_progress, lead_entity)?
            .enemies
            .as_ref()?
            .target
    }

    pub(super) fn step_one_ai_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
        else {
            return Ok(());
        };
        let Some(mut progress) = tick
            .state()
            .get(ids.ai_attack_progress, player_entity)
            .cloned()
        else {
            return Ok(());
        };

        if tick.frame().get() >= Self::ENEMY_MANAGER_FIRST_FRAME {
            if let Some(mut manager) = progress.enemies.take() {
                self.update_enemy_manager(tick, ids, player, &mut manager);
                progress.enemies = Some(manager);
            }
        }
        let enemy = if progress.lead == player {
            progress.enemies.as_ref().and_then(|manager| manager.target)
        } else {
            self.ai_target_player(tick.state(), ids, player)
        };

        let mut launched = false;
        if progress.check_timer == 0 {
            progress.check_timer = Self::ATTACK_CHECK_FRAMES;
            let low = self.command_points_low(tick, ids, player);
            let (urgency, did_launch) = crate::skirmish_ai::advance_attack_urgency(
                &self.attack_urgency_values,
                progress.urgency,
                low,
            );
            progress.urgency = urgency;
            launched = did_launch;
        } else {
            progress.check_timer = progress.check_timer.saturating_sub(1);
        }

        if launched {
            progress.launches = progress.launches.saturating_add(1);
            diag!(
                "skirmish-ai: player {} attack urgency reached {} - launching (launch {})",
                player.get(),
                self.attack_urgency_values.launch,
                progress.launches
            );
            self.launch_attack(tick, ids, player, enemy, &mut progress)?;
        }

        self.advance_attack_wave(tick, ids, player, enemy, &mut progress)?;

        tick.state_mut()
            .insert(ids.ai_attack_progress, player_entity, progress)?;
        Ok(())
    }

    fn update_enemy_manager(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        lead: PlayerId,
        manager: &mut crate::skirmish_ai::EnemyManager,
    ) {
        let defeated: Vec<PlayerId> = manager
            .candidates
            .iter()
            .copied()
            .filter(|&player| self.player_defeated(tick.state(), ids, player))
            .collect();
        let distances: Vec<(PlayerId, f32)> = manager
            .candidates
            .iter()
            .map(|&player| (player, self.start_distance(lead, player)))
            .collect();
        let changed = manager.update(
            |player| defeated.contains(&player),
            |player| {
                distances
                    .iter()
                    .find(|(other, _)| *other == player)
                    .map_or(0.0, |(_, distance)| *distance)
            },
            || tick.rng().below(100).saturating_add(1),
        );
        if changed {
            let side = |player: PlayerId| {
                self.player_sides
                    .iter()
                    .find(|(p, _)| *p == player)
                    .map_or("?", |(_, side)| side.as_str())
            };
            match manager.target {
                Some(target) => diag!(
                    "skirmish-ai: group of player {} targets player {} (side {}), {} candidate(s) \
                     left",
                    lead.get(),
                    target.get(),
                    side(target),
                    manager.candidates.len()
                ),
                None => diag!(
                    "skirmish-ai: group of player {} has no target left",
                    lead.get()
                ),
            }
        }
    }

    fn start_distance(&self, from: PlayerId, to: PlayerId) -> f32 {
        let start = |player: PlayerId| {
            self.player_starts
                .iter()
                .find(|(p, _)| *p == player)
                .map(|(_, at)| *at)
        };
        let (Some(a), Some(b)) = (start(from), start(to)) else {
            return 0.0;
        };
        let dz = self.ground_height(b.x, b.y) - self.ground_height(a.x, a.y);
        let flat = b.minus(a).length_squared();
        (flat + dz * dz).sqrt()
    }

    pub(super) fn idle_forces(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        committed: &[Entity],
    ) -> Vec<Entity> {
        let mut out = Vec::new();
        if let Some(store) = tick.state().store(ids.horde) {
            for (entity, horde) in store.iter() {
                if committed.contains(&entity) || !tick.state().is_live(entity) {
                    continue;
                }
                let owned = horde.members.iter().any(|&member| {
                    tick.state()
                        .get(ids.owner, member)
                        .is_some_and(|o| o.0 == player)
                });
                if owned {
                    out.push(entity);
                }
            }
        }
        if let Some(store) = tick.state().store(ids.owner) {
            for (entity, owner) in store.iter() {
                if owner.0 != player || !tick.state().is_live(entity) || committed.contains(&entity)
                {
                    continue;
                }
                let is_member_or_horde = tick.state().contains(ids.member, entity)
                    || tick.state().contains(ids.horde, entity);
                let is_armed = tick.state().contains(ids.armed, entity);
                let is_immobile = Self::is_immobile(tick, ids, entity);
                let has_attack_order = tick.state().contains(ids.attack_order, entity);
                if Self::is_eligible_idle_force(
                    is_member_or_horde,
                    is_armed,
                    is_immobile,
                    has_attack_order,
                ) {
                    out.push(entity);
                }
            }
        }
        out
    }

    pub(super) const fn is_eligible_idle_force(
        is_member_or_horde: bool,
        is_armed: bool,
        is_immobile: bool,
        has_attack_order: bool,
    ) -> bool {
        !is_member_or_horde && is_armed && !is_immobile && !has_attack_order
    }

    pub(super) fn horde_members(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entities: &[Entity],
    ) -> Vec<Entity> {
        let mut out = Vec::new();
        for &entity in entities {
            if let Some(horde) = tick.state().get(ids.horde, entity) {
                out.extend(
                    horde
                        .members
                        .iter()
                        .copied()
                        .filter(|&m| tick.state().is_live(m)),
                );
            } else {
                out.push(entity);
            }
        }
        out
    }

    pub(super) fn nearest_enemy_structure(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        from: Vec2,
    ) -> Option<Entity> {
        let store = tick.state().store(ids.owner)?;
        let mut best: Option<(Entity, f32)> = None;
        for (entity, owner) in store.iter() {
            if owner.0 != player || !tick.state().is_live(entity) {
                continue;
            }
            if !self.passes_victory_filter(tick, ids, entity) {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity) else {
                continue;
            };
            let distance = from.minus(body.0.position).length_squared();
            if best.is_none_or(|(_, current)| distance < current) {
                best = Some((entity, distance));
            }
        }
        best.map(|(entity, _)| entity)
    }

    pub(super) fn launch_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        enemy: Option<PlayerId>,
        progress: &mut crate::skirmish_ai::AiAttackProgress,
    ) -> Result<()> {
        let committed: Vec<Entity> = progress
            .wave
            .as_ref()
            .map(|wave| wave.hordes.clone())
            .unwrap_or_default();
        let idle = self.idle_forces(tick, ids, player, &committed);
        if idle.is_empty() {
            diag!(
                "skirmish-ai: player {} launched with no idle army - nothing sent",
                player.get()
            );
            return Ok(());
        }
        let Some(enemy) = enemy else {
            diag!(
                "skirmish-ai: player {} launched with no target player - army held",
                player.get()
            );
            return Ok(());
        };

        let members = self.horde_members(tick, ids, &idle);
        let now = tick.frame();
        self.apply(
            tick,
            CommandKey::new(now, player, 0),
            &RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )?;

        match &mut progress.wave {
            Some(wave) => {
                match wave.phase {
                    crate::skirmish_ai::AttackPhase::EnRoute { marker } => {
                        self.apply(
                            tick,
                            CommandKey::new(now, player, 1),
                            &RotwkCommand::MoveTo {
                                x: marker.x,
                                y: marker.y,
                                queue: false,
                            },
                        )?;
                    }
                    crate::skirmish_ai::AttackPhase::Assaulting { target } => {
                        self.apply(
                            tick,
                            CommandKey::new(now, player, 1),
                            &RotwkCommand::AttackObject { target },
                        )?;
                    }
                }
                diag!(
                    "skirmish-ai: player {} reinforces its attack wave with {} more force(s)",
                    player.get(),
                    idle.len()
                );
                wave.hordes.extend(idle);
            }
            None => {
                let base = self
                    .player_structure_centre(tick, ids, player)
                    .unwrap_or(Vec2::ZERO);
                let roll = tick.rng().below(3);
                let marker = crate::skirmish_ai::pick_route(&self.route_markers, base, roll);
                let phase = if let Some(marker) = marker {
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 1),
                        &RotwkCommand::MoveTo {
                            x: marker.x,
                            y: marker.y,
                            queue: false,
                        },
                    )?;
                    diag!(
                        "skirmish-ai: player {} launches {} force(s) at enemy {} via a route \
                         marker ({:.0}, {:.0})",
                        player.get(),
                        idle.len(),
                        enemy.get(),
                        marker.x,
                        marker.y
                    );
                    crate::skirmish_ai::AttackPhase::EnRoute { marker }
                } else if let Some(target) = self.nearest_enemy_structure(tick, ids, enemy, base) {
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 1),
                        &RotwkCommand::AttackObject { target },
                    )?;
                    diag!(
                        "skirmish-ai: player {} launches {} force(s) directly at enemy {}",
                        player.get(),
                        idle.len(),
                        enemy.get()
                    );
                    crate::skirmish_ai::AttackPhase::Assaulting { target }
                } else {
                    diag!(
                        "skirmish-ai: player {} launched but enemy {} has no \
                         VictoryConditionStructureObjectFilter-passing structure - army held idle",
                        player.get(),
                        enemy.get()
                    );
                    return Ok(());
                };
                progress.wave = Some(crate::skirmish_ai::AttackWave {
                    phase,
                    hordes: idle,
                });
            }
        }
        Ok(())
    }

    pub(super) fn advance_attack_wave(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        enemy: Option<PlayerId>,
        progress: &mut crate::skirmish_ai::AiAttackProgress,
    ) -> Result<()> {
        let Some(wave) = &mut progress.wave else {
            return Ok(());
        };
        wave.hordes.retain(|&entity| tick.state().is_live(entity));
        if wave.hordes.is_empty() {
            progress.wave = None;
            return Ok(());
        }

        match wave.phase {
            crate::skirmish_ai::AttackPhase::EnRoute { .. } => {
                let all_arrived = wave
                    .hordes
                    .iter()
                    .all(|&entity| !tick.state().contains(ids.path, entity));
                if !all_arrived {
                    return Ok(());
                }
                let Some(enemy) = enemy else {
                    return Ok(());
                };
                let base = self
                    .player_structure_centre(tick, ids, player)
                    .unwrap_or(Vec2::ZERO);
                if let Some(target) = self.nearest_enemy_structure(tick, ids, enemy, base) {
                    let members = self.horde_members(tick, ids, &wave.hordes);
                    let now = tick.frame();
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 0),
                        &RotwkCommand::SetSelection {
                            replace: true,
                            units: members,
                        },
                    )?;
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 1),
                        &RotwkCommand::AttackObject { target },
                    )?;
                    diag!(
                        "skirmish-ai: player {} arrived at its route marker - assaulting enemy {}",
                        player.get(),
                        enemy.get()
                    );
                    wave.phase = crate::skirmish_ai::AttackPhase::Assaulting { target };
                }
            }
            crate::skirmish_ai::AttackPhase::Assaulting { target } => {
                let still_good =
                    tick.state().is_live(target) && self.passes_victory_filter(tick, ids, target);
                if still_good {
                    return Ok(());
                }
                let Some(enemy) = enemy else {
                    return Ok(());
                };
                let base = self
                    .player_structure_centre(tick, ids, player)
                    .unwrap_or(Vec2::ZERO);
                if let Some(next) = self.nearest_enemy_structure(tick, ids, enemy, base) {
                    let members = self.horde_members(tick, ids, &wave.hordes);
                    let now = tick.frame();
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 0),
                        &RotwkCommand::SetSelection {
                            replace: true,
                            units: members,
                        },
                    )?;
                    self.apply(
                        tick,
                        CommandKey::new(now, player, 1),
                        &RotwkCommand::AttackObject { target: next },
                    )?;
                    diag!(
                        "skirmish-ai: player {} re-targets enemy {}'s next structure",
                        player.get(),
                        enemy.get()
                    );
                    wave.phase = crate::skirmish_ai::AttackPhase::Assaulting { target: next };
                } else {
                    diag!(
                        "skirmish-ai: player {} has no more targets against enemy {} - wave stands down",
                        player.get(),
                        enemy.get()
                    );
                    progress.wave = None;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn attack_census(&self, world: &World, ids: Ids) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.ai_seeds.len());
        for seed in &self.ai_seeds {
            let Some(player_entity) =
                crate::economy::player_entity(world, Self::economy_ids(ids), seed.player)
            else {
                continue;
            };
            let Some(progress) = world.get(ids.ai_attack_progress, player_entity) else {
                continue;
            };
            let wave = match &progress.wave {
                None => String::from("no wave"),
                Some(wave) => format!(
                    "{} force(s) {}",
                    wave.hordes.len(),
                    match wave.phase {
                        crate::skirmish_ai::AttackPhase::EnRoute { .. } => "en route",
                        crate::skirmish_ai::AttackPhase::Assaulting { .. } => "assaulting",
                    }
                ),
            };
            lines.push(format!(
                "skirmish-ai: player {} attack: urgency A{}/D{}/E{}, enemy {}, {} launch(es), {wave}",
                seed.player.get(),
                progress.urgency.attack,
                progress.urgency.defense,
                progress.urgency.economy,
                self.ai_target_player(world, ids, seed.player)
                    .map_or_else(|| String::from("none"), |e| e.get().to_string()),
                progress.launches,
            ));
        }
        lines
    }
}
