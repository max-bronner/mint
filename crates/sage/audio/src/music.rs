use std::sync::mpsc;
use std::sync::Arc;

use mint_audio::{AudioEngine, Clip, Play, VoiceId};
use mint_sage_ini::audio::{AudioEventDef, AudioEventKind, ControlFlags, MultisoundDef};
use mint_vfs::Vfs;

use crate::library::AudioLibrary;
use crate::rng::ClientRng;

const MAX_PICK_DEPTH: u8 = 4;

const CACHED_TRACKS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicRequest {
    pub name: String,
    pub times: Option<u32>,
    pub fade_out: bool,
    pub fade_in: bool,
    pub notify: Option<String>,
}

impl MusicRequest {
    #[must_use]
    pub fn forever(name: &str) -> Self {
        Self {
            name: name.to_string(),
            times: None,
            fade_out: false,
            fade_in: false,
            notify: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MusicEvent {
    Started {
        request: String,
        track: String,
        seconds: f64,
    },
    Finished {
        request: String,
        notify: Option<String>,
    },
    Failed {
        request: String,
        reason: String,
    },
}

#[derive(Debug)]
enum State {
    Loading { ticket: u64, track: String },
    Ready { track: String, clip: Arc<Clip> },
    Voiced(Sounding),
    Done,
}

#[derive(Debug)]
struct Sounding {
    id: VoiceId,
    track: String,
    length: f64,
    plays: Option<u32>,
    heard: f64,
    held: bool,
    fade_on_kill: bool,
}

#[derive(Debug)]
struct Entry {
    request: MusicRequest,
    left: Option<u32>,
    picks_again: bool,
    state: State,
}

type Decoded = (u64, String, Result<Arc<Clip>, String>);

type Job = (u64, String, bool, Vec<u8>);

pub struct MusicSystem {
    stack: Vec<Entry>,
    fade_ms: u32,
    next_ticket: u64,
    jobs: Option<mpsc::Sender<Job>>,
    results: Option<mpsc::Receiver<Decoded>>,
    worker: Option<std::thread::JoinHandle<()>>,
    cache: Vec<(String, Arc<Clip>)>,
    events: Vec<MusicEvent>,
}

impl MusicSystem {
    #[must_use]
    pub fn new(fade_ms: u32) -> Self {
        Self {
            stack: Vec::new(),
            fade_ms,
            next_ticket: 1,
            jobs: None,
            results: None,
            worker: None,
            cache: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn replace(&mut self, engine: &AudioEngine, request: MusicRequest, ctx: &mut Context<'_>) {
        if let Some(top) = self.stack.pop() {
            self.silence(engine, top, request.fade_out);
        }
        self.begin(request, ctx);
    }

    pub fn push(&mut self, engine: &AudioEngine, request: MusicRequest, ctx: &mut Context<'_>) {
        let fade = if request.fade_out { self.fade_ms } else { 0 };
        if let Some(Entry {
            state: State::Voiced(sounding),
            ..
        }) = self.stack.last_mut()
        {
            engine.pause(sounding.id, fade);
            sounding.held = true;
        }
        self.begin(request, ctx);
    }

    pub fn pop(&mut self, engine: &AudioEngine, fade_out: bool, fade_in: bool) {
        if let Some(top) = self.stack.pop() {
            self.silence(engine, top, fade_out);
        }
        let fade = if fade_in { self.fade_ms } else { 0 };
        if let Some(Entry {
            state: State::Voiced(sounding),
            ..
        }) = self.stack.last_mut()
        {
            if sounding.held {
                engine.resume(sounding.id, fade);
                sounding.held = false;
            }
        }
    }

    pub fn stop(&mut self, engine: &AudioEngine, fade: bool) {
        while let Some(entry) = self.stack.pop() {
            self.silence(engine, entry, fade);
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.stack.last().map(|e| e.request.name.as_str())
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|e| !matches!(e.state, State::Done))
    }

    #[must_use]
    pub fn describe(&self) -> String {
        if self.stack.is_empty() {
            return "music: silent".to_string();
        }
        let parts: Vec<String> = self
            .stack
            .iter()
            .map(|e| {
                let state = match &e.state {
                    State::Loading { track, .. } => format!("loading {track}"),
                    State::Ready { track, .. } => format!("ready {track}"),
                    State::Voiced(s) if s.held => format!("held {}", s.track),
                    State::Voiced(s) => format!("playing {} {:.0}s", s.track, s.heard),
                    State::Done => "done".to_string(),
                };
                format!("{} [{state}]", e.request.name)
            })
            .collect();
        format!("music: {}", parts.join(" < "))
    }

    pub fn update(
        &mut self,
        engine: &AudioEngine,
        dt: f64,
        ctx: &mut Context<'_>,
    ) -> Vec<MusicEvent> {
        self.receive();
        let fade_ms = self.fade_ms;
        let mut start = None;
        let mut ended = false;
        if let Some(top) = self.stack.last_mut() {
            match &mut top.state {
                State::Ready { .. } => start = Some(()),
                State::Voiced(sounding) if !sounding.held => {
                    sounding.heard += dt.max(0.0);
                    if let Some(plays) = sounding.plays {
                        if sounding.heard >= sounding.length * f64::from(plays) {
                            ended = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if start.is_some() {
            self.start_top(engine, fade_ms, ctx.library);
        }
        if ended {
            self.top_ended(ctx);
        }
        std::mem::take(&mut self.events)
    }

    fn top_ended(&mut self, ctx: &mut Context<'_>) {
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        let plays_made = match &top.state {
            State::Voiced(s) => s.plays.unwrap_or(1),
            _ => 1,
        };
        top.left = top.left.map(|left| left.saturating_sub(plays_made));
        let more = top.left.is_none_or(|left| left > 0);
        if more && top.picks_again {
            let name = top.request.name.clone();
            top.state = State::Done;
            let next =
                pick(ctx, &name, 0).map_or(State::Done, |(track, _)| self.prepare(ctx, track));
            if let Some(top) = self.stack.last_mut() {
                top.state = next;
            }
            return;
        }
        top.state = State::Done;
        self.events.push(MusicEvent::Finished {
            request: top.request.name.clone(),
            notify: top.request.notify.clone(),
        });
    }

    fn start_top(&mut self, engine: &AudioEngine, fade_ms: u32, library: &AudioLibrary) {
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        let State::Ready { track, clip } = std::mem::replace(&mut top.state, State::Done) else {
            return;
        };
        let def = library.get(&track);
        let control = def.map(|d| d.control).unwrap_or_default();
        let mut play = Play::new(Arc::clone(&clip), library.key(&track).unwrap_or_default());
        play.stream = true;
        play.bus = def
            .map_or(
                mint_sage_ini::audio::Submix::Music,
                AudioEventDef::effective_submix,
            )
            .bus();
        play.gain = def.and_then(|d| d.volume).unwrap_or(1.0).max(0.0);
        play.priority = mint_audio::Priority::Critical;
        let plays = if top.picks_again { Some(1) } else { top.left };
        match plays {
            None => play.looping = true,
            Some(n) => play.repeats = n.saturating_sub(1),
        }
        if top.request.fade_in || control.has(ControlFlags::FADE_ON_START) {
            play.fade_in_ms = fade_ms;
        }
        #[allow(clippy::cast_precision_loss)]
        let length = clip.frames() as f64 / f64::from(clip.sample_rate().max(1));
        let id = engine.play(play);
        self.events.push(MusicEvent::Started {
            request: top.request.name.clone(),
            track: track.clone(),
            seconds: length,
        });
        top.state = State::Voiced(Sounding {
            id,
            track,
            length,
            plays,
            heard: 0.0,
            held: false,
            fade_on_kill: control.has(ControlFlags::FADE_ON_KILL),
        });
    }

    fn begin(&mut self, request: MusicRequest, ctx: &mut Context<'_>) {
        let mut entry = Entry {
            left: request.times,
            request,
            picks_again: false,
            state: State::Done,
        };
        match pick(ctx, &entry.request.name, 0) {
            Some((track, picks_again)) => {
                entry.picks_again = picks_again;
                entry.state = self.prepare(ctx, track);
                if matches!(entry.state, State::Done) {
                    self.events.push(MusicEvent::Failed {
                        request: entry.request.name.clone(),
                        reason: "its file is not in the archives".to_string(),
                    });
                }
            }
            None => self.events.push(MusicEvent::Failed {
                request: entry.request.name.clone(),
                reason: "not a MusicTrack or a Multisound of them".to_string(),
            }),
        }
        self.stack.push(entry);
    }

    fn silence(&mut self, engine: &AudioEngine, entry: Entry, fade: bool) {
        if let State::Voiced(sounding) = entry.state {
            let ms = if fade || sounding.fade_on_kill {
                self.fade_ms
            } else {
                0
            };
            engine.fade_out(sounding.id, ms);
        }
    }

    fn prepare(&mut self, ctx: &mut Context<'_>, track: String) -> State {
        if let Some(clip) = self.cached(&track) {
            return State::Ready { track, clip };
        }
        match self.request_decode(ctx, &track) {
            Some(ticket) => State::Loading { ticket, track },
            None => State::Done,
        }
    }

    fn cached(&self, track: &str) -> Option<Arc<Clip>> {
        self.cache
            .iter()
            .find(|(name, _)| name == track)
            .map(|(_, clip)| Arc::clone(clip))
    }

    fn request_decode(&mut self, ctx: &mut Context<'_>, track: &str) -> Option<u64> {
        let def = ctx.library.get(track)?;
        let path = ctx.library.variant_path(def, "")?;
        let bytes = ctx.vfs.read(&path).ok()?;
        let ticket = self.next_ticket;
        self.next_ticket = self.next_ticket.saturating_add(1);
        let jobs = self.jobs.get_or_insert_with(|| {
            let (job_tx, job_rx) = mpsc::channel::<Job>();
            let (done_tx, done_rx) = mpsc::channel::<Decoded>();
            let handle = std::thread::Builder::new()
                .name("music decode".to_string())
                .spawn(move || {
                    while let Ok((ticket, track, wav, bytes)) = job_rx.recv() {
                        let clip = if wav {
                            Clip::from_wav(&bytes).map_err(|e| e.to_string())
                        } else {
                            mint_audio::decode_compressed(&bytes).map_err(|e| e.to_string())
                        }
                        .map(Arc::new);
                        if done_tx.send((ticket, track, clip)).is_err() {
                            break;
                        }
                    }
                })
                .ok();
            self.results = Some(done_rx);
            self.worker = handle;
            job_tx
        });
        let wav = path.ends_with(".wav");
        jobs.send((ticket, track.to_string(), wav, bytes)).ok()?;
        Some(ticket)
    }

    fn receive(&mut self) {
        let finished: Vec<Decoded> = match &self.results {
            Some(results) => results.try_iter().collect(),
            None => return,
        };
        for done in finished {
            self.accept(done);
        }
    }

    fn accept(&mut self, (ticket, track, clip): Decoded) {
        let waiting =
            |entry: &Entry| matches!(entry.state, State::Loading { ticket: t, .. } if t == ticket);
        let clip = match clip {
            Ok(clip) => clip,
            Err(reason) => {
                for entry in self.stack.iter_mut().filter(|e| waiting(e)) {
                    entry.state = State::Done;
                    self.events.push(MusicEvent::Failed {
                        request: entry.request.name.clone(),
                        reason: format!("{track}: {reason}"),
                    });
                }
                return;
            }
        };
        self.cache.retain(|(name, _)| name != &track);
        self.cache.push((track.clone(), Arc::clone(&clip)));
        if self.cache.len() > CACHED_TRACKS {
            self.cache.remove(0);
        }
        for entry in self.stack.iter_mut().filter(|e| waiting(e)) {
            entry.state = State::Ready {
                track: track.clone(),
                clip: Arc::clone(&clip),
            };
        }
    }

    pub fn settle(&mut self, timeout: std::time::Duration) {
        let deadline = std::time::Instant::now() + timeout;
        while self
            .stack
            .iter()
            .any(|e| matches!(e.state, State::Loading { .. }))
        {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let done = match &self.results {
                Some(results) => results.recv_timeout(left),
                None => return,
            };
            match done {
                Ok(done) => self.accept(done),
                Err(_) => return,
            }
        }
    }
}

impl Drop for MusicSystem {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct Context<'a> {
    pub library: &'a AudioLibrary,
    pub vfs: &'a Vfs,
    pub rng: &'a mut ClientRng,
}

fn pick(ctx: &mut Context<'_>, name: &str, depth: u8) -> Option<(String, bool)> {
    if let Some(def) = ctx.library.get(name) {
        return (def.kind == AudioEventKind::Music).then(|| (def.name.clone(), false));
    }
    if depth >= MAX_PICK_DEPTH {
        return None;
    }
    let multi: MultisoundDef = ctx.library.multisound(name)?.clone();
    let weights: Vec<u32> = multi.subsounds.iter().map(|s| s.weight).collect();
    let index = ctx.rng.next_weighted(&weights)?;
    let sub = multi.subsounds.get(index)?;
    let (track, nested_loop) = pick(ctx, &sub.name, depth.saturating_add(1))?;
    Some((track, nested_loop || (multi.play_one && multi.looping)))
}
