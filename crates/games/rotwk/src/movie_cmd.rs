use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mint_app::{App, InputEvent, Key, WindowConfig};
use mint_movie::{PlayFlags, VideoSurface};
use mint_render::{Camera, Renderer, Scene, UiQuad};
use mint_sage_audio::SageAudio;
use mint_sage_movie::{MovieLibrary, MovieRoots, MovieSession};
use mint_vfs::Vfs;

struct Args {
    installs: Vec<PathBuf>,
    name: String,
    language: String,
    skippable: bool,
    shots: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        installs: Vec::new(),
        name: String::new(),
        language: "english".to_owned(),
        skippable: false,
        shots: None,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |what: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} wants {what}"))
        };
        match arg.as_str() {
            "--install" => parsed.installs.push(PathBuf::from(value("a path")?)),
            "--language" => parsed.language = value("a name")?,
            "--movie-shots" => parsed.shots = Some(PathBuf::from(value("a directory")?)),
            "--skippable" => parsed.skippable = true,
            other if parsed.installs.is_empty() => parsed.installs.push(PathBuf::from(other)),
            other if parsed.name.is_empty() => parsed.name = other.to_owned(),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    if parsed.installs.is_empty() || parsed.name.is_empty() {
        return Err(
            "usage: mint-rotwk --play-movie <install dir> [--install <dir>] <VideoName> \
             [--language <name>] [--skippable] [--movie-shots <dir>]"
                .to_owned(),
        );
    }
    Ok(parsed)
}

pub(crate) fn open_audio(vfs: &Vfs) -> Option<SageAudio> {
    match mint_sage_audio::load_library(vfs) {
        Ok(library) => Some(SageAudio::open(library)),
        Err(error) => {
            println!("movie: audio library unreadable ({error}) - playing silent");
            None
        }
    }
}

fn fitted(
    quads: &mut Vec<UiQuad>,
    texture: mint_render::TextureHandle,
    size: (u32, u32),
    picture: (u32, u32),
) {
    let (fw, fh) = (size.0 as f32, size.1 as f32);
    let (pw, ph) = (picture.0.max(1) as f32, picture.1.max(1) as f32);
    let scale = (fw / pw).min(fh / ph);
    let (w, h) = (pw * scale, ph * scale);
    quads.push(
        UiQuad::new((fw - w) / 2.0, (fh - h) / 2.0, w, h)
            .with_texture(texture, [0.0, 0.0, 1.0, 1.0]),
    );
}

pub(crate) fn present(
    renderer: &mut Renderer,
    surface: &mut VideoSurface,
    session: &MovieSession,
    fresh: bool,
    last: &mut Option<mint_render::TextureHandle>,
    cover: f32,
) -> mint_render::Result<()> {
    if let Some(planes) = session.planes().filter(|_| fresh) {
        match surface.upload(renderer, &planes) {
            Ok(()) => {
                *last = surface.render(renderer).ok().flatten();
            }
            Err(error) => println!("movie: could not upload a frame: {error}"),
        }
    }
    let mut quads = Vec::new();
    if let (Some(texture), Some(picture)) = (*last, surface.size()) {
        fitted(&mut quads, texture, renderer.size(), picture);
    }
    if cover > 0.0 {
        let (w, h) = renderer.size();
        quads.push(
            UiQuad::new(0.0, 0.0, w as f32, h as f32)
                .with_color_transform([0.0, 0.0, 0.0, cover.min(1.0)], [0.0; 4]),
        );
    }
    let mut scene = Scene::new(Camera::default());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    renderer.render(&scene.with_ui_quads(&quads))?;
    Ok(())
}

fn report(session: &MovieSession, skipped: bool) {
    let player = session.player();
    let info = player.info();
    println!(
        "movie: {} frames shown of {} (decoded {}, dropped {}), {}x{} at {}/{}, alpha {}, skipped {skipped}, loops {}",
        player.frames_shown(),
        info.frame_count,
        player.frames_decoded(),
        player.frames_dropped(),
        info.width,
        info.height,
        info.rate_numerator,
        info.rate_denominator,
        info.has_alpha,
        player.loops()
    );
    println!(
        "movie: decode time {} ms",
        session.decode_time().as_millis()
    );
    for (event, outcome) in session.audio_started() {
        println!("movie: audio {event}: {}", outcome.label());
    }
    if let Some(failure) = player.failure() {
        println!("movie: decoding stopped: {failure}");
    }
}

