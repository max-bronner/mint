use std::sync::Arc;

use mint_render::{Blend, DepthTest, Instance, Renderer, RibbonStyle, TextureHandle, Vec3};
use mint_sage_asset::Resolver;
use mint_vfs::Vfs;

use super::UnitVisuals;

const QUEUE_LINE_WIDTH: f32 = 1.5;

pub(crate) const QUEUE_LINE_LIFT: f32 = 1.0;

const QUEUE_LINE_TILE: f32 = 4.0;

const QUEUE_LINE_COLOUR: [f32; 3] = [0.96, 0.77, 0.47];

const ACTIVE_TEXTURE: &str = "EXLaser.tga";
const LOOP_TEXTURE: &str = "EXLaser2.tga";
const MARKER_MODEL: &str = "SCMoveHintSml";

pub(super) struct QueueArt {
    active: Option<TextureHandle>,
    looped: Option<TextureHandle>,
    marker: Option<Arc<mint_sage_model::CachedModel>>,
}

#[cfg(test)]
impl QueueArt {
    pub(super) fn untextured() -> Self {
        Self {
            active: None,
            looped: None,
            marker: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Section {
    pub(super) points: Vec<[f32; 3]>,
    pub(super) looped: bool,
}

pub(super) fn sections(points: &[[f32; 3]], loop_from: Option<usize>) -> Vec<Section> {
    let split = loop_from.map_or(points.len(), |from| from.saturating_add(1));
    let mut sections = vec![Section {
        points: points
            .get(..split.min(points.len()))
            .unwrap_or(&[])
            .to_vec(),
        looped: false,
    }];
    if let Some(from) = loop_from {
        let mut closed = points.get(from..).unwrap_or(&[]).to_vec();
        if let Some(&start) = closed.first() {
            closed.push(start);
        }
        sections.push(Section {
            points: closed,
            looped: true,
        });
    }
    sections
}

pub(super) fn style(section: &Section, art: &QueueArt, seconds: f32) -> RibbonStyle {
    RibbonStyle {
        width: QUEUE_LINE_WIDTH,
        tile: QUEUE_LINE_TILE,
        color: QUEUE_LINE_COLOUR,
        texture: if section.looped {
            art.looped
        } else {
            art.active
        },
        blend: Blend::AlphaBlend,
        uv_offset: if section.looped {
            [0.0, 1.0 - seconds.rem_euclid(1.0)]
        } else {
            [0.0; 2]
        },
        depth_test: DepthTest::Always,
    }
}

impl UnitVisuals {
    fn load_queue_art(renderer: &mut Renderer, vfs: &Vfs, resolver: &Resolver) -> QueueArt {
        let compressed = renderer.supports_block_compression();
        let mut texture =
            |name: &str| match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
                Ok(loaded) => renderer
                    .upload_texture(&loaded.data)
                    .map_err(|error| println!("order lines: {name} would not upload: {error}"))
                    .ok(),
                Err(error) => {
                    println!("order lines: {name} did not load ({error}) - plain colour stands in");
                    None
                }
            };
        let active = texture(ACTIVE_TEXTURE);
        let looped = texture(LOOP_TEXTURE);
        let mut cache = mint_sage_model::ModelCache::new();
        let marker = cache.get_or_load(renderer, vfs, resolver, MARKER_MODEL);
        if marker.is_none() {
            if let Some((name, reason)) = cache.failures().first() {
                println!("order lines: {name} did not load ({reason}) - no marker is drawn");
            }
        }
        QueueArt {
            active,
            looped,
            marker,
        }
    }

    pub(crate) fn push_order_queues(
        &mut self,
        renderer: &mut Renderer,
        files: (&Vfs, &Resolver),
        strips: &[(Vec<[f32; 3]>, Option<usize>)],
        eye: Vec3,
        seconds: f32,
        out: &mut Vec<Instance>,
    ) {
        if self.queue_art.is_none() {
            self.queue_art = Some(Self::load_queue_art(renderer, files.0, files.1));
        }
        let Some(art) = self.queue_art.take() else {
            return;
        };
        self.queue_ribbons.begin();
        for (points, loop_from) in strips {
            for section in sections(points, *loop_from) {
                let _ = self.queue_ribbons.push(
                    renderer,
                    &section.points,
                    eye.to_array(),
                    &style(&section, &art, seconds),
                    out,
                );
            }
            if let Some(marker) = &art.marker {
                for point in points.iter().skip(1) {
                    self.push_move_hint(
                        Some(marker),
                        point[0],
                        point[1],
                        point[2] - QUEUE_LINE_LIFT,
                        seconds,
                        out,
                    );
                }
            }
        }
        self.queue_art = Some(art);
    }
}
