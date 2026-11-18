use mint_sage_ini::MissionDef;

use crate::game_mode::GameMode;
use crate::transition::TransitionHandler;

pub(crate) const DEFAULT_FADE_UP_FRAMES: u32 = 7;
const FADE_WHOLE_SCREEN: &str = "FadeWholeScreen";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchStartPresentation {
    pub(crate) intro_movie: Option<String>,
    pub(crate) load_still: Option<String>,
    pub(crate) load_voice: Option<String>,
    pub(crate) load_music: Option<String>,
    pub(crate) fade_up_frames: u32,
}

impl Default for MatchStartPresentation {
    fn default() -> Self {
        Self {
            intro_movie: None,
            load_still: None,
            load_voice: None,
            load_music: None,
            fade_up_frames: DEFAULT_FADE_UP_FRAMES,
        }
    }
}

impl MatchStartPresentation {
    pub(crate) fn for_mission(mission: &MissionDef, play_intro_movie: bool) -> Self {
        Self {
            intro_movie: mission.intro_movie.clone().filter(|_| play_intro_movie),
            load_still: mission.load_screen_image.clone(),
            load_voice: None,
            load_music: mission.load_screen_music_track.clone(),
            fade_up_frames: mission.fade_up(),
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TailAction {
    None,
    DropLoadScreen,
    PlayMovie(String),
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    FadeToBlack,
    Dropped,
    Movie,
    WaitFadeUp,
    FadeUp,
}

#[derive(Debug, Clone)]
pub(crate) struct StartTail {
    phase: Phase,
    movie: Option<String>,
    fade_up_tick: u64,
}

impl StartTail {
    pub(crate) fn begin(
        mode: GameMode,
        presentation: &MatchStartPresentation,
        handler: &mut TransitionHandler,
    ) -> Option<Self> {
        if !matches!(
            mode,
            GameMode::SinglePlayer | GameMode::SinglePlayerPractice
        ) {
            return None;
        }
        handler.set_group(FADE_WHOLE_SCREEN);
        handler.set_hold(true);
        Some(Self {
            phase: Phase::FadeToBlack,
            movie: presentation.intro_movie.clone(),
            fade_up_tick: retail_frames_to_ticks(presentation.fade_up_frames),
        })
    }

    pub(crate) fn tick(
        &mut self,
        handler: &mut TransitionHandler,
        movie_playing: bool,
        sim_frame: u64,
    ) -> TailAction {
        match self.phase {
            Phase::FadeToBlack if handler.is_finished() => {
                self.phase = Phase::Dropped;
                TailAction::DropLoadScreen
            }
            Phase::Dropped => match self.movie.take() {
                Some(name) => {
                    handler.set_hold(false);
                    handler.reset();
                    self.phase = Phase::Movie;
                    TailAction::PlayMovie(name)
                }
                None => {
                    self.phase = Phase::WaitFadeUp;
                    TailAction::None
                }
            },
            Phase::Movie if !movie_playing => {
                self.phase = Phase::WaitFadeUp;
                TailAction::None
            }
            Phase::WaitFadeUp if sim_frame >= self.fade_up_tick => {
                handler.suspend();
                handler.reverse(FADE_WHOLE_SCREEN);
                handler.resume();
                self.phase = Phase::FadeUp;
                TailAction::None
            }
            Phase::FadeUp if !handler.has_current() => TailAction::Finished,
            _ => TailAction::None,
        }
    }

    pub(crate) fn blocks_simulation(&self) -> bool {
        matches!(
            self.phase,
            Phase::FadeToBlack | Phase::Dropped | Phase::Movie
        )
    }
}

fn retail_frames_to_ticks(frames: u32) -> u64 {
    let per_retail_frame = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
    u64::from(frames).saturating_mul(u64::from(per_retail_frame.max(1)))
}

#[cfg(test)]
mod tests;
