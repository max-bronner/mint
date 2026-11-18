use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_app::{App, InputEvent, Key, OrbitCamera, PointerButton, WindowConfig};
use mint_render::{
    Blend, DirectionalLight, Instance, MeshHandle, Renderer, Scene, TextureHandle, Vec3,
};
use mint_sage_asset::{sample_pose, Resolver};
use mint_sage_model::convert::{
    pose_to_matrices, sample_uv_transforms_for, skin_mesh, Part, SkinSource, UvMapperState,
};
use mint_sage_model::{
    find_models, load_animation, load_bump_map, load_model, load_normal_map, load_texture,
    LoadedAnimation, LoadedModel,
};
use mint_sage_viewer::{mount_install, screenshot, Error, Result};
use mint_sage_w3d::{Animation, Hierarchy, Model};
use mint_vfs::Vfs;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Default)]
struct Options {
    installs: Vec<PathBuf>,
    model: Option<String>,
    screenshot: Option<PathBuf>,
    list: Option<String>,
    list_animations: Option<String>,
    animation: Option<String>,
    frame: f32,
    paused: bool,
    width: u32,
    height: u32,
    textures: bool,
    cull_override: Option<bool>,
    pick_proxies: bool,
    show_hidden: bool,
    view: Option<FixedView>,
    ambient: f32,
}

const DEFAULT_LIGHTS: [DirectionalLight; 3] = [
    DirectionalLight {
        direction: Vec3::new(-0.62914, 0.34874, -0.69465),
        color: [1.247, 1.207, 1.043],
    },
    DirectionalLight {
        direction: Vec3::new(0.32877, -0.90329, -0.27563),
        color: [0.745, 0.831, 0.894],
    },
    DirectionalLight {
        direction: Vec3::new(0.80704, 0.58635, -0.06975),
        color: [0.69, 0.667, 0.69],
    },
];

const ALPHA_CUTOFF: f32 = 96.0 / 255.0;

const USAGE: &str = "\
usage: mint-sage-viewer --install <dir> [--install <dir>] <model.w3d> [options]

  --install <dir>          installation to mount; repeatable, later shadow earlier
  --screenshot <file>      render one frame offscreen, write a 32-bit TGA, exit
  --size <W>x<H>           framebuffer size (default 1280x720)
  --list <substring>       list matching model names and exit
  --list-animations [sub]  list every reachable file containing an animation -
                           name, frame count, frame rate, target hierarchy -
                           and exit; an optional substring narrows it
  --animation <name>       pose the model with this animation instead of its
                           bind pose. A bad name, a file with no animation, an
                           unresolved skeleton, or an animation that moves
                           none of this model's parts are all errors
  --frame <n>              start (or, with --screenshot, render) at this
                           frame; fractional values sample between frames
  --paused                 start with playback paused
  --no-textures            draw untextured
  --cull                   force back-face culling on for every mesh
  --no-cull                force back-face culling off for every mesh
                           (default: each mesh's own TWO_SIDED flag decides)
  --pick-proxies           draw PICK meshes, which are mouse-picking proxies
                           the engine never draws
  --show-hidden            draw meshes the file marks HIDDEN by default
  --view <name>            start at one of W3DView's six fixed viewpoints -
                           front|back|left|right|top|bottom - looking at the
                           model's centre, keeping the same auto-fit distance
                           as the default view. Which of the six W3DView
                           itself starts with is not known, so this is not a
                           claim that any one of them is the original's
                           default; omit --view for today's automatic angle
  --ambient <0..1>         scales a white scene ambient colour (default 0.3,
                           DefaultW3D.fx's own AmbientLightColor - see the
                           DEFAULT_LIGHTS doc comment). The source sets this
                           to 1.0 for its 2D and 3D interface scenes, which is
                           what a menu/interface model (sfe_*.w3d) was
                           actually drawn against

With --animation open in a window: Space toggles play/pause, `,`/`.` step one
frame back or forward (and pause).

Do not write a screenshot inside the mint repository: a rendered frame of
retail content is game content.";

fn parse_arguments() -> std::result::Result<Options, String> {
    parse_from(std::env::args().skip(1))
}

fn parse_from(arguments: impl Iterator<Item = String>) -> std::result::Result<Options, String> {
    let mut options = Options {
        width: 1280,
        height: 720,
        textures: true,
        ambient: 0.3,
        ..Options::default()
    };
    let mut arguments = arguments.peekable();

    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "--install" => options.installs.push(PathBuf::from(value("--install")?)),
            "--screenshot" => options.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--list" => options.list = Some(value("--list")?),
            "--list-animations" => {
                let filter = match arguments.peek() {
                    Some(next) if !next.starts_with("--") => arguments.next(),
                    _ => None,
                };
                options.list_animations = Some(filter.unwrap_or_default());
            }
            "--animation" => options.animation = Some(value("--animation")?),
            "--frame" => {
                let text = value("--frame")?;
                options.frame = text
                    .parse()
                    .map_err(|_| format!("--frame is not a number: {text}"))?;
            }
            "--paused" => options.paused = true,
            "--no-textures" => options.textures = false,
            "--cull" => options.cull_override = Some(true),
            "--no-cull" => options.cull_override = Some(false),
            "--pick-proxies" => options.pick_proxies = true,
            "--show-hidden" => options.show_hidden = true,
            "--view" => {
                let text = value("--view")?;
                options.view = Some(FixedView::parse(&text).ok_or_else(|| {
                    format!("--view wants front|back|left|right|top|bottom, got {text}")
                })?);
            }
            "--ambient" => {
                let text = value("--ambient")?;
                options.ambient = text
                    .parse()
                    .map_err(|_| format!("--ambient is not a number: {text}"))?;
            }
            "--size" => {
                let text = value("--size")?;
                let (w, h) = text
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--size wants WxH, got {text}"))?;
                options.width = w
                    .parse()
                    .map_err(|_| format!("--size width is not a number: {w}"))?;
                options.height = h
                    .parse()
                    .map_err(|_| format!("--size height is not a number: {h}"))?;
            }
            "--help" | "-h" => return Err(USAGE.to_owned()),
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => options.model = Some(other.to_owned()),
        }
    }

    if options.installs.is_empty() {
        return Err(format!("at least one --install is required\n\n{USAGE}"));
    }
    if options.model.is_none() && options.list.is_none() && options.list_animations.is_none() {
        return Err(format!("a model name is required\n\n{USAGE}"));
    }
    Ok(options)
}

