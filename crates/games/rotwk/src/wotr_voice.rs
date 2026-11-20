use std::collections::BTreeMap;
use std::sync::Arc;

use mint_render::Camera;
use mint_sage_audio::{AudioLibrary, Eva, EvaLine, EventRequest, Outcome, SageAudio};
use mint_sage_livingworld::tutorial::SessionAudio;
use mint_sage_worldmap::MapSound;
use mint_vfs::Vfs;

use crate::game_data::RETAIL_LOGIC_HZ;

#[derive(Debug, Default)]
pub(crate) struct LineClock {
    left_ms: Vec<f64>,
}

impl LineClock {
    pub(crate) fn start(&mut self, length_ms: u64) -> Option<u32> {
        let handle = u32::try_from(self.left_ms.len()).ok()?;
        #[allow(clippy::cast_precision_loss)]
        self.left_ms.push(length_ms as f64);
        Some(handle)
    }

    pub(crate) fn advance(&mut self, ms: f64) {
        for left in &mut self.left_ms {
            *left = (*left - ms).max(0.0);
        }
    }

    pub(crate) fn playing(&self, handle: u32) -> bool {
        usize::try_from(handle)
            .ok()
            .and_then(|i| self.left_ms.get(i))
            .is_some_and(|left| *left > 0.0)
    }
}

#[derive(Debug, Default)]
pub(crate) struct UpdateClock {
    rest_ms: f64,
}

impl UpdateClock {
    pub(crate) const UPDATE_MS: f64 = 1000.0 / 5.0;

    pub(crate) fn due(&mut self, dt_ms: f64) -> u32 {
        self.rest_ms += dt_ms.max(0.0);
        let whole = (self.rest_ms / Self::UPDATE_MS).floor();
        self.rest_ms -= whole * Self::UPDATE_MS;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = whole.clamp(0.0, 1000.0) as u32;
        whole
    }
}

pub(crate) struct WotrVoice {
    audio: SageAudio,
    vfs: Arc<Vfs>,
    clock: LineClock,
    eva: Option<Eva>,
    side: String,
    eva_frame: u32,
    eva_rest_ms: f64,
    lengths: BTreeMap<String, u64>,
    map_events: Vec<String>,
}

const EVA_FRAME_MS: f64 = 1000.0 / 30.0;

fn eva_defs(vfs: &Vfs) -> Option<mint_sage_ini::eva::EvaDefs> {
    let table = mint_sage_ini::table::BlockTable::corpus_path();
    let mut documents = Vec::new();
    for path in ["data/ini/default/eva.ini", "data/ini/eva.ini"] {
        let Ok(bytes) = vfs.read(path) else { continue };
        if let Ok(document) =
            mint_sage_ini::Document::parse(path, &String::from_utf8_lossy(&bytes), &table)
        {
            documents.push(document);
        }
    }
    (!documents.is_empty())
        .then(|| mint_sage_ini::eva::EvaDefs::read(&documents, &mint_sage_ini::MacroTable::new()))
}

impl WotrVoice {
    pub(crate) fn open(vfs: Arc<Vfs>, side: String, silent: bool) -> Self {
        let library = mint_sage_audio::load_library(&vfs).unwrap_or_else(|error| {
            println!("wotr-voice: could not read the audio INI ({error}) - running silent");
            AudioLibrary::default()
        });
        let eva = eva_defs(&vfs).map(|defs| Eva::new(&defs, RETAIL_LOGIC_HZ));
        let audio = if silent {
            SageAudio::silent(library)
        } else {
            SageAudio::open_named(library, "wotr voice")
        };
        println!(
            "wotr-voice: {} events, EVA {} event(s), side {side:?}, {}",
            audio.library().len(),
            eva.as_ref().map_or(0, Eva::len),
            audio.readout()
        );
        Self {
            audio,
            vfs,
            clock: LineClock::default(),
            eva,
            side,
            eva_frame: 0,
            eva_rest_ms: 0.0,
            lengths: BTreeMap::new(),
            map_events: Vec::new(),
        }
    }

    fn start(&mut self, event: &str) -> Option<u64> {
        let outcome = self.audio.play(&self.vfs, &EventRequest::new(event));
        if outcome != Outcome::Queued {
            println!("wotr-voice: {event} did not play ({})", outcome.label());
            return None;
        }
        Some(self.length_of(event))
    }

    fn length_of(&mut self, event: &str) -> u64 {
        if let Some(length) = self.lengths.get(event) {
            return *length;
        }
        let length = self
            .audio
            .decoded_duration_ms(&self.vfs, event)
            .unwrap_or(0);
        self.lengths.insert(event.to_owned(), length);
        length
    }

    pub(crate) fn play_map_sounds(&mut self, sounds: Vec<MapSound>) {
        for sound in sounds {
            let request = match sound.position {
                Some(at) => EventRequest::at(&sound.event, at),
                None => EventRequest::new(&sound.event),
            };
            let outcome = self.audio.play(&self.vfs, &request);
            println!(
                "wotr-voice: map sound {} {:?} ({})",
                sound.event,
                sound.position,
                outcome.label()
            );
            if outcome == Outcome::Queued && !self.map_events.contains(&sound.event) {
                self.map_events.push(sound.event);
            }
        }
    }

