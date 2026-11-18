use std::collections::HashMap;

use mint_render::{RenderTargetHandle, Renderer, Scene};
use mint_sage_apt_player::{InstanceId, MovieId};

use crate::host::{MovieView, SurfaceIcon};

pub type SurfaceKey = (MovieId, InstanceId);

const MAX_EDGE: f32 = 8192.0;

#[derive(Default)]
pub struct SceneSurfaces {
    targets: HashMap<SurfaceKey, (RenderTargetHandle, u32, u32)>,
    free: Vec<RenderTargetHandle>,
}

#[must_use]
pub fn target_size(rect: [f32; 4]) -> (u32, u32) {
    let [l, t, r, b] = rect;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let edge = |v: f32| v.round().clamp(1.0, MAX_EDGE) as u32;
    (edge(r - l), edge(b - t))
}

impl SceneSurfaces {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        key: SurfaceKey,
        size: (u32, u32),
        scene: &Scene<'_>,
    ) -> mint_render::Result<SurfaceIcon> {
        let target = self.target(renderer, key, size)?;
        renderer.render_to_target(target, scene)?;
        let texture = renderer
            .render_target_texture(target)
            .ok_or(mint_render::Error::UnknownHandle)?;
        Ok(SurfaceIcon {
            texture,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
            premultiplied: true,
            rotated: false,
            ignores_color_transform: false,
        })
    }

    pub fn retain(&mut self, renderer: &mut Renderer, movie: MovieId, live: &[InstanceId]) {
        let gone: Vec<SurfaceKey> = self
            .targets
            .keys()
            .filter(|(m, id)| *m == movie && !live.contains(id))
            .copied()
            .collect();
        for key in gone {
            if let Some((target, _, _)) = self.targets.remove(&key) {
                if renderer.resize_render_target(target, 1, 1).is_ok() {
                    self.free.push(target);
                }
            }
        }
    }

    fn target(
        &mut self,
        renderer: &mut Renderer,
        key: SurfaceKey,
        (w, h): (u32, u32),
    ) -> mint_render::Result<RenderTargetHandle> {
        match self.targets.get(&key).copied() {
            Some((target, tw, th)) if (tw, th) == (w, h) => Ok(target),
            Some((target, _, _)) => {
                renderer.resize_render_target(target, w, h)?;
                self.targets.insert(key, (target, w, h));
                Ok(target)
            }
            None => {
                let target = match self.free.pop() {
                    Some(target) => {
                        renderer.resize_render_target(target, w, h)?;
                        target
                    }
                    None => renderer.create_render_target(w, h)?,
                };
                self.targets.insert(key, (target, w, h));
                Ok(target)
            }
        }
    }
}

pub fn surfaces_of_type(view: &mut MovieView, wanted: impl Fn(&str) -> bool) -> Vec<InstanceId> {
    view.surface_instances()
        .into_iter()
        .filter(|&id| {
            view.player
                .own_string(id, "_type")
                .is_some_and(|t| wanted(&t))
        })
        .collect()
}