fn run() -> Result<()> {
    let options = match parse_arguments() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return Ok(());
        }
    };

    let mut vfs = Vfs::new();
    let started = Instant::now();
    for install in &options.installs {
        let summary = mount_install(&mut vfs, install)?;
        println!(
            "mounted {}: {} archives, {} entries, {} loose files",
            install.display(),
            summary.archives,
            summary.entries,
            summary.loose
        );
        for failure in &summary.failures {
            eprintln!("  archive failed: {failure}");
        }
    }
    println!(
        "{} visible paths after shadowing, in {:.2}s\n",
        vfs.len(),
        started.elapsed().as_secs_f64()
    );

    if let Some(needle) = &options.list {
        let found = find_models(&vfs, needle);
        println!("{} models match {needle:?}:", found.len());
        for name in found.iter().take(200) {
            println!("  {name}");
        }
        if found.len() > 200 {
            println!("  ... and {} more", found.len().saturating_sub(200));
        }
        return Ok(());
    }

    if let Some(needle) = &options.list_animations {
        list_animations(&vfs, needle);
        return Ok(());
    }

    let Some(name) = options.model.clone() else {
        return Ok(());
    };
    let resolver = Resolver::new();
    let loaded = load_model(&vfs, &resolver, &name)?;
    report_model(&name, &loaded);

    let playback = match options.animation.as_deref() {
        Some(anim_name) => Some(load_playback(
            &vfs,
            &resolver,
            anim_name,
            &loaded,
            options.frame,
            options.paused,
        )?),
        None => None,
    };

    match &options.screenshot {
        Some(path) => screenshot_one_frame(&vfs, &resolver, &options, &loaded, &playback, path),
        None => open_window(&vfs, &resolver, &options, loaded, playback),
    }
}

