use mint_movement::Vec2;
use mint_sage_ini::Document;
use mint_sage_map::{Object, MAP_XY_FACTOR};
use mint_sage_modules::{GeometryPiece, ObjectRecord, PieceShape, TemplateRecords};

const WALK_ON_TOP_OF_WALL: &str = "WALK_ON_TOP_OF_WALL";

const SCALEABLE_WALL: &str = "SCALEABLE_WALL";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Footprint {
    Box { major: f32, minor: f32 },
    Disc { radius: f32 },
}

impl Footprint {
    #[must_use]
    pub fn reach(self) -> f32 {
        match self {
            Self::Box { major, minor } => (major * major + minor * minor).sqrt(),
            Self::Disc { radius } => radius,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FootprintRecord {
    pub offset: Vec2,
    pub footprint: Footprint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Obstacle {
    pub id: u32,
    pub centre: Vec2,
    pub angle: f32,
    pub records: Vec<FootprintRecord>,
}

impl Obstacle {
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.records
            .iter()
            .map(|record| {
                let Vec2 { x, y } = record.offset;
                (x * x + y * y).sqrt() + record.footprint.reach()
            })
            .fold(0.0f32, f32::max)
    }
}

#[derive(Debug, Default)]
pub struct TemplateIndex {
    entries: Vec<(String, Vec<FootprintRecord>, bool, bool, f32, f32, f32)>,
    pub templates_seen: u32,
    pub excluded_kind: u32,
    pub not_structure: u32,
    pub mobile: u32,
    pub too_small: u32,
    pub no_radius: u32,
}

impl TemplateIndex {
    #[must_use]
    pub fn build(documents: &[Document]) -> Self {
        let mut module_index = mint_sage_modules::TemplateIndex::new();
        for document in documents {
            module_index.insert_document(document);
        }
        let (records, _) = TemplateRecords::build_collecting(&module_index);
        Self::from_records(&records)
    }

    #[must_use]
    pub fn from_records(records: &TemplateRecords) -> Self {
        let mut index = Self::default();
        for record in records.iter() {
            index.templates_seen = index.templates_seen.saturating_add(1);
            if let Some(footprint) = index.classify(record) {
                let is_wall = record.has_kind(WALK_ON_TOP_OF_WALL);
                let is_scaleable = record.has_kind(SCALEABLE_WALL);
                let (major_radius, minor_radius, height) = if is_scaleable {
                    (
                        record.geometry.major_radius.unwrap_or(0.0),
                        record.geometry.minor_radius.unwrap_or(0.0),
                        record.geometry.height.unwrap_or(0.0),
                    )
                } else {
                    (0.0, 0.0, 0.0)
                };
                index.entries.push((
                    record.name.to_ascii_lowercase(),
                    footprint,
                    is_wall,
                    is_scaleable,
                    major_radius,
                    minor_radius,
                    height,
                ));
            }
        }
        index.entries.sort_by(|a, b| a.0.cmp(&b.0));
        index.entries.dedup_by(|a, b| a.0 == b.0);
        index
    }

    fn classify(&mut self, record: &ObjectRecord) -> Option<Vec<FootprintRecord>> {
        if record.has_kind("PROJECTILE")
            || record.has_kind("BRIDGE_TOWER")
            || record.has_kind("DO_NOT_CLASSIFY")
        {
            self.excluded_kind = self.excluded_kind.saturating_add(1);
            return None;
        }
        if !record.has_kind("STRUCTURE") {
            self.not_structure = self.not_structure.saturating_add(1);
            return None;
        }
        if !record.has_kind("IMMOBILE") {
            self.mobile = self.mobile.saturating_add(1);
            return None;
        }
        if record.geometry_is_small == Some(true) {
            self.too_small = self.too_small.saturating_add(1);
            return None;
        }
        let records: Vec<FootprintRecord> = record
            .geometry
            .shown(&[])
            .filter(|piece| piece.offset_z <= MAX_STAMPED_OFFSET_Z)
            .map(convert_piece)
            .filter(|record| record.footprint.reach() > 0.0)
            .collect();
        if records.is_empty() {
            self.no_radius = self.no_radius.saturating_add(1);
            return None;
        }
        Some(records)
    }

    #[must_use]
    pub fn lookup(&self, template_name: &str) -> Option<Vec<FootprintRecord>> {
        let key = template_name.to_ascii_lowercase();
        let at = self
            .entries
            .binary_search_by(|(name, ..)| name.as_str().cmp(key.as_str()))
            .ok()?;
        self.entries.get(at).map(|(_, records, ..)| records.clone())
    }

    #[must_use]
    pub fn is_wall(&self, template_name: &str) -> bool {
        let key = template_name.to_ascii_lowercase();
        self.entries
            .binary_search_by(|(name, ..)| name.as_str().cmp(key.as_str()))
            .ok()
            .and_then(|at| self.entries.get(at))
            .is_some_and(|(_, _, is_wall, ..)| *is_wall)
    }

    #[must_use]
    pub fn is_scaleable(&self, template_name: &str) -> bool {
        let key = template_name.to_ascii_lowercase();
        self.entries
            .binary_search_by(|(name, ..)| name.as_str().cmp(key.as_str()))
            .ok()
            .and_then(|at| self.entries.get(at))
            .is_some_and(|(_, _, _, is_scaleable, ..)| *is_scaleable)
    }

    #[must_use]
    pub fn wall_geometry(&self, template_name: &str) -> Option<(f32, f32, f32)> {
        let key = template_name.to_ascii_lowercase();
        let (_, _, _, is_scaleable, major_radius, minor_radius, height) = self
            .entries
            .binary_search_by(|(name, ..)| name.as_str().cmp(key.as_str()))
            .ok()
            .and_then(|at| self.entries.get(at))?;
        is_scaleable.then_some((*major_radius, *minor_radius, *height))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[must_use]
pub fn resolve(objects: &[Object], templates: &TemplateIndex) -> Vec<Obstacle> {
    let mut obstacles = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        let Some(records) = templates.lookup(&object.template_name) else {
            continue;
        };
        if templates.is_wall(&object.template_name) {
            continue;
        }
        if !object.x.is_finite() || !object.y.is_finite() {
            continue;
        }
        if !object.z.is_finite() || object.z > MAP_XY_FACTOR {
            continue;
        }
        let angle = if object.angle.is_finite() {
            object.angle
        } else {
            0.0
        };
        obstacles.push(Obstacle {
            id: u32::try_from(index).unwrap_or(u32::MAX),
            centre: Vec2::new(object.x, object.y),
            angle,
            records,
        });
    }
    obstacles
}

#[must_use]
pub fn scaleable_ids(objects: &[Object], templates: &TemplateIndex) -> Vec<u32> {
    let mut ids = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        if templates.lookup(&object.template_name).is_none() {
            continue;
        }
        if templates.is_wall(&object.template_name) {
            continue;
        }
        if !object.x.is_finite() || !object.y.is_finite() {
            continue;
        }
        if !object.z.is_finite() || object.z > MAP_XY_FACTOR {
            continue;
        }
        if templates.is_scaleable(&object.template_name) {
            ids.push(u32::try_from(index).unwrap_or(u32::MAX));
        }
    }
    ids
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallGeometry {
    pub id: u32,
    pub centre: Vec2,
    pub angle: f32,
    pub major_radius: f32,
    pub minor_radius: f32,
    pub height: f32,
}

#[must_use]
pub fn scaleable_walls(objects: &[Object], templates: &TemplateIndex) -> Vec<WallGeometry> {
    let mut walls = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        if templates.lookup(&object.template_name).is_none() {
            continue;
        }
        if templates.is_wall(&object.template_name) {
            continue;
        }
        if !object.x.is_finite() || !object.y.is_finite() {
            continue;
        }
        if !object.z.is_finite() || object.z > MAP_XY_FACTOR {
            continue;
        }
        let Some((major_radius, minor_radius, height)) =
            templates.wall_geometry(&object.template_name)
        else {
            continue;
        };
        let angle = if object.angle.is_finite() {
            object.angle
        } else {
            0.0
        };
        walls.push(WallGeometry {
            id: u32::try_from(index).unwrap_or(u32::MAX),
            centre: Vec2::new(object.x, object.y),
            angle,
            major_radius,
            minor_radius,
            height,
        });
    }
    walls
}

pub const MAX_STAMPED_OFFSET_Z: f32 = 10.0;

fn convert_piece(piece: &GeometryPiece) -> FootprintRecord {
    let footprint = match piece.shape {
        PieceShape::Box { major, minor } => Footprint::Box { major, minor },
        PieceShape::Disc { radius } => Footprint::Disc { radius },
    };
    let (x, y) = piece.offset;
    FootprintRecord {
        offset: Vec2::new(x, y),
        footprint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, MemorySources, Reader};

    fn tree(text: &str) -> Vec<Document> {
        let mut sources = MemorySources::new();
        sources.insert("data/ini/test.ini", text.as_bytes().to_vec());
        let table = BlockTable::corpus_path();
        let reader = Reader::new(&sources);
        vec![reader
            .read("data/ini/test.ini", &table)
            .expect("the fixture parses")]
    }

    #[test]
    fn the_retail_rule_keeps_a_wall_and_a_no_collide_tower_and_drops_the_rest() {
        let documents = tree(
            "Object AmonSulWall\n  \
               KindOf = PRELOAD STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
               Geometry = BOX\n  GeometryMajorRadius = 21.0\n  GeometryMinorRadius = 16.0\n\
             End\n\
             Object OutpostTower\n  \
               KindOf = STRUCTURE IMMOBILE NO_COLLIDE\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 30.0\n\
             End\n\
             Object MirkwoodShrub01\n  \
               KindOf = SHRUBBERY SHRUB STRUCTURE IMMOBILE IGNORED_IN_GUI OPTIMIZED_PROP\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 3.2\n  GeometryIsSmall = Yes\n\
             End\n\
             Object EreborEdge\n  KindOf = STRUCTURE IMMOBILE\nEnd\n\
             Object SiegeTower\n  KindOf = STRUCTURE SELECTABLE\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 15.0\n\
             End\n\
             Object MapRock\n  KindOf = IMMOBILE ROCK\n  Geometry = CYLINDER\n  \
               GeometryMajorRadius = 10.0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        assert_eq!(index.templates_seen, 6);
        assert_eq!(index.not_structure, 1, "the rock is not a structure");
        assert_eq!(index.mobile, 1, "the siege tower is not immobile");
        assert_eq!(index.too_small, 1);
        assert_eq!(index.no_radius, 1);
        assert_eq!(index.len(), 2);
        assert_eq!(
            index.lookup("amonsulwall"),
            Some(vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Box {
                    major: 21.0,
                    minor: 16.0
                }
            }])
        );
        assert_eq!(
            index.lookup("OutpostTower"),
            Some(vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Disc { radius: 30.0 }
            }]),
            "NO_COLLIDE is not a test `classifyObjectFootprint` makes"
        );
        assert_eq!(index.lookup("MirkwoodShrub01"), None);
        assert_eq!(index.lookup("EreborEdge"), None);
        assert_eq!(index.lookup("SiegeTower"), None);
        assert_eq!(index.lookup("MapRock"), None);
    }

