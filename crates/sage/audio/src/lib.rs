pub mod bank;
pub mod eva;
pub mod library;
pub mod microphone;
pub mod music;
pub mod rng;
pub mod runtime;

pub use bank::{BankStats, SoundBank, MIN_BUDGET_BYTES};
pub use eva::{Eva, EvaLine};
pub use library::{AudioLibrary, EventKey};
pub use music::{MusicEvent, MusicRequest, MusicSystem};
pub use rng::ClientRng;
pub use runtime::{Audience, EventRequest, Outcome, Overrides, Reach, RequestStats, SageAudio};

pub use mint_audio::mixer::spatial_level;
pub use mint_audio::{AudioEngine, Listener, MixerConfig, MixerStats, Rolloff, Spatial, VoiceId};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("could not read {path}")]
    Ini {
        path: String,
        #[source]
        source: mint_sage_ini::Error,
    },

    #[error("could not read {path} from the mounted archives")]
    Vfs {
        path: String,
        #[source]
        source: mint_vfs::Error,
    },
}

pub type Result<T> = core::result::Result<T, Error>;

pub fn load_library(vfs: &mint_vfs::Vfs) -> Result<AudioLibrary> {
    use mint_sage_ini::table::BlockTable;
    use mint_sage_ini::{audio::AudioIndex, Document};

    let table = BlockTable::audio_path();
    let mut index = AudioIndex::new();

    let mut paths: Vec<String> = vfs
        .paths_under("data/ini")
        .filter(|p| {
            let path = p.as_str();
            path.ends_with(".ini") && is_audio_document(path)
        })
        .map(|p| p.as_str().to_string())
        .collect();
    paths.sort_by_key(|path| (!path.contains("/default/"), path.clone()));

    let mut documents = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = vfs.read(&path).map_err(|source| Error::Vfs {
            path: path.clone(),
            source,
        })?;
        let text = String::from_utf8_lossy(&bytes);
        let document = Document::parse(&path, &text, &table).map_err(|source| Error::Ini {
            path: path.clone(),
            source,
        })?;
        documents.push(document);
    }

    for document in &documents {
        index.insert_defines(document.defines());
    }
    for document in &documents {
        index.insert_document(document);
    }

    Ok(AudioLibrary::from_index(index))
}

fn is_audio_document(path: &str) -> bool {
    const NAMES: [&str; 8] = [
        "soundeffects.ini",
        "voice.ini",
        "speech.ini",
        "music.ini",
        "ambientstream.ini",
        "largegroupaudio.ini",
        "audiosettings.ini",
        "miscaudio.ini",
    ];
    let file = path.rsplit('/').next().unwrap_or(path);
    NAMES.iter().any(|name| file.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests;