fn list_animations(vfs: &Vfs, needle: &str) {
    let needle = needle.to_ascii_lowercase();
    let mut candidates: Vec<_> = vfs
        .paths()
        .filter(|path| path.extension() == Some("w3d"))
        .filter(|path| needle.is_empty() || path.as_str().to_ascii_lowercase().contains(&needle))
        .collect();
    candidates.sort();
    candidates.dedup();

    println!(
        "scanning {} file(s) matching {needle:?} for animations:",
        candidates.len()
    );
    let mut found = 0usize;
    for path in &candidates {
        let Ok(bytes) = vfs.read_path(path) else {
            continue;
        };
        let Ok(model) = Model::parse(&bytes) else {
            continue;
        };
        let Some(animation) = model.animations.first() else {
            continue;
        };
        found = found.saturating_add(1);
        println!(
            "  {path}  {:?}  {} frame(s) @ {} fps  hierarchy {:?}",
            animation.name, animation.num_frames, animation.frame_rate, animation.hierarchy_name
        );
    }
    println!(
        "{found} of {} candidate file(s) contain an animation",
        candidates.len()
    );
}

fn load_playback(
    vfs: &Vfs,
    resolver: &Resolver,
    anim_name: &str,
    loaded: &LoadedModel,
    start_frame: f32,
    start_paused: bool,
) -> Result<AnimationPlayback> {
    let found: LoadedAnimation = load_animation(vfs, resolver, anim_name)?;
    println!(
        "animation {anim_name} -> {} ({:?}, {} frame(s) @ {} fps, hierarchy {:?})",
        found.path,
        found.animation.name,
        found.animation.num_frames,
        found.animation.frame_rate,
        found.animation.hierarchy_name
    );

    let Some(hierarchy) = &loaded.hierarchy else {
        return Err(Error::NoModelSkeleton {
            model: loaded.path.to_string(),
            animation: anim_name.to_owned(),
        });
    };
    if !hierarchy
        .header
        .name
        .eq_ignore_ascii_case(&found.animation.hierarchy_name)
    {
        eprintln!(
            "  warning: the model's hierarchy is {:?}, the animation names {:?} - posing may be \
             wrong if these are not really the same skeleton",
            hierarchy.header.name, found.animation.hierarchy_name
        );
    }

    let probe = sample_pose(hierarchy, &found.animation, start_frame);
    if probe.out_of_range_channels > 0 {
        println!(
            "  {} channel(s) name a pivot outside this {}-pivot hierarchy and are dropped",
            probe.out_of_range_channels,
            hierarchy.len()
        );
    }
    if probe.root_channels_ignored > 0 {
        println!(
            "  {} channel(s) name pivot 0 (the object root); EA's evaluator never lets an \
             animation drive the root, so these specific channels are not applied - pivot 0 \
             still poses at its own (bind-pose) transform, and anything rigidly bound to it \
             still moves with the rest of the hierarchy",
            probe.root_channels_ignored
        );
    }

    Ok(AnimationPlayback {
        requested: anim_name.to_owned(),
        animation: found.animation,
        hierarchy: hierarchy.clone(),
        frame: start_frame,
        playing: !start_paused,
        last_tick: Instant::now(),
    })
}

fn report_model(name: &str, loaded: &LoadedModel) {
    println!("{name} -> {}", loaded.path);
    if let Some(skeleton) = &loaded.skeleton {
        println!("  skeleton: {skeleton}");
    }
    let bounds = loaded.bounds();
    println!(
        "  {} part(s), {} vertices, {} triangles",
        loaded.model.parts.len(),
        loaded.model.vertex_count(),
        loaded.model.triangle_count()
    );
    if !bounds.is_empty() {
        println!(
            "  bounds {:?} .. {:?}, radius {:.2}",
            bounds.min().to_array(),
            bounds.max().to_array(),
            bounds.radius()
        );
    }
    for note in &loaded.model.notes {
        println!("  note: {note}");
    }
    if let Some(anim_name) = &loaded.embedded_animation_name {
        println!(
            "  carries its own animation {anim_name:?} (pass --animation {anim_name} to play it)"
        );
    }
}

struct AnimatedPart {
    mesh: MeshHandle,
    source: SkinSource,
}

struct RigidPart {
    instance: usize,
    pivot: usize,
    base: (Blend, [f32; 4]),
}

struct Uploaded {
    instances: Vec<Instance>,
    animated: Vec<AnimatedPart>,
    rigid: Vec<RigidPart>,
    uv_animated: Vec<UvAnimated>,
}

#[must_use]
fn material_alpha(part: &Part) -> f32 {
    if part.blend == Blend::AlphaBlend && !part.diffuse_from_vertex_color {
        part.material_opacity
    } else {
        1.0
    }
}

