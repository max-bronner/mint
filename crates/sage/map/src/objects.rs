use crate::container::{read_chunks, NameTable, RawChunk};
use crate::dictionary::{read_dictionary, Dictionary, Value};
use crate::reader::Reader;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Object {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub angle: f32,
    pub flags: i32,
    pub template_name: String,
    pub properties: Dictionary,
}

fn read_object(body: &[u8], chunk: &RawChunk, names: &NameTable) -> Result<Object> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    let angle = r.f32()?;
    let flags = r.i32()?;
    let template_name = r.ascii_string()?;
    let properties = read_dictionary(&mut r, names)?;

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: "Object",
            offset: r.position(),
            remaining: r.remaining(),
        });
    }

    Ok(Object {
        x,
        y,
        z,
        angle,
        flags,
        template_name,
        properties,
    })
}

pub const WAYPOINT_TEMPLATE: &str = "*Waypoints/Waypoint";

pub const WAYPOINT_NAME_KEY: &str = "waypointName";

pub const ORIGINAL_OWNER_KEY: &str = "originalOwner";

impl Object {
    #[must_use]
    pub fn original_player(&self) -> Option<&str> {
        match self.properties.get(ORIGINAL_OWNER_KEY)? {
            Value::Str(owner) | Value::WideStr(owner) => owner.split('/').next(),
            _ => None,
        }
    }

    #[must_use]
    pub fn waypoint_name(&self) -> Option<&str> {
        if !self.template_name.eq_ignore_ascii_case(WAYPOINT_TEMPLATE) {
            return None;
        }
        match self.properties.get(WAYPOINT_NAME_KEY)? {
            Value::Str(name) | Value::WideStr(name) => Some(name.as_str()),
            _ => None,
        }
    }
}

#[must_use]
pub fn named_waypoint<'a>(objects: &'a [Object], name: &str) -> Option<&'a Object> {
    objects.iter().find(|object| {
        object
            .waypoint_name()
            .is_some_and(|found| found.eq_ignore_ascii_case(name))
    })
}

#[must_use]
pub fn start_waypoint_name(slot: u8) -> String {
    format!("Player_{}_Start", u16::from(slot).saturating_add(1))
}

pub(crate) fn read_objects(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
) -> Result<Vec<Object>> {
    let end = chunk.end()?;
    let children = read_chunks(body, chunk.body, end, names)?;
    let mut out = Vec::new();
    for child in &children {
        if child.name != "Object" {
            return Err(Error::UnexpectedChild {
                parent: "ObjectsList",
                found: child.name.clone(),
            });
        }
        out.push(read_object(body, child, names)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::MapBuilder;

    #[test]
    fn objects_list_round_trips_and_dictionary_survives() {
        let mut b = MapBuilder::new();
        let entries = vec![
            b.dict_str("waypointID", "1"),
            b.dict_bool("objectEnabled", true),
        ];
        let object_payload = b.object_fields(1.0, 2.0, 0.0, 1.5, 0, "*Waypoints/Waypoint", entries);
        let object_bytes = b.nested_chunk("Object", 3, &object_payload);
        b.chunk("ObjectsList", 3, &object_bytes);
        let body = b.build();

        let (names, chunks) = parse_top_level(&body).expect("valid container");
        let list = chunks
            .iter()
            .find(|c| c.name == "ObjectsList")
            .expect("present");
        let objects = read_objects(&body, list, &names).expect("valid objects list");

        assert_eq!(objects.len(), 1);
        let obj = &objects[0];
        assert_eq!(obj.template_name, "*Waypoints/Waypoint");
        assert_eq!(obj.x, 1.0);
        assert_eq!(obj.angle, 1.5);
        assert_eq!(
            obj.properties.get("waypointID"),
            Some(&crate::dictionary::Value::Str("1".to_string()))
        );
    }

    fn placed(template: &str, pairs: Vec<(String, Value)>) -> Object {
        Object {
            x: 10.0,
            y: 20.0,
            z: 0.0,
            angle: 0.0,
            flags: 0,
            template_name: template.to_owned(),
            properties: Dictionary::from_pairs(pairs),
        }
    }

    #[test]
    fn a_start_waypoint_is_found_by_its_one_based_name_and_nothing_else_matches() {
        assert_eq!(start_waypoint_name(0), "Player_1_Start");
        assert_eq!(start_waypoint_name(255), "Player_256_Start");
        let name = |n: &str| vec![(String::from("waypointName"), Value::Str(n.to_owned()))];
        let objects = vec![
            placed("GondorArcher", name("Player_1_Start")),
            placed("*Waypoints/Waypoint", name("Player_2_Start")),
            placed("*Waypoints/Waypoint", name("player_1_start")),
            placed(
                "*Waypoints/Waypoint",
                vec![(String::from("waypointName"), Value::Int(1))],
            ),
        ];
        let found = named_waypoint(&objects, &start_waypoint_name(0)).unwrap();
        assert_eq!(found.waypoint_name(), Some("player_1_start"));
        assert!(named_waypoint(&objects, "Player_3_Start").is_none());
        assert_eq!(objects[3].waypoint_name(), None, "a name must be a string");
    }

    #[test]
    fn empty_objects_list_is_valid() {
        let mut b = MapBuilder::new();
        b.chunk("ObjectsList", 3, &[]);
        let body = b.build();
        let (names, chunks) = parse_top_level(&body).expect("valid container");
        let list = chunks
            .iter()
            .find(|c| c.name == "ObjectsList")
            .expect("present");
        let objects = read_objects(&body, list, &names).expect("valid, empty");
        assert!(objects.is_empty());
    }

    #[test]
    fn non_object_child_is_an_error() {
        let mut b = MapBuilder::new();
        let stray = b.nested_chunk("NotAnObject", 1, &[]);
        b.chunk("ObjectsList", 3, &stray);
        let body = b.build();
        let (names, chunks) = parse_top_level(&body).expect("valid container");
        let list = chunks
            .iter()
            .find(|c| c.name == "ObjectsList")
            .expect("present");
        let err = read_objects(&body, list, &names).unwrap_err();
        assert!(matches!(err, Error::UnexpectedChild { .. }));
    }
}
