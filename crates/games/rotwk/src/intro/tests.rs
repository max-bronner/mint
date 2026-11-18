use super::*;

fn run(intro: &mut Intro, frames: usize, dt: f32) -> Vec<IntroAction> {
    let mut playing = false;
    let mut actions = Vec::new();
    for _ in 0..frames {
        let action = intro.step(playing, true, dt);
        playing = matches!(action, IntroAction::PlayMovie { .. });
        if action != IntroAction::None {
            actions.push(action);
        }
    }
    actions
}

fn movies() -> Vec<IntroAction> {
    MOVIES
        .iter()
        .map(|&name| IntroAction::PlayMovie {
            name,
            faded: name == FADED_MOVIE,
        })
        .collect()
}

#[test]
fn the_movies_play_in_order_then_the_title_holds_before_the_shell_map_loads() {
    let mut intro = Intro::new(true, true);
    let mut want = movies();
    want.push(IntroAction::ShowTitle);
    let before_end = run(&mut intro, MOVIES.len() * 2 + 1 + 5, 1.0);
    assert_eq!(before_end, want);
    assert_eq!(run(&mut intro, 1, 1.0), vec![IntroAction::Finished]);
}

#[test]
fn with_the_shell_map_off_the_title_fades_out_before_the_shell_starts() {
    let mut intro = Intro::new(false, false);
    assert_eq!(intro.step(false, true, 0.0), IntroAction::ShowTitle);
    assert_eq!(
        intro.step(false, true, TITLE_SECONDS),
        IntroAction::FadeOutTitle
    );
    assert_eq!(intro.step(false, false, 0.1), IntroAction::None);
    assert!(intro.title_shown());
    assert_eq!(intro.step(false, true, 0.1), IntroAction::Finished);
}

#[test]
fn one_skip_during_a_logo_skips_every_remaining_logo_and_the_next_movie_is_the_intro() {
    let mut intro = Intro::new(true, true);
    assert_eq!(intro.step(false, true, 0.0), movies()[0]);
    intro.skip_logos();
    let intro_movie = movies().pop();
    assert_eq!(Some(intro.step(false, true, 0.0)), intro_movie);
    intro.skip_logos();
    assert_eq!(intro.step(false, true, 0.0), IntroAction::None);
    assert_eq!(intro.step(false, true, 0.0), IntroAction::ShowTitle);
}