fn upload(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    loaded: &LoadedModel,
    want_textures: bool,
    want_pick_proxies: bool,
    want_hidden: bool,
) -> Result<Uploaded> {
    let compressed_ok = renderer.supports_block_compression();
    let mut instances = Vec::with_capacity(loaded.model.parts.len());
    let mut animated = Vec::new();
    let mut rigid = Vec::new();
    let mut uv_animated = Vec::new();
    let mut compressed_bytes = 0usize;
    let mut decoded_bytes = 0usize;
    let mut skipped_pick = 0usize;
    let mut skipped_hidden = 0usize;
    let mut normal_mapped = 0usize;

    for part in &loaded.model.parts {
        if part.pick_proxy && !want_pick_proxies {
            skipped_pick = skipped_pick.saturating_add(1);
            println!(
                "  {} is a picking proxy, not drawn (--pick-proxies to see it)",
                part.name
            );
            continue;
        }
        if part.hidden && !want_hidden {
            skipped_hidden = skipped_hidden.saturating_add(1);
            println!(
                "  {} is HIDDEN by default, not drawn (--show-hidden to see it)",
                part.name
            );
            continue;
        }
        let mesh: MeshHandle = match renderer.upload_mesh(&part.mesh) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("  part {} could not be uploaded: {error}", part.name);
                continue;
            }
        };
        if let Some(source) = &part.skin {
            animated.push(AnimatedPart {
                mesh,
                source: source.clone(),
            });
        }

        let mut texture: Option<TextureHandle> = None;
        if want_textures {
            if let Some(wanted) = &part.texture {
                let loaded = if part.bump_offset {
                    load_bump_map(vfs, resolver, wanted)
                } else {
                    load_texture(vfs, resolver, wanted, compressed_ok)
                };
                match loaded {
                    Ok(mut loaded_texture) => {
                        loaded_texture.data = loaded_texture
                            .data
                            .with_addressing(part.address_u, part.address_v);
                        let size = format!(
                            "{}x{}",
                            loaded_texture.data.width(),
                            loaded_texture.data.height()
                        );
                        match renderer.upload_texture(&loaded_texture.data) {
                            Ok(handle) => {
                                texture = Some(handle);
                                if loaded_texture.compressed_upload {
                                    compressed_bytes = compressed_bytes
                                        .saturating_add(loaded_texture.source_bytes);
                                } else {
                                    decoded_bytes = decoded_bytes.saturating_add(
                                        loaded_texture
                                            .data
                                            .levels
                                            .iter()
                                            .map(|level| level.data.len())
                                            .sum(),
                                    );
                                }
                                println!(
                                    "  {} <- {} ({size}, {})",
                                    part.name,
                                    loaded_texture.path,
                                    if loaded_texture.compressed_upload {
                                        "blocks uploaded as-is"
                                    } else {
                                        "decoded to RGBA"
                                    }
                                );
                            }
                            Err(error) => {
                                eprintln!("  {}: texture upload failed: {error}", part.name);
                            }
                        }
                    }
                    Err(error) => eprintln!("  {}: {error}", part.name),
                }
            }
        }

        let mut normal_map: Option<TextureHandle> = None;
        if want_textures {
            if let Some(wanted) = &part.normal_map {
                match load_normal_map(vfs, resolver, wanted, compressed_ok) {
                    Ok(map) => match renderer.upload_texture(&map.data) {
                        Ok(handle) => {
                            normal_map = Some(handle);
                            normal_mapped = normal_mapped.saturating_add(1);
                            println!(
                                "  {} <- {} (normal map, {}x{})",
                                part.name,
                                map.path,
                                map.data.width(),
                                map.data.height()
                            );
                        }
                        Err(error) => {
                            eprintln!("  {}: normal map upload failed: {error}", part.name);
                        }
                    },
                    Err(error) => eprintln!("  {}: normal map: {error}", part.name),
                }
            }
        }

        let mut texture2: Option<TextureHandle> = None;
        if want_textures {
            if let Some(wanted) = &part.texture2 {
                match load_texture(vfs, resolver, wanted, compressed_ok) {
                    Ok(mut loaded_texture) => {
                        loaded_texture.data = loaded_texture
                            .data
                            .with_addressing(part.address2_u, part.address2_v);
                        match renderer.upload_texture(&loaded_texture.data) {
                            Ok(handle) => {
                                texture2 = Some(handle);
                                println!(
                                    "  {} <- {} (stage 2, {}x{}, {:?}/{:?})",
                                    part.name,
                                    loaded_texture.path,
                                    loaded_texture.data.width(),
                                    loaded_texture.data.height(),
                                    part.detail_combine,
                                    part.detail_alpha_combine
                                );
                            }
                            Err(error) => {
                                eprintln!(
                                    "  {}: stage 2 texture upload failed: {error}",
                                    part.name
                                );
                            }
                        }
                    }
                    Err(error) => eprintln!("  {}: stage 2 texture: {error}", part.name),
                }
            }
        }

        instances.push(Instance {
            mesh,
            texture,
            normal_map,
            transform: part.transform,
            tint: [1.0, 1.0, 1.0, material_alpha(part)],
            alpha_cutoff: if part.alpha_test { ALPHA_CUTOFF } else { 0.0 },
            blend: part.blend,
            cull: part.cull,
            texture2,
            detail_combine: part.detail_combine,
            detail_alpha_combine: part.detail_alpha_combine,
            material_ambient: part.material_ambient,
            material_diffuse: part.material_diffuse,
            material_emissive: part.material_emissive,
            env_map0: part.env_map0,
            env_map1: part.env_map1,
            bump_offset: part.bump_offset.then_some([[1.0, 0.0], [0.0, 1.0]]),
            sort_level: part.sort_level,
            unlit: part.unlit,
            ambient_from_vertex_color: part.ambient_from_vertex_color,
            diffuse_from_vertex_color: part.diffuse_from_vertex_color,
            dcg_lighting_disabled: part.dcg_lighting_disabled,
            dcg_multiplies_result: part.dcg_multiplies_result,
            modulate2x: part.modulate2x,
            ..Instance::new(mesh)
        });
        if let Some(pivot) = part.pivot {
            let base = instances
                .last()
                .map_or((Blend::Opaque, [1.0; 4]), |i| (i.blend, i.tint));
            rigid.push(RigidPart {
                instance: instances.len().saturating_sub(1),
                pivot,
                base,
            });
        }
        if let Some(state) = &part.uv_mapper {
            uv_animated.push(UvAnimated {
                instance: instances.len().saturating_sub(1),
                state: state.clone(),
            });
        }
    }

    let culled = instances.iter().filter(|instance| instance.cull).count();
    println!(
        "  uploaded {} instance(s) ({culled} back-face culled, {} two-sided), {} animated \
         ({} skinned, {} rigidly bound to a bone), {normal_mapped} with a normal map, \
         {skipped_pick} picking prox(ies) skipped, {skipped_hidden} HIDDEN mesh(es) skipped, \
         {} with an animated UV mapper; {compressed_bytes} bytes of compressed blocks passed \
         through, {decoded_bytes} bytes decoded to RGBA",
        instances.len(),
        instances.len().saturating_sub(culled),
        animated.len().saturating_add(rigid.len()),
        animated.len(),
        rigid.len(),
        uv_animated.len()
    );
    Ok(Uploaded {
        instances,
        animated,
        rigid,
        uv_animated,
    })
}

