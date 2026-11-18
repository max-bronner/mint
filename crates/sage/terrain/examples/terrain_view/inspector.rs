use mint_render::{Camera, Vec3};
use mint_sage_map::{cell_to_world, BlendTileData, HeightMapData, Object, MAP_XY_FACTOR};
use mint_sage_terrain::{
    pick_surface, ObjectProxy, RoadNetwork, RoadOptions, RoadSegments, Terrain, TerrainLayer,
};

use std::collections::BTreeMap;

use crate::{map_cliff_note, TemplateNote};

pub(crate) use mint_sage_scene::ClassNote;

pub(crate) struct CellContext<'a> {
    pub(crate) height_map: &'a HeightMapData,
    pub(crate) blend_tile: &'a BlendTileData,
    pub(crate) terrain: &'a Terrain,
    pub(crate) classes: &'a [ClassNote],
    pub(crate) map_label: &'a str,
    pub(crate) draw_blend: bool,
    pub(crate) objects: &'a [Object],
    pub(crate) proxies: &'a [ObjectProxy],
    pub(crate) templates: Option<&'a BTreeMap<String, TemplateNote>>,
    pub(crate) roads: Option<RoadReport<'a>>,
}

pub(crate) struct RoadReport<'a> {
    pub(crate) segments: &'a RoadSegments,
    pub(crate) network: &'a RoadNetwork,
    pub(crate) options: RoadOptions,
}

