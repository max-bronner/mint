#![allow(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, GetCurrentThread,
    SetThreadPriority, WaitForSingleObject, THREAD_PRIORITY_TIME_CRITICAL,
};

use crate::device::{Device, DeviceFormat, DeviceStats, Source};
use crate::Error;

const WAVE_FORMAT_PCM: u32 = 1;

const WAVE_FORMAT_IEEE_FLOAT: u32 = 3;

const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

const BUFFER_DURATION_100NS: i64 = 300_000;

const WAIT_TIMEOUT_MS: u32 = 2_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleKind {
    Float32,
    Pcm16,
}

#[derive(Default)]
struct Counters {
    wakes: AtomicU64,
    underruns: AtomicU64,
    buffer_us: AtomicU64,
    max_gap_us: AtomicU64,
    max_render_us: AtomicU64,
}

pub(crate) struct WasapiDevice {
    format: DeviceFormat,
    kind: SampleKind,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    thread: Option<JoinHandle<()>>,
}

impl Device for WasapiDevice {
    fn format(&self) -> DeviceFormat {
        self.format
    }

    fn describe(&self) -> String {
        format!(
            "WASAPI shared mode, {} Hz, {} channels, {}",
            self.format.sample_rate,
            self.format.channels,
            match self.kind {
                SampleKind::Float32 => "32-bit float",
                SampleKind::Pcm16 => "16-bit PCM",
            }
        )
    }

    fn stats(&self) -> DeviceStats {
        let c = &self.counters;
        DeviceStats {
            wakes: c.wakes.load(Ordering::Relaxed),
            underruns: c.underruns.load(Ordering::Relaxed),
            buffer_us: c.buffer_us.load(Ordering::Relaxed),
            max_gap_us: c.max_gap_us.load(Ordering::Relaxed),
            max_render_us: c.max_render_us.load(Ordering::Relaxed),
        }
    }
}

impl Drop for WasapiDevice {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn open(source: Box<dyn Source>) -> Result<WasapiDevice, Error> {
    let stop = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::default());
    let (tx, rx) = mpsc::channel::<Result<(DeviceFormat, SampleKind), String>>();
    let thread_stop = Arc::clone(&stop);
    let thread_counters = Arc::clone(&counters);

    let thread = std::thread::Builder::new()
        .name("mint-audio-wasapi".to_string())
        .spawn(move || run(source, &thread_stop, &thread_counters, &tx))
        .map_err(|e| Error::NoDevice(format!("could not start the render thread: {e}")))?;

    match rx.recv() {
        Ok(Ok((format, kind))) => Ok(WasapiDevice {
            format,
            kind,
            stop,
            counters,
            thread: Some(thread),
        }),
        Ok(Err(message)) => {
            let _ = thread.join();
            Err(Error::NoDevice(message))
        }
        Err(_) => {
            let _ = thread.join();
            Err(Error::NoDevice(
                "the render thread stopped before reporting a format".to_string(),
            ))
        }
    }
}

fn run(
    mut source: Box<dyn Source>,
    stop: &AtomicBool,
    counters: &Counters,
    tx: &mpsc::Sender<Result<(DeviceFormat, SampleKind), String>>,
) {
    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if init.is_err() {
        let _ = tx.send(Err(format!("CoInitializeEx failed: {init:?}")));
        return;
    }

    let mut task_index = 0u32;
    let mmcss =
        unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut task_index) };
    if mmcss.is_err() {
        let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
    }

    let outcome = render_loop(&mut source, stop, counters, tx);
    if let Err(message) = outcome {
        let _ = tx.send(Err(message));
    }

    if let Ok(handle) = mmcss {
        let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
    }
    unsafe { CoUninitialize() };
}