struct UvAnimated {
    instance: usize,
    state: UvMapperState,
}

fn apply_uv_mappers(instances: &mut [Instance], uv_animated: &[UvAnimated], seconds: f32) {
    for entry in uv_animated {
        let ((uv_linear, uv_offset), (uv2_linear, uv2_offset), bump) =
            sample_uv_transforms_for(&entry.state, seconds);
        if let Some(instance) = instances.get_mut(entry.instance) {
            instance.uv_linear = uv_linear;
            instance.uv_offset = uv_offset;
            instance.uv2_linear = uv2_linear;
            instance.uv2_offset = uv2_offset;
            if instance.bump_offset.is_some() {
                instance.bump_offset = Some(bump);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixedView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl FixedView {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    fn apply(self, camera: &mut OrbitCamera) {
        use std::f32::consts::{FRAC_PI_2, PI};
        let (up, pitch, yaw) = match self {
            Self::Front => (Vec3::Z, 0.0, 0.0),
            Self::Back => (Vec3::Z, 0.0, PI),
            Self::Left => (Vec3::Z, 0.0, -FRAC_PI_2),
            Self::Right => (Vec3::Z, 0.0, FRAC_PI_2),
            Self::Top => (Vec3::Y, 0.0, FRAC_PI_2),
            Self::Bottom => (Vec3::Y, 0.0, -FRAC_PI_2),
        };
        camera.up = up;
        camera.pitch = pitch;
        camera.yaw = yaw;
    }
}

fn framed_camera(
    loaded: &LoadedModel,
    width: u32,
    height: u32,
    view: Option<FixedView>,
) -> OrbitCamera {
    let mut camera = OrbitCamera::new(Vec3::Z);
    let aspect = width as f32 / (height.max(1) as f32);
    camera.frame(&loaded.bounds(), aspect);
    if let Some(view) = view {
        view.apply(&mut camera);
    }
    camera
}

struct AnimationPlayback {
    requested: String,
    animation: Animation,
    hierarchy: Hierarchy,
    frame: f32,
    playing: bool,
    last_tick: Instant,
}

impl AnimationPlayback {
    fn frame_count(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let count = self.animation.num_frames.max(1) as f32;
        count
    }

    fn tick(&mut self) -> mint_sage_asset::SampledPose {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        if self.playing {
            #[allow(clippy::cast_precision_loss)]
            let rate = self.animation.frame_rate as f32;
            self.frame = (self.frame + elapsed * rate).rem_euclid(self.frame_count());
        }
        sample_pose(&self.hierarchy, &self.animation, self.frame)
    }

    fn step(&mut self, delta: f32) {
        self.playing = false;
        self.frame = (self.frame + delta).rem_euclid(self.frame_count());
    }
}

fn apply_pose(
    renderer: &mut Renderer,
    instances: &mut [Instance],
    animated: &[AnimatedPart],
    rigid: &[RigidPart],
    pose: &mint_sage_asset::SampledPose,
) {
    let bones = pose_to_matrices(&pose.world);
    for part in animated {
        let reposed = skin_mesh(&part.source, &bones);
        if let Err(error) = renderer.update_mesh_vertices(part.mesh, &reposed) {
            eprintln!("  animation: could not update a part's pose: {error}");
        }
    }
    for part in rigid {
        let Some(matrix) = bones.get(part.pivot) else {
            continue;
        };
        let opacity = pose.opacity.get(part.pivot).copied().unwrap_or(1.0);
        let visible = pose.visible.get(part.pivot).copied().unwrap_or(true);
        if let Some(instance) = instances.get_mut(part.instance) {
            instance.transform = *matrix;
            (instance.blend, instance.tint) =
                mint_sage_model::faded(part.base.0, part.base.1, opacity);
            instance.visible = visible;
        }
    }
}

fn require_animation_moves_something(
    playback: &AnimationPlayback,
    uploaded: &Uploaded,
    loaded: &LoadedModel,
) -> Result<()> {
    if !uploaded.animated.is_empty() || !uploaded.rigid.is_empty() {
        return Ok(());
    }
    Err(Error::AnimationHasNoEffect {
        animation: playback.requested.clone(),
        hierarchy: playback.hierarchy.header.name.clone(),
        model: loaded.path.to_string(),
        parts: uploaded.instances.len(),
    })
}

fn screenshot_one_frame(
    vfs: &Vfs,
    resolver: &Resolver,
    options: &Options,
    loaded: &LoadedModel,
    playback: &Option<AnimationPlayback>,
    path: &Path,
) -> Result<()> {
    let mut renderer = Renderer::offscreen(options.width, options.height)?;
    println!(
        "  {} via {} ({}), block compression {}",
        renderer.info().adapter,
        renderer.info().backend,
        renderer.info().driver,
        if renderer.supports_block_compression() {
            "available"
        } else {
            "UNAVAILABLE, textures will be decoded on the CPU"
        }
    );
    renderer.set_cull_override(options.cull_override);

    let mut uploaded = upload(
        &mut renderer,
        vfs,
        resolver,
        loaded,
        options.textures,
        options.pick_proxies,
        options.show_hidden,
    )?;
    if let Some(playback) = playback {
        require_animation_moves_something(playback, &uploaded, loaded)?;
        let pose = sample_pose(&playback.hierarchy, &playback.animation, playback.frame);
        apply_pose(
            &mut renderer,
            &mut uploaded.instances,
            &uploaded.animated,
            &uploaded.rigid,
            &pose,
        );
    }
    apply_uv_mappers(&mut uploaded.instances, &uploaded.uv_animated, 0.0);
    let (width, height) = renderer.size();
    let camera = framed_camera(loaded, width, height, options.view);
    let mut scene = Scene::new(camera.camera()).with_instances(&uploaded.instances);
    scene.lights = DEFAULT_LIGHTS;
    scene.ambient_color = [options.ambient, options.ambient, options.ambient];
    renderer.render(&scene)?;

    let rgba = renderer.read_pixels()?;
    screenshot::write_tga(path, width, height, &rgba)?;
    println!("wrote {width}x{height} screenshot to {}", path.display());
    Ok(())
}

fn open_window(
    vfs: &Vfs,
    resolver: &Resolver,
    options: &Options,
    loaded: LoadedModel,
    playback: Option<AnimationPlayback>,
) -> Result<()> {
    let config = WindowConfig {
        title: format!("mint - {}", loaded.path),
        width: options.width,
        height: options.height,
        ..WindowConfig::default()
    };

    let want_textures = options.textures;
    let cull_override = options.cull_override;
    let want_pick_proxies = options.pick_proxies;
    let want_hidden = options.show_hidden;
    let ambient = options.ambient;
    let view = options.view;

    mint_app::run(&config, move |renderer| {
        println!(
            "  {} via {} ({}), block compression {}",
            renderer.info().adapter,
            renderer.info().backend,
            renderer.info().driver,
            if renderer.supports_block_compression() {
                "available"
            } else {
                "UNAVAILABLE, textures will be decoded on the CPU"
            }
        );
        renderer.set_cull_override(cull_override);
        let uploaded = upload(
            renderer,
            vfs,
            resolver,
            &loaded,
            want_textures,
            want_pick_proxies,
            want_hidden,
        )?;
        if let Some(playback) = &playback {
            require_animation_moves_something(playback, &uploaded, &loaded)?;
        }
        println!("\ncontrols:");
        println!("  drag        rotate (vertical drag is inverted, by preference)");
        println!("  scroll      zoom");
        if playback.is_some() {
            println!("  space       play/pause");
            println!("  , / .       step one frame back/forward (pauses)");
        }
        let (width, height) = renderer.size();
        Ok(Viewer {
            camera: framed_camera(&loaded, width, height, view),
            instances: uploaded.instances,
            animated: uploaded.animated,
            rigid: uploaded.rigid,
            uv_animated: uploaded.uv_animated,
            started: Instant::now(),
            playback,
            dragging: false,
            pointer: None,
            ambient,
        })
    })?;
    Ok(())
}

struct Viewer {
    camera: OrbitCamera,
    instances: Vec<Instance>,
    animated: Vec<AnimatedPart>,
    rigid: Vec<RigidPart>,
    uv_animated: Vec<UvAnimated>,
    started: Instant,
    playback: Option<AnimationPlayback>,
    dragging: bool,
    pointer: Option<(f32, f32)>,
    ambient: f32,
}

impl App for Viewer {
    fn input(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::PointerButton { button, pressed } => {
                if button == PointerButton::Primary {
                    self.dragging = pressed;
                    if !pressed {
                        self.pointer = None;
                    }
                }
            }
            InputEvent::PointerMoved { x, y } => {
                if let (true, Some((last_x, last_y))) = (self.dragging, self.pointer) {
                    self.camera.orbit(x - last_x, y - last_y);
                }
                self.pointer = Some((x, y));
            }
            InputEvent::Scroll { lines } => self.camera.zoom(lines),
            InputEvent::Key { key, pressed: true } => {
                if let Some(playback) = &mut self.playback {
                    match key {
                        Key::Space => playback.playing = !playback.playing,
                        Key::Character(',') => playback.step(-1.0),
                        Key::Character('.') => playback.step(1.0),
                        _ => {}
                    }
                }
            }
            InputEvent::Resized { .. } => {}
            _ => {}
        }
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        if let Some(playback) = &mut self.playback {
            let pose = playback.tick();
            apply_pose(
                renderer,
                &mut self.instances,
                &self.animated,
                &self.rigid,
                &pose,
            );
        }
        apply_uv_mappers(
            &mut self.instances,
            &self.uv_animated,
            self.started.elapsed().as_secs_f32(),
        );
        let mut scene = Scene::new(self.camera.camera()).with_instances(&self.instances);
        scene.lights = DEFAULT_LIGHTS;
        scene.ambient_color = [self.ambient, self.ambient, self.ambient];
        renderer.render(&scene)
    }
}

const _: fn() = || {
    fn assert_boxable<T: std::error::Error + Send + Sync + 'static>() {}
    assert_boxable::<Error>();
};

#[cfg(test)]
mod tests;
