use std::collections::BTreeMap;

use mint_render::{Blend, Instance, MeshData, MeshHandle, Renderer, TextureHandle, Vertex};
use mint_sage_asset::Resolver;
use mint_sage_terrain::{WaveField, WAVE_LIFT, WAVE_VELOCITY_UNIT};
use mint_vfs::Vfs;

const TRACK_LIFT: f32 = 1.5;

#[derive(Debug, Clone, Copy)]
struct Track {
    start: [f32; 2],
    dir: [f32; 2],
    perp: [f32; 2],
    z: f32,
    final_width: f32,
    initial_width: f32,
    initial_height: f32,
    velocity: f32,
    time_to_reach: f32,
    front_acc: f32,
    time_to_stop: f32,
    back_acc: f32,
    compress: f32,
    fade: f32,
    total: f32,
    offset: f32,
    flip_u: bool,
}

impl Track {
    #[allow(clippy::too_many_arguments)]
    fn new(
        centre: [f32; 2],
        dir: [f32; 2],
        z: f32,
        params: &mint_sage_terrain::WaveParams,
        offset: f32,
        flip_u: bool,
    ) -> Option<Self> {
        let distance = params.distance_from_shore;
        let velocity = params.initial_velocity * WAVE_VELOCITY_UNIT / 1000.0;
        let final_width = params.final_width;
        let final_height = params.final_height;
        if !(velocity > 0.0 && final_height > 0.0 && distance > 0.0) {
            return None;
        }
        let initial_width = final_width * params.initial_width_fraction / 100.0;
        let initial_height = initial_width * params.initial_height_fraction / 100.0;
        let time_to_reach = ((distance - final_height) / velocity).max(0.0);
        let front_acc = -(velocity * velocity) / (2.0 * final_height);
        let time_to_stop = -velocity / front_acc;
        let time_to_retreat = (2.0 * final_height / front_acc).abs().sqrt();
        let total = time_to_reach + time_to_stop + time_to_retreat;
        let back_acc = 2.0 * initial_height / (time_to_stop * time_to_stop);
        Some(Self {
            start: [centre[0] - dir[0] * distance, centre[1] - dir[1] * distance],
            dir,
            perp: [dir[1], -dir[0]],
            z,
            final_width,
            initial_width,
            initial_height,
            velocity,
            time_to_reach,
            front_acc,
            time_to_stop,
            back_acc,
            compress: params.time_to_compress,
            fade: params.time_to_fade.max(1.0),
            total,
            offset,
            flip_u,
        })
    }

    fn quad(&self, ms: f32) -> ([[f32; 2]; 4], f32) {
        let elapsed = (self.offset + ms).rem_euclid(self.total.max(1.0));
        let along = |p: [f32; 2], d: f32| [p[0] + self.dir[0] * d, p[1] + self.dir[1] * d];
        let side = |p: [f32; 2], d: f32| [p[0] + self.perp[0] * d, p[1] + self.perp[1] * d];
        let (front, tail, width, alpha);
        if elapsed < self.time_to_reach {
            let t = elapsed / self.time_to_reach.max(1.0);
            let w = (self.initial_width + t * (self.final_width - self.initial_width))
                / self.final_width;
            width = self.final_width * w;
            let f = along(self.start, self.velocity * elapsed);
            front = side(f, -0.5 * width);
            tail = along(front, -self.initial_height);
            alpha = t;
        } else {
            width = self.final_width;
            let beach = along(self.start, self.velocity * self.time_to_reach);
            let e = elapsed - self.time_to_reach;
            front = side(
                along(beach, self.velocity * e + 0.5 * self.front_acc * e * e),
                -0.5 * width,
            );
            let back_off = (elapsed - self.time_to_reach - self.time_to_stop).max(0.0);
            alpha = 1.0 - (back_off / self.fade).min(1.0);
            let mut t = along(beach, -self.initial_height);
            if elapsed > self.time_to_reach + self.time_to_stop + self.compress {
                let held = self.time_to_stop + self.compress;
                t = along(t, 0.5 * self.back_acc * held * held);
                let since = elapsed - (self.time_to_reach + self.time_to_stop + self.compress);
                t = along(t, 0.5 * self.front_acc * since * since);
            } else {
                t = along(t, 0.5 * self.back_acc * e * e);
            }
            tail = side(t, -0.5 * width);
        }
        ([tail, side(tail, width), front, side(front, width)], alpha)
    }
}

