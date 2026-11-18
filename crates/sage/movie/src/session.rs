use std::time::{Duration, Instant};

use mint_movie::{FramePlanes, MoviePlayer, PlayFlags, Tick};
use mint_sage_audio::{EventRequest, Outcome, SageAudio};
use mint_vfs::Vfs;

use crate::library::{MovieLibrary, MovieRoots};
use crate::source::{OpenSourceError, Vp6Source};

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("no Video named '{0}'")]
    UnknownVideo(String),
    #[error("could not open VP6 video file for {0}")]
    NoFile(String),
    #[error("could not read {0}: {1}")]
    Unreadable(String, String),
    #[error("{0}: {1}")]
    Unplayable(String, OpenSourceError),
}

pub struct MovieSession {
    player: MoviePlayer<Vp6Source>,
    clock: Option<Instant>,
    filename: String,
    audio: Vec<(String, Outcome)>,
    playing: Vec<String>,
    update_time: Duration,
}

impl MovieSession {
    pub fn open(
        name: &str,
        library: &MovieLibrary,
        roots: &MovieRoots,
        skippable: bool,
        flags: PlayFlags,
    ) -> Result<Self, OpenError> {
        let filename = library
            .filename(name)
            .ok_or_else(|| OpenError::UnknownVideo(name.to_owned()))?
            .to_owned();
        let path = roots
            .find(&filename)
            .ok_or_else(|| OpenError::NoFile(filename.clone()))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| OpenError::Unreadable(path.display().to_string(), e.to_string()))?;
        let source = Vp6Source::open(bytes)
            .map_err(|e| OpenError::Unplayable(path.display().to_string(), e))?;
        Ok(Self {
            player: MoviePlayer::new(source, flags, skippable, 0, 0),
            clock: None,
            filename,
            audio: Vec::new(),
            playing: Vec::new(),
            update_time: Duration::ZERO,
        })
    }

    pub fn start(&mut self, audio: Option<(&mut SageAudio, &Vfs)>) {
        let mut latency = 0;
        if let Some((sage, vfs)) = audio {
            latency = sage.library().settings().mixahead_latency_during_movies_ms;
            for event in [format!("{}_Music", self.filename), self.filename.clone()] {
                if sage.library().get(&event).is_some() {
                    let outcome = sage.play(vfs, &EventRequest::new(&event));
                    self.playing.push(event.clone());
                    self.audio.push((event, outcome));
                }
            }
        }
        self.player.set_start(0, latency);
        self.clock = Some(Instant::now());
    }

    fn now_ms(&self) -> Option<i64> {
        let clock = self.clock?;
        Some(i64::try_from(clock.elapsed().as_millis()).unwrap_or(i64::MAX))
    }

    pub fn update(&mut self, audio: Option<&mut SageAudio>) -> Tick {
        let Some(now) = self.now_ms() else {
            return Tick::default();
        };
        let began = Instant::now();
        let tick = self.player.update(now);
        self.update_time += began.elapsed();
        if tick.done {
            self.stop_audio(audio);
        }
        tick
    }

    pub fn skip(&mut self, audio: Option<&mut SageAudio>) -> bool {
        let skipped = self.player.skip();
        if skipped {
            self.stop_audio(audio);
        }
        skipped
    }

    fn stop_audio(&mut self, audio: Option<&mut SageAudio>) {
        if let Some(sage) = audio {
            for event in self.playing.drain(..) {
                sage.stop_event(&event);
            }
        }
    }

    #[must_use]
    pub fn planes(&self) -> Option<FramePlanes<'_>> {
        self.player.planes()
    }

    #[must_use]
    pub fn player(&self) -> &MoviePlayer<Vp6Source> {
        &self.player
    }

    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    #[must_use]
    pub fn decode_time(&self) -> Duration {
        self.update_time
    }

    #[must_use]
    pub fn audio_started(&self) -> &[(String, Outcome)] {
        &self.audio
    }
}
