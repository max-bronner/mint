use std::sync::Arc;

use crate::hud::{CheckSpec, ComboItemSpec, ComboSpec, ImageSource, SliderSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gadget {
    MusicVolume,
    SoundFxVolume,
    VoiceVolume,
    AmbientVolume,
    MovieVolume,
    Brightness,
    ScrollSpeed,
    HealthBars,
    AlternateMouseSetup,
    Eax3,
    HighAudioQuality,
    Detail,
}

pub(crate) const ALL: [(&str, Gadget); 12] = [
    ("Options::MusicVolume", Gadget::MusicVolume),
    ("Options::SoundFxVolume", Gadget::SoundFxVolume),
    ("Options::VoiceVolume", Gadget::VoiceVolume),
    ("Options::AmbientVolume", Gadget::AmbientVolume),
    ("Options::MovieVolume", Gadget::MovieVolume),
    ("Options::Brightness", Gadget::Brightness),
    ("Options::ScrollSpeed", Gadget::ScrollSpeed),
    ("Options::HealthBars", Gadget::HealthBars),
    ("Options::AlternateMouseSetUp", Gadget::AlternateMouseSetup),
    ("Options::EAX3", Gadget::Eax3),
    ("Options::HighAudioQuality", Gadget::HighAudioQuality),
    ("Options::Detail", Gadget::Detail),
];

impl Gadget {
    pub(crate) fn named(name: &str) -> Option<Self> {
        ALL.iter().find(|(n, _)| *n == name).map(|(_, g)| *g)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GadgetValue {
    Slider(u32),
    Check(bool),
    Combo(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OptionsPreferences {
    pub(crate) music_volume: u32,
    pub(crate) sound_fx_volume: u32,
    pub(crate) voice_volume: u32,
    pub(crate) ambient_volume: u32,
    pub(crate) movie_volume: u32,
    pub(crate) brightness: u32,
    pub(crate) scroll_speed: u32,
    pub(crate) health_bars: bool,
    pub(crate) alternate_mouse_setup: bool,
    pub(crate) eax3: bool,
    pub(crate) high_audio_quality: bool,
    pub(crate) detail: u32,
}

impl Default for OptionsPreferences {
    fn default() -> Self {
        Self {
            music_volume: 100,
            sound_fx_volume: 100,
            voice_volume: 100,
            ambient_volume: 100,
            movie_volume: 100,
            brightness: 50,
            scroll_speed: 50,
            health_bars: false,
            alternate_mouse_setup: true,
            eax3: false,
            high_audio_quality: true,
            detail: 4,
        }
    }
}

impl OptionsPreferences {
    pub(crate) fn from_audio_settings(settings: &mint_sage_ini::audio::AudioSettingsDef) -> Self {
        Self {
            music_volume: percent_of(settings.default_music_volume),
            sound_fx_volume: percent_of(settings.default_sound_volume),
            voice_volume: percent_of(settings.default_voice_volume),
            ambient_volume: percent_of(settings.default_ambient_volume),
            movie_volume: percent_of(settings.default_movie_volume),
            ..Self::default()
        }
    }
}

fn percent_of(fraction: f32) -> u32 {
    let scaled = (fraction.clamp(0.0, 1.0) * 100.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = scaled as u32;
    percent
}

pub(crate) const DETAIL_ENTRIES: [(&str, u32); 6] = [
    ("GUI:UltraHigh", 4),
    ("GUI:High", 3),
    ("GUI:Medium", 2),
    ("GUI:Low", 1),
    ("GUI:VeryLow", 0),
    ("GUI:Custom", 5),
];

pub(crate) fn image_source(value: GadgetValue, detail_labels: &[String; 6]) -> ImageSource {
    match value {
        GadgetValue::Slider(v) => ImageSource::Slider(SliderSpec {
            value: i32::try_from(v).unwrap_or(i32::MAX),
            enabled: true,
        }),
        GadgetValue::Check(checked) => ImageSource::Check(CheckSpec {
            checked,
            enabled: true,
        }),
        GadgetValue::Combo(level) => ImageSource::Combo(Arc::new(ComboSpec {
            items: detail_labels
                .iter()
                .cloned()
                .map(ComboItemSpec::text)
                .collect(),
            selected: DETAIL_ENTRIES.iter().position(|&(_, l)| l == level),
            enabled: true,
        })),
    }
}

impl OptionsPreferences {
    pub(crate) fn value(&self, gadget: Gadget) -> GadgetValue {
        match gadget {
            Gadget::MusicVolume => GadgetValue::Slider(self.music_volume),
            Gadget::SoundFxVolume => GadgetValue::Slider(self.sound_fx_volume),
            Gadget::VoiceVolume => GadgetValue::Slider(self.voice_volume),
            Gadget::AmbientVolume => GadgetValue::Slider(self.ambient_volume),
            Gadget::MovieVolume => GadgetValue::Slider(self.movie_volume),
            Gadget::Brightness => GadgetValue::Slider(self.brightness),
            Gadget::ScrollSpeed => GadgetValue::Slider(self.scroll_speed),
            Gadget::HealthBars => GadgetValue::Check(self.health_bars),
            Gadget::AlternateMouseSetup => GadgetValue::Check(self.alternate_mouse_setup),
            Gadget::Eax3 => GadgetValue::Check(self.eax3),
            Gadget::HighAudioQuality => GadgetValue::Check(self.high_audio_quality),
            Gadget::Detail => GadgetValue::Combo(self.detail),
        }
    }

    pub(crate) fn set(&mut self, gadget: Gadget, value: GadgetValue) {
        match (gadget, value) {
            (Gadget::MusicVolume, GadgetValue::Slider(v)) => self.music_volume = v,
            (Gadget::SoundFxVolume, GadgetValue::Slider(v)) => self.sound_fx_volume = v,
            (Gadget::VoiceVolume, GadgetValue::Slider(v)) => self.voice_volume = v,
            (Gadget::AmbientVolume, GadgetValue::Slider(v)) => self.ambient_volume = v,
            (Gadget::MovieVolume, GadgetValue::Slider(v)) => self.movie_volume = v,
            (Gadget::Brightness, GadgetValue::Slider(v)) => self.brightness = v,
            (Gadget::ScrollSpeed, GadgetValue::Slider(v)) => self.scroll_speed = v,
            (Gadget::HealthBars, GadgetValue::Check(c)) => self.health_bars = c,
            (Gadget::AlternateMouseSetup, GadgetValue::Check(c)) => self.alternate_mouse_setup = c,
            (Gadget::Eax3, GadgetValue::Check(c)) => self.eax3 = c,
            (Gadget::HighAudioQuality, GadgetValue::Check(c)) => self.high_audio_quality = c,
            (Gadget::Detail, GadgetValue::Combo(d)) => self.detail = d,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Gadget, GadgetValue, OptionsPreferences, DETAIL_ENTRIES};

    #[test]
    fn every_known_name_round_trips_through_its_gadget() {
        for (name, gadget) in [
            ("Options::MusicVolume", Gadget::MusicVolume),
            ("Options::SoundFxVolume", Gadget::SoundFxVolume),
            ("Options::Brightness", Gadget::Brightness),
            ("Options::ScrollSpeed", Gadget::ScrollSpeed),
            ("Options::HealthBars", Gadget::HealthBars),
            ("Options::Detail", Gadget::Detail),
        ] {
            assert_eq!(Gadget::named(name), Some(gadget), "{name}");
        }
        assert_eq!(Gadget::named("Options::NotOurs"), None);
    }

    #[test]
    fn a_gadgets_value_reads_its_own_field_only() {
        let mut prefs = OptionsPreferences {
            music_volume: 40,
            ..OptionsPreferences::default()
        };
        assert_eq!(prefs.value(Gadget::MusicVolume), GadgetValue::Slider(40));
        assert_eq!(
            prefs.value(Gadget::SoundFxVolume),
            GadgetValue::Slider(100),
            "other sliders are untouched"
        );
        prefs.health_bars = true;
        assert_eq!(prefs.value(Gadget::HealthBars), GadgetValue::Check(true));
    }

    #[test]
    fn detail_rows_are_added_in_the_engines_own_order() {
        assert_eq!(
            DETAIL_ENTRIES,
            [
                ("GUI:UltraHigh", 4),
                ("GUI:High", 3),
                ("GUI:Medium", 2),
                ("GUI:Low", 1),
                ("GUI:VeryLow", 0),
                ("GUI:Custom", 5),
            ]
        );
    }

    #[test]
    fn level_0_selects_the_verylow_row() {
        let selected = DETAIL_ENTRIES.iter().position(|&(_, l)| l == 0);
        assert_eq!(selected, Some(4), "VeryLow is the fifth row");
    }

    #[test]
    fn the_default_preferences_are_ultrahigh() {
        assert_eq!(OptionsPreferences::default().detail, 4);
    }

    #[test]
    fn the_sliders_start_at_audiosettings_own_defaults() {
        let settings = mint_sage_ini::audio::AudioSettingsDef {
            default_sound_volume: 0.70,
            default_voice_volume: 0.70,
            default_ambient_volume: 0.50,
            default_movie_volume: 0.70,
            default_music_volume: 0.70,
            ..mint_sage_ini::audio::AudioSettingsDef::default()
        };
        let prefs = OptionsPreferences::from_audio_settings(&settings);
        assert_eq!(prefs.sound_fx_volume, 70);
        assert_eq!(prefs.voice_volume, 70);
        assert_eq!(prefs.ambient_volume, 50);
        assert_eq!(prefs.movie_volume, 70);
        assert_eq!(prefs.music_volume, 70);
        assert_eq!(prefs.detail, 4);
    }
}