#[derive(Default)]
pub struct WaveTracks {
    groups: Vec<(MeshHandle, Option<TextureHandle>, Vec<Track>)>,
    pub count: usize,
    pub pca_skipped: usize,
}

impl WaveTracks {
    pub fn load(
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        field: &WaveField,
    ) -> Self {
        let mut by_texture: BTreeMap<String, Vec<Track>> = BTreeMap::new();
        let mut flip = false;
        let mut pca_skipped = 0usize;
        for area in &field.areas {
            let p = &area.params;
            if p.pca {
                pca_skipped += 1;
                continue;
            }
            let z = area.height - WAVE_LIFT + TRACK_LIFT;
            for line in &area.lines {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = ((line.length / p.final_width.max(1.0)).floor() as usize).clamp(1, 512);
                for k in 0..n {
                    let s = (k as f32 + 0.5) * line.length / n as f32;
                    let centre = [
                        line.start[0] + line.along[0] * s,
                        line.start[1] + line.along[1] * s,
                    ];
                    flip = !flip;
                    let tracks = by_texture
                        .entry(area.texture.to_ascii_lowercase())
                        .or_default();
                    if let Some(t) = Track::new(centre, line.shore, z, p, 0.0, flip) {
                        tracks.push(t);
                    }
                    if p.time_offset_second_wave > 0.0 {
                        if let Some(t) =
                            Track::new(centre, line.shore, z, p, p.time_offset_second_wave, !flip)
                        {
                            tracks.push(t);
                        }
                    }
                }
            }
        }
        let compressed = renderer.supports_block_compression();
        let mut out = Self {
            pca_skipped,
            ..Self::default()
        };
        for (name, tracks) in by_texture {
            let texture = mint_sage_model::load_texture(vfs, resolver, &name, compressed)
                .ok()
                .and_then(|loaded| renderer.upload_texture(&loaded.data).ok());
            let mesh = build(&tracks, 0.0);
            if let Ok(handle) = renderer.upload_mesh(&mesh) {
                out.count += tracks.len();
                out.groups.push((handle, texture, tracks));
            }
        }
        out
    }

    #[must_use]
    pub fn instances(&self) -> Vec<Instance> {
        self.groups
            .iter()
            .map(|(mesh, texture, _)| Instance {
                texture: *texture,
                blend: Blend::AlphaBlend,
                cull: false,
                unlit: true,
                vertex_alpha: true,
                visible: false,
                ..Instance::new(*mesh)
            })
            .collect()
    }

    pub fn push(&self, renderer: &mut Renderer, seconds: f32) {
        for (mesh, _, tracks) in &self.groups {
            if let Err(error) =
                renderer.update_mesh_vertices(*mesh, &build(tracks, seconds * 1000.0))
            {
                crate::once::report_once(&format!("  wave tracks: {error}"));
            }
        }
    }
}

fn build(tracks: &[Track], ms: f32) -> MeshData {
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(tracks.len() * 4),
        indices: Vec::with_capacity(tracks.len() * 6),
    };
    for track in tracks {
        let (corners, alpha) = track.quad(ms);
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        let (u0, u1) = if track.flip_u { (1.0, 0.0) } else { (0.0, 1.0) };
        for (corner, uv) in corners
            .iter()
            .zip([[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]])
        {
            mesh.vertices.push(
                Vertex::new([corner[0], corner[1], track.z], [0.0, 0.0, 1.0], uv)
                    .with_color([1.0, 1.0, 1.0, alpha]),
            );
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }
    mesh
}
