use std::sync::Arc;

use mint_sage_audio::{AudioLibrary, MusicEvent, MusicRequest, Outcome, SageAudio};
use mint_vfs::Vfs;

use crate::diag::diag;

pub(crate) const LOW_LOD_SHELL_MUSIC: &str = "LowLODShellMusic";
pub(crate) const HIGH_LOD_SHELL_MUSIC: &str = "HighLODShellMusic";
pub(crate) const FULL_SCREEN_SUB_MENU_MUSIC: &str = "FullScreenSubMenuMusic";
pub(crate) const SCORE_SCREEN_MUSIC: &str = "ScoreScreenMusic";
pub(crate) const COMBO_BOX_CLICK: &str = "ComboBoxClick";

const NO_SOUND: &str = "NoSound";

pub(crate) struct UiAudio {
    audio: SageAudio,
    vfs: Arc<Vfs>,
}

impl UiAudio {
    pub(crate) fn open(vfs: Arc<Vfs>) -> Self {
        let library = match mint_sage_audio::load_library(&vfs) {
            Ok(library) => library,
            Err(error) => {
                println!("audio: could not read the audio INI ({error}) - running silent");
                AudioLibrary::default()
            }
        };
        let audio = SageAudio::open(library);
        println!("ui sound: shell audio open - {}", audio.readout());
        Self { audio, vfs }
    }

    fn misc(&self, field: &str) -> Option<String> {
        self.audio
            .library()
            .misc(field)
            .filter(|name| !name.eq_ignore_ascii_case(NO_SOUND))
            .map(str::to_string)
    }

    pub(crate) fn play_named(&mut self, event: &str) -> Outcome {
        let vfs = Arc::clone(&self.vfs);
        let outcome = self
            .audio
            .play(&vfs, &mint_sage_audio::EventRequest::new(event));
        diag!("button sound: movie plays {event} ({})", outcome.label());
        outcome
    }

    pub(crate) fn play_misc(&mut self, field: &str) {
        let Some(event) = self.misc(field) else {
            diag!("ui sound: {field} names no sound");
            return;
        };
        let vfs = Arc::clone(&self.vfs);
        let outcome = self
            .audio
            .play(&vfs, &mint_sage_audio::EventRequest::new(&event));
        diag!("ui sound: {field} plays {event} ({})", outcome.label());
    }

    pub(crate) fn main_menu(&mut self, shell_map_on: bool) {
        let field = if shell_map_on {
            HIGH_LOD_SHELL_MUSIC
        } else {
            LOW_LOD_SHELL_MUSIC
        };
        let Some(event) = self.misc(field) else {
            return;
        };
        if self.audio.music().current() == Some(event.as_str()) && self.audio.music().is_playing() {
            return;
        }
        self.audio.music_stop(true);
        let vfs = Arc::clone(&self.vfs);
        self.audio
            .music_replace(&vfs, MusicRequest::forever(&event));
        diag!("music: main menu - {field} = {event}");
    }

    pub(crate) fn sub_menu(&mut self) {
        let Some(event) = self.misc(FULL_SCREEN_SUB_MENU_MUSIC) else {
            return;
        };
        let request = MusicRequest {
            fade_in: true,
            ..MusicRequest::forever(&event)
        };
        let vfs = Arc::clone(&self.vfs);
        self.audio.music_replace(&vfs, request);
        diag!("music: submenu - {FULL_SCREEN_SUB_MENU_MUSIC} = {event}");
    }

    pub(crate) fn score_screen(&mut self) {
        let Some(event) = self.misc(SCORE_SCREEN_MUSIC) else {
            return;
        };
        let vfs = Arc::clone(&self.vfs);
        self.audio.music_stop(true);
        self.audio
            .music_replace(&vfs, MusicRequest::forever(&event));
        diag!("music: score screen - {SCORE_SCREEN_MUSIC} = {event}");
    }

    pub(crate) fn loading(&mut self, music: Option<&str>) {
        let Some(event) = music.filter(|name| !name.eq_ignore_ascii_case(NO_SOUND)) else {
            diag!("music: loading - the local faction names no LoadScreenMusic");
            return;
        };
        let vfs = Arc::clone(&self.vfs);
        self.audio.music_stop(true);
        self.audio.music_replace(&vfs, MusicRequest::forever(event));
        diag!("music: loading - LoadScreenMusic = {event}");
    }

    pub(crate) fn loading_save_file(&mut self) {
        let event = self.misc(crate::campaign_still::SAVE_FILE_LOAD_MUSIC);
        self.loading(event.as_deref());
    }

    pub(crate) fn set_pool_gain(&self, gain: f32) {
        self.audio.set_pool_gain(gain);
    }

    pub(crate) fn frame(&mut self, dt: f32) {
        let vfs = Arc::clone(&self.vfs);
        for event in self.audio.music_update(&vfs, f64::from(dt.max(0.0))) {
            log_music(&event);
        }
    }

    pub(crate) fn settle(&mut self) {
        self.audio.music_settle(std::time::Duration::from_secs(10));
    }

    pub(crate) fn describe(&self) -> String {
        self.audio.music().describe()
    }
}

pub(crate) fn log_music(event: &MusicEvent) {
    match event {
        MusicEvent::Started {
            request,
            track,
            seconds,
        } => diag!("music: {request} starts {track} ({seconds:.1}s)"),
        MusicEvent::Finished { request, notify } => {
            diag!("music: {request} finished (flag {notify:?})");
        }
        MusicEvent::Failed { request, reason } => diag!("music: {request} cannot play - {reason}"),
    }
}
