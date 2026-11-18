use crate::battle::{reborrow, AutoResolve};
use crate::model::{ArmyId, Command, Event, Phase, PlayerId, PlayerKind, RegionId, Rejected};
use crate::state::LivingWorldState;

const MAX_STEPS: u8 = 16;

pub fn apply(
    state: &mut LivingWorldState,
    player: PlayerId,
    command: Command,
) -> Result<Vec<Event>, Rejected> {
    apply_with(state, None, player, command)
}

pub fn apply_with(
    state: &mut LivingWorldState,
    mut resolver: Option<&mut dyn AutoResolve>,
    player: PlayerId,
    command: Command,
) -> Result<Vec<Event>, Rejected> {
    if state.ended.is_some() {
        return Err(Rejected::SessionEnded);
    }
    let seat = state.player(player).ok_or(Rejected::UnknownPlayer)?;
    if seat.defeated {
        return Err(Rejected::PlayerDefeated);
    }
    if state.awaiting.is_some()
        && !matches!(
            command,
            Command::ApplyBattleResult { .. } | Command::Surrender { .. }
        )
    {
        return Err(Rejected::AwaitingBattle);
    }
    let phase = state.phase;
    let need = |wanted: Phase| {
        if phase == wanted {
            Ok(())
        } else {
            Err(Rejected::WrongPhase)
        }
    };
    let mut events = match command {
        Command::EndPhase => {
            need(Phase::Planning)?;
            if let Some(p) = state.player_mut(player) {
                p.ready = true;
            }
            Vec::new()
        }
        Command::MoveArmy { army, dest } => {
            need(Phase::Planning)?;
            state.check_order(player, army, dest, false)?;
            let mut events = Vec::new();
            state.set_order(army, dest, &mut events);
            events
        }
        Command::CancelMove { army } => {
            need(Phase::Planning)?;
            let own = state.armies.get(&army).ok_or(Rejected::NoSuchArmy)?;
            if own.player != player {
                return Err(Rejected::NotYourArmy);
            }
            let region = own.region;
            let mut events = Vec::new();
            state.set_order(army, region, &mut events);
            events
        }
        Command::ChooseRetreat { army, dest } => {
            need(Phase::PlanRetreats)?;
            state.order_retreat(player, army, dest)?
        }
        Command::Build {
            region,
            plot,
            building,
        } => {
            need(Phase::Planning)?;
            state.order_build(player, region, plot, building)?
        }
        Command::Demolish { region, plot } => {
            need(Phase::Planning)?;
            state.order_demolish(player, region, plot)?
        }
        Command::DisbandMember { army, member } => {
            state.order_disband(player, army, Some(member))?
        }
        Command::DisbandAll { army } => state.order_disband(player, army, None)?,
        Command::SwapArmyMembers {
            a,
            b,
            a_members,
            b_members,
        } => {
            need(Phase::Planning)?;
            state.order_swap(player, a, b, &a_members, &b_members)?
        }
        Command::QueueArmy {
            region,
            plot,
            entry,
        } => state.order_queue(player, region, plot, entry)?,
        Command::CancelQueuedArmy {
            region,
            plot,
            index,
        } => state.order_cancel_queued(player, region, plot, index)?,
        Command::ChooseSharedCapture {
            region,
            player: chosen,
        } => {
            need(Phase::ResolveBattles)?;
            state.order_choose_capture(player, region, chosen)?
        }
        Command::ResolveBattle { region, mode } => {
            need(Phase::ResolveBattles)?;
            state.order_resolve(player, region, mode, reborrow(&mut resolver))?
        }
        Command::ApplyBattleResult { region, result } => {
            need(Phase::ResolveBattles)?;
            state.order_apply_result(player, region, &result)?
        }
        Command::LinkMemberForUpgrade { army, member } => {
            need(Phase::Planning)?;
            state.order_link_upgrade(player, army, member)?
        }
        Command::UnlinkMemberForUpgrade { army, member } => {
            need(Phase::Planning)?;
            state.order_unlink_upgrade(player, army, member)?
        }
        Command::ChooseBattleMode { battle, method } => {
            need(Phase::ResolveBattles)?;
            state.order_choose_mode(player, battle, method, reborrow(&mut resolver))?
        }
        Command::SwapArmiesInBattle {
            battle,
            side,
            slot,
            a,
            b,
        } => {
            need(Phase::ResolveBattles)?;
            state.order_swap_in_battle(player, battle, side, slot, a, b)?
        }
        Command::ForceRealTime { battle } => {
            need(Phase::ResolveBattles)?;
            state.order_force_real_time(battle)?
        }
        Command::Surrender { player: target } => state.order_surrender(player, target)?,
        Command::AutoResolveStep { battle, round } => {
            state.order_dialog_step(player, battle, Some(round))?
        }
        Command::AutoResolveSkip { battle } => state.order_dialog_step(player, battle, None)?,
        Command::LeaveSession => vec![Event::PlayerLeft { player }],
    };
    flush(state, &mut events);
    pump(state, resolver, &mut events);
    Ok(events)
}

fn flush(state: &mut LivingWorldState, events: &mut Vec<Event>) {
    for army in state.flush_destroy() {
        events.push(Event::ArmyDestroyed { army });
    }
}

