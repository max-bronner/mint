use std::collections::BTreeMap;

use mint_sage_ini::audio::{AudioEventDef, AudioEventKind, AudioIndex, AudioSettingsDef};
use mint_vfs::Vfs;

pub use mint_audio::EventKey;

#[derive(Debug, Clone, Default)]
pub struct AudioLibrary {
    index: AudioIndex,
    keys: BTreeMap<String, u32>,
    names: Vec<String>,
}

impl AudioLibrary {
    #[must_use]
    pub fn from_index(index: AudioIndex) -> Self {
        let mut keys = BTreeMap::new();
        let mut names = Vec::with_capacity(index.len());
        for (key, (name, def)) in index.events().enumerate() {
            let key = u32::try_from(key).unwrap_or(u32::MAX);
            keys.insert(name.to_string(), key);
            names.push(def.name.clone());
        }
        Self { index, keys, names }
    }

    #[must_use]
    pub fn index(&self) -> &AudioIndex {
        &self.index
    }

    #[must_use]
    pub fn settings(&self) -> &AudioSettingsDef {
        self.index.settings()
    }

    #[must_use]
    pub fn key(&self, name: &str) -> Option<EventKey> {
        self.keys
            .get(&name.to_ascii_lowercase())
            .map(|k| EventKey(*k))
    }

    #[must_use]
    pub fn name(&self, key: EventKey) -> Option<&str> {
        self.names
            .get(usize::try_from(key.0).unwrap_or(usize::MAX))
            .map(String::as_str)
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AudioEventDef> {
        self.index.get(name)
    }

    #[must_use]
    pub fn multisound(&self, name: &str) -> Option<&mint_sage_ini::audio::MultisoundDef> {
        self.index.multisound(name)
    }

    #[must_use]
    pub fn misc(&self, key: &str) -> Option<&str> {
        self.index.misc(key)
    }

    #[must_use]
    pub fn crowd_response(&self, name: &str) -> Option<&mint_sage_ini::audio::CrowdResponseDef> {
        self.index.crowd_response(name)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    #[must_use]
    pub fn variant_path(&self, def: &AudioEventDef, variant: &str) -> Option<String> {
        let folder = self.settings().folder_for(def.kind);
        match def.kind {
            AudioEventKind::Sound => {
                if variant.is_empty() {
                    return None;
                }
                let extension = self.settings().sounds_extension.trim_matches('.');
                Some(format!("{folder}/{variant}.{extension}").to_ascii_lowercase())
            }
            _ => {
                let filename = def.filename.as_ref()?;
                if filename.is_empty() {
                    return None;
                }
                Some(format!("{folder}/{filename}").to_ascii_lowercase())
            }
        }
    }

    #[must_use]
    pub fn variant_paths(&self, def: &AudioEventDef) -> Vec<String> {
        match def.kind {
            AudioEventKind::Sound => def
                .sounds
                .iter()
                .filter_map(|variant| self.variant_path(def, &variant.name))
                .collect(),
            _ => self.variant_path(def, "").into_iter().collect(),
        }
    }

    #[must_use]
    pub fn event_duration_ms(&self, vfs: &Vfs, name: &str) -> Option<u64> {
        let def = self.get(name)?;
        let longest_of = |paths: Vec<String>| {
            paths
                .into_iter()
                .filter_map(|path| {
                    let bytes = vfs.read(&path).ok()?;
                    Some(mint_audio_format::probe_wav(&bytes).ok()?.duration_ms())
                })
                .max()
        };
        let body = longest_of(self.variant_paths(def))?;
        let lead_and_tail = match def.kind {
            AudioEventKind::Sound => [&def.attack, &def.decay]
                .into_iter()
                .filter_map(|variants| {
                    longest_of(
                        variants
                            .iter()
                            .filter_map(|variant| self.variant_path(def, &variant.name))
                            .collect(),
                    )
                })
                .fold(0_u64, u64::saturating_add),
            _ => 0,
        };
        Some(body.saturating_add(lead_and_tail))
    }
}
