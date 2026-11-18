use super::*;
use crate::match_stats::EndStatus;
use crate::match_stats::{Sample, SAMPLE_PERIOD};
use mint_sim::PlayerId;

fn labels() -> TimeLabels {
    TimeLabels {
        days: "%d Days".into(),
        day: "1 Day".into(),
        hours: "%d Hours".into(),
        hour: "1 Hour".into(),
        minutes: "%d Minutes".into(),
        minute: "1 Minute".into(),
        seconds: "%d Seconds".into(),
        second: "1 Second".into(),
    }
}

fn player(units_built: u32, killed: u32, lost: u32) -> SummaryPlayer {
    SummaryPlayer {
        name: "Test".into(),
        rgb: [1, 2, 3],
        figures: PlayerFigures {
            player: PlayerId::new(0),
            side: None,
            status: EndStatus::Undecided,
            end_tick: 37 * mint_sim::SIMULATION_HZ,
            structures_built: 0,
            structures_lost: 0,
            structures_destroyed: 0,
            fortresses: 0,
            units_built,
            units_lost: lost,
            units_killed: killed,
            favourite: None,
            earned: 0,
            spent: 0,
            spent_on_structures: 0,
            spent_on_units: 0,
            spent_on_heroes: 0,
            first_hero_tick: None,
            heroes_built: 0,
            heroes_lost: 0,
            fortress_ticks: Vec::new(),
            samples: Vec::new(),
        },
    }
}

fn texts(list: &ListSpec, row: usize) -> Vec<String> {
    list.rows[row]
        .iter()
        .map(|cell| match cell {
            CellSpec::Text(text) => text.clone(),
            CellSpec::Image { .. } | CellSpec::ScaledImage { .. } => String::new(),
        })
        .collect()
}

fn list(players: Vec<SummaryPlayer>, focus: usize) -> ListSpec {
    MatchSummary { players }.stat_list(focus, &|key| key.to_owned(), &labels())
}

#[test]
fn a_ratio_with_a_zero_top_is_a_dash_and_a_zero_bottom_counts_as_one() {
    let at = |row: &str| {
        STAT_ROWS
            .iter()
            .position(|(label, _)| *label == row)
            .unwrap()
    };
    let kd = at("STAT:RTS_UNIT_KILL_DEATH_RATIO");
    let two = list(vec![player(0, 0, 0), player(0, 3, 0), player(0, 3, 2)], 1);
    assert_eq!(texts(&two, kd)[1..], ["-", "3", "1.5"]);
}

#[test]
fn the_column_count_follows_the_player_count_and_focus_picks_the_players() {
    let one = list(vec![player(1, 0, 0)], 1);
    assert_eq!(one.columns, [(50, false), (50, true)]);
    assert_eq!(texts(&one, 5), ["STAT:RTS_UNITS_CREATED", "1"]);

    assert_eq!(
        list(vec![player(1, 0, 0), player(2, 0, 0)], 1)
            .columns
            .len(),
        4
    );

    let three = vec![player(1, 0, 0), player(2, 0, 0), player(3, 0, 0)];
    let spec = list(three.clone(), 1);
    assert_eq!(spec.columns.len(), 4);
    assert_eq!(texts(&spec, 5)[1..], ["1", "2", "3"]);
    assert_eq!(texts(&list(three, 2), 5)[1..], ["1", "3", ""]);
}

#[test]
fn session_length_reads_in_the_time_words() {
    let words = labels();
    assert_eq!(duration(37, &words), "37 Seconds");
    assert_eq!(duration(1, &words), "1 Second");
    assert_eq!(duration(0, &words), "0 Seconds");
    assert_eq!(duration(61, &words), "1 Minute 1 Second");
}

#[test]
fn the_colour_is_a_c_hash_x_of_the_24_bit_value() {
    assert_eq!(color_extern([0xff, 0x80, 0]), "0xff8000");
    assert_eq!(color_extern([0, 0, 0]), "0");
}

#[test]
fn the_y_axis_top_is_the_maximum_rounded_up_over_ten_intervals() {
    let tops: Vec<i64> = [0, 9, 235, 1234].iter().map(|m| 10 * y_step(*m)).collect();
    assert_eq!(tops, [10, 10, 240, 1300]);
}

fn sampled(units: &[i16], status: EndStatus, fortress_ticks: Vec<u32>) -> SummaryPlayer {
    let mut entry = player(0, 0, 0);
    entry.figures.end_tick = 4 * SAMPLE_PERIOD;
    entry.figures.status = status;
    entry.figures.fortress_ticks = fortress_ticks;
    entry.figures.samples = units
        .iter()
        .map(|&units| Sample {
            units,
            ..Sample::default()
        })
        .collect();
    entry
}

#[test]
fn the_graph_holds_each_sample_and_ends_at_the_players_last() {
    let summary = MatchSummary {
        players: vec![sampled(
            &[1, 1, 5],
            EndStatus::Defeated,
            vec![SAMPLE_PERIOD],
        )],
    };
    let graph = summary.graph(&[0.5], &|_| [8.0, 4.0]);
    let units = &graph
        .plots
        .iter()
        .find(|(name, _)| name == "Units")
        .unwrap()
        .1;
    assert_eq!(
        units.lines[1].points,
        vec![[0.0, 0.9], [0.5, 0.9], [0.5, 0.5], [0.75, 0.5]]
    );
    assert_eq!(units.lines[0].width, 3.0);
    assert_eq!(units.lines[0].rgba[3], 63.0 / 255.0);
    assert_eq!(units.lines[1].rgba[3], 127.0 / 255.0);
    let markers: Vec<_> = units
        .markers
        .iter()
        .map(|m| (m.image.as_str(), m.at))
        .collect();
    assert_eq!(
        markers,
        vec![
            ("AptTimelineDefeated", [0.75, 0.5]),
            ("AptTimelineFortress", [0.5, 0.9]),
        ]
    );
}

#[test]
fn the_y_axis_follows_the_tab() {
    let mut entry = sampled(&[0, 235], EndStatus::Undecided, Vec::new());
    entry.figures.samples[1].score = 9.0;
    let summary = MatchSummary {
        players: vec![entry],
    };
    assert_eq!(summary.axis_labels(Series::Units).1[10], "240");
    assert_eq!(summary.axis_labels(Series::FinalScore).1[10], "10");
    assert_eq!(Series::named(Some("Score")), Series::FinalScore);
}

#[test]
fn a_campaign_timeline_is_victorious_only_after_a_script_victory() {
    let figures = vec![player(0, 0, 0).figures];
    let status = |victory| campaign_figures(&figures, victory)[0].status;
    assert_eq!(status(true), EndStatus::Victorious);
    assert_eq!(status(false), EndStatus::Defeated);
}
