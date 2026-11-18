use std::sync::atomic::{AtomicU64, Ordering};

pub const LOOPBACK_VAR: &str = "MINT_AUDIO_LOOPBACK";

static STALLS: AtomicU64 = AtomicU64::new(0);
static STALL_LOST_US: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn stalls() -> (u64, u64) {
    (
        STALLS.load(Ordering::Relaxed),
        STALL_LOST_US.load(Ordering::Relaxed),
    )
}

pub fn start_from_env() {
    #[cfg(windows)]
    imp::start_from_env();
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    clippy::cast_possible_truncation
)]
mod imp {
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;
    use std::sync::Once;
    use std::time::Duration;

    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    use super::{STALLS, STALL_LOST_US};
    use crate::tap::{header, patch};
    use crate::wasapi::{negotiate, SampleKind};

    use super::LOOPBACK_VAR;

    const BUFFER_100NS: i64 = 10_000_000;

    const POLL: Duration = Duration::from_millis(5);

    const MAX_FILL_SECONDS: u64 = 10;

    static STARTED: Once = Once::new();

    pub(super) fn start_from_env() {
        let Some(path) = std::env::var_os(LOOPBACK_VAR).map(PathBuf::from) else {
            return;
        };
        STARTED.call_once(|| {
            let spawned = std::thread::Builder::new()
                .name("mint-audio-loopback".to_string())
                .spawn(move || {
                    if let Err(error) = capture(&path) {
                        println!("audio loopback: {}: {error}", path.display());
                    }
                });
            if spawned.is_ok() {
                println!("audio loopback: capturing the default render endpoint");
            }
        });
    }

    fn capture(path: &Path) -> Result<(), String> {
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if init.is_err() {
            return Err(format!("CoInitializeEx failed: {init:?}"));
        }
        let outcome = capture_loop(path);
        unsafe { CoUninitialize() };
        outcome
    }

