use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_apt::{
    decode_block, hit_bounds, parse_button_actions, parse_clip_actions, parse_const, parse_frames,
    resolve_import, AptMovie, Block, ButtonAction, Character, ClipAction, ConstItem, DecodeLimits,
    Frame, FrameItem, MovieSet,
};

use crate::avm::Value;
use crate::geom::Matrix;
use crate::Error;

const MAX_BOUNDS_DEPTH: u32 = 8;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ButtonHit {
    pub records: Vec<(Matrix, [f32; 4])>,
    pub mesh: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MovieId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimelineId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CharacterRef {
    pub movie: MovieId,
    pub id: u32,
}

pub const MAX_FRAMES: usize = u16::MAX as usize;

pub const MAX_IMPORT_CHASE: u32 = 8;

#[derive(Debug)]
pub struct Timeline {
    pub movie: MovieId,
    pub frames: Vec<Frame>,
    labels: BTreeMap<String, u16>,
    pub init_actions: Vec<u32>,
}

impl Timeline {
    #[must_use]
    pub fn frame_count(&self) -> u16 {
        u16::try_from(self.frames.len()).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn label(&self, label: &str) -> Option<u16> {
        self.labels.get(&label.to_ascii_lowercase()).copied()
    }

    pub fn labels_at(&self, frame: u16) -> impl Iterator<Item = &str> {
        self.labels
            .iter()
            .filter(move |(_, f)| **f == frame)
            .map(|(l, _)| l.as_str())
    }

    #[must_use]
    pub fn frame(&self, number: u16) -> Option<&Frame> {
        self.frames.get(usize::from(number).checked_sub(1)?)
    }
}

#[derive(Debug)]
pub struct LibraryMovie {
    pub movie: AptMovie,
    pub apt: Vec<u8>,
    pub root: TimelineId,
    sprites: BTreeMap<u32, TimelineId>,
    clip_actions: BTreeMap<u32, Vec<ClipAction>>,
    button_actions: BTreeMap<u32, Vec<ButtonAction>>,
    resolved: Vec<Option<CharacterRef>>,
    pub(crate) consts: Vec<Value>,
    blocks: BTreeMap<u32, Option<Arc<Block>>>,
    hits: BTreeMap<u32, ButtonHit>,
}

#[derive(Debug, Default)]
pub struct LoadReport {
    pub broken_timelines: Vec<(String, u32, Error)>,
    pub broken_clip_actions: Vec<(String, u32, Error)>,
    pub broken_button_actions: Vec<(String, u32, Error)>,
    pub unresolved_imports: Vec<(String, u32)>,
}

#[derive(Debug, Default)]
pub struct Library {
    movies: Vec<LibraryMovie>,
    timelines: Vec<Timeline>,
    by_name: BTreeMap<String, MovieId>,
    report: LoadReport,
    linked: bool,
}

fn normalize(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".apt").unwrap_or(&lower).to_owned()
}

impl Library {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: &str, const_bytes: &[u8], apt: Vec<u8>) -> Result<MovieId, Error> {
        let key = normalize(name);
        if let Some(id) = self.by_name.get(&key) {
            return Ok(*id);
        }
        let parse = |source| Error::Parse {
            movie: key.clone(),
            source,
        };
        let movie = AptMovie::load(&key, const_bytes, &apt).map_err(parse)?;
        let consts = parse_const(const_bytes)
            .map_err(parse)?
            .items
            .into_iter()
            .map(|item| match item {
                ConstItem::Text(text) => Value::str(&text),
                ConstItem::Number(n) => Value::Number(f64::from(n)),
            })
            .collect();
        let id = MovieId(u32::try_from(self.movies.len()).map_err(|_| Error::Limit {
            what: "movies",
            limit: u32::MAX as usize,
        })?);
        let root_frames =
            parse_timeline(&apt, movie.record.frame_count, movie.record.frames_offset).map_err(
                |e| match e {
                    TimelineError::Parse(source) => parse(source),
                    TimelineError::TooLong => Error::Limit {
                        what: "frames per timeline",
                        limit: MAX_FRAMES,
                    },
                },
            )?;
        let root = self.push_timeline(id, root_frames)?;

        let mut sprites = BTreeMap::new();
        let mut button_actions = BTreeMap::new();
        for (index, character) in movie.characters.iter().enumerate() {
            let Ok(char_id) = u32::try_from(index) else {
                break;
            };
            match character {
                Character::Sprite(sprite) => {
                    let frames =
                        match parse_timeline(&apt, sprite.frame_count, sprite.frames_offset) {
                            Ok(frames) => frames,
                            Err(e) => {
                                self.report.broken_timelines.push((
                                    key.clone(),
                                    char_id,
                                    e.into_error(&key),
                                ));
                                Vec::new()
                            }
                        };
                    sprites.insert(char_id, self.push_timeline(id, frames)?);
                }
                Character::Button(button) if button.action_count > 0 => {
                    match parse_button_actions(&apt, button) {
                        Ok(actions) => {
                            button_actions.insert(char_id, actions);
                        }
                        Err(source) => self.report.broken_button_actions.push((
                            key.clone(),
                            char_id,
                            parse(source),
                        )),
                    }
                }
                _ => {}
            }
        }

        let mut clip_actions = BTreeMap::new();
        let first = root.0;
        for timeline in self.timelines.iter().skip(first as usize) {
            for frame in &timeline.frames {
                for item in &frame.items {
                    let FrameItem::PlaceObject(place) = item else {
                        continue;
                    };
                    let Some(offset) = place.clip_actions_offset.filter(|o| *o != 0) else {
                        continue;
                    };
                    if clip_actions.contains_key(&offset) {
                        continue;
                    }
                    match parse_clip_actions(&apt, offset) {
                        Ok(actions) => {
                            clip_actions.insert(offset, actions);
                        }
                        Err(source) => {
                            self.report.broken_clip_actions.push((
                                key.clone(),
                                offset,
                                parse(source),
                            ));
                            clip_actions.insert(offset, Vec::new());
                        }
                    }
                }
            }
        }

        let mut offsets: Vec<u32> = Vec::new();
        for timeline in self.timelines.iter().skip(first as usize) {
            offsets.extend(timeline.init_actions.iter().copied());
            for frame in &timeline.frames {
                offsets.extend(frame.items.iter().filter_map(|item| match item {
                    FrameItem::Action(action) => Some(action.block_offset),
                    _ => None,
                }));
            }
        }
        offsets.extend(clip_actions.values().flatten().map(|a| a.block_offset));
        offsets.extend(button_actions.values().flatten().map(|a| a.block_offset));
        let limits = DecodeLimits::default();
        let blocks = offsets
            .into_iter()
            .map(|offset| {
                (
                    offset,
                    decode_block(&apt, offset, &limits).ok().map(Arc::new),
                )
            })
            .collect();

        self.movies.push(LibraryMovie {
            movie,
            apt,
            root,
            sprites,
            clip_actions,
            button_actions,
            resolved: Vec::new(),
            consts,
            blocks,
            hits: BTreeMap::new(),
        });
        self.by_name.insert(key, id);
        self.linked = false;
        Ok(id)
    }

    fn push_timeline(&mut self, movie: MovieId, frames: Vec<Frame>) -> Result<TimelineId, Error> {
        let id = TimelineId(
            u32::try_from(self.timelines.len()).map_err(|_| Error::Limit {
                what: "timelines",
                limit: u32::MAX as usize,
            })?,
        );
        let mut labels = BTreeMap::new();
        let mut init_actions = Vec::new();
        for (index, frame) in frames.iter().enumerate() {
            let number = u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX);
            for item in &frame.items {
                match item {
                    FrameItem::FrameLabel(label) => {
                        labels
                            .entry(label.label.to_ascii_lowercase())
                            .or_insert(number);
                    }
                    FrameItem::InitAction(init) => init_actions.push(init.block_offset),
                    _ => {}
                }
            }
        }
        self.timelines.push(Timeline {
            movie,
            frames,
            labels,
            init_actions,
        });
        Ok(id)
    }

    pub fn link(&mut self) {
        if self.linked {
            return;
        }
        let mut set = MovieSet::new();
        for m in &self.movies {
            set.insert(m.movie.clone());
        }
        self.report.unresolved_imports.clear();
        let mut all = Vec::with_capacity(self.movies.len());
        for m in &self.movies {
            let mut resolved = Vec::with_capacity(m.movie.characters.len());
            for (index, character) in m.movie.characters.iter().enumerate() {
                let Ok(id) = u32::try_from(index) else {
                    break;
                };
                let r = self.chase(&set, &m.movie, id, character, 0);
                if r.is_none() && matches!(character, Character::Placeholder) {
                    self.report
                        .unresolved_imports
                        .push((m.movie.name.clone(), id));
                }
                resolved.push(r);
            }
            all.push(resolved);
        }
        for (m, resolved) in self.movies.iter_mut().zip(all) {
            m.resolved = resolved;
        }
        let mut hits = Vec::with_capacity(self.movies.len());
        for (index, m) in self.movies.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            let owner = MovieId(index);
            let mut per = BTreeMap::new();
            for (id, character) in m.movie.characters.iter().enumerate() {
                let (Character::Button(button), Ok(id)) = (character, u32::try_from(id)) else {
                    continue;
                };
                let records = button
                    .records
                    .iter()
                    .filter(|r| r.flags & 0x8 != 0)
                    .filter_map(|r| {
                        let target = self.resolve(owner, r.character)?;
                        let bounds = self.character_bounds(target, 0)?;
                        let matrix = Matrix {
                            a: r.transform.m00,
                            b: r.transform.m01,
                            c: r.transform.m10,
                            d: r.transform.m11,
                            tx: r.translate.x,
                            ty: r.translate.y,
                        };
                        Some((matrix, bounds))
                    })
                    .collect();
                let mesh = hit_bounds(&m.apt, button)
                    .ok()
                    .flatten()
                    .map(|r| [r.left, r.top, r.right, r.bottom]);
                per.insert(id, ButtonHit { records, mesh });
            }
            hits.push(per);
        }
        for (m, per) in self.movies.iter_mut().zip(hits) {
            m.hits = per;
        }
        self.linked = true;
    }

    #[must_use]
    pub fn character_bounds(&self, r: CharacterRef, depth: u32) -> Option<[f32; 4]> {
        let rect = |b: &mint_sage_apt::Rect| [b.left, b.top, b.right, b.bottom];
        match self.character(r)? {
            Character::Shape { bounds, .. }
            | Character::EditText { bounds, .. }
            | Character::Text { bounds, .. } => Some(rect(bounds)),
            Character::Button(button) => Some(rect(&button.bounds)),
            Character::Morph { start_shape, .. } => {
                let start = self.resolve(r.movie, *start_shape)?;
                if depth >= MAX_BOUNDS_DEPTH {
                    return None;
                }
                self.character_bounds(start, depth.saturating_add(1))
            }
            Character::Sprite(_) => {
                if depth >= MAX_BOUNDS_DEPTH {
                    return None;
                }
                let timeline = self.timeline(self.sprite_timeline(r)?)?;
                let mut acc: Option<[f32; 4]> = None;
                for item in &timeline.frame(1)?.items {
                    let FrameItem::PlaceObject(po) = item else {
                        continue;
                    };
                    let Some(child) = po.character.and_then(|c| self.resolve(timeline.movie, c))
                    else {
                        continue;
                    };
                    let Some(b) = self.character_bounds(child, depth.saturating_add(1)) else {
                        continue;
                    };
                    let m = po
                        .matrix
                        .as_ref()
                        .map_or(Matrix::IDENTITY, Matrix::from_placement);
                    let b = m.transform_rect(b);
                    acc = Some(acc.map_or(b, |a| {
                        [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[2].max(b[2]),
                            a[3].max(b[3]),
                        ]
                    }));
                }
                acc
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn button_hit(&self, r: CharacterRef) -> Option<&ButtonHit> {
        self.movie(r.movie)?.hits.get(&r.id)
    }

    #[must_use]
    pub fn block(&self, movie: MovieId, offset: u32) -> Option<Option<Arc<Block>>> {
        self.movie(movie)?.blocks.get(&offset).cloned()
    }

    #[must_use]
    pub fn block_counts(&self) -> (usize, usize) {
        let mut decoded = 0usize;
        let mut failed = 0usize;
        for m in &self.movies {
            for b in m.blocks.values() {
                if b.is_some() {
                    decoded = decoded.saturating_add(1);
                } else {
                    failed = failed.saturating_add(1);
                }
            }
        }
        (decoded, failed)
    }

    fn chase(
        &self,
        set: &MovieSet,
        movie: &AptMovie,
        id: u32,
        character: &Character,
        hops: u32,
    ) -> Option<CharacterRef> {
        let owner = *self.by_name.get(&movie.name)?;
        if !matches!(character, Character::Placeholder) {
            return Some(CharacterRef { movie: owner, id });
        }
        if hops >= MAX_IMPORT_CHASE {
            return None;
        }
        let import = movie.imports.iter().find(|i| i.local_character == id)?;
        let target = resolve_import(set, import).ok()?;
        self.chase(
            set,
            target.movie,
            target.character_id,
            target.character,
            hops.saturating_add(1),
        )
    }

    #[must_use]
    pub fn is_linked(&self) -> bool {
        self.linked
    }

    #[must_use]
    pub fn movie_id(&self, name: &str) -> Option<MovieId> {
        self.by_name.get(&normalize(name)).copied()
    }

    #[must_use]
    pub fn movie(&self, id: MovieId) -> Option<&LibraryMovie> {
        self.movies.get(id.0 as usize)
    }

    pub fn movies(&self) -> impl Iterator<Item = (MovieId, &LibraryMovie)> {
        self.movies
            .iter()
            .enumerate()
            .filter_map(|(i, m)| Some((MovieId(u32::try_from(i).ok()?), m)))
    }

    #[must_use]
    pub fn timeline(&self, id: TimelineId) -> Option<&Timeline> {
        self.timelines.get(id.0 as usize)
    }

    #[must_use]
    pub fn resolve(&self, movie: MovieId, id: u32) -> Option<CharacterRef> {
        *self.movie(movie)?.resolved.get(id as usize)?
    }

    #[must_use]
    pub fn character(&self, r: CharacterRef) -> Option<&Character> {
        self.movie(r.movie)?.movie.characters.get(r.id as usize)
    }

    #[must_use]
    pub fn sprite_timeline(&self, r: CharacterRef) -> Option<TimelineId> {
        self.movie(r.movie)?.sprites.get(&r.id).copied()
    }

    #[must_use]
    pub fn clip_actions(&self, movie: MovieId, offset: u32) -> &[ClipAction] {
        self.movie(movie)
            .and_then(|m| m.clip_actions.get(&offset))
            .map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn button_actions(&self, r: CharacterRef) -> &[ButtonAction] {
        self.movie(r.movie)
            .and_then(|m| m.button_actions.get(&r.id))
            .map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn report(&self) -> &LoadReport {
        &self.report
    }
}

enum TimelineError {
    Parse(mint_sage_apt::Error),
    TooLong,
}

impl TimelineError {
    fn into_error(self, movie: &str) -> Error {
        match self {
            Self::Parse(source) => Error::Parse {
                movie: movie.to_owned(),
                source,
            },
            Self::TooLong => Error::Limit {
                what: "frames per timeline",
                limit: MAX_FRAMES,
            },
        }
    }
}

fn parse_timeline(apt: &[u8], count: u32, offset: u32) -> Result<Vec<Frame>, TimelineError> {
    if count as usize > MAX_FRAMES {
        return Err(TimelineError::TooLong);
    }
    parse_frames(apt, count, offset).map_err(TimelineError::Parse)
}
