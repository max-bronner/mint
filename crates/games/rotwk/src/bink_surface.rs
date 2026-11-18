use std::path::PathBuf;
use std::sync::Arc;

use mint_movie::{PlayFlags, VideoSurface};
use mint_render::{Renderer, TextureHandle};
use mint_sage_apt_player::InstanceId;
use mint_sage_apt_viewer::host::{MovieView, SurfaceContent, SurfaceIcon};
use mint_sage_apt_viewer::scene_surface::{surfaces_of_type, SurfaceKey};
use mint_sage_movie::{MovieLibrary, MovieRoots, MovieSession};
use mint_vfs::Vfs;

const BINK_MOVIE: &str = "BinkMovie";
const HOLD_LAST_FRAME: u32 = 0x80;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    name: String,
    flags: u32,
    use_alpha: bool,
}

impl Request {
    fn of(view: &MovieView, id: InstanceId) -> Option<Self> {
        let name = view.player.own_string(id, "_MovieName")?.to_string();
        let on = |key: &str| view.player.own_value(id, key).is_some_and(|v| v.to_bool());
        let use_alpha = on("_UseAlpha");
        let mut flags = 0;
        if on("_Loop") {
            flags |= PlayFlags::LOOP;
        }
        if use_alpha {
            flags |= PlayFlags::STRETCH;
        }
        if on("_HoldLastFrame") {
            flags |= HOLD_LAST_FRAME;
        }
        Some(Self {
            name,
            flags,
            use_alpha,
        })
    }
}

struct Playing {
    key: SurfaceKey,
    request: Request,
    session: Option<MovieSession>,
    surface: Option<VideoSurface>,
    texture: Option<TextureHandle>,
    frames: u32,
}

pub(crate) struct BinkSurfaces {
    vfs: Arc<Vfs>,
    roots: MovieRoots,
    library: Option<MovieLibrary>,
    playing: Vec<Playing>,
}

impl BinkSurfaces {
    pub(crate) fn new(vfs: Arc<Vfs>, install_roots: &[PathBuf]) -> Self {
        Self {
            vfs,
            roots: MovieRoots::new(install_roots, crate::campaign_flow::LANGUAGE, None),
            library: None,
            playing: Vec::new(),
        }
    }

    pub(crate) fn draw(&mut self, renderer: &mut Renderer, view: &mut MovieView) {
        let movie = view.movie_id();
        let shown = surfaces_of_type(view, |t| t == BINK_MOVIE);
        self.playing
            .retain(|p| p.key.0 != movie || shown.contains(&p.key.1));
        for id in shown {
            let key = (movie, id);
            let request = Request::of(view, id);
            let index = match self.playing.iter().position(|p| p.key == key) {
                Some(index) if self.playing.get(index).map(|p| &p.request) == request.as_ref() => {
                    index
                }
                found => {
                    if let Some(index) = found {
                        self.playing.remove(index);
                    }
                    let Some(request) = request else {
                        view.set_surface(id, Some(SurfaceContent::Hidden));
                        continue;
                    };
                    let playing = self.open(renderer, key, request);
                    self.playing.push(playing);
                    self.playing.len().saturating_sub(1)
                }
            };
            let content = self
                .playing
                .get_mut(index)
                .and_then(|p| p.advance(renderer))
                .map_or(SurfaceContent::Hidden, |texture| {
                    SurfaceContent::Icon(SurfaceIcon {
                        texture,
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0; 4],
                        premultiplied: false,
                        rotated: false,
                        ignores_color_transform: false,
                    })
                });
            view.set_surface(id, Some(content));
        }
    }

    fn open(&mut self, renderer: &mut Renderer, key: SurfaceKey, request: Request) -> Playing {
        let vfs = Arc::clone(&self.vfs);
        let library = self.library.get_or_insert_with(|| MovieLibrary::load(&vfs));
        let flags = PlayFlags(request.flags & (PlayFlags::LOOP | PlayFlags::STRETCH));
        let session = match MovieSession::open(&request.name, library, &self.roots, false, flags) {
            Ok(mut session) => {
                session.start(None);
                println!(
                    "bink surface: playing {} ({})",
                    request.name,
                    session.filename()
                );
                Some(session)
            }
            Err(error) => {
                println!("bink surface: {} not played: {error}", request.name);
                None
            }
        };
        let surface = session.as_ref().and_then(|_| {
            VideoSurface::new(renderer)
                .inspect_err(|error| println!("bink surface: no video surface ({error})"))
                .ok()
        });
        Playing {
            key,
            request,
            session,
            surface,
            texture: None,
            frames: 0,
        }
    }
}

impl Playing {
    fn advance(&mut self, renderer: &mut Renderer) -> Option<TextureHandle> {
        let session = self.session.as_mut()?;
        let surface = self.surface.as_mut()?;
        if session.player().is_done() {
            return (self.request.flags & HOLD_LAST_FRAME != 0)
                .then_some(self.texture)
                .flatten();
        }
        let tick = session.update(None);
        if tick.rendered {
            if let Some(mut planes) = session.planes() {
                if !self.request.use_alpha {
                    planes.alpha = None;
                }
                match surface.upload(renderer, &planes) {
                    Ok(()) => {
                        self.texture = surface.render(renderer).ok().flatten();
                        self.frames = self.frames.saturating_add(1);
                    }
                    Err(error) => println!("bink surface: could not upload a frame: {error}"),
                }
            }
        }
        if tick.done && self.request.flags & HOLD_LAST_FRAME == 0 {
            println!(
                "bink surface: {} ended after {} frame(s)",
                self.request.name, self.frames
            );
            self.texture = None;
        }
        self.texture
    }
}
