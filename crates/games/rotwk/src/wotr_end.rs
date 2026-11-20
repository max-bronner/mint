use mint_sage_ini::living_world::campaign::{ScenarioDef, VictoryTypeDef};
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_livingworld::{LivingWorldState, PlayerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Victorious,
    Defeated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndRow {
    pub(crate) template: String,
    pub(crate) team: i32,
    pub(crate) regions: i32,
    pub(crate) armies: usize,
    pub(crate) defeated: bool,
    pub(crate) colour: [u8; 3],
    pub(crate) side: Option<String>,
}

impl EndRow {
    pub(crate) fn colour_extern(&self) -> String {
        crate::post_match::color_extern(self.colour)
    }

    pub(crate) fn faction_icon(&self) -> Option<String> {
        self.side.as_deref().map(crate::game::faction_icon_image)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndReport {
    pub(crate) outcome: Outcome,
    pub(crate) title_key: &'static str,
    pub(crate) scenario_key: Option<String>,
    pub(crate) condition_key: Option<String>,
    pub(crate) image: String,
    pub(crate) turn: u32,
    pub(crate) local: usize,
    pub(crate) rows: Vec<EndRow>,
}

fn title_key(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Victorious => "APT:EndVictorious",
        Outcome::Defeated => "APT:EndDefeat",
    }
}

fn image_name(evil: bool, outcome: Outcome) -> String {
    format!(
        "StrategicEndGame{}{}",
        if evil { "Evil" } else { "Good" },
        if outcome == Outcome::Victorious {
            "Win"
        } else {
            "Lose"
        }
    )
}

fn condition_key(
    outcome: Outcome,
    victory: Option<&VictoryTypeDef>,
    scenario: Option<&ScenarioDef>,
) -> Option<String> {
    let victorious = outcome == Outcome::Victorious;
    if scenario.is_some_and(|s| s.use_mp_rules_victory_condition == Some(true)) {
        let v = victory?;
        return if victorious {
            v.display_victorious_text.clone()
        } else {
            v.display_defeated_text.clone()
        };
    }
    let s = scenario?;
    if victorious {
        s.display_victorious_text.clone()
    } else {
        s.display_defeated_text.clone()
    }
}

pub(crate) fn end_report(
    state: &LivingWorldState,
    data: &LivingWorldData,
    evil: &dyn Fn(&str) -> bool,
    side: &dyn Fn(&str) -> Option<String>,
    swatches: &[[u8; 3]],
) -> Option<EndReport> {
    let winner = state.ended?;
    let local = state
        .players
        .iter()
        .position(|p| p.kind == PlayerKind::Human)
        .unwrap_or(0);
    let me = state.players.get(local)?;
    let survivors = state.players.iter().filter(|p| !p.defeated).count();
    let outcome = outcome_of(winner, me.team, me.defeated, survivors);
    let campaign = data
        .campaigns
        .iter()
        .find(|c| c.name == state.rules.campaign);
    let victory = campaign.and_then(|c| c.victory_types.get(state.rules.victory_type));
    let scenario = campaign.and_then(|c| c.scenarios.get(state.rules.scenario));
    let faction = state
        .rules
        .templates
        .get(usize::from(me.template.0))
        .and_then(|t| t.faction.as_deref());
    let rows = state
        .players
        .iter()
        .enumerate()
        .map(|(seat, p)| EndRow {
            template: state
                .rules
                .templates
                .get(usize::from(p.template.0))
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            colour: swatches.get(seat).copied().unwrap_or([255; 3]),
            side: state
                .rules
                .templates
                .get(usize::from(p.template.0))
                .and_then(|t| t.faction.as_deref())
                .and_then(side),
            team: p.team,
            regions: state.regions_owned(p.id),
            armies: state
                .armies
                .values()
                .filter(|a| a.player == p.id && !a.destroy_queued)
                .count(),
            defeated: p.defeated,
        })
        .collect();
    Some(EndReport {
        outcome,
        title_key: title_key(outcome),
        scenario_key: scenario.and_then(|s| s.display_name.clone()),
        condition_key: condition_key(outcome, victory, scenario),
        image: image_name(faction.is_some_and(evil), outcome),
        turn: state.turn,
        local,
        rows,
    })
}

fn outcome_of(winner: Option<i32>, team: i32, defeated: bool, survivors: usize) -> Outcome {
    let won = !defeated
        && match winner {
            Some(winning) => team == winning,
            None => survivors == 1,
        };
    if won {
        Outcome::Victorious
    } else {
        Outcome::Defeated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs(use_mp: Option<bool>) -> (VictoryTypeDef, ScenarioDef) {
        let victory = VictoryTypeDef {
            display_victorious_text: Some("V:Won".to_owned()),
            display_defeated_text: Some("V:Lost".to_owned()),
            ..VictoryTypeDef::default()
        };
        let scenario = ScenarioDef {
            display_victorious_text: Some("S:Won".to_owned()),
            display_defeated_text: Some("S:Lost".to_owned()),
            use_mp_rules_victory_condition: use_mp,
            ..ScenarioDef::default()
        };
        (victory, scenario)
    }

    #[test]
    fn the_victory_types_text_is_used_only_when_the_scenario_uses_the_mp_rules() {
        let key = |use_mp, outcome| {
            let (victory, scenario) = defs(use_mp);
            condition_key(outcome, Some(&victory), Some(&scenario))
        };
        assert_eq!(
            key(Some(true), Outcome::Victorious).as_deref(),
            Some("V:Won")
        );
        assert_eq!(
            key(Some(true), Outcome::Defeated).as_deref(),
            Some("V:Lost")
        );
        assert_eq!(
            key(Some(false), Outcome::Victorious).as_deref(),
            Some("S:Won")
        );
        assert_eq!(key(None, Outcome::Defeated).as_deref(), Some("S:Lost"));
        let (_, scenario) = defs(Some(true));
        assert_eq!(
            condition_key(Outcome::Victorious, None, Some(&scenario)),
            None,
            "the victory type's text, not the scenario's, even when it is missing"
        );
    }

    #[test]
    fn the_title_and_the_picture_follow_the_outcome_and_the_side() {
        assert_eq!(title_key(Outcome::Victorious), "APT:EndVictorious");
        assert_eq!(title_key(Outcome::Defeated), "APT:EndDefeat");
        assert_eq!(
            image_name(false, Outcome::Victorious),
            "StrategicEndGameGoodWin"
        );
        assert_eq!(
            image_name(false, Outcome::Defeated),
            "StrategicEndGameGoodLose"
        );
        assert_eq!(
            image_name(true, Outcome::Victorious),
            "StrategicEndGameEvilWin"
        );
        assert_eq!(
            image_name(true, Outcome::Defeated),
            "StrategicEndGameEvilLose"
        );
    }

    #[test]
    fn a_row_publishes_its_swatch_and_the_icon_of_its_side_to_the_score_page() {
        let row = |colour, side: Option<&str>| EndRow {
            template: "PlayerMen".to_owned(),
            team: 1,
            regions: 3,
            armies: 2,
            defeated: false,
            colour,
            side: side.map(str::to_owned),
        };
        assert_eq!(row([0x00, 0x00, 0xFF], None).colour_extern(), "0xff");
        assert_eq!(row([0xFF, 0x00, 0x00], None).colour_extern(), "0xff0000");
        assert_eq!(row([0, 0, 0], None).colour_extern(), "0");
        assert_eq!(
            row([0; 3], Some("Men")).faction_icon().as_deref(),
            Some("AptIconMen")
        );
        assert_eq!(row([0; 3], None).faction_icon(), None);
    }

    #[test]
    fn the_local_player_wins_with_its_team_or_alone() {
        let cases = [
            (Some(1), 1, false, 3, Outcome::Victorious),
            (Some(1), 0, false, 3, Outcome::Defeated),
            (Some(1), 1, true, 3, Outcome::Defeated),
            (None, -1, false, 1, Outcome::Victorious),
            (None, -1, false, 2, Outcome::Defeated),
        ];
        for (winner, team, defeated, survivors, want) in cases {
            assert_eq!(outcome_of(winner, team, defeated, survivors), want);
        }
    }
}
