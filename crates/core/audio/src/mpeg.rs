#![allow(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use mint_audio_format::Pcm16;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MFAudioFormat_PCM, MFCreateMFByteStreamOnStream, MFCreateMediaType,
    MFCreateSourceReaderFromByteStream, MFMediaType_Audio, MFShutdown, MFStartup, MFSTARTUP_LITE,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_VERSION,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED, STREAM_SEEK_SET,
};

const MAX_MINUTES: u64 = 15;

pub(crate) fn decode(bytes: &[u8]) -> Result<Pcm16, String> {
    let len = u32::try_from(bytes.len()).map_err(|_| "file larger than 4 GiB".to_string())?;
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) };
    let result = match started {
        Ok(()) => {
            let decoded = unsafe { read_all(bytes, len) };
            let _ = unsafe { MFShutdown() };
            decoded
        }
        Err(error) => Err(format!("Media Foundation did not start: {error}")),
    };
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

unsafe fn read_all(bytes: &[u8], len: u32) -> Result<Pcm16, String> {
    let step = |what: &str| {
        let what = what.to_string();
        move |error: windows::core::Error| format!("{what}: {error}")
    };
    let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).map_err(step("memory stream"))?;
    stream
        .Write(bytes.as_ptr().cast(), len, None)
        .ok()
        .map_err(step("memory stream write"))?;
    stream
        .Seek(0, STREAM_SEEK_SET, None)
        .map_err(step("memory stream seek"))?;
    let byte_stream = MFCreateMFByteStreamOnStream(&stream).map_err(step("byte stream"))?;
    let reader: IMFSourceReader =
        MFCreateSourceReaderFromByteStream(&byte_stream, None).map_err(step("source reader"))?;

    let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    reader
        .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
        .map_err(step("stream selection"))?;
    reader
        .SetStreamSelection(audio, true)
        .map_err(step("no audio stream"))?;
    let wanted = MFCreateMediaType().map_err(step("media type"))?;
    wanted
        .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
        .map_err(step("media type"))?;
    wanted
        .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
        .map_err(step("media type"))?;
    wanted
        .SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
        .map_err(step("media type"))?;
    reader
        .SetCurrentMediaType(audio, None, &wanted)
        .map_err(step("no PCM conversion"))?;
    let actual = reader
        .GetCurrentMediaType(audio)
        .map_err(step("output type"))?;
    let sample_rate = actual
        .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
        .map_err(step("output rate"))?;
    let channels = actual
        .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
        .map_err(step("output channels"))?;
    let channels = u16::try_from(channels)
        .ok()
        .filter(|c| (1..=8).contains(c))
        .ok_or_else(|| format!("{channels} channels"))?;
    if sample_rate == 0 {
        return Err("zero sample rate".to_string());
    }
    let cap = u64::from(sample_rate)
        .saturating_mul(60 * MAX_MINUTES)
        .saturating_mul(u64::from(channels));
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);

    let mut samples: Vec<i16> = Vec::new();
    loop {
        let mut flags = 0u32;
        let mut sample = None;
        reader
            .ReadSample(audio, 0, None, Some(&mut flags), None, Some(&mut sample))
            .map_err(step("read"))?;
        if let Some(sample) = sample {
            let buffer = sample
                .ConvertToContiguousBuffer()
                .map_err(step("sample buffer"))?;
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer
                .Lock(&mut data, None, Some(&mut length))
                .map_err(step("lock"))?;
            if !data.is_null() {
                let bytes = std::slice::from_raw_parts(data, length as usize);
                let room = cap.saturating_sub(samples.len());
                samples.extend(bytes.chunks_exact(2).take(room).map(|pair| match pair {
                    [low, high] => i16::from_le_bytes([*low, *high]),
                    _ => 0,
                }));
            }
            let _ = buffer.Unlock();
        }
        if flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
            return Err("source reader reported a stream error".to_string());
        }
        if flags & (MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32) != 0 {
            return Err("audio format changed mid-stream".to_string());
        }
        if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 || samples.len() >= cap {
            break;
        }
    }
    let whole = samples
        .len()
        .saturating_sub(samples.len() % usize::from(channels));
    samples.truncate(whole);
    Ok(Pcm16 {
        sample_rate,
        channels,
        samples,
    })
}
