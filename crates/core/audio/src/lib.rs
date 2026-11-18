use thiserror::Error as ThisError;

pub mod device;
pub mod engine;
pub mod mixer;
#[cfg(windows)]
mod mpeg;
pub mod offline;

#[cfg(windows)]
mod wasapi;

pub use device::{Device, DeviceFormat, Source};
pub use engine::AudioEngine;
pub use mixer::{
    Admission, Clip, CullReason, EventKey, Listener, Mixer, MixerConfig, MixerStats, Play,
    Priority, Rolloff, Spatial, VoiceId, MAX_BUSES,
};

#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    #[error("no audio device: {0}")]
    NoDevice(String),

    #[error("could not decode audio")]
    Decode(#[from] mint_audio_format::Error),
    #[error("could not decode compressed audio: {0}")]
    Compressed(String),
}

pub fn decode_compressed(bytes: &[u8]) -> Result<Clip> {
    #[cfg(windows)]
    {
        mpeg::decode(bytes)
            .map(Clip::from_pcm16)
            .map_err(Error::Compressed)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        Err(Error::Compressed(
            "no compressed-audio decoder on this platform yet".to_string(),
        ))
    }
}

pub type Result<T> = core::result::Result<T, Error>;