impl CellContext<'_> {
    pub(crate) fn pick(&self, camera: &Camera, viewport: (u32, u32), px: f32, py: f32) {
        let Some(point) = camera
            .ray_through_pixel(viewport, px, py)
            .and_then(|ray| pick_surface(self.height_map, self.terrain.vertical_scale, ray))
        else {
            println!(
                "
--- no terrain under pixel ({px:.0}, {py:.0}) ---"
            );
            return;
        };
        let border = f32::from(u16::try_from(self.height_map.border_size).unwrap_or(u16::MAX));
        let to_cell = |world: f32, last: u32| -> u32 {
            floor_index(world / MAP_XY_FACTOR + border, u32::MAX)
                .unwrap_or(0)
                .min(last)
        };
        let x = to_cell(point.x, self.blend_tile.width.saturating_sub(1));
        let y = to_cell(point.y, self.blend_tile.height.saturating_sub(1));
        self.report(x, y, Some(point));
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn report(&self, x: u32, y: u32, pick: Option<Vec3>) {
        let index = usize::try_from(u64::from(y) * u64::from(self.blend_tile.width) + u64::from(x))
            .unwrap_or(usize::MAX);

        let bt = self.blend_tile;
        let layout = &self.terrain.layout;
        let scale = self.terrain.vertical_scale;
        let sample = |dx: u32, dy: u32| {
            self.height_map
                .sample(x.saturating_add(dx), y.saturating_add(dy))
                .unwrap_or(0)
        };
        let (ha, hb, hc, hd) = (sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1));
        let across_ad = i32::from(ha).saturating_sub(i32::from(hd)).abs();
        let across_bc = i32::from(hb).saturating_sub(i32::from(hc)).abs();

        let tile_raw = bt.tile_index.get(index).copied().unwrap_or(0);
        let cell_tile = layout.cell(tile_raw);
        let cliff_index = bt.cliff_info_index.get(index).copied().unwrap_or(0);
        let base_class = cell_tile.map(|tile| tile.class);
        let cliff =
            mint_sage_terrain::cell_cliff(layout, &bt.cliff_records, cliff_index, base_class);

        println!("\n--- cell ({x}, {y}) --- {} ---", self.map_label);
        println!("  cliff mode    overlays placed, map's cliff texture, gate on");
        println!(
            "  world         x {:.1}  y {:.1}  z {:.1}   (pick {})",
            cell_to_world(i32::try_from(x).unwrap_or(0), self.height_map.border_size),
            cell_to_world(i32::try_from(y).unwrap_or(0), self.height_map.border_size),
            f32::from(ha) * scale,
            pick.map_or_else(
                || "not from a click".to_owned(),
                |p| format!("{:.1}, {:.1}, {:.1}", p.x, p.y, p.z)
            )
        );
        println!(
            "  height        raw {ha} -> {:.2} world at scale {scale}",
            f32::from(ha) * scale
        );
        println!("  quad corners  A {ha}  B(+x) {hb}  C(+y) {hc}  D(+x+y) {hd}");
        println!(
            "  fold          along {}  (|A-D| {across_ad} vs |B-C| {across_bc})",
            if across_ad < across_bc { "A-D" } else { "B-C" }
        );
        println!(
            "  blend tile    version {}, grid {}x{}, border {}",
            bt.version, bt.width, bt.height, self.height_map.border_size
        );
        println!(
            "  tile index    {tile_raw} raw -> tile {} sub-tile {}",
            tile_raw >> 2,
            tile_raw & 3
        );
        match cell_tile {
            Some(tile) => println!(
                "  base class    {} {}  uv [{:.4}, {:.4}] at (x, y) .. \
                 [{:.4}, {:.4}] at (x+1, y+1)",
                tile.class,
                self.class_note(tile.class),
                tile.uv_xy[0],
                tile.uv_xy[1],
                tile.uv_x1y1[0],
                tile.uv_x1y1[1]
            ),
            None => println!(
                "  base class    !! NONE !! tile {} is past every declared class",
                tile_raw >> 2
            ),
        }

        for (label, array) in [
            ("blend       ", &bt.blend_index),
            ("extra blend ", &bt.extra_blend_index),
        ] {
            let value = array.get(index).copied().unwrap_or(0);
            let record = usize::try_from(value)
                .ok()
                .and_then(|value| value.checked_sub(1))
                .and_then(|slot| bt.blend_records.get(slot));
            let Some(record) = record else {
                println!("  {label}  none (index {value})");
                continue;
            };
            let overlay = mint_sage_terrain::cell_blend(layout, &bt.blend_records, value);
            let mut flags = Vec::new();
            for (set, name) in [
                (record.horizontal, "horizontal"),
                (record.vertical, "vertical"),
                (record.right_diagonal, "right-diagonal"),
                (record.left_diagonal, "left-diagonal"),
                (record.inverted, "inverted"),
            ] {
                if set {
                    flags.push(name);
                }
            }
            println!(
                "  {label}  index {value} -> record {}: tile {} -> class {}, flags [{}], long-diagonal {}, customBlendEdgeClass {}",
                value.saturating_sub(1),
                record.tile_index >> 2,
                overlay.map_or_else(
                    || "!! unresolved !!".to_owned(),
                    |overlay| format!("{} {}", overlay.tile.class, self.class_note(overlay.tile.class))
                ),
                flags.join(" "),
                record.long_diagonal,
                record.custom_blend_edge_class
            );
            if let Some(overlay) = overlay {
                println!(
                    "                corner alpha A {:.0} B {:.0} C {:.0} D {:.0}",
                    overlay.ramp[0].clamp(0.0, 1.0),
                    overlay.ramp[1].clamp(0.0, 1.0),
                    overlay.ramp[2].clamp(0.0, 1.0),
                    overlay.ramp[3].clamp(0.0, 1.0)
                );
            }
        }

        match usize::try_from(cliff_index)
            .ok()
            .and_then(|value| value.checked_sub(1))
            .and_then(|slot| bt.cliff_records.get(slot))
        {
            Some(record) => {
                let record_class = u16::try_from(record.tile_index)
                    .ok()
                    .and_then(|value| layout.cell(value))
                    .map(|tile| tile.class);
                println!(
                    "  cliff         index {cliff_index} -> record {}: tile {} -> class {}, flip {}, second byte {}",
                    cliff_index.saturating_sub(1),
                    record.tile_index >> 2,
                    record_class.map_or_else(
                        || "!! names no class !!".to_owned(),
                        |class| format!("{class} {}", self.class_note(class))
                    ),
                    record.flip,
                    record.unknown
                );
                if cliff.is_none() {
                    println!(
                        "                !! NOT APPLIED: the record's class is not this cell's base class {}, so the cell keeps its base tile rectangle !!",
                        base_class.map_or_else(|| "(none)".to_owned(), |class| class.to_string())
                    );
                }
                println!(
                    "                stored uv (ccw from (x,y)) {:?}",
                    record.tex_coords
                );
                let drawn = cliff.map(|cliff| self.terrain.map_cliff_class.unwrap_or(cliff.class));
                println!(
                    "                map cliff texture (cliff record 0) = {}",
                    map_cliff_note(self.terrain, bt)
                );
                match drawn {
                    Some(class) => println!(
                        "                this cell draws its face in class {class} {} - {}",
                        self.class_note(class),
                        if Some(class) == self.terrain.map_cliff_class {
                            "USING the map's cliff texture"
                        } else {
                            "NOT the map's cliff texture"
                        }
                    ),
                    None => println!(
                        "                this cell draws no cliff face: the gate dropped the \
                         record, so it keeps its base tile rectangle in either mode"
                    ),
                }
            }
            None => println!("  cliff         none (index {cliff_index})"),
        }

        let cell_byte = bt
            .unknown_cell_byte
            .as_ref()
            .and_then(|bytes| bytes.get(index).copied());
        println!(
            "  cell u8       {}",
            cell_byte.map_or_else(|| "absent in this version".to_owned(), |b| b.to_string())
        );
        let stride = usize::try_from(bt.width.saturating_add(7) / 8).unwrap_or(0);
        let planes: Vec<String> = bt
            .bit_planes
            .iter()
            .enumerate()
            .map(|(plane, bytes)| {
                let byte = usize::try_from(y)
                    .ok()
                    .and_then(|y| y.checked_mul(stride))
                    .and_then(|row| row.checked_add(usize::try_from(x / 8).ok()?))
                    .and_then(|at| bytes.get(at).copied())
                    .unwrap_or(0);
                let bit = (byte >> (x % 8)) & 1;
                format!("{plane}:{bit}")
            })
            .collect();
        println!("  bit planes    {}", planes.join(" "));

        let draws = self.draws(x, y);
        if draws.is_empty() {
            println!("  draws         !! no part found - the cell is outside every chunk !!");
        }
        for draw in &draws {
            println!(
                "  draws         chunk {} part {} instance {}, {} layer, class {}",
                draw.chunk,
                draw.part,
                draw.instance
                    .map_or_else(|| "not uploaded (--no-blend)".to_owned(), |n| n.to_string()),
                layer_name(draw.layer),
                draw.class.map_or_else(
                    || "!!! NONE - THIS CELL DRAWS UNTEXTURED !!!".to_owned(),
                    |class| format!("{class} {}", self.class_note(class))
                )
            );
            println!(
                "                quad at vertex {} of {}, corner alpha A {:.0} B {:.0} C {:.0} D {:.0}",
                draw.vertex,
                draw.vertices,
                draw.alpha[0].clamp(0.0, 1.0),
                draw.alpha[1].clamp(0.0, 1.0),
                draw.alpha[2].clamp(0.0, 1.0),
                draw.alpha[3].clamp(0.0, 1.0)
            );
            println!("                uploaded uv {:?}", draw.uv);
        }
        for (label, layer, value) in [
            ("base", TerrainLayer::Base, 1),
            (
                "blend",
                TerrainLayer::Blend,
                bt.blend_index.get(index).copied().unwrap_or(0),
            ),
            (
                "extra blend",
                TerrainLayer::ExtraBlend,
                bt.extra_blend_index.get(index).copied().unwrap_or(0),
            ),
        ] {
            let drawn = draws.iter().any(|draw| draw.layer == layer);
            if value != 0 && !drawn {
                println!(
                    "  !! the {label} layer has index {value} but NO PART DRAWS IT for this cell !!"
                );
            }
            if value == 0 && drawn {
                println!("  !! a {label} part draws this cell though its index is 0 !!");
            }
        }

        let neighbour = |dx: i64, dy: i64| -> String {
            let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
            if nx < 0 || ny < 0 || nx >= i64::from(bt.width) || ny >= i64::from(bt.height) {
                return "off-map".to_owned();
            }
            let at = usize::try_from(ny * i64::from(bt.width) + nx).unwrap_or(usize::MAX);
            bt.tile_index
                .get(at)
                .and_then(|value| layout.cell(*value))
                .map_or_else(
                    || "none".to_owned(),
                    |tile| format!("{} {}", tile.class, self.class_note(tile.class)),
                )
        };
        println!(
            "  neighbours    -x {}  +x {}  -y {}  +y {}",
            neighbour(-1, 0),
            neighbour(1, 0),
            neighbour(0, -1),
            neighbour(0, 1)
        );
        self.report_objects(x, y);
        self.report_roads(x, y);
    }

    fn report_roads(&self, x: u32, y: u32) {
        let Some(roads) = &self.roads else {
            return;
        };
        let border = self.height_map.border_size;
        let half = MAP_XY_FACTOR / 2.0;
        let (cx, cy) = (
            cell_to_world(i32::try_from(x).unwrap_or(0), border) + half,
            cell_to_world(i32::try_from(y).unwrap_or(0), border) + half,
        );
        let reach = MAP_XY_FACTOR * f32::from(ROAD_REPORT_CELLS);
        let mut near: Vec<(f32, usize)> = roads
            .segments
            .segments
            .iter()
            .enumerate()
            .map(|(slot, segment)| (point_to_segment(cx, cy, segment.start, segment.end), slot))
            .filter(|(distance, _)| *distance <= reach)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

        let stats = &roads.network.stats;
        println!(
            "  roads         {} segment(s) within {ROAD_REPORT_CELLS} cell(s), {} on the map, \
             {} drawn",
            near.len(),
            stats.segments,
            stats.drawn
        );
        println!(
            "                lift {} world units, height rule {}",
            roads.options.lift,
            roads.options.height.label()
        );
        for (distance, slot) in near.iter().take(ROAD_REPORT_LIMIT) {
            let Some(segment) = roads.segments.segments.get(*slot) else {
                continue;
            };
            let name = self
                .objects
                .get(segment.first)
                .map_or("!! no such object !!", |object| {
                    object.template_name.as_str()
                });
            let drawn = roads
                .network
                .meshes
                .iter()
                .find(|road| road.name.eq_ignore_ascii_case(name));
            println!(
                "                #{} {name:?} ({:.1}, {:.1}) -> ({:.1}, {:.1}), {distance:.1} away, {}",
                segment.first,
                segment.start[0],
                segment.start[1],
                segment.end[0],
                segment.end[1],
                match drawn {
                    Some(road) => format!(
                        "width {} x {}",
                        road.style.width, road.style.width_in_texture
                    ),
                    None => "NOT DRAWN - no usable Road declaration".to_owned(),
                }
            );
        }
    }

    fn model_line(&self, template: &str) -> Option<String> {
        let note = self.templates?.get(&template.to_ascii_lowercase())?;
        Some(match (&note.model, note.parts) {
            (Some(model), Some(parts)) => format!("model {model} ({parts} part(s))"),
            (Some(model), None) => format!("model {model} (not loaded this run)"),
            (None, _) if note.why.is_empty() => "model none".to_owned(),
            (None, _) => format!("model none - {}", note.why),
        })
    }

    fn report_objects(&self, x: u32, y: u32) {
        let border = self.height_map.border_size;
        let (cx, cy) = (
            cell_to_world(i32::try_from(x).unwrap_or(0), border),
            cell_to_world(i32::try_from(y).unwrap_or(0), border),
        );
        let reach = MAP_XY_FACTOR * f32::from(OBJECT_REPORT_CELLS);
        let mut near: Vec<&ObjectProxy> = self
            .proxies
            .iter()
            .filter(|proxy| {
                (proxy.position.x - cx) >= -reach
                    && (proxy.position.x - cx) <= reach + MAP_XY_FACTOR
                    && (proxy.position.y - cy) >= -reach
                    && (proxy.position.y - cy) <= reach + MAP_XY_FACTOR
            })
            .collect();
        near.sort_by(|a, b| {
            let d = |proxy: &ObjectProxy| {
                let (dx, dy) = (proxy.position.x - cx, proxy.position.y - cy);
                dx * dx + dy * dy
            };
            d(a).total_cmp(&d(b)).then(a.index.cmp(&b.index))
        });

        println!(
            "  objects       {} within {OBJECT_REPORT_CELLS} cell(s), {} on the map",
            near.len(),
            self.proxies.len()
        );
        for proxy in near.iter().take(OBJECT_REPORT_LIMIT) {
            let object = self.objects.get(proxy.index);
            println!(
                "                #{} {:?}",
                proxy.index,
                object.map_or("!! no such object !!", |object| object
                    .template_name
                    .as_str())
            );
            if let Some(line) = object.and_then(|object| self.model_line(&object.template_name)) {
                println!("                  {line}");
            }
            println!(
                "                  world {:.1}, {:.1}, {:.1}   ground {}  z offset {:.1}",
                proxy.position.x,
                proxy.position.y,
                proxy.position.z,
                proxy.ground.map_or_else(
                    || "!! OFF THE HEIGHTMAP !!".to_owned(),
                    |ground| format!("{ground:.1}")
                ),
                object.map_or(0.0, |object| object.z)
            );
            println!(
                "                  angle {:.4} rad = {:.1} deg  flags {}  {}{}",
                proxy.angle,
                proxy.angle.to_degrees(),
                object.map_or(0, |object| object.flags),
                if proxy.waypoint {
                    "waypoint"
                } else {
                    "not a waypoint"
                },
                if proxy.align_to_terrain {
                    ", alignToTerrain"
                } else {
                    ""
                }
            );
        }
        if near.len() > OBJECT_REPORT_LIMIT {
            println!(
                "                ... and {} more",
                near.len().saturating_sub(OBJECT_REPORT_LIMIT)
            );
        }
    }

    fn class_note(&self, class: usize) -> String {
        match self.classes.get(class) {
            None => "!! class index past the class list !!".to_owned(),
            Some(note) => format!(
                "{:?} -> {}{}",
                note.name,
                note.file.as_deref().unwrap_or("!! not in terrain.ini !!"),
                if note.loaded {
                    ""
                } else {
                    " !! TEXTURE DID NOT LOAD !!"
                }
            ),
        }
    }

    fn draws(&self, x: u32, y: u32) -> Vec<CellDraw> {
        let border = self.height_map.border_size;
        let corner = [
            cell_to_world(i32::try_from(x).unwrap_or(i32::MAX), border),
            cell_to_world(i32::try_from(y).unwrap_or(i32::MAX), border),
        ];
        let mut found = Vec::new();
        let mut instance = 0usize;
        for (chunk_index, chunk) in self.terrain.chunks.iter().enumerate() {
            for (part_index, part) in chunk.parts.iter().enumerate() {
                let uploaded = self.draw_blend || part.layer == TerrainLayer::Base;
                let slot = uploaded.then_some(instance);
                if uploaded {
                    instance = instance.saturating_add(1);
                }
                let Some(vertex) = part
                    .mesh
                    .vertices
                    .chunks_exact(4)
                    .position(|quad| {
                        quad.first().is_some_and(|a| {
                            a.position.first() == Some(&corner[0])
                                && a.position.get(1) == Some(&corner[1])
                        })
                    })
                    .map(|quad| quad.saturating_mul(4))
                else {
                    continue;
                };
                let at = |offset: usize| part.mesh.vertices.get(vertex.saturating_add(offset));
                let alpha = [0usize, 1, 2, 3].map(|slot| {
                    at(slot)
                        .and_then(|v| v.color.get(3).copied())
                        .unwrap_or(f32::NAN)
                });
                let uv = [0usize, 1, 2, 3].map(|slot| at(slot).map_or([f32::NAN; 2], |v| v.uv));
                found.push(CellDraw {
                    chunk: chunk_index,
                    part: part_index,
                    instance: slot,
                    layer: part.layer,
                    class: part.texture_class,
                    vertex,
                    vertices: part.mesh.vertices.len(),
                    alpha,
                    uv,
                });
            }
        }
        found
    }
}

