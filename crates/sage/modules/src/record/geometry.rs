use mint_sage_ini::{Block, Field};

use crate::effects::Name;

use super::Reads;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geometry {
    pub shape: Option<String>,
    pub major_radius: Option<f32>,
    pub minor_radius: Option<f32>,
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub health_box_height: Option<f32>,
    pub pieces: Vec<GeometryPiece>,
}

impl Geometry {
    pub fn shown<'a>(
        &'a self,
        overrides: &'a [(Name, bool)],
    ) -> impl Iterator<Item = &'a GeometryPiece> + 'a {
        self.pieces
            .iter()
            .filter(move |piece| piece.shown(overrides))
    }

    #[must_use]
    pub fn shown_radius(&self, overrides: &[(Name, bool)]) -> Option<f32> {
        largest(self.shown(overrides).filter_map(GeometryPiece::radius))
    }

    #[must_use]
    pub fn shown_height(&self, overrides: &[(Name, bool)]) -> Option<f32> {
        largest(self.shown(overrides).filter_map(|piece| piece.height))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryPiece {
    pub offset: (f32, f32),
    pub offset_z: f32,
    pub shape: PieceShape,
    pub name: Option<String>,
    pub active: bool,
    pub height: Option<f32>,
    pub for_health_box: bool,
}

impl GeometryPiece {
    #[must_use]
    pub fn shown(&self, overrides: &[(Name, bool)]) -> bool {
        self.name
            .as_deref()
            .and_then(|name| {
                overrides
                    .iter()
                    .find(|(group, _)| group.eq_ignore_ascii_case(name))
            })
            .map_or(self.active, |(_, shown)| *shown)
    }

    #[must_use]
    pub fn radius(&self) -> Option<f32> {
        let radius = match self.shape {
            PieceShape::Box { major, minor } => major.max(minor),
            PieceShape::Disc { radius } => radius,
        };
        (radius > 0.0).then_some(radius)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PieceShape {
    Box { major: f32, minor: f32 },
    Disc { radius: f32 },
}

pub(super) fn geometry(block: &Block, reads: &Reads<'_>) -> Geometry {
    struct Record {
        is_box: bool,
        name: Option<String>,
        active: bool,
        major: Option<f32>,
        minor: Option<f32>,
        height: Option<f32>,
        offset: (f32, f32),
        offset_z: f32,
        for_health_box: bool,
    }
    let mut records: Vec<Record> = Vec::new();
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if keyword.eq_ignore_ascii_case("Geometry")
            || keyword.eq_ignore_ascii_case("AdditionalGeometry")
        {
            records.push(Record {
                is_box: field
                    .value()
                    .is_some_and(|value| value.eq_ignore_ascii_case("BOX")),
                name: None,
                active: true,
                major: None,
                minor: None,
                height: None,
                offset: (0.0, 0.0),
                offset_z: 0.0,
                for_health_box: true,
            });
            continue;
        }
        let Some(current) = records.last_mut() else {
            continue;
        };
        let value = || reads.number(Some(field));
        if keyword.eq_ignore_ascii_case("GeometryName") {
            current.name = field.value().map(str::to_owned);
        } else if keyword.eq_ignore_ascii_case("GeometryActive") {
            current.active = reads.flag(Some(field)).unwrap_or(true);
        } else if keyword.eq_ignore_ascii_case("GeometryMajorRadius") {
            current.major = value();
        } else if keyword.eq_ignore_ascii_case("GeometryMinorRadius") {
            current.minor = value();
        } else if keyword.eq_ignore_ascii_case("GeometryHeight") {
            current.height = value();
        } else if keyword.eq_ignore_ascii_case("GeometryUsedForHealthBox") {
            current.for_health_box = reads.flag(Some(field)).unwrap_or(true);
        } else if keyword.eq_ignore_ascii_case("GeometryOffset") {
            let axis = |tag: &str| reads.tagged(field, tag);
            current.offset = (axis("X").unwrap_or(0.0), axis("Y").unwrap_or(0.0));
            current.offset_z = axis("Z").unwrap_or(0.0);
        }
    }
    let Some(primary) = records.first() else {
        return Geometry::default();
    };
    let (major_radius, minor_radius) = (primary.major, primary.minor);
    let pieces: Vec<GeometryPiece> = records
        .into_iter()
        .map(|record| {
            let major = record.major.filter(|r| r.is_finite());
            let minor = record.minor.filter(|r| r.is_finite());
            let shape = if record.is_box {
                let major = major.or(minor).unwrap_or(0.0);
                let minor = minor.or(Some(major)).unwrap_or(0.0);
                PieceShape::Box { major, minor }
            } else {
                let radius = match (major, minor) {
                    (Some(a), Some(b)) => a.max(b),
                    (Some(a), None) | (None, Some(a)) => a,
                    (None, None) => 0.0,
                };
                PieceShape::Disc { radius }
            };
            GeometryPiece {
                offset: record.offset,
                offset_z: record.offset_z,
                shape,
                name: record.name,
                active: record.active,
                height: record.height,
                for_health_box: record.for_health_box,
            }
        })
        .collect();
    let mut geometry = Geometry {
        shape: block
            .field("Geometry")
            .and_then(Field::value)
            .map(str::to_owned),
        major_radius,
        minor_radius,
        radius: None,
        height: None,
        health_box_height: None,
        pieces,
    };
    geometry.radius = geometry.shown_radius(&[]);
    geometry.height = geometry.shown_height(&[]);
    geometry.health_box_height = largest(
        geometry
            .shown(&[])
            .filter(|piece| piece.for_health_box)
            .filter_map(|piece| piece.height),
    );
    geometry
}

fn largest(values: impl Iterator<Item = f32>) -> Option<f32> {
    values.fold(None, |best: Option<f32>, value| {
        Some(best.map_or(value, |best| best.max(value)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::tests::record;

    #[test]
    fn named_groups_are_shown_until_an_override_hides_them() {
        let text = "Object Barracks\n  \
            Geometry = CYLINDER\n  GeometryMajorRadius = 8.0\n  GeometryHeight = 10.0\n  \
            AdditionalGeometry = BOX\n  GeometryName = Orig\n  GeometryMajorRadius = 45.0\n  GeometryMinorRadius = 50.0\n  GeometryHeight = 40.0\n  \
            AdditionalGeometry = BOX\n  GeometryName = V2\n  GeometryMajorRadius = 30.0\n  GeometryHeight = 60.0\n  \
            AdditionalGeometry = BOX\n  GeometryName = Walls\n  GeometryActive = No\n  GeometryMajorRadius = 70.0\n  GeometryHeight = 90.0\n\
            End\n";
        let geometry = record(text, "Barracks").unwrap().geometry;
        assert_eq!((geometry.radius, geometry.height), (Some(50.0), Some(60.0)));
        let level_one: Vec<(Name, bool)> =
            vec![(Name::from("orig"), true), (Name::from("V2"), false)];
        assert_eq!(
            (
                geometry.shown_radius(&level_one),
                geometry.shown_height(&level_one)
            ),
            (Some(50.0), Some(40.0))
        );
        let walls: Vec<(Name, bool)> =
            vec![(Name::from("Orig"), false), (Name::from("Walls"), true)];
        assert_eq!(
            geometry
                .shown(&walls)
                .filter_map(GeometryPiece::radius)
                .collect::<Vec<_>>(),
            [8.0, 30.0, 70.0]
        );
    }

    #[test]
    fn every_geometry_piece_keeps_its_own_offset_and_shape() {
        let text = "Object WargSentry\n  \
            Geometry              = BOX\n  \
            GeometryMajorRadius   = 5.0\n  GeometryMinorRadius = 25.0\n  \
            GeometryOffset        = X:45.0 Y:15.0 Z:0\n  \
            AdditionalGeometry    = BOX\n  \
            GeometryMajorRadius   = 5.0\n  GeometryMinorRadius = 5.0\n  \
            GeometryOffset        = X:0.0 Y:45.0 Z:0\n\
            End\n";
        let sentry = record(text, "WargSentry").unwrap();
        let pieces = &sentry.geometry.pieces;
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].offset, (45.0, 15.0));
        assert_eq!(
            pieces[0].shape,
            PieceShape::Box {
                major: 5.0,
                minor: 25.0
            }
        );
        assert_eq!(pieces[1].offset, (0.0, 45.0));
        assert_eq!(
            pieces[1].shape,
            PieceShape::Box {
                major: 5.0,
                minor: 5.0
            }
        );
    }

    #[test]
    fn a_geometry_offset_reads_its_z_tag() {
        let text = "Object FireTestTower\n  \
            Geometry            = SPHERE\n  \
            GeometryMajorRadius = 5.0\n  \
            GeometryOffset      = X:0 Y:0 Z:15.0\n  \
            AdditionalGeometry  = SPHERE\n  \
            GeometryMajorRadius = 5.0\n\
            End\n";
        let pieces = record(text, "FireTestTower").unwrap().geometry.pieces;
        assert_eq!(pieces[0].offset_z, 15.0);
        assert_eq!(pieces[1].offset_z, 0.0);
    }
}
