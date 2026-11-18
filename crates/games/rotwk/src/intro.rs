pub(crate) const MOVIES: [&str; 4] = [
    "EALogoMovie",
    "NewLineLogo",
    "TolkienLogo",
    "Overall_Game_Intro",
];
const FADED_MOVIE: &str = "Overall_Game_Intro";
pub(crate) const TITLE_IMAGE: &str = "TitleScreenLogo";
const TITLE_SECONDS: f32 = 5.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntroAction {
    None,
    PlayMovie { name: &'static str, faded: bool },
    ShowTitle,
    FadeOutTitle,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Movie { next: usize, playing: bool },
    Title { shown: f32 },
    FadingOut,
    Done,
}

#[derive(Debug)]
pub(crate) struct Intro {
    stage: Stage,
    shell_map_on: bool,
}

impl Intro {
    pub(crate) fn new(play_movies: bool, shell_map_on: bool) -> Self {
        let stage = if play_movies {
            Stage::Movie {
                next: 0,
                playing: false,
            }
        } else {
            Stage::Title { shown: -1.0 }
        };
        Self {
            stage,
            shell_map_on,
        }
    }

    pub(crate) fn skip_logos(&mut self) {
        let Stage::Movie {
            next,
            playing: true,
        } = self.stage
        else {
            return;
        };
        let intro = MOVIES.iter().position(|name| *name == FADED_MOVIE);
        if let Some(intro) = intro.filter(|intro| next <= *intro) {
            self.stage = Stage::Movie {
                next: intro,
                playing: true,
            };
        }
    }

    pub(crate) fn title_shown(&self) -> bool {
        matches!(self.stage, Stage::Title { shown } if shown >= 0.0)
            || self.stage == Stage::FadingOut
    }

    pub(crate) fn step(&mut self, movie_playing: bool, fade_done: bool, dt: f32) -> IntroAction {
        match self.stage {
            Stage::Movie { playing: true, .. } if movie_playing => IntroAction::None,
            Stage::Movie { next, .. } => match MOVIES.get(next) {
                Some(&name) => {
                    self.stage = Stage::Movie {
                        next: next.saturating_add(1),
                        playing: true,
                    };
                    IntroAction::PlayMovie {
                        name,
                        faded: name == FADED_MOVIE,
                    }
                }
                None => {
                    self.stage = Stage::Title { shown: -1.0 };
                    IntroAction::None
                }
            },
            Stage::Title { shown } if shown < 0.0 => {
                self.stage = Stage::Title { shown: 0.0 };
                IntroAction::ShowTitle
            }
            Stage::Title { shown } => {
                let shown = shown + dt.max(0.0);
                if shown < TITLE_SECONDS {
                    self.stage = Stage::Title { shown };
                    IntroAction::None
                } else if self.shell_map_on {
                    self.stage = Stage::Done;
                    IntroAction::Finished
                } else {
                    self.stage = Stage::FadingOut;
                    IntroAction::FadeOutTitle
                }
            }
            Stage::FadingOut if fade_done => {
                self.stage = Stage::Done;
                IntroAction::Finished
            }
            Stage::FadingOut | Stage::Done => IntroAction::None,
        }
    }
}

#[cfg(test)]
mod tests;
