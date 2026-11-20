use mint_sage_livingworld::Phase;

pub(super) const CLIP: &str = "main.endTurnButton";

pub(super) fn enabled(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::Planning | Phase::ResolveBattles | Phase::PlanRetreats
    )
}

pub(super) fn state_label(on: bool) -> &'static str {
    if on {
        "_up"
    } else {
        "_disabled"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Enable(bool),
    PlayAlertFlash,
    HaltAlertFlash,
}

#[derive(Debug, Default)]
pub(super) struct EndTurn {
    phase: Option<Phase>,
    flashed: bool,
}

impl EndTurn {
    pub(super) fn update(&mut self, phase: Phase, can_end: bool, dialog_open: bool) -> Vec<Action> {
        let mut out = Vec::new();
        if self.phase != Some(phase) {
            if self.flashed {
                out.push(Action::HaltAlertFlash);
            }
            if self.phase.is_some_and(enabled) {
                out.push(Action::Enable(false));
            }
            if enabled(phase) {
                out.push(Action::Enable(true));
            }
            self.phase = Some(phase);
            self.flashed = false;
        }
        let flashes = matches!(phase, Phase::ResolveBattles | Phase::PlanRetreats);
        let blocked = phase == Phase::ResolveBattles && dialog_open;
        if flashes && can_end && !blocked && !self.flashed {
            self.flashed = true;
            out.push(Action::PlayAlertFlash);
        }
        out
    }
}