    fn capture_loop(path: &Path) -> Result<(), String> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|e| format!("no device enumerator: {e}"))?;
        let endpoint = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .map_err(|e| format!("no default render endpoint: {e}"))?;
        let client: IAudioClient = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
            .map_err(|e| format!("could not activate the audio client: {e}"))?;
        let mix = unsafe { client.GetMixFormat() }.map_err(|e| format!("no mix format: {e}"))?;
        if mix.is_null() {
            return Err("GetMixFormat returned nothing".to_string());
        }
        let negotiated = unsafe { negotiate(mix) };
        let init = negotiated.and_then(|found| {
            unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    BUFFER_100NS,
                    0,
                    mix,
                    None,
                )
            }
            .map_err(|e| format!("loopback Initialize failed: {e}"))?;
            Ok(found)
        });
        unsafe { CoTaskMemFree(Some(mix.cast::<core::ffi::c_void>().cast_const())) };
        let (format, kind) = init?;
        let capture: IAudioCaptureClient =
            unsafe { client.GetService() }.map_err(|e| format!("no capture client: {e}"))?;
        unsafe { client.Start() }.map_err(|e| format!("loopback Start failed: {e}"))?;

        let io = |e: std::io::Error| e.to_string();
        let mut wav = BufWriter::new(File::create(path).map_err(io)?);
        let mut log = BufWriter::new(File::create(path.with_extension("log")).map_err(io)?);
        header(&mut wav, format, 0).map_err(io)?;
        writeln!(
            log,
            "loopback; format {} Hz {} ch; columns: event file_frame endpoint_frame qpc_100ns",
            format.sample_rate, format.channels
        )
        .map_err(io)?;

        let channels = usize::from(format.channels.max(1));
        let mut written: u64 = 0;
        let mut next_position: Option<u64> = None;
        let mut last_sync: u64 = 0;
        let mut last_patch: u64 = 0;
        let rate = u64::from(format.sample_rate.max(1));
        let slack = rate / 1000;
        let mut origin: Option<(u64, u64)> = None;
        let mut last_drift: Option<i128> = None;
        loop {
            std::thread::sleep(POLL);
            loop {
                let pending = unsafe { capture.GetNextPacketSize() }
                    .map_err(|e| format!("GetNextPacketSize failed: {e}"))?;
                if pending == 0 {
                    break;
                }
                let mut data: *mut u8 = core::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                let mut position: u64 = 0;
                let mut qpc: u64 = 0;
                unsafe {
                    capture.GetBuffer(
                        &mut data,
                        &mut frames,
                        &mut flags,
                        Some(&mut position),
                        Some(&mut qpc),
                    )
                }
                .map_err(|e| format!("GetBuffer failed: {e}"))?;

                let flag = |f: i32| u32::try_from(f).is_ok_and(|f| flags & f != 0);
                if let Some(expected) = next_position {
                    if position > expected {
                        let missing = position.saturating_sub(expected);
                        let fill = missing.min(rate.saturating_mul(MAX_FILL_SECONDS));
                        let zeros = vec![0u8; usize::try_from(fill).unwrap_or(0) * channels * 4];
                        wav.write_all(&zeros).map_err(io)?;
                        writeln!(log, "gap {written} {expected} {qpc} missing {missing}")
                            .map_err(io)?;
                        written = written.saturating_add(fill);
                    }
                }
                let (position0, qpc0) = *origin.get_or_insert((position, qpc));
                let drift = i128::from(position)
                    - i128::from(position0)
                    - i128::from(qpc.saturating_sub(qpc0)) * i128::from(rate) / 10_000_000;
                if let Some(before) = last_drift {
                    let lost = before - drift;
                    if lost > i128::from(slack) {
                        let lost = u64::try_from(lost).unwrap_or(0);
                        writeln!(log, "stall {written} {position} {qpc} lost {lost}")
                            .map_err(io)?;
                        STALLS.fetch_add(1, Ordering::Relaxed);
                        STALL_LOST_US
                            .fetch_add(lost.saturating_mul(1_000_000) / rate, Ordering::Relaxed);
                    }
                }
                last_drift = Some(drift);
                if flag(AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0) && next_position.is_some() {
                    writeln!(log, "discontinuity {written} {position} {qpc}").map_err(io)?;
                }
                if flag(AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0) {
                    writeln!(log, "timestamp-error {written} {position} {qpc}").map_err(io)?;
                }
                if next_position.is_none() || written.saturating_sub(last_sync) >= rate {
                    writeln!(log, "sync {written} {position} {qpc} drift {drift}").map_err(io)?;
                    last_sync = written;
                }

                let samples = (frames as usize).saturating_mul(channels);
                if flag(AUDCLNT_BUFFERFLAGS_SILENT.0) || data.is_null() {
                    wav.write_all(&vec![0u8; samples * 4]).map_err(io)?;
                } else {
                    let bytes = unsafe { read_samples(data, samples, kind) };
                    wav.write_all(&bytes).map_err(io)?;
                }
                written = written.saturating_add(u64::from(frames));
                next_position = Some(position.saturating_add(u64::from(frames)));
                unsafe { capture.ReleaseBuffer(frames) }
                    .map_err(|e| format!("ReleaseBuffer failed: {e}"))?;
            }
            if written.saturating_sub(last_patch) >= rate {
                patch(&mut wav, format, written).map_err(io)?;
                log.flush().map_err(io)?;
                last_patch = written;
            }
        }
    }

    unsafe fn read_samples(data: *const u8, samples: usize, kind: SampleKind) -> Vec<u8> {
        let mut out = Vec::with_capacity(samples.saturating_mul(4));
        for index in 0..samples {
            let value = match kind {
                SampleKind::Float32 => unsafe { data.cast::<f32>().add(index).read_unaligned() },
                SampleKind::Pcm16 => {
                    f32::from(unsafe { data.cast::<i16>().add(index).read_unaligned() }) / 32768.0
                }
            };
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }
}
