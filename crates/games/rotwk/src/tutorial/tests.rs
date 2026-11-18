use super::*;

fn tutorials(text: &str) -> (Tutorials, ValueErrors) {
    let source = format!("GameData\n{text}End\n");
    let document = mint_sage_ini::Document::parse(
        "data/ini/gamedata.ini",
        &source,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let block = mint_sage_ini::game_data::merged_game_data(&[document]);
    let errors = ValueErrors::new();
    (Tutorials::read(&block, &errors), errors)
}

#[test]
fn the_fade_up_rounds_up_to_logic_frames_and_defaults_to_seven() {
    let frames = |ms: &str| {
        let text = format!("  BasicTutorialMillisecondsAfterStartToStartFadeUp = {ms}\n");
        tutorials(&text).0.block(TutorialKind::Basic).fade_up_frames
    };
    assert_eq!(frames("1400"), 7);
    assert_eq!(frames("1401"), 8);
    assert_eq!(
        tutorials("").0.block(TutorialKind::Advanced).fade_up_frames,
        7
    );
}

#[test]
fn a_bad_fade_up_is_a_value_error() {
    let (_, errors) = tutorials("  AdvancedTutorialMillisecondsAfterStartToStartFadeUp = soon\n");
    assert_eq!(errors.take().len(), 1);
}

#[test]
fn a_block_with_a_map_starts_in_mode_six_on_easy_with_no_intro_movie_unset() {
    let (data, _) = tutorials(
        "  BasicTutorialMap = Maps\\Tut\\Tut.map\n  BasicTutorialLoadScreenStillImage = Still\n  \
         BasicTutorialObjective = A B\n",
    );
    let block = data.block(TutorialKind::Basic);
    assert_eq!(block.objective, ["A", "B"]);
    let plan = block.plan().unwrap();
    assert_eq!(plan.config.map, "Maps/Tut/Tut.map");
    assert_eq!(plan.config.mode, GameMode::SinglePlayerPractice);
    assert!(plan.config.mode.is_single_player(None));
    assert_eq!(plan.config.difficulty, Difficulty::Easy);
    assert_eq!(
        plan.config.carryover,
        mint_sage_campaign::Carryover::default()
    );
    assert_eq!(plan.presentation.intro_movie, None);
    assert_eq!(plan.presentation.load_still.as_deref(), Some("Still"));
}

#[test]
fn a_block_without_a_map_starts_nothing() {
    let (data, _) = tutorials("  AdvancedTutorialLoadScreenStillImage = Still\n");
    assert!(data.block(TutorialKind::Advanced).plan().is_none());
}

#[test]
fn on_tutorial_knows_basic_advanced_and_strategic_only() {
    assert_eq!(
        OnTutorial::from_arg("Basic"),
        OnTutorial::Start(TutorialKind::Basic)
    );
    assert_eq!(
        OnTutorial::from_arg("Advanced"),
        OnTutorial::Start(TutorialKind::Advanced)
    );
    assert_eq!(OnTutorial::from_arg("Strategic"), OnTutorial::WarOfTheRing);
    assert_eq!(OnTutorial::from_arg("Expert"), OnTutorial::Nothing);
}