impl LivingWorldState {
    #[must_use]
    pub fn can_end_phase(&self, p: PlayerId) -> bool {
        self.done_with_phase(p)
    }

    fn done_with_phase(&self, p: PlayerId) -> bool {
        let Some(seat) = self.player(p) else {
            return true;
        };
        if seat.ready || seat.kind == PlayerKind::Inert {
            return true;
        }
        match self.phase {
            Phase::Planning => false,
            Phase::ResolveBattles => {
                !self.battles.iter().any(|b| b.players.contains(&p))
                    && !self
                        .decisions
                        .values()
                        .any(|d| !d.resolved && self.decision_chooser(d) == Some(p))
            }
            Phase::PlanRetreats => self
                .retreat_entries
                .iter()
                .filter_map(|id| self.armies.get(id))
                .filter(|a| a.player == p)
                .all(|a| a.order.is_some()),
            Phase::MoveArmies | Phase::FakePhaseOne | Phase::RetreatArmies | Phase::Complete => {
                true
            }
        }
    }

    fn phase_condition(&self) -> bool {
        match self.phase {
            Phase::ResolveBattles => self.battles.is_empty(),
            Phase::PlanRetreats => self
                .retreat_entries
                .iter()
                .filter_map(|id| self.armies.get(id))
                .all(|a| a.order.is_some()),
            _ => true,
        }
    }

    fn can_advance(&self) -> bool {
        if self.phase == Phase::Planning
            && !self.live_players().any(|p| p.kind != PlayerKind::Inert)
        {
            return false;
        }
        self.awaiting.is_none()
            && self.live_players().all(|p| self.done_with_phase(p.id))
            && self.phase_condition()
    }

    fn leave_phase(&mut self, events: &mut Vec<Event>) {
        match self.phase {
            Phase::MoveArmies => {
                self.snap_orders();
                self.garrison_merge(events);
                self.register_armies();
                self.battles_and_captures(events);
            }
            Phase::ResolveBattles => {
                self.apply_captures(events);
                self.check_victory(events);
                self.rebuild_bonuses(events);
                self.decisions.clear();
            }
            Phase::RetreatArmies => self.snap_orders(),
            Phase::Planning => self.reset_upgrade_counts(),
            _ => {}
        }
    }

    fn enter_phase(&mut self, resolver: Option<&mut dyn AutoResolve>, events: &mut Vec<Event>) {
        match self.phase {
            Phase::Planning => self.enter_planning(events),
            Phase::MoveArmies => self.head_on_clashes(events),
            Phase::ResolveBattles => {
                self.auto_resolve_humanless(resolver, events);
            }
            Phase::PlanRetreats => self.retreat_triage(events),
            _ => {}
        }
    }

    pub(crate) fn enter_planning(&mut self, events: &mut Vec<Event>) {
        self.check_victory(events);
        self.garrison_merge(events);
        self.troop_upgrades();
    }

    fn clear_ready(&mut self) {
        for p in &mut self.players {
            p.ready = false;
        }
    }

    fn boundary(&self, events: &mut Vec<Event>) {
        events.push(Event::PhaseChanged {
            turn: self.turn,
            phase: self.phase,
            hash: self.state_hash(),
        });
    }

    fn start_next_turn(&mut self, events: &mut Vec<Event>) {
        for army in self.armies.values_mut() {
            if army.prev_region == Some(army.region) {
                army.turns_here = army.turns_here.saturating_add(1);
            } else {
                army.turns_here = 0;
                army.prev_region = Some(army.region);
            }
        }
        self.retreat_entries.clear();
        self.battles.clear();
        for index in 0..self.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            self.region_income(region, events);
            self.queue_tick(region, events);
            self.construction_tick(region, events);
        }
        self.turn = self.turn.saturating_add(1);
        self.phase = Phase::Planning;
        self.clear_ready();
        self.enter_planning(events);
        self.boundary(events);
    }

    fn advance(&mut self, resolver: Option<&mut dyn AutoResolve>, events: &mut Vec<Event>) {
        self.leave_phase(events);
        flush(self, events);
        if self.ended.is_some() {
            return;
        }
        let Some(next) = self.phase.next() else {
            return;
        };
        self.phase = next;
        self.clear_ready();
        self.enter_phase(resolver, events);
        flush(self, events);
        self.boundary(events);
    }

    #[must_use]
    pub fn retreat_pending(&self, p: PlayerId) -> Vec<ArmyId> {
        self.retreat_entries
            .iter()
            .copied()
            .filter(|id| {
                self.armies
                    .get(id)
                    .is_some_and(|a| a.player == p && a.order.is_none())
            })
            .collect()
    }
}

fn pump(
    state: &mut LivingWorldState,
    mut resolver: Option<&mut dyn AutoResolve>,
    events: &mut Vec<Event>,
) {
    for _ in 0..MAX_STEPS {
        if state.ended.is_some() {
            return;
        }
        if state.phase == Phase::Complete {
            state.start_next_turn(events);
            flush(state, events);
            continue;
        }
        if !state.can_advance() {
            return;
        }
        state.advance(reborrow(&mut resolver), events);
    }
}
