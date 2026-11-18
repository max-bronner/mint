use super::*;
use mint_sage_ini::{BlockTable, Document};

const GROUPS: &str = "WindowTransition TestFade\n  Window\n    Transition = SCREENFADE\n      StartFrame = 0\n      EndFrame = 20\n    End\n    FrameDelay = 0\n  END\n  Window\n    Transition = SOUNDFADE\n      StartFrame = 0\n      EndFrame = 20\n    End\n    FrameDelay = 0\n  END\n  FireOnce = YES\nEND\n\nWindowTransition TestFilm\n  Window\n    Transition = SCREENFADE\n      StartFrame = 0\n      EndFrame = 5\n    End\n    FrameDelay = 0\n  END\n  FireOnce = YES\nEND\n";

fn handler(name: &str) -> TransitionHandler {
    let text = GROUPS.replace("TestFade", name);
    let document = Document::parse("test.ini", &text, &BlockTable::corpus_path()).unwrap();
    let mut groups = crate::transition::Groups::default();
    groups.insert_document(&document);
    TransitionHandler::new(groups)
}

fn mission(movie: Option<&str>, fade_ms: Option<u32>) -> MissionDef {
    MissionDef {
        name: "m".into(),
        map: Some("map".into()),
        intro_movie: movie.map(str::to_owned),
        load_screen_image: Some("still".into()),
        load_screen_music_track: Some("music".into()),
        fade_up_frames: fade_ms.map(mint_sage_ini::ms_to_frames),
        delay_carryover_spawning_of: Vec::new(),
    }
}

#[test]
fn a_retry_drops_the_intro_movie_and_an_unset_fade_up_is_seven_frames() {
    let start = MatchStartPresentation::for_mission(&mission(Some("Clip"), None), true);
    assert_eq!(start.intro_movie.as_deref(), Some("Clip"));
    assert_eq!(start.fade_up_frames, 7);
    let retry = MatchStartPresentation::for_mission(&mission(Some("Clip"), Some(1000)), false);
    assert_eq!(retry.intro_movie, None);
    assert_eq!(retry.fade_up_frames, 5);
}

#[test]
fn only_modes_zero_and_six_have_a_start_tail() {
    let presentation = MatchStartPresentation::default();
    for (mode, has) in [
        (GameMode::SinglePlayer, true),
        (GameMode::SinglePlayerPractice, true),
        (GameMode::Skirmish, false),
        (GameMode::Replay, false),
        (GameMode::Internet, false),
    ] {
        let mut h = handler("FadeWholeScreen");
        assert_eq!(
            StartTail::begin(mode, &presentation, &mut h).is_some(),
            has,
            "{mode:?}"
        );
    }
}

#[test]
fn the_tail_plays_the_movie_after_black_and_fades_up_at_the_set_frame() {
    let mut h = handler("FadeWholeScreen");
    let presentation = MatchStartPresentation {
        intro_movie: Some("Clip".into()),
        fade_up_frames: 7,
        ..MatchStartPresentation::default()
    };
    let mut tail = StartTail::begin(GameMode::SinglePlayer, &presentation, &mut h).unwrap();
    assert_eq!(tail.tick(&mut h, false, 0), TailAction::None);
    for _ in 0..40 {
        h.update();
    }
    assert_eq!(tail.tick(&mut h, false, 0), TailAction::DropLoadScreen);
    assert!(tail.blocks_simulation());
    assert_eq!(
        tail.tick(&mut h, false, 0),
        TailAction::PlayMovie("Clip".into())
    );
    assert_eq!(tail.tick(&mut h, true, 0), TailAction::None);
    assert!(tail.blocks_simulation());
    assert_eq!(tail.tick(&mut h, false, 0), TailAction::None);
    assert!(!tail.blocks_simulation());
    h.set_hold(true);
    h.set_group("TestFilm");
    for _ in 0..10 {
        h.update();
    }
    assert_eq!(tail.tick(&mut h, false, 41), TailAction::None);
    for _ in 0..10 {
        h.update();
    }
    assert!(
        h.screen_alpha() > 0.99,
        "still black before the fade-up frame"
    );
    assert_eq!(tail.tick(&mut h, false, 42), TailAction::None);
    assert_eq!(tail.tick(&mut h, false, 42), TailAction::None);
    for _ in 0..60 {
        h.update();
    }
    assert_eq!(tail.tick(&mut h, false, 43), TailAction::Finished);
}
