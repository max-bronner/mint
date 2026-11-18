use std::path::{Path, PathBuf};

use mint_app::CursorRequest;
use mint_cursor::Cursor as DecodedCursor;
use mint_sage_ini::{cursor_file, BlockTable, Document, MouseCursorIndex};
use mint_vfs::Vfs;

const MAX_DIRECTIONS: u32 = 8;

pub(crate) const ENTER_CURSOR: &str = "EnterFriendly";

pub(crate) const DELIVER_RING_CURSOR: &str = "DeliverRing";

const COMMAND_FALLBACK: &str = "Target";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorKind {
    Arrow,
    Select,
    Move,
    AttackObject,
    Scroll,
    SetRallyPoint,
    GenericInvalid,
}

impl CursorKind {
    const ALL: [Self; 7] = [
        Self::Arrow,
        Self::Select,
        Self::Move,
        Self::AttackObject,
        Self::Scroll,
        Self::SetRallyPoint,
        Self::GenericInvalid,
    ];

    const fn block_name(self) -> &'static str {
        match self {
            Self::Arrow => "Arrow",
            Self::Select => "Select",
            Self::Move => "Move",
            Self::AttackObject => "AttackObj",
            Self::Scroll => "Scroll",
            Self::SetRallyPoint => "SetRallyPoint",
            Self::GenericInvalid => "GenericInvalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Want {
    Keep,
    Show(CursorKind),
    Scroll(f32, f32),
    Named(String),
    Command(Option<String>),
    Hidden,
}

impl Want {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Keep => "Keep",
            Self::Show(kind) => kind.block_name(),
            Self::Scroll(..) => CursorKind::Scroll.block_name(),
            Self::Named(name) => name,
            Self::Command(name) => name.as_deref().unwrap_or(COMMAND_FALLBACK),
            Self::Hidden => "None",
        }
    }
}

#[derive(Debug, Clone)]
struct Frame {
    rgba: Vec<u8>,
    width: u16,
    height: u16,
    hotspot: (u16, u16),
    seconds: f32,
}

impl Frame {
    fn request(&self) -> CursorRequest {
        CursorRequest::Image {
            rgba: self.rgba.clone(),
            width: self.width,
            height: self.height,
            hotspot: self.hotspot,
        }
    }
}

#[derive(Debug, Clone)]
struct Image {
    kind: CursorKind,
    direction: u32,
    file: String,
    frames: Vec<Frame>,
}

