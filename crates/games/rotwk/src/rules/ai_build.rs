use super::*;
use crate::diag::diag;

const PLACEMENT_RETRY_STEP: f32 = 15.0;

impl RotwkRules {
    pub(super) const AI_ECONOMY_INTERVAL_FRAMES: u64 = 90;

    pub(super) const AI_POWER_DETECTION_RANGE: f32 = 400.0;

    const ARMY_PRODUCTION_ATTEMPT_FRAMES: u32 = SIMULATION_HZ * 2;

    pub(super) const ATTACK_CHECK_FRAMES: u32 = SIMULATION_HZ * 2;

    pub(super) const ENEMY_MANAGER_FIRST_FRAME: u64 = 10;

    pub(super) fn step_skirmish_ai(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        for slot in 0..self.ai_plan.len() {
            let Some((player, tasks)) = self.ai_plan.get(slot).cloned() else {
                continue;
            };
            self.step_one_ai_player(tick, ids, player, &tasks)?;
        }
        for slot in 0..self.ai_army_plans.len() {
            let Some(plan) = self.ai_army_plans.get(slot).cloned() else {
                continue;
            };
            self.step_one_ai_army(tick, ids, &plan)?;
        }
        for slot in 0..self.ai_seeds.len() {
            let Some(player) = self.ai_seeds.get(slot).map(|seed| seed.player) else {
                continue;
            };
            self.step_one_ai_attack(tick, ids, player)?;
        }
        for slot in 0..self.ai_seeds.len() {
            let Some(player) = self.ai_seeds.get(slot).map(|seed| seed.player) else {
                continue;
            };
            self.step_ai_upgrades_and_powers(tick, ids, player)?;
            self.step_ai_spells(tick, ids, player)?;
        }
        Ok(())
    }