    pub(crate) fn place_ears(&mut self, camera: &Camera) {
        let forward = (camera.target - camera.eye).normalize();
        let right = forward.cross(camera.up).normalize();
        self.audio.set_camera(
            camera.eye.to_array(),
            camera.target.to_array(),
            right.to_array(),
        );
    }

    pub(crate) fn stop_map_sounds(&mut self) {
        for event in self.map_events.drain(..) {
            self.audio.stop_event(&event);
        }
    }

    pub(crate) fn report_eva(&mut self, event: &str) -> bool {
        let Some(eva) = self.eva.as_mut() else {
            return false;
        };
        let Some(id) = eva.id(event) else {
            println!("wotr-voice: no EVA event {event:?}");
            return false;
        };
        eva.report(id, self.eva_frame, None);
        let sound = eva.def(id).and_then(|def| def.sound_for(&self.side));
        println!(
            "wotr-voice: EVA {event} reported; side {:?} sound {sound:?}; sides {:?}",
            self.side,
            eva.def(id).map(|def| def.side_sounds.clone())
        );
        true
    }

    pub(crate) fn advance(&mut self, dt_ms: f64) {
        self.clock.advance(dt_ms);
        self.eva_rest_ms += dt_ms.max(0.0);
        while self.eva_rest_ms >= EVA_FRAME_MS {
            self.eva_rest_ms -= EVA_FRAME_MS;
            self.eva_frame = self.eva_frame.saturating_add(1);
            self.speak();
        }
    }

    fn speak(&mut self) {
        let Some(mut eva) = self.eva.take() else {
            return;
        };
        let vfs = Arc::clone(&self.vfs);
        let mut played: Vec<(String, u64)> = Vec::new();
        {
            let audio = &mut self.audio;
            let lengths = &mut self.lengths;
            #[allow(clippy::cast_possible_truncation)]
            let mut play = |line: &EvaLine| -> Option<u32> {
                let outcome = audio.play(&vfs, &EventRequest::new(&line.sound));
                if outcome != Outcome::Queued {
                    println!(
                        "wotr-voice: EVA {} did not play ({})",
                        line.sound,
                        outcome.label()
                    );
                    return None;
                }
                let length = match lengths.get(&line.sound) {
                    Some(length) => *length,
                    None => {
                        let length = audio.decoded_duration_ms(&vfs, &line.sound).unwrap_or(0);
                        lengths.insert(line.sound.clone(), length);
                        length
                    }
                };
                played.push((line.sound.clone(), length));
                Some(u32::try_from(length).unwrap_or(u32::MAX))
            };
            #[allow(clippy::cast_possible_truncation)]
            let elapsed = EVA_FRAME_MS as f32;
            eva.update(self.eva_frame, elapsed, &self.side, None, &mut play);
        }
        for (sound, length) in played {
            println!("wotr-voice: EVA line {sound} plays for {length} ms");
        }
        self.eva = Some(eva);
    }
}

impl SessionAudio for WotrVoice {
    fn play(&mut self, name: &str) -> Option<u32> {
        let length = self.start(name)?;
        println!("wotr-voice: narration {name} plays for {length} ms");
        self.clock.start(length)
    }

    fn playing(&self, handle: u32) -> bool {
        self.clock.playing(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::{LineClock, UpdateClock};

    #[test]
    fn a_sound_plays_until_its_length_has_passed_in_frame_time() {
        let mut clock = LineClock::default();
        let line = clock.start(1000).unwrap();
        let silent = clock.start(0).unwrap();
        assert!(clock.playing(line));
        assert!(
            !clock.playing(silent),
            "a zero-length sound is already over"
        );
        clock.advance(999.0);
        assert!(clock.playing(line));
        clock.advance(1.0);
        assert!(!clock.playing(line), "exactly its length has passed");
        assert!(
            !clock.playing(99),
            "a handle that was never started is not playing"
        );
    }

    #[test]
    fn a_later_sound_is_timed_from_its_own_start() {
        let mut clock = LineClock::default();
        let first = clock.start(500).unwrap();
        clock.advance(400.0);
        let second = clock.start(500).unwrap();
        clock.advance(200.0);
        assert!(!clock.playing(first));
        assert!(clock.playing(second));
    }

    #[test]
    fn frames_of_any_size_make_five_updates_a_second() {
        let mut clock = UpdateClock::default();
        let small: u32 = (0..1000).map(|_| clock.due(16.0)).sum();
        assert_eq!(small, 80, "16 s of 16 ms frames hold 80 updates");
        let mut clock = UpdateClock::default();
        assert_eq!(clock.due(199.0), 0);
        assert_eq!(clock.due(1.0), 1);
        assert_eq!(clock.due(650.0), 3, "a long frame owes every update in it");
        assert_eq!(clock.due(150.0), 1, "the remainder is kept");
    }
}
