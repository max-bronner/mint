use mint_movement::{Facing, Vec2};
use mint_sage_modules::ObjectRecord;

use super::formation_preview::{decal_of, PreviewDecals};
use super::RotwkRules;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ArrowPiece {
    pub(crate) centre: Vec2,
    pub(crate) angle: f32,
    pub(crate) size_x: f32,
    pub(crate) size_y: f32,
    pub(crate) texture: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ArrowPieceSize {
    pub(crate) texture: Option<String>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ArrowSizes {
    pub(crate) head: ArrowPieceSize,
    pub(crate) body: ArrowPieceSize,
    pub(crate) base: ArrowPieceSize,
    pub(crate) body_template_width: f32,
}

impl RotwkRules {
    pub(crate) fn formation_arrow(
        &self,
        anchor: Vec2,
        drag: Vec2,
        mode: PreviewDecals,
    ) -> Vec<ArrowPiece> {
        let [head_name, body_name, base_name] = &self.drag_formation.facing_arrow_templates;
        let body_record = body_name
            .as_deref()
            .and_then(|name| self.arrow_record(name));
        let sizes = ArrowSizes {
            head: self.arrow_piece_size(head_name.as_deref(), mode),
            body: self.decal_piece_size(body_record, mode),
            base: self.arrow_piece_size(base_name.as_deref(), mode),
            body_template_width: body_record
                .and_then(|record| record.formation_preview_decal.as_ref())
                .map_or(0.0, |decal| decal.width),
        };
        formation_arrow(anchor, drag, &sizes)
    }

    fn arrow_record(&self, name: &str) -> Option<&ObjectRecord> {
        let kind = self.kinds.kind(name)?;
        self.kinds.get(kind).map(|entry| &entry.record)
    }

    fn arrow_piece_size(&self, name: Option<&str>, mode: PreviewDecals) -> ArrowPieceSize {
        let record = name.and_then(|name| self.arrow_record(name));
        self.decal_piece_size(record, mode)
    }

    fn decal_piece_size(
        &self,
        record: Option<&ObjectRecord>,
        mode: PreviewDecals,
    ) -> ArrowPieceSize {
        let Some((decal, _)) = record.and_then(|record| decal_of(record, mode)) else {
            return ArrowPieceSize::default();
        };
        ArrowPieceSize {
            texture: Some(decal.texture.clone()),
            width: decal.width,
            height: decal.height,
        }
    }
}

const FORWARD_OFFSET: f32 = 30.0;

const BASE_OVERLAP: f32 = 10.0;

#[must_use]
pub(crate) fn formation_arrow(anchor: Vec2, d: Vec2, sizes: &ArrowSizes) -> Vec<ArrowPiece> {
    let (Some(u), Some(facing)) = (d.normalised(), Facing::from_vector(d)) else {
        return Vec::new();
    };
    let length = d.length();
    let n = Vec2::new(-u.y, u.x);
    let theta = facing.radians();
    let half_pi = core::f32::consts::FRAC_PI_2;

    let w_s = if sizes.base.texture.is_some() {
        sizes.base.width
    } else {
        0.0
    };
    let h_b = if sizes.body.texture.is_some() {
        sizes.body.height
    } else {
        0.0
    };
    let h_h = if sizes.head.texture.is_some() {
        sizes.head.height
    } else {
        0.0
    };
    let perimeter = length.max((sizes.body_template_width + w_s) / 2.0);
    let forward = FORWARD_OFFSET + h_b / 2.0;
    let centre = anchor.plus(u.scale(forward));

    let mut pieces = Vec::new();
    if let Some(texture) = sizes.body.texture.clone() {
        pieces.push(ArrowPiece {
            centre,
            angle: theta - half_pi,
            size_x: 2.0 * perimeter - w_s,
            size_y: h_b,
            texture,
        });
    }
    if let Some(texture) = sizes.base.texture.clone() {
        let reach = (perimeter - BASE_OVERLAP).max(0.0);
        pieces.push(ArrowPiece {
            centre: centre.plus(n.scale(reach)),
            angle: theta + half_pi,
            size_x: w_s,
            size_y: sizes.base.height,
            texture: texture.clone(),
        });
        pieces.push(ArrowPiece {
            centre: centre.minus(n.scale(reach)),
            angle: theta - half_pi,
            size_x: w_s,
            size_y: sizes.base.height,
            texture,
        });
    }
    if let Some(texture) = sizes.head.texture.clone() {
        pieces.push(ArrowPiece {
            centre: anchor.plus(u.scale(FORWARD_OFFSET + h_b + h_h / 2.0)),
            angle: theta - half_pi,
            size_x: sizes.head.width,
            size_y: h_h,
            texture,
        });
    }
    pieces
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;

    fn sizes() -> ArrowSizes {
        ArrowSizes {
            head: ArrowPieceSize {
                texture: Some("Head".into()),
                width: 0.0,
                height: 50.0,
            },
            body: ArrowPieceSize {
                texture: Some("Body".into()),
                width: 0.0,
                height: 10.0,
            },
            base: ArrowPieceSize {
                texture: Some("Base".into()),
                width: 20.0,
                height: 0.0,
            },
            body_template_width: 40.0,
        }
    }

    fn by_texture<'a>(pieces: &'a [ArrowPiece], texture: &str) -> &'a ArrowPiece {
        pieces.iter().find(|p| p.texture == texture).unwrap()
    }

    #[test]
    fn a_long_drag_places_the_bar_caps_and_head_by_drag_length() {
        let pieces = formation_arrow(Vec2::ZERO, Vec2::new(100.0, 0.0), &sizes());
        let half_pi = core::f32::consts::FRAC_PI_2;

        let body = by_texture(&pieces, "Body");
        assert!((body.centre.x - 35.0).abs() < 1e-3 && body.centre.y.abs() < 1e-3);
        assert!((body.angle + half_pi).abs() < 1e-3);
        assert!((body.size_x - 180.0).abs() < 1e-3);

        let bases: Vec<&ArrowPiece> = pieces.iter().filter(|p| p.texture == "Base").collect();
        assert_eq!(bases.len(), 2);
        let plus = bases.iter().find(|b| b.centre.y > 0.0).unwrap();
        assert!((plus.centre.x - 35.0).abs() < 1e-3 && (plus.centre.y - 90.0).abs() < 1e-3);
        assert!((plus.angle - half_pi).abs() < 1e-3);
        let minus = bases.iter().find(|b| b.centre.y < 0.0).unwrap();
        assert!((minus.centre.x - 35.0).abs() < 1e-3 && (minus.centre.y + 90.0).abs() < 1e-3);
        assert!((minus.angle + half_pi).abs() < 1e-3);

        let head = by_texture(&pieces, "Head");
        assert!((head.centre.x - 65.0).abs() < 1e-3 && head.centre.y.abs() < 1e-3);
        assert!((head.angle + half_pi).abs() < 1e-3);
    }

    #[test]
    fn a_short_drag_raises_the_perimeter_to_the_template_floor() {
        let pieces = formation_arrow(Vec2::ZERO, Vec2::new(20.0, 0.0), &sizes());
        let body = by_texture(&pieces, "Body");
        assert!((body.size_x - 40.0).abs() < 1e-3, "{body:?}");
        let bases: Vec<&ArrowPiece> = pieces.iter().filter(|p| p.texture == "Base").collect();
        let plus = bases.iter().find(|b| b.centre.y > 0.0).unwrap();
        assert!((plus.centre.y - 20.0).abs() < 1e-3, "{plus:?}");
    }

    #[test]
    fn a_zero_length_drag_draws_no_arrow() {
        assert!(formation_arrow(Vec2::ZERO, Vec2::ZERO, &sizes()).is_empty());
    }

    #[test]
    fn a_missing_piece_is_skipped_and_its_size_counts_as_zero() {
        let mut sizes = sizes();
        sizes.base.texture = None;
        let pieces = formation_arrow(Vec2::ZERO, Vec2::new(100.0, 0.0), &sizes);
        assert!(pieces.iter().all(|p| p.texture != "Base"));
        let body = by_texture(&pieces, "Body");
        assert!((body.size_x - 200.0).abs() < 1e-3, "{body:?}");
    }
}