    pub(super) fn step_ai_upgrades_and_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        if tick.frame().get() % Self::AI_ECONOMY_INTERVAL_FRAMES != 0 {
            return Ok(());
        }
        let now = tick.frame();
        let owned: Vec<Entity> = tick
            .state()
            .store(ids.owner)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, owner)| owner.0 == player)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        if let Some((press_on, button, is_player)) =
            self.ai_upgrade_choice(tick.state(), ids, player)
        {
            self.apply(
                tick,
                CommandKey::new(now, player, 0),
                &RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![press_on],
                },
            )?;
            let command = if is_player {
                RotwkCommand::PlayerUpgrade { button }
            } else {
                RotwkCommand::ObjectUpgrade { button }
            };
            self.apply(tick, CommandKey::new(now, player, 1), &command)?;
        }
        for &structure in &owned {
            let Some(kind) = tick.state().get(ids.kind, structure).copied() else {
                continue;
            };
            let Some(entry) = self.kinds.get(kind) else {
                continue;
            };
            let Some((_, TargetKind::None | TargetKind::Confirm)) = entry.power_buttons.first()
            else {
                continue;
            };
            let Some(position) = tick
                .state()
                .get(ids.body, structure)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let range_squared = Self::AI_POWER_DETECTION_RANGE * Self::AI_POWER_DETECTION_RANGE;
            let enemy_near = self.living(tick, ids).iter().any(|(foe, _)| {
                self.object_hostile(tick.state(), structure, foe.entity) && {
                    let dx = foe.position.x - position.x;
                    let dy = foe.position.y - position.y;
                    dx * dx + dy * dy <= range_squared
                }
            });
            if !enemy_near {
                continue;
            }
            self.apply(
                tick,
                CommandKey::new(now, player, 3),
                &RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![structure],
                },
            )?;
            self.apply(
                tick,
                CommandKey::new(now, player, 4),
                &RotwkCommand::FireSpecialPower {
                    button: 0,
                    target: None,
                    at: None,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn ai_upgrade_choice(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> Option<(Entity, u8, bool)> {
        let economy = Self::economy_ids(ids);
        let producers = world.store(ids.producer)?;
        for (structure, producer) in producers.iter() {
            if world.get(ids.owner, structure).map(|owner| owner.0) != Some(player)
                || !world.is_live(structure)
                || world.contains(ids.doomed, structure)
            {
                continue;
            }
            for is_player in [false, true] {
                let (menus, menu) = if is_player {
                    (&self.player_upgrade_menus, producer.player_menu)
                } else {
                    (&self.object_upgrade_menus, producer.object_menu)
                };
                let Some(menu) = menu else {
                    continue;
                };
                let offered = menus.get(usize::from(menu)).map_or(0, Vec::len);
                let gate = |world: &World, s: Entity, m: u16, b: u8| {
                    self.upgrade_gate(world, ids, s, is_player, m, b)
                };
                let accepted = (0..offered)
                    .filter_map(|button| u8::try_from(button).ok())
                    .find(|button| {
                        crate::economy::upgrade_check(
                            world,
                            economy,
                            &self.kinds,
                            &self.upgrade_bits,
                            player,
                            structure,
                            *button,
                            is_player,
                            menus,
                            &self.upgrade_buildables,
                            &gate,
                            self.production_cost_change_percent(player),
                        )
                        .is_ok()
                    });
                let Some(button) = accepted else {
                    continue;
                };
                let press_on = world
                    .get(ids.horde, structure)
                    .and_then(|horde| {
                        horde
                            .members
                            .iter()
                            .copied()
                            .filter(|m| world.is_live(*m) && !world.contains(ids.doomed, *m))
                            .min_by_key(|m| m.index())
                    })
                    .unwrap_or(structure);
                return Some((press_on, button, is_player));
            }
        }
        None
    }

    pub(super) fn hero_page_offer(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> Option<(Entity, u8, i64)> {
        let slot = self
            .roster_of(world, ids, player)?
            .0
            .iter()
            .position(|entry| {
                entry.kind == Some(kind)
                    && matches!(
                        entry.state,
                        crate::heroes::HeroState::Available | crate::heroes::HeroState::Dead { .. }
                    )
            })?;
        let button = u8::try_from(slot).ok()?;
        let mut short_of_money = None;
        for &structure in world.store(ids.producer)?.entities() {
            match self.hero_order(world, ids, player, structure, button) {
                Ok(order) => return Some((structure, button, order.cost)),
                Err(crate::heroes::HeroRefusal::CantAfford { cost }) => {
                    short_of_money = short_of_money.or(Some((structure, button, cost)));
                }
                Err(_) => {}
            }
        }
        short_of_money
    }

    pub(super) fn press_hero_page(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
        button: u8,
    ) -> Result<bool> {
        let now = tick.frame();
        self.apply(
            tick,
            CommandKey::new(now, player, 0),
            &RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )?;
        self.apply(
            tick,
            CommandKey::new(now, player, 1),
            &RotwkCommand::RecruitHero { button },
        )?;
        Ok(self
            .roster_of(tick.state(), ids, player)
            .and_then(|roster| roster.0.get(usize::from(button)))
            .is_some_and(|entry| matches!(entry.state, crate::heroes::HeroState::Queued { .. })))
    }

    pub(super) fn try_revive_a_hero(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        spendable: i64,
    ) -> Result<bool> {
        let dead: Vec<crate::unit::Kind> = self
            .roster_of(tick.state(), ids, player)
            .map(|roster| {
                roster
                    .0
                    .iter()
                    .filter(|entry| matches!(entry.state, crate::heroes::HeroState::Dead { .. }))
                    .filter_map(|entry| entry.kind)
                    .collect()
            })
            .unwrap_or_default();
        for kind in dead {
            let Some((structure, button, cost)) =
                self.hero_page_offer(tick.state(), ids, player, kind)
            else {
                continue;
            };
            if cost <= spendable && self.press_hero_page(tick, ids, player, structure, button)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn step_hero_build_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        plan: &crate::skirmish_ai::ArmyPlan,
        progress: &mut crate::skirmish_ai::AiArmyProgress,
        spendable: i64,
    ) -> Result<()> {
        let player = plan.player;
        let order = &plan.hero_build_order;
        let mut head = usize::try_from(progress.hero_index).unwrap_or(usize::MAX);
        while let Some(entry) = order.get(head) {
            let settled =
                entry.is_none_or(|kind| self.hero_is_settled(tick.state(), ids, player, kind));
            if !settled {
                break;
            }
            head = head.saturating_add(1);
        }
        progress.hero_index = u32::try_from(head).unwrap_or(u32::MAX);
        for (at, entry) in order.iter().enumerate().skip(head) {
            let Some(kind) = *entry else {
                continue;
            };
            if self.hero_is_settled(tick.state(), ids, player, kind) {
                continue;
            }
            let on_roster = self
                .roster_of(tick.state(), ids, player)
                .is_some_and(|roster| roster.0.iter().any(|slot| slot.kind == Some(kind)));
            if !on_roster {
                if at == head && self.try_queue(tick, ids, player, kind, spendable)? {
                    progress.hero_index = progress.hero_index.saturating_add(1);
                }
                return Ok(());
            }
            if let Some((structure, button, cost)) =
                self.hero_page_offer(tick.state(), ids, player, kind)
            {
                if cost <= spendable {
                    self.press_hero_page(tick, ids, player, structure, button)?;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    pub(super) fn hero_is_settled(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> bool {
        let entry = self
            .roster_of(world, ids, player)
            .and_then(|roster| roster.0.iter().find(|entry| entry.kind == Some(kind)));
        match entry {
            Some(entry) => matches!(
                entry.state,
                crate::heroes::HeroState::Alive(_) | crate::heroes::HeroState::Queued { .. }
            ),
            None => !self.kinds.trains(kind),
        }
    }

    pub(super) fn step_one_ai_player(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        tasks: &[crate::skirmish_ai::AiTask],
    ) -> Result<()> {
        use crate::skirmish_ai::AiTaskState;

        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
        else {
            return Ok(());
        };
        let Some(mut progress) = tick.state().get(ids.ai_progress, player_entity).cloned() else {
            return Ok(());
        };

        for (index, state) in progress.tasks.iter_mut().enumerate() {
            match *state {
                AiTaskState::Ordered(porter) => {
                    if let Some(site) = Self::site_of_builder(tick, ids, porter) {
                        *state = AiTaskState::Underway(site);
                    } else if !tick.state().is_live(porter)
                        || !tick.state().contains(ids.build_task, porter)
                    {
                        *state = AiTaskState::Pending;
                    }
                }
                AiTaskState::Underway(site) => {
                    if !tick.state().is_live(site) {
                        *state = AiTaskState::Pending;
                    } else if tick.state().get(ids.construction, site).is_none() {
                        diag!(
                            "skirmish-ai: player {} finished building task {index} ({})",
                            player.get(),
                            self.kind_name(tasks.get(index).map(|t| t.kind))
                        );
                        *state = AiTaskState::Built(site);
                    }
                }
                AiTaskState::Built(site) => {
                    if !tick.state().is_live(site) {
                        diag!(
                            "skirmish-ai: player {} lost task {index} ({}) - queued for rebuild",
                            player.get(),
                            self.kind_name(tasks.get(index).map(|t| t.kind))
                        );
                        *state = AiTaskState::Destroyed(tick.frame());
                    }
                }
                AiTaskState::Pending | AiTaskState::Destroyed(_) => {}
            }
        }

        if progress.structure_timer > 0 {
            progress.structure_timer = progress.structure_timer.saturating_sub(1);
            tick.state_mut()
                .insert(ids.ai_progress, player_entity, progress)?;
            return Ok(());
        }

        let building_now = progress
            .tasks
            .iter()
            .any(|state| matches!(state, AiTaskState::Ordered(_) | AiTaskState::Underway(_)));

        let purse = crate::economy::purse_of(tick.state(), economy, player).amount();
        progress.structure_timer = crate::skirmish_ai::structure_timer_frames(
            self.ai_timing.structure_seconds,
            self.ai_timing.wealthy_rate,
            self.ai_timing.poor_rate,
            self.ai_timing.wealthy_threshold,
            self.ai_timing.poor_threshold,
            purse,
        );

        if building_now {
            tick.state_mut()
                .insert(ids.ai_progress, player_entity, progress)?;
            return Ok(());
        }

        let max_phase = crate::skirmish_ai::allowed_phase(tasks, &progress.tasks);
        let now = tick.frame();
        let delay = LogicFrames::from_seconds(self.ai_timing.rebuild_delay_seconds);
        let next = tasks.iter().enumerate().find(|(index, task)| {
            task.phase <= max_phase
                && match progress.tasks.get(*index) {
                    Some(AiTaskState::Pending) => true,
                    Some(AiTaskState::Destroyed(at)) => {
                        crate::skirmish_ai::rebuild_ready(*at, now, delay)
                    }
                    _ => false,
                }
        });
        let Some((index, task)) = next else {
            tick.state_mut()
                .insert(ids.ai_progress, player_entity, progress)?;
            return Ok(());
        };

        if let Some(entry) = self.kinds.get(task.kind) {
            if let Some((cost, _)) = self.price_for(tick.state(), ids, player, &entry.record) {
                let purse = crate::economy::purse_of(tick.state(), economy, player).amount();
                if !mint_sage_economy::Money::new(purse).can_afford(cost) {
                    tick.state_mut()
                        .insert(ids.ai_progress, player_entity, progress)?;
                    return Ok(());
                }
            }
        }

        let Some(porter) = self.find_idle_porter(tick, ids, player) else {
            diag!(
                "skirmish-ai: player {} wants to build {} but has no idle porter",
                player.get(),
                self.kind_name(Some(task.kind))
            );
            tick.state_mut()
                .insert(ids.ai_progress, player_entity, progress)?;
            return Ok(());
        };
        let from = tick
            .state()
            .get(ids.body, porter)
            .map_or(task.position, |body| body.0.position);

        let Some((x, y)) = self.find_buildable_spot(tick, ids, player, task, from) else {
            diag!(
                "skirmish-ai: player {} found no buildable spot for {} near ({:.0}, {:.0}) - \
                 every spot in the retry spiral was refused",
                player.get(),
                self.kind_name(Some(task.kind)),
                task.position.x,
                task.position.y
            );
            tick.state_mut()
                .insert(ids.ai_progress, player_entity, progress)?;
            return Ok(());
        };

        self.apply(
            tick,
            CommandKey::new(now, player, 0),
            &RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter],
            },
        )?;
        self.apply(
            tick,
            CommandKey::new(now, player, 1),
            &RotwkCommand::Construct {
                kind: task.kind.0,
                x,
                y,
                angle: task.angle,
            },
        )?;

        if tick.state().contains(ids.build_task, porter) {
            if let Some(slot) = progress.tasks.get_mut(index) {
                *slot = AiTaskState::Ordered(porter);
            }
            diag!(
                "skirmish-ai: player {} ordered porter {} to build {} at ({x:.0}, {y:.0}) - \
                 task {index}, priority {}",
                player.get(),
                porter.index(),
                self.kind_name(Some(task.kind)),
                task.priority
            );
        }
        tick.state_mut()
            .insert(ids.ai_progress, player_entity, progress)?;
        Ok(())
    }

    pub(super) fn step_one_ai_army(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        plan: &crate::skirmish_ai::ArmyPlan,
    ) -> Result<()> {
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, plan.player)
        else {
            return Ok(());
        };
        let Some(mut progress) = tick
            .state()
            .get(ids.ai_army_progress, player_entity)
            .copied()
        else {
            return Ok(());
        };

        if progress.production_timer > 0 {
            progress.production_timer = progress.production_timer.saturating_sub(1);
            tick.state_mut()
                .insert(ids.ai_army_progress, player_entity, progress)?;
            return Ok(());
        }
        progress.production_timer = Self::ARMY_PRODUCTION_ATTEMPT_FRAMES;

        let elapsed_seconds = tick
            .frame()
            .since(Frame::ZERO)
            .map_or(0.0, LogicFrames::as_seconds);
        let phase = crate::skirmish_ai::army_phase(
            elapsed_seconds,
            plan.phase_duration_rush,
            plan.phase_duration_mid_game,
        );
        let phase_index = usize::from(phase.saturating_sub(1)).min(2);

        let points = crate::economy::points_of(tick.state(), economy, plan.player);
        let queued_points = self.queued_command_points(tick, ids, plan.player);
        let used_and_queued = points.used().saturating_add(queued_points);
        let floor = plan
            .must_use_command_point_percentage
            .get(phase_index)
            .copied()
            .unwrap_or(0.0);
        let must_spend =
            crate::skirmish_ai::below_command_point_floor(used_and_queued, points.limit(), floor);

        let purse = crate::economy::purse_of(tick.state(), economy, plan.player).amount();
        let percent_to_save = plan
            .percent_to_save
            .get(phase_index)
            .copied()
            .unwrap_or(0.0);
        let spendable = crate::skirmish_ai::spendable_purse(purse, percent_to_save, must_spend);

        if !self.try_revive_a_hero(tick, ids, plan.player, spendable)? {
            self.step_hero_build_order(tick, ids, plan, &mut progress, spendable)?;
        }

        let counts: Vec<u32> = plan
            .members
            .iter()
            .map(|member| {
                member
                    .kind
                    .map_or(0, |kind| self.army_count(tick, ids, plan.player, kind))
            })
            .collect();
        let remaining_points = points.limit().saturating_sub(used_and_queued);
        for index in crate::skirmish_ai::deviation_order(&plan.members, &counts, phase) {
            let Some(kind) = plan.members.get(index).and_then(|member| member.kind) else {
                continue;
            };
            let fits_points = self
                .buildables
                .iter()
                .find(|buildable| buildable.kind == kind)
                .is_some_and(|buildable| buildable.command_points <= remaining_points);
            if !fits_points {
                continue;
            }
            if self.try_queue(tick, ids, plan.player, kind, spendable)? {
                break;
            }
        }

        if let Some(porter_kind) = plan.porter_kind {
            let have = self.army_count(tick, ids, plan.player, porter_kind);
            if i64::from(have) < plan.porter_target {
                self.try_queue(tick, ids, plan.player, porter_kind, purse)?;
            }
        }

        tick.state_mut()
            .insert(ids.ai_army_progress, player_entity, progress)?;
        Ok(())
    }

    pub(super) fn player_structure_centre(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Option<Vec2> {
        let store = tick.state().store(ids.owner)?;
        let mut lo: Option<(f32, f32)> = None;
        let mut hi: Option<(f32, f32)> = None;
        for (entity, owner) in store.iter() {
            if owner.0 != player || !tick.state().is_live(entity) {
                continue;
            }
            let is_structure = tick
                .state()
                .get(ids.shape, entity)
                .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE));
            if !is_structure {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity) else {
                continue;
            };
            let (x, y) = (body.0.position.x, body.0.position.y);
            lo = Some(lo.map_or((x, y), |(lx, ly)| (lx.min(x), ly.min(y))));
            hi = Some(hi.map_or((x, y), |(hx, hy)| (hx.max(x), hy.max(y))));
        }
        match (lo, hi) {
            (Some((lx, ly)), Some((hx, hy))) => Some(Vec2::new((lx + hx) / 2.0, (ly + hy) / 2.0)),
            _ => None,
        }
    }

    pub(super) fn command_points_low(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> bool {
        let economy = Self::economy_ids(ids);
        let points = crate::economy::points_of(tick.state(), economy, player);
        let queued = self.queued_command_points(tick, ids, player);
        let used_and_queued = points.used().saturating_add(queued);
        let available = points.limit().saturating_sub(used_and_queued);
        crate::skirmish_ai::command_points_low(
            available,
            self.attack_urgency_values.low_command_points,
        )
    }

    pub(super) fn passes_victory_filter(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let Some(kind) = tick.state().get(ids.kind, entity) else {
            return false;
        };
        let Some(entry) = self.kinds.get(*kind) else {
            return false;
        };
        self.victory_structure_filter
            .passes(&entry.record.kind_of, &entry.record.name)
    }

    pub(super) fn queued_command_points(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> i32 {
        let Some(store) = tick.state().store(ids.producer) else {
            return 0;
        };
        let mut total = 0i32;
        for (entity, producer) in store.iter() {
            if tick
                .state()
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player)
            {
                total = total.saturating_add(producer.queue.reserved_command_points());
            }
        }
        total
    }

    pub(super) fn army_count(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> u32 {
        let mut count = 0u32;
        if let Some(store) = tick.state().store(ids.kind) {
            for (entity, found) in store.iter() {
                if *found == kind
                    && tick.state().is_live(entity)
                    && tick
                        .state()
                        .get(ids.owner, entity)
                        .is_some_and(|owner| owner.0 == player)
                {
                    count = count.saturating_add(1);
                }
            }
        }
        if let Some(store) = tick.state().store(ids.producer) {
            for (entity, producer) in store.iter() {
                if tick
                    .state()
                    .get(ids.owner, entity)
                    .is_none_or(|owner| owner.0 != player)
                {
                    continue;
                }
                for entry in producer.queue.entries() {
                    let Ok(index) = usize::try_from(entry.item.index()) else {
                        continue;
                    };
                    if u16::try_from(index)
                        .ok()
                        .and_then(|index| self.buildable_at(index))
                        .is_some_and(|b| b.kind == kind)
                    {
                        count = count.saturating_add(entry.quantity_remaining());
                    }
                }
            }
        }
        count
    }

    pub(super) fn find_producer_for(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: crate::unit::Kind,
    ) -> Option<(Entity, u8)> {
        let store = tick.state().store(ids.producer)?;
        for (entity, _) in store.iter() {
            if !tick.state().is_live(entity) {
                continue;
            }
            if tick
                .state()
                .get(ids.owner, entity)
                .is_none_or(|owner| owner.0 != player)
            {
                continue;
            }
            let Some(structure_kind) = tick.state().get(ids.kind, entity).copied() else {
                continue;
            };
            let Some(entry) = self.kinds.get(structure_kind) else {
                continue;
            };
            if let Some(button) = entry
                .builds
                .iter()
                .position(|built| *built == target)
                .and_then(|position| u8::try_from(position).ok())
            {
                return Some((entity, button));
            }
        }
        None
    }

    pub(super) fn try_queue(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: crate::unit::Kind,
        spendable: i64,
    ) -> Result<bool> {
        if self.is_disallowed(tick.state(), ids, player, target) {
            return Ok(false);
        }
        let Some(cost) = self
            .buildables
            .iter()
            .find(|buildable| buildable.kind == target)
            .map(|buildable| buildable.cost)
        else {
            return Ok(false);
        };
        if !mint_sage_economy::Money::new(spendable).can_afford(cost) {
            return Ok(false);
        }
        let Some((structure, button)) = self.find_producer_for(tick, ids, player, target) else {
            return Ok(false);
        };
        let now = tick.frame();
        self.apply(
            tick,
            CommandKey::new(now, player, 0),
            &RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )?;
        self.apply(
            tick,
            CommandKey::new(now, player, 1),
            &RotwkCommand::QueueUnit { button },
        )?;
        Ok(true)
    }

    fn site_of_builder(tick: &Tick<'_>, ids: Ids, builder: Entity) -> Option<Entity> {
        tick.state()
            .store(ids.construction)?
            .iter()
            .find(|(_, construction)| construction.builder == Some(builder))
            .map(|(site, _)| site)
    }

    pub(super) fn find_idle_porter(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Option<Entity> {
        let owners = tick.state().store(ids.owner)?;
        for (entity, owner) in owners.iter() {
            if owner.0 != player || !tick.state().is_live(entity) {
                continue;
            }
            if tick.state().get(ids.build_task, entity).is_some() {
                continue;
            }
            if tick.state().get(ids.concealed, entity).is_some() {
                continue;
            }
            let is_dozer = tick
                .state()
                .get(ids.shape, entity)
                .is_some_and(|shape| shape.kind_of.contains(KindOf::DOZER));
            if is_dozer {
                return Some(entity);
            }
        }
        None
    }

    pub(super) fn find_buildable_spot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        task: &crate::skirmish_ai::AiTask,
        from: Vec2,
    ) -> Option<(f32, f32)> {
        let entry = self.kinds.get(task.kind)?;
        let footprint = footprint_of(&entry.record);
        if footprint.is_empty() {
            return None;
        }
        for offset in crate::skirmish_ai::retry_spiral(PLACEMENT_RETRY_STEP) {
            let centre = Vec2::new(task.position.x + offset.x, task.position.y + offset.y);
            match self.validate_placement(
                tick.state(),
                ids,
                player,
                &entry.record,
                &footprint,
                centre,
                task.angle,
                from,
            ) {
                Ok(()) => return Some((centre.x, centre.y)),
                Err(reason) => diag!(
                    "skirmish-ai: player {} refused {} at ({:.0}, {:.0}) - {}",
                    player.get(),
                    entry.record.name,
                    centre.x,
                    centre.y,
                    reason.message()
                ),
            }
        }
        None
    }

    pub(super) fn kind_name(&self, kind: Option<crate::unit::Kind>) -> &str {
        kind.and_then(|kind| self.kinds.get(kind))
            .map_or("?", |entry| entry.record.name.as_str())
    }

    pub(crate) fn skirmish_ai_census(&self, world: &World, ids: Ids) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.ai_plan.len());
        for (player, tasks) in &self.ai_plan {
            let Some(player_entity) =
                crate::economy::player_entity(world, Self::economy_ids(ids), *player)
            else {
                continue;
            };
            let Some(progress) = world.get(ids.ai_progress, player_entity) else {
                continue;
            };
            let built = progress
                .tasks
                .iter()
                .filter(|state| matches!(state, crate::skirmish_ai::AiTaskState::Built(_)))
                .count();
            let ordered = progress
                .tasks
                .iter()
                .filter(|state| {
                    matches!(
                        state,
                        crate::skirmish_ai::AiTaskState::Ordered(_)
                            | crate::skirmish_ai::AiTaskState::Underway(_)
                    )
                })
                .count();
            let destroyed = progress
                .tasks
                .iter()
                .filter(|state| matches!(state, crate::skirmish_ai::AiTaskState::Destroyed(_)))
                .count();
            lines.push(format!(
                "skirmish-ai: player {} has {built} of {} base building(s) standing \
                 ({ordered} under construction, {destroyed} awaiting rebuild)",
                player.get(),
                tasks.len()
            ));
        }
        lines
    }
}
