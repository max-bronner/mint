use std::path::PathBuf;

use mint_movie::{PlayFlags, VideoSurface};
use mint_render::{Renderer, TextureHandle, UiQuad};
use mint_sage_movie::{MovieLibrary, MovieRoots, MovieSession};

pub(crate) const VIDEO: &str = "SmallRing";

pub(crate) struct LoadingRing {
    session: MovieSession,
    surface: VideoSurface,
    texture: Option<TextureHandle>,
    started: bool,
}

impl LoadingRing {
    pub(crate) fn open(
        renderer: &mut Renderer,
        library: &MovieLibrary,
        install_roots: &[PathBuf],
        language: &str,
    ) -> Option<Self> {
        let roots = MovieRoots::new(install_roots, language, None);
        let flags = PlayFlags(PlayFlags::LOOP);
        let session = match MovieSession::open(VIDEO, library, &roots, false, flags) {
            Ok(session) => session,
            Err(error) => {
                println!("loading ring: {VIDEO} not played: {error}");
                return None;
            }
        };
        let surface = VideoSurface::new(renderer)
            .inspect_err(|error| println!("loading ring: no video surface ({error})"))
            .ok()?;
        println!("loading ring: playing {VIDEO}");
        Some(Self {
            session,
            surface,
            texture: None,
            started: false,
        })
    }

    pub(crate) fn quad(&mut self, renderer: &mut Renderer, rect: [f32; 4]) -> Option<UiQuad> {
        if !self.started {
            self.started = true;
            self.session.start(None);
        }
        let tick = self.session.update(None);
        if tick.rendered {
            if let Some(planes) = self.session.planes() {
                match self.surface.upload(renderer, &planes) {
                    Ok(()) => self.texture = self.surface.render(renderer).ok().flatten(),
                    Err(error) => println!("loading ring: could not upload a frame: {error}"),
                }
            }
        }
        let texture = self.texture?;
        Some(
            UiQuad::new(rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1])
                .with_texture(texture, [0.0, 0.0, 1.0, 1.0]),
        )
    }
}
