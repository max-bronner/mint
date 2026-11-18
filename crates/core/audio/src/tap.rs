use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;

use crate::device::DeviceFormat;

static OPENED: AtomicU32 = AtomicU32::new(0);

fn numbered(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "mix".to_string(), |s| s.to_string_lossy().into_owned());
    path.with_file_name(format!("{stem}.{n}.wav"))
}

pub const TAP_VAR: &str = "MINT_AUDIO_TAP";

struct Block {
    format: DeviceFormat,
    mix: Vec<f32>,
    streams: Vec<f32>,
    notes: Vec<String>,
}

pub(crate) struct Tap {
    tx: mpsc::Sender<Block>,
    pub(crate) stem: Vec<f32>,
    pub(crate) notes: Vec<String>,
}

impl Tap {
    pub(crate) fn from_env(engine: &str) -> Option<Self> {
        let base = PathBuf::from(std::env::var_os(TAP_VAR)?);
        let n = OPENED.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        let path = numbered(&base, n);
        println!("audio tap: engine {n} ({engine}) -> {}", path.display());
        let title = format!("engine {n} ({engine})");
        let (tx, rx) = mpsc::channel::<Block>();
        std::thread::Builder::new()
            .name("mint-audio-tap".to_string())
            .spawn(move || {
                if let Err(error) = write_all(&path, &title, &rx) {
                    println!("audio tap: {}: {error}", path.display());
                }
            })
            .ok()?;
        Some(Self {
            tx,
            stem: Vec::new(),
            notes: Vec::new(),
        })
    }

    pub(crate) fn send(&mut self, format: DeviceFormat, mix: &[f32]) {
        let _ = self.tx.send(Block {
            format,
            mix: mix.to_vec(),
            streams: self.stem.get(..mix.len()).unwrap_or(&[]).to_vec(),
            notes: std::mem::take(&mut self.notes),
        });
    }
}

fn write_all(path: &Path, title: &str, rx: &mpsc::Receiver<Block>) -> std::io::Result<()> {
    let mut mix = BufWriter::new(File::create(path)?);
    let mut streams = BufWriter::new(File::create(path.with_extension("streams.wav"))?);
    let mut log = BufWriter::new(File::create(path.with_extension("log"))?);
    let mut frames: u64 = 0;
    let mut callback: u64 = 0;
    let mut format = None;
    while let Ok(block) = rx.recv() {
        let channels = u64::from(block.format.channels.max(1));
        if format.is_none() {
            format = Some(block.format);
            header(&mut mix, block.format, 0)?;
            header(&mut streams, block.format, 0)?;
            writeln!(
                log,
                "{title}; format {} Hz {} ch; columns: callback first_frame frames notes",
                block.format.sample_rate, block.format.channels
            )?;
        }
        let count = u64::try_from(block.mix.len()).unwrap_or(0) / channels;
        if !block.notes.is_empty() {
            writeln!(
                log,
                "{callback} {frames} {count} {}",
                block.notes.join("; ")
            )?;
        }
        for sample in &block.mix {
            mix.write_all(&sample.to_le_bytes())?;
        }
        for sample in &block.streams {
            streams.write_all(&sample.to_le_bytes())?;
        }
        frames = frames.saturating_add(count);
        callback = callback.saturating_add(1);
        if callback % 100 == 0 {
            if let Some(format) = format {
                for file in [&mut mix, &mut streams] {
                    patch(file, format, frames)?;
                }
                log.flush()?;
            }
        }
    }
    if let Some(format) = format {
        for file in [&mut mix, &mut streams] {
            patch(file, format, frames)?;
        }
    }
    log.flush()
}

fn data_bytes(format: DeviceFormat, frames: u64) -> u32 {
    let bytes = frames
        .saturating_mul(u64::from(format.channels.max(1)))
        .saturating_mul(4);
    u32::try_from(bytes).unwrap_or(u32::MAX - 64)
}

pub(crate) fn header(
    out: &mut impl Write,
    format: DeviceFormat,
    frames: u64,
) -> std::io::Result<()> {
    let channels = format.channels.max(1);
    let data = data_bytes(format, frames);
    out.write_all(b"RIFF")?;
    out.write_all(&data.saturating_add(36).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&3u16.to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&format.sample_rate.to_le_bytes())?;
    out.write_all(
        &format
            .sample_rate
            .saturating_mul(u32::from(channels))
            .saturating_mul(4)
            .to_le_bytes(),
    )?;
    out.write_all(&channels.saturating_mul(4).to_le_bytes())?;
    out.write_all(&32u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())
}

pub(crate) fn patch(
    file: &mut BufWriter<File>,
    format: DeviceFormat,
    frames: u64,
) -> std::io::Result<()> {
    file.flush()?;
    let end = file.stream_position()?;
    file.seek(SeekFrom::Start(0))?;
    header(file, format, frames)?;
    file.seek(SeekFrom::Start(end))?;
    file.flush()
}