struct MovieApp {
    session: MovieSession,
    surface: VideoSurface,
    audio: Option<SageAudio>,
    vfs: Arc<Vfs>,
    texture: Option<mint_render::TextureHandle>,
    started: bool,
    skipped: bool,
    finished: bool,
}

impl App for MovieApp {
    fn input(&mut self, event: &InputEvent) {
        if let InputEvent::Key {
            key: Key::Escape,
            pressed: false,
        } = event
        {
            if self.session.skip(self.audio.as_mut()) {
                self.skipped = true;
            }
        }
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        if !self.started {
            self.started = true;
            let vfs = Arc::clone(&self.vfs);
            self.session
                .start(self.audio.as_mut().map(|sage| (sage, vfs.as_ref())));
        }
        let tick = self.session.update(self.audio.as_mut());
        present(
            renderer,
            &mut self.surface,
            &self.session,
            tick.rendered,
            &mut self.texture,
            0.0,
        )?;
        if tick.done && !self.finished {
            self.finished = true;
            report(&self.session, self.skipped);
        }
        Ok(())
    }

    fn exit_requested(&self) -> bool {
        self.finished
    }
}

fn shots(
    mut session: MovieSession,
    mut audio: Option<SageAudio>,
    vfs: &Vfs,
    dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut renderer = Renderer::offscreen(1280, 720).map_err(|e| e.to_string())?;
    let mut surface = VideoSurface::new(&mut renderer).map_err(|e| e.to_string())?;
    let total = session.player().info().frame_count;
    let marks = [total / 10, total / 2, total.saturating_mul(9) / 10];
    let mut taken = 0usize;
    let mut texture = None;
    session.start(audio.as_mut().map(|sage| (sage, vfs)));
    let started = Instant::now();
    let mut present_time = Duration::ZERO;
    loop {
        let tick = session.update(audio.as_mut());
        let began = Instant::now();
        present(
            &mut renderer,
            &mut surface,
            &session,
            tick.rendered,
            &mut texture,
            0.0,
        )
        .map_err(|e| e.to_string())?;
        present_time += began.elapsed();
        let position = session.player().frames_decoded();
        if tick.rendered && marks.get(taken).is_some_and(|mark| position > *mark) {
            let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
            let path = dir.join(format!("{}-{}.tga", session.filename(), taken + 1));
            let (w, h) = renderer.size();
            crate::game::write_tga(&path, w, h, &pixels).map_err(|e| e.to_string())?;
            println!(
                "movie: wrote {} at frame {position} ({} ms)",
                path.display(),
                started.elapsed().as_millis()
            );
            taken += 1;
        }
        if tick.done {
            break;
        }
        std::thread::yield_now();
    }
    report(&session, false);
    println!("movie: present time {} ms", present_time.as_millis());
    Ok(())
}

pub(crate) fn play_movie_command(args: &[String]) -> ExitCode {
    let args = match parse(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let Some((primary, extras)) = args.installs.split_first() else {
        return ExitCode::FAILURE;
    };
    let vfs = Arc::new(crate::mount_primary_last(primary, extras));
    let library = MovieLibrary::load(&vfs);
    println!("movie: {} Video block(s) read", library.len());
    for problem in library.errors() {
        println!("movie: {problem:?}");
    }
    let mut priority: Vec<PathBuf> = vec![primary.clone()];
    priority.extend(extras.iter().cloned());
    let roots = MovieRoots::new(&priority, &args.language, None);
    let audio = open_audio(&vfs);
    let flags = PlayFlags::default();
    let opened = MovieSession::open(&args.name, &library, &roots, args.skippable, flags);
    let session = match opened {
        Ok(session) => session,
        Err(error) => {
            println!("movie: not played: {error}");
            return ExitCode::SUCCESS;
        }
    };
    if let Some(dir) = &args.shots {
        return match shots(session, audio, &vfs, dir) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    let config = WindowConfig {
        title: format!("mint - {}", args.name),
        ..WindowConfig::default()
    };
    let result = mint_app::run(&config, move |renderer| {
        let surface = VideoSurface::new(renderer)?;
        Ok(MovieApp {
            session,
            surface,
            audio,
            vfs,
            texture: None,
            started: false,
            skipped: false,
            finished: false,
        })
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