fn render_loop(
    source: &mut Box<dyn Source>,
    stop: &AtomicBool,
    counters: &Counters,
    tx: &mpsc::Sender<Result<(DeviceFormat, SampleKind), String>>,
) -> Result<(), String> {
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
    let init = negotiated.and_then(|(format, kind)| {
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                BUFFER_DURATION_100NS,
                0,
                mix,
                None,
            )
        }
        .map_err(|e| format!("IAudioClient::Initialize failed: {e}"))?;
        Ok((format, kind))
    });
    unsafe { CoTaskMemFree(Some(mix.cast::<core::ffi::c_void>().cast_const())) };
    let (format, kind) = init?;

    let event = unsafe { CreateEventW(None, false, false, None) }
        .map_err(|e| format!("could not create the render event: {e}"))?;
    unsafe { client.SetEventHandle(event) }.map_err(|e| format!("SetEventHandle failed: {e}"))?;

    let buffer_frames =
        unsafe { client.GetBufferSize() }.map_err(|e| format!("GetBufferSize failed: {e}"))?;
    let render: IAudioRenderClient =
        unsafe { client.GetService() }.map_err(|e| format!("no render client: {e}"))?;
    unsafe { client.Start() }.map_err(|e| format!("IAudioClient::Start failed: {e}"))?;

    let _ = tx.send(Ok((format, kind)));

    let channels = usize::from(format.channels.max(1));
    let mut scratch = vec![0.0f32; (buffer_frames as usize).saturating_mul(channels)];
    counters.buffer_us.store(
        u64::from(buffer_frames).saturating_mul(1_000_000) / u64::from(format.sample_rate.max(1)),
        Ordering::Relaxed,
    );
    let micros = |since: Instant| u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut last_wake: Option<Instant> = None;

    while !stop.load(Ordering::Relaxed) {
        let waited = unsafe { WaitForSingleObject(event, WAIT_TIMEOUT_MS) };
        if waited.0 != 0 {
            break;
        }
        let padding = match unsafe { client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(_) => break,
        };
        let woke = Instant::now();
        if let Some(last) = last_wake {
            counters
                .max_gap_us
                .fetch_max(micros(last), Ordering::Relaxed);
            if padding == 0 {
                counters.underruns.fetch_add(1, Ordering::Relaxed);
            }
        }
        last_wake = Some(woke);
        counters.wakes.fetch_add(1, Ordering::Relaxed);
        let available = buffer_frames.saturating_sub(padding);
        if available == 0 {
            continue;
        }
        let Ok(ptr) = (unsafe { render.GetBuffer(available) }) else {
            break;
        };

        let wanted = (available as usize).saturating_mul(channels);
        let slice = scratch.get_mut(..wanted).unwrap_or(&mut []);
        let mixing = Instant::now();
        source.render(slice, format);
        counters
            .max_render_us
            .fetch_max(micros(mixing), Ordering::Relaxed);
        unsafe { write_frames(ptr, slice, kind) };

        if unsafe { render.ReleaseBuffer(available, 0) }.is_err() {
            break;
        }
    }

    let _ = unsafe { client.Stop() };
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(event) };
    Ok(())
}

pub(crate) unsafe fn negotiate(
    mix: *const WAVEFORMATEX,
) -> Result<(DeviceFormat, SampleKind), String> {
    let header = unsafe { core::ptr::read_unaligned(mix) };
    let tag = header.wFormatTag;
    let bits = header.wBitsPerSample;
    let channels = header.nChannels;
    let rate = header.nSamplesPerSec;

    let effective = if tag == WAVE_FORMAT_EXTENSIBLE {
        let extended = unsafe { core::ptr::read_unaligned(mix.cast::<WAVEFORMATEXTENSIBLE>()) };
        extended.SubFormat.data1
    } else {
        u32::from(tag)
    };

    let kind = match (effective, bits) {
        (WAVE_FORMAT_IEEE_FLOAT, 32) => SampleKind::Float32,
        (WAVE_FORMAT_PCM, 16) => SampleKind::Pcm16,
        _ => {
            return Err(format!(
                "unsupported device mix format: tag {effective}, {bits} bits, \
                 {channels} channels at {rate} Hz"
            ))
        }
    };

    if channels == 0 || rate == 0 {
        return Err(format!("device reported {channels} channels at {rate} Hz"));
    }

    Ok((
        DeviceFormat {
            sample_rate: rate,
            channels,
        },
        kind,
    ))
}

unsafe fn write_frames(ptr: *mut u8, samples: &[f32], kind: SampleKind) {
    match kind {
        SampleKind::Float32 => {
            let dst = ptr.cast::<f32>();
            for (index, value) in samples.iter().enumerate() {
                unsafe { dst.add(index).write_unaligned(value.clamp(-1.0, 1.0)) };
            }
        }
        SampleKind::Pcm16 => {
            let dst = ptr.cast::<i16>();
            for (index, value) in samples.iter().enumerate() {
                let scaled = (value.clamp(-1.0, 1.0) * 32767.0) as i32;
                unsafe {
                    dst.add(index)
                        .write_unaligned(scaled.clamp(-32768, 32767) as i16)
                };
            }
        }
    }
}
