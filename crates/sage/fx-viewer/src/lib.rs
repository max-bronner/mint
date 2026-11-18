pub mod app;
pub mod camera;
pub mod cli;
pub mod install;
pub mod sidebar;
pub mod subject;
pub mod tree;

use std::path::{Path, PathBuf};

use mint_sage_scene::ParticleScene;
use mint_vfs::Vfs;

pub use app::Viewer;
pub use install::{mount_install, MountSummary};
pub use tree::{Category, FxTree, Node, NodeKind, Role, Sources};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("no installation at {path}")]
    NoInstall { path: PathBuf },
    #[error("no installation given; pass --install <path>")]
    NoInstallGiven,
    #[error("mounted {archives} archive(s) but found no FXList or particle system")]
    NoParticleDocuments { archives: usize },
    #[error("text: {0}")]
    Text(#[from] mint_ui::Error),
    #[error("the placeholder glyph texture could not be built")]
    NoGlyphTexture,
    #[error("window: {0}")]
    Window(#[from] mint_app::Error),
    #[error("shot: {0}")]
    Shot(#[from] std::io::Error),
    #[error("render: {0}")]
    Render(#[from] mint_render::Error),
}

pub struct Loaded {
    pub vfs: Vfs,
    pub tree: FxTree,
    pub scene: ParticleScene,
    pub objects: mint_sage_ini::ObjectIndex,
    pub summary: String,
}

pub fn load(installs: &[PathBuf]) -> Result<Loaded> {
    if installs.is_empty() {
        return Err(Error::NoInstallGiven);
    }
    let mut vfs = Vfs::new();
    let mut archives = 0usize;
    for root in installs {
        let summary = mount_install(&mut vfs, Path::new(root))?;
        archives = archives.saturating_add(summary.archives);
        for failure in &summary.failures {
            eprintln!("archive skipped: {failure}");
        }
    }
    let (objects, names, object_report) = mint_sage_model::load_object_index_with_names(
        &vfs,
        &[tree::WHICH_SPECIAL_WEAPON, tree::UNPACK_TIME],
    );
    let (index, lists, report) = mint_sage_particle::load_particle_index(&vfs);
    if index.is_empty() && lists.is_empty() {
        return Err(Error::NoParticleDocuments { archives });
    }
    let summary = format!(
        "loaded {} particle system(s) and {} FXList(s) from {}/{} document(s) in {} archive(s); \
         {} stray End, {} unclosed block; {} object template(s) from {} document(s), {} \
         definition(s) with name-shaped fields",
        index.len(),
        lists.len(),
        report.parsed,
        report.documents,
        archives,
        report.stray_ends,
        report.unclosed_blocks,
        objects.len(),
        object_report.parsed,
        names.len()
    );
    let tree = FxTree::build(Sources {
        index: &index,
        lists: &lists,
        objects: &objects,
        names: &names,
    });
    Ok(Loaded {
        vfs,
        tree,
        scene: ParticleScene::new(index, lists),
        objects,
        summary,
    })
}

pub fn run_headless(loaded: Loaded, options: &cli::Options, name: &str) -> Result<()> {
    println!("{}", loaded.summary);
    let mut renderer = mint_render::Renderer::offscreen(options.width, options.height)?;
    let Loaded {
        vfs,
        tree,
        scene,
        objects,
        ..
    } = loaded;
    let viewport = (options.width, options.height);
    let mut viewer = Viewer::new(&mut renderer, scene, tree, vfs, objects, viewport, None)?;
    viewer.play_headless(&mut renderer, name, options.row.as_deref(), options.frames)?;
    if let Some(path) = options.shot.as_deref() {
        let pixels = renderer.read_pixels()?;
        write_tga(path, options.width, options.height, &pixels)?;
        println!("shot: {}", path.display());
    }
    Ok(())
}

fn write_tga(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(rgba.len().saturating_add(18));
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&u16::try_from(width).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&u16::try_from(height).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&[32, 0x20]);
    for pixel in rgba.chunks_exact(4) {
        if let [r, g, b, a] = pixel {
            out.extend_from_slice(&[*b, *g, *r, *a]);
        }
    }
    std::fs::write(path, out)
}

pub fn run_window(loaded: Loaded, options: &cli::Options) -> Result<()> {
    println!("{}", loaded.summary);
    println!("{}", cli::KEYS);
    let config = mint_app::WindowConfig {
        title: "mint fx-viewer".to_owned(),
        width: options.width,
        height: options.height,
        ..mint_app::WindowConfig::default()
    };
    let viewport = (options.width, options.height);
    let select = options.select.clone();
    let Loaded {
        vfs,
        tree,
        scene,
        objects,
        ..
    } = loaded;
    let mut held = Some((scene, tree, vfs, objects, select));
    mint_app::run(&config, move |renderer| {
        let (scene, tree, vfs, objects, select) = held
            .take()
            .ok_or_else(|| Box::<dyn std::error::Error + Send + Sync>::from("built twice"))?;
        let viewer = Viewer::new(
            renderer,
            scene,
            tree,
            vfs,
            objects,
            viewport,
            select.as_deref(),
        )?;
        Ok(viewer)
    })?;
    Ok(())
}