    #[test]
    fn a_part_raised_past_ten_does_not_stamp() {
        let documents = tree(
            "Object Gatehouse\n  KindOf = STRUCTURE IMMOBILE\n  \
               Geometry = BOX\n  GeometryMajorRadius = 10.0\n  GeometryMinorRadius = 10.0\n  \
               AdditionalGeometry = BOX\n  GeometryMajorRadius = 6.0\n  GeometryMinorRadius = 6.0\n  \
                 GeometryOffset = X:0 Y:0 Z:10\n  \
               AdditionalGeometry = BOX\n  GeometryMajorRadius = 7.0\n  GeometryMinorRadius = 7.0\n  \
                 GeometryOffset = X:0 Y:0 Z:11\n\
             End\n\
             Object Arch\n  KindOf = STRUCTURE IMMOBILE\n  \
               Geometry = BOX\n  GeometryMajorRadius = 10.0\n  GeometryMinorRadius = 10.0\n  \
                 GeometryOffset = X:0 Y:0 Z:100\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let radii: Vec<Footprint> = index
            .lookup("Gatehouse")
            .unwrap()
            .into_iter()
            .map(|record| record.footprint)
            .collect();
        assert_eq!(
            radii,
            [
                Footprint::Box {
                    major: 10.0,
                    minor: 10.0
                },
                Footprint::Box {
                    major: 6.0,
                    minor: 6.0
                }
            ]
        );
        assert_eq!(index.lookup("Arch"), None, "nothing left to stamp");
    }

    #[test]
    fn projectile_bridge_tower_and_do_not_classify_are_excluded_even_as_structures() {
        let documents = tree(
            "Object Catapult\n  KindOf = STRUCTURE IMMOBILE PROJECTILE\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 10.0\n\
             End\n\
             Object Tower\n  KindOf = STRUCTURE IMMOBILE BRIDGE_TOWER\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 10.0\n\
             End\n\
             Object Marker\n  KindOf = STRUCTURE IMMOBILE DO_NOT_CLASSIFY\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 10.0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        assert_eq!(index.excluded_kind, 3);
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn a_cylinder_becomes_a_disc_and_a_one_radius_box_becomes_a_square() {
        let documents = tree(
            "Object Rock\n  KindOf = STRUCTURE IMMOBILE\n  Geometry = CYLINDER\n  \
               GeometryMajorRadius = 38.0\n\
             End\n\
             Object Trebuchet\n  KindOf = STRUCTURE IMMOBILE\n  Geometry = BOX\n  \
               GeometryMajorRadius = 25.0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let disc = |radius| {
            Some(vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Disc { radius },
            }])
        };
        assert_eq!(index.lookup("rock"), disc(38.0));
        assert_eq!(
            index.lookup("trebuchet"),
            Some(vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Box {
                    major: 25.0,
                    minor: 25.0
                }
            }])
        );
    }

    #[test]
    fn a_template_with_additional_geometry_keeps_every_record_and_its_offset() {
        let documents = tree(
            "Object WargSentry\n  KindOf = STRUCTURE IMMOBILE\n  \
               Geometry              = BOX\n  \
               GeometryMajorRadius   = 5.0\n  GeometryMinorRadius = 25.0\n  \
               GeometryOffset        = X:45.0 Y:15.0 Z:0\n  \
               AdditionalGeometry    = BOX\n  \
               GeometryMajorRadius   = 5.0\n  GeometryMinorRadius = 5.0\n  \
               GeometryOffset        = X:0.0 Y:45.0 Z:0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let records = index.lookup("WargSentry").unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].offset, Vec2::new(45.0, 15.0));
        assert_eq!(
            records[0].footprint,
            Footprint::Box {
                major: 5.0,
                minor: 25.0
            }
        );
        assert_eq!(records[1].offset, Vec2::new(0.0, 45.0));
        assert!(
            Obstacle {
                id: 0,
                centre: Vec2::new(0.0, 0.0),
                angle: 0.0,
                records,
            }
            .reach()
                > 45.0
        );
    }

    #[test]
    fn a_record_hidden_at_creation_is_left_out() {
        let documents = tree(
            "Object Range\n  KindOf = STRUCTURE IMMOBILE\n  \
               Geometry = BOX\n  GeometryName = Orig\n  \
               GeometryMajorRadius = 33.0\n  GeometryMinorRadius = 40.0\n  \
               AdditionalGeometry = BOX\n  GeometryName = Orig\n  \
               GeometryMajorRadius = 15.0\n  GeometryOffset = X:10 Y:0 Z:0\n  \
               AdditionalGeometry = BOX\n  GeometryName = V2\n  GeometryActive = No\n  \
               GeometryMajorRadius = 57.5\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let records = index.lookup("Range").unwrap();
        assert_eq!(records.len(), 2, "the hidden V2 record is left out");
        assert!(records.iter().all(|record| record.footprint
            != Footprint::Box {
                major: 57.5,
                minor: 57.5
            }));
    }

    #[test]
    fn a_box_reaches_to_its_corner() {
        let footprint = Footprint::Box {
            major: 3.0,
            minor: 4.0,
        };
        assert!((footprint.reach() - 5.0).abs() < 1e-5);
        assert!((Footprint::Disc { radius: 7.0 }.reach() - 7.0).abs() < 1e-5);
    }

    #[test]
    fn a_placement_with_non_finite_coordinates_is_dropped() {
        let documents = tree(
            "Object Wall\n  KindOf = STRUCTURE IMMOBILE\n  Geometry = BOX\n  \
               GeometryMajorRadius = 10.0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let objects = vec![
            object("Wall", f32::NAN, 0.0, 0.0),
            object("Wall", 0.0, f32::INFINITY, 0.0),
            object("Wall", 10.0, 20.0, f32::NAN),
            object("Wall", 30.0, 40.0, 1.0),
            object("NotAWall", 50.0, 60.0, 0.0),
        ];
        let obstacles = resolve(&objects, &index);
        assert_eq!(obstacles.len(), 2);
        assert_eq!(obstacles.first().map(|o| o.id), Some(2));
        assert_eq!(obstacles.first().map(|o| o.angle), Some(0.0));
        assert_eq!(obstacles.get(1).map(|o| o.id), Some(3));
    }

    #[test]
    fn an_object_more_than_a_cell_above_the_terrain_is_not_stamped() {
        let documents = tree(
            "Object Platform\n  KindOf = STRUCTURE IMMOBILE\n  Geometry = CYLINDER\n  \
               GeometryMajorRadius = 10.0\n\
             End\n",
        );
        let index = TemplateIndex::build(&documents);
        let grounded = Object {
            z: MAP_XY_FACTOR,
            ..object("Platform", 0.0, 0.0, 0.0)
        };
        let airborne = Object {
            z: MAP_XY_FACTOR + 1.0,
            ..object("Platform", 100.0, 100.0, 0.0)
        };
        let obstacles = resolve(&[grounded, airborne], &index);
        assert_eq!(obstacles.len(), 1);
        assert_eq!(
            obstacles.first().map(|o| o.centre),
            Some(Vec2::new(0.0, 0.0))
        );
    }

    fn object(template: &str, x: f32, y: f32, angle: f32) -> Object {
        Object {
            x,
            y,
            z: 0.0,
            angle,
            flags: 0,
            template_name: template.to_owned(),
            properties: mint_sage_map::Dictionary::default(),
        }
    }
}