struct CellDraw {
    chunk: usize,
    part: usize,
    instance: Option<usize>,
    layer: TerrainLayer,
    class: Option<usize>,
    vertex: usize,
    vertices: usize,
    alpha: [f32; 4],
    uv: [[f32; 2]; 4],
}

const OBJECT_REPORT_CELLS: u16 = 2;

const OBJECT_REPORT_LIMIT: usize = 8;

const ROAD_REPORT_CELLS: u16 = 4;

const ROAD_REPORT_LIMIT: usize = 6;

fn point_to_segment(px: f32, py: f32, start: [f32; 2], end: [f32; 2]) -> f32 {
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let length_sq = dx * dx + dy * dy;
    let (vx, vy) = (px - start[0], py - start[1]);
    let t = if length_sq > 0.0 {
        ((vx * dx + vy * dy) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (ox, oy) = (vx - dx * t, vy - dy * t);
    (ox * ox + oy * oy).sqrt()
}

fn layer_name(layer: TerrainLayer) -> &'static str {
    match layer {
        TerrainLayer::Base => "base",
        TerrainLayer::Blend => "blend",
        TerrainLayer::ExtraBlend => "extra-blend",
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn floor_index(value: f32, limit: u32) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let floored = value.floor();
    if floored >= f32::from(u16::MAX) {
        return None;
    }
    let index = floored as u32;
    (index < limit).then_some(index)
}
