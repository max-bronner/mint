use mint_sage_livingworld::Phase;

pub(super) fn phase_title(phase: Phase) -> Option<&'static str> {
    match phase {
        Phase::Planning => Some("APT:TacticalPhaseTitle"),
        Phase::ResolveBattles => Some("APT:BattlePhaseTitle"),
        Phase::PlanRetreats => Some("APT:RetreatPhaseTitle"),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub(super) struct Checklist {
    clip: Option<String>,
    shown: Option<(u32, Phase)>,
}

impl Checklist {
    pub(super) fn loaded(&mut self, clip: &str) {
        self.clip = Some(clip.to_owned());
        self.shown = None;
    }

    pub(super) fn changes(
        &mut self,
        turn: u32,
        phase: Phase,
        label: impl Fn(&str) -> Option<String>,
    ) -> Vec<(String, String)> {
        let Some(clip) = self.clip.as_deref() else {
            return Vec::new();
        };
        if self.shown == Some((turn, phase)) {
            return Vec::new();
        }
        self.shown = Some((turn, phase));
        let title = phase_title(phase).and_then(label).unwrap_or_default();
        vec![
            (format!("{clip}_TurnNumber"), turn.to_string()),
            (format!("{clip}_PhaseTitle"), title),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_turn_and_the_phase_title_go_to_the_reported_clip_once_per_change() {
        let mut list = Checklist::default();
        assert!(list.changes(1, Phase::Planning, |_| None).is_empty());
        list.loaded("_level0.main.checklist");
        let label = |key: &str| Some(format!("<{key}>"));
        assert_eq!(
            list.changes(3, Phase::Planning, label),
            vec![
                (
                    "_level0.main.checklist_TurnNumber".to_owned(),
                    "3".to_owned()
                ),
                (
                    "_level0.main.checklist_PhaseTitle".to_owned(),
                    "<APT:TacticalPhaseTitle>".to_owned()
                ),
            ]
        );
        assert!(list.changes(3, Phase::Planning, label).is_empty());
        let moving = list.changes(3, Phase::MoveArmies, label);
        assert_eq!(moving[1].1, "");
    }
}