impl Image {
    fn step_at(&self, elapsed: f32) -> usize {
        let cycle: f32 = self.frames.iter().map(|frame| frame.seconds).sum();
        if self.frames.len() < 2 || cycle <= 0.0 || !elapsed.is_finite() {
            return 0;
        }
        let mut left = elapsed.max(0.0) % cycle;
        for (step, frame) in self.frames.iter().enumerate() {
            if left < frame.seconds {
                return step;
            }
            left -= frame.seconds;
        }
        self.frames.len().saturating_sub(1)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Cursors {
    images: Vec<Image>,
    directions: Vec<(CursorKind, u32)>,
    last_sent: Option<(String, u32)>,
    step: usize,
    elapsed: f32,
    index: Option<MouseCursorIndex>,
    install_roots: Vec<PathBuf>,
    named: Vec<(String, Option<Image>)>,
}

impl Cursors {
    pub(crate) fn load(vfs: &Vfs, install_roots: &[PathBuf]) -> Result<Self, String> {
        let Some(index) = load_index(vfs)? else {
            println!("cursors: data/ini/mouse.ini not found - every cursor is the platform arrow");
            return Ok(Self::default());
        };
        let mut cursors = Self {
            install_roots: install_roots.to_vec(),
            ..Self::default()
        };
        let mut wanted = 0u32;
        for kind in CursorKind::ALL {
            let Some(def) = index.get(kind.block_name()) else {
                println!(
                    "cursors: no MouseCursor {} - platform arrow",
                    kind.block_name()
                );
                continue;
            };
            let directions = def.directions.unwrap_or(1).clamp(1, MAX_DIRECTIONS);
            cursors.directions.push((kind, directions));
            for direction in 0..directions {
                wanted = wanted.saturating_add(1);
                let Some(file) = cursor_file(def, direction) else {
                    println!("cursors: MouseCursor {} has no Texture", kind.block_name());
                    continue;
                };
                let Some(path) = resolve_on_disk(install_roots, &file) else {
                    println!(
                        "cursors: {} wants data/cursors/{file}, not found in any install",
                        kind.block_name()
                    );
                    continue;
                };
                match decode_file(&path) {
                    Ok(frames) => cursors.images.push(Image {
                        kind,
                        direction,
                        file,
                        frames,
                    }),
                    Err(reason) => println!(
                        "cursors: {} ({}): {reason}",
                        kind.block_name(),
                        path.display()
                    ),
                }
            }
        }
        println!(
            "cursors: {} of {wanted} image(s) decoded for Arrow, Select, Move, AttackObj, Scroll, SetRallyPoint, GenericInvalid",
            cursors.images.len()
        );
        cursors.index = Some(index);
        Ok(cursors)
    }

    fn load_named(&mut self, name: &str) {
        let key = name.to_ascii_lowercase();
        if !self.named.iter().any(|(held, _)| *held == key) {
            let image = self.decode_named(name);
            self.named.push((key, image));
        }
    }

    fn image_for(&self, name: &str, direction: u32) -> Option<&Image> {
        self.images
            .iter()
            .find(|image| {
                image.direction == direction && image.kind.block_name().eq_ignore_ascii_case(name)
            })
            .or_else(|| {
                self.named
                    .iter()
                    .find(|(held, _)| held == name && direction == 0)
                    .and_then(|(_, image)| image.as_ref())
            })
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        if dt.is_finite() && dt > 0.0 {
            self.elapsed += dt;
        }
    }

    fn decode_named(&self, name: &str) -> Option<Image> {
        let def = self.index.as_ref()?.get(name)?;
        let file = cursor_file(def, 0)?;
        let path = resolve_on_disk(&self.install_roots, &file)?;
        match decode_file(&path) {
            Ok(frames) => Some(Image {
                kind: CursorKind::Arrow,
                direction: 0,
                file,
                frames,
            }),
            Err(reason) => {
                println!("cursors: {name} ({}): {reason}", path.display());
                None
            }
        }
    }

    pub(crate) fn request(&mut self, want: Want) -> CursorRequest {
        let (name, direction) = match want {
            Want::Keep => return CursorRequest::Unchanged,
            Want::Show(kind) => (kind.block_name().to_owned(), 0),
            Want::Scroll(x, y) => (
                CursorKind::Scroll.block_name().to_owned(),
                direction_frame(x, y, self.directions_of(CursorKind::Scroll)),
            ),
            Want::Named(name) => {
                self.load_named(&name);
                (name, 0)
            }
            Want::Command(name) => {
                let known = name.filter(|name| {
                    self.index
                        .as_ref()
                        .is_some_and(|index| index.get(name).is_some())
                });
                let name = known.unwrap_or_else(|| COMMAND_FALLBACK.to_owned());
                self.load_named(&name);
                (name, 0)
            }
            Want::Hidden => {
                let sent = (String::new(), 0);
                if self.last_sent.as_ref() == Some(&sent) {
                    return CursorRequest::Unchanged;
                }
                self.last_sent = Some(sent);
                println!("cursor: None (hidden)");
                return CursorRequest::Hidden;
            }
        };
        let key = (name.to_ascii_lowercase(), direction);
        if self.last_sent.as_ref() == Some(&key) {
            let Some(image) = self.image_for(&key.0, direction) else {
                return CursorRequest::Unchanged;
            };
            let step = image.step_at(self.elapsed);
            if step == self.step {
                return CursorRequest::Unchanged;
            }
            let request = image
                .frames
                .get(step)
                .map_or(CursorRequest::Unchanged, Frame::request);
            self.step = step;
            return request;
        }
        let request = match self
            .image_for(&key.0, direction)
            .and_then(|image| Some((image, image.frames.first()?)))
        {
            Some((image, frame)) => {
                println!(
                    "cursor: {name}[{direction}] {} {}x{} hotspot {},{}{}",
                    image.file,
                    frame.width,
                    frame.height,
                    frame.hotspot.0,
                    frame.hotspot.1,
                    if image.frames.len() > 1 {
                        format!(", {} animated steps", image.frames.len())
                    } else {
                        String::new()
                    }
                );
                frame.request()
            }
            None => {
                println!("cursor: {name}[{direction}] has no image - platform arrow");
                CursorRequest::Default
            }
        };
        self.last_sent = Some(key);
        self.step = 0;
        self.elapsed = 0.0;
        request
    }

    fn directions_of(&self, kind: CursorKind) -> u32 {
        self.directions
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(1, |(_, d)| *d)
    }
}

fn direction_frame(x: f32, y: f32, directions: u32) -> u32 {
    if directions <= 1 || !(x.is_finite() && y.is_finite()) || (x == 0.0 && y == 0.0) {
        return 0;
    }
    let theta = mint_math::f32::wrap_tau(mint_math::f32::atan2(y, x));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let frame = (theta / (std::f32::consts::TAU / directions as f32) + 0.5) as u32;
    if frame >= directions {
        0
    } else {
        frame
    }
}

fn load_index(vfs: &Vfs) -> Result<Option<MouseCursorIndex>, String> {
    const PATH: &str = "data/ini/mouse.ini";
    if !vfs.contains(PATH) {
        return Ok(None);
    }
    let bytes = vfs.read(PATH).map_err(|error| format!("{PATH}: {error}"))?;
    let text = mint_sage_ini::lex::decode_latin1(&bytes);
    let document = Document::parse(PATH, &text, &BlockTable::corpus_path())
        .map_err(|error| format!("{PATH}: {error}"))?;
    let mut index = MouseCursorIndex::new();
    index.insert_document(&document);
    Ok(Some(index))
}

fn resolve_on_disk(install_roots: &[PathBuf], file: &str) -> Option<PathBuf> {
    install_roots
        .iter()
        .map(|root| root.join("data").join("cursors").join(file))
        .find(|path| path.is_file())
}

fn decode_file(path: &Path) -> Result<Vec<Frame>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let frames = match mint_cursor::decode(&bytes).map_err(|e| e.to_string())? {
        DecodedCursor::Static(image) => vec![frame_of(&image, 0)?],
        DecodedCursor::Animated(anim) => (0..anim.steps())
            .filter_map(|step| anim.step(step))
            .map(|(image, jiffies)| frame_of(image, jiffies))
            .collect::<Result<Vec<_>, _>>()?,
    };
    if frames.is_empty() {
        return Err("animated cursor with no frames".to_owned());
    }
    Ok(frames)
}

fn frame_of(image: &mint_cursor::CursorImage, jiffies: u32) -> Result<Frame, String> {
    let width = u16::try_from(image.width()).map_err(|_| "frame too wide".to_owned())?;
    let height = u16::try_from(image.height()).map_err(|_| "frame too tall".to_owned())?;
    #[allow(clippy::cast_precision_loss)]
    let seconds = jiffies as f32 / 60.0;
    Ok(Frame {
        rgba: image.pixels().to_vec(),
        width,
        height,
        hotspot: image.hotspot(),
        seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_frames_start_right_and_run_clockwise_on_screen() {
        assert_eq!(direction_frame(1.0, 0.0, 8), 0);
        assert_eq!(direction_frame(1.0, 1.0, 8), 1);
        assert_eq!(direction_frame(0.0, 1.0, 8), 2);
        assert_eq!(direction_frame(-1.0, 0.0, 8), 4);
        assert_eq!(direction_frame(0.0, -1.0, 8), 6);
        assert_eq!(direction_frame(1.0, -1.0, 8), 7);
    }

    #[test]
    fn just_below_a_full_turn_rounds_to_frame_zero() {
        assert_eq!(direction_frame(1.0, -0.01, 8), 0);
    }

    #[test]
    fn no_offset_or_one_direction_is_frame_zero() {
        assert_eq!(direction_frame(0.0, 0.0, 8), 0);
        assert_eq!(direction_frame(0.0, 1.0, 1), 0);
        assert_eq!(direction_frame(f32::NAN, 1.0, 8), 0);
    }

    fn frame(shade: u8, seconds: f32) -> Frame {
        Frame {
            rgba: vec![shade; 4],
            width: 1,
            height: 1,
            hotspot: (0, 0),
            seconds,
        }
    }

    fn with_frames(kind: CursorKind, direction: u32, frames: Vec<Frame>) -> Cursors {
        Cursors {
            images: vec![Image {
                kind,
                direction,
                file: "x.cur".to_owned(),
                frames,
            }],
            directions: vec![(CursorKind::Scroll, 8)],
            ..Cursors::default()
        }
    }

    fn with_one_image(kind: CursorKind, direction: u32) -> Cursors {
        with_frames(kind, direction, vec![frame(0, 0.0)])
    }

    fn shade_of(request: &CursorRequest) -> Option<u8> {
        match request {
            CursorRequest::Image { rgba, .. } => rgba.first().copied(),
            _ => None,
        }
    }

    #[test]
    fn an_animated_cursor_steps_on_its_own_rate_and_loops() {
        let mut cursors = with_frames(CursorKind::Move, 0, vec![frame(10, 0.1), frame(20, 0.2)]);
        let shown = Want::Show(CursorKind::Move);
        assert_eq!(shade_of(&cursors.request(shown.clone())), Some(10));
        cursors.advance(0.05);
        assert_eq!(cursors.request(shown.clone()), CursorRequest::Unchanged);
        cursors.advance(0.1);
        assert_eq!(shade_of(&cursors.request(shown.clone())), Some(20));
        cursors.advance(0.1);
        assert_eq!(cursors.request(shown.clone()), CursorRequest::Unchanged);
        cursors.advance(0.1);
        assert_eq!(shade_of(&cursors.request(shown)), Some(10));
    }

    #[test]
    fn a_new_cursor_starts_from_its_first_step() {
        let mut cursors = with_frames(CursorKind::Move, 0, vec![frame(10, 0.1), frame(20, 0.3)]);
        let _ = cursors.request(Want::Show(CursorKind::Move));
        cursors.advance(0.15);
        let _ = cursors.request(Want::Show(CursorKind::Select));
        assert_eq!(
            shade_of(&cursors.request(Want::Show(CursorKind::Move))),
            Some(10)
        );
        cursors.advance(0.05);
        assert_eq!(
            cursors.request(Want::Show(CursorKind::Move)),
            CursorRequest::Unchanged
        );
    }

    #[test]
    fn a_change_is_sent_once_then_unchanged() {
        let mut cursors = with_one_image(CursorKind::Move, 0);
        let first = cursors.request(Want::Show(CursorKind::Move));
        assert!(matches!(first, CursorRequest::Image { .. }));
        assert_eq!(
            cursors.request(Want::Show(CursorKind::Move)),
            CursorRequest::Unchanged
        );
    }

    #[test]
    fn keep_never_sends_anything() {
        let mut cursors = with_one_image(CursorKind::Move, 0);
        assert_eq!(cursors.request(Want::Keep), CursorRequest::Unchanged);
        let _ = cursors.request(Want::Show(CursorKind::Move));
        assert_eq!(cursors.request(Want::Keep), CursorRequest::Unchanged);
    }

    #[test]
    fn a_kind_without_art_asks_for_the_platform_arrow() {
        let mut cursors = with_one_image(CursorKind::Move, 0);
        assert_eq!(
            cursors.request(Want::Show(CursorKind::Select)),
            CursorRequest::Default
        );
    }

    #[test]
    fn scrolling_picks_the_direction_image() {
        let mut cursors = with_one_image(CursorKind::Scroll, 2);
        assert!(matches!(
            cursors.request(Want::Scroll(0.0, 1.0)),
            CursorRequest::Image { .. }
        ));
        assert_eq!(
            cursors.request(Want::Scroll(1.0, 0.0)),
            CursorRequest::Default
        );
    }

    #[test]
    fn the_first_root_that_has_the_file_wins() {
        let base = std::env::temp_dir().join(format!("mint-cursors-{}", std::process::id()));
        let (first, second) = (base.join("a"), base.join("b"));
        for root in [&first, &second] {
            std::fs::create_dir_all(root.join("data").join("cursors")).unwrap();
            std::fs::write(root.join("data").join("cursors").join("x.cur"), b"").unwrap();
        }
        let found = resolve_on_disk(&[first.clone(), second], "x.cur").unwrap();
        assert!(found.starts_with(&first));
        let _ = std::fs::remove_dir_all(&base);
    }
}
