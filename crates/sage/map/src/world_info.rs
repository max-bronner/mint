use crate::container::{NameTable, RawChunk};
use crate::dictionary::{read_dictionary, Dictionary};
use crate::reader::Reader;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct WorldInfo {
    pub properties: Dictionary,
}

pub(crate) fn read_world_info(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
) -> Result<WorldInfo> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let properties = read_dictionary(&mut r, names)?;
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: "WorldInfo",
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(WorldInfo { properties })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::MapBuilder;

    #[test]
    fn world_info_dictionary_round_trips() {
        let mut b = MapBuilder::new();
        let entries = vec![b.dict_int("weather", 2), b.dict_str("mapName", "Osgiliath")];
        let payload = b.dict(entries);
        b.chunk("WorldInfo", 1, &payload);
        let body = b.build();

        let (names, chunks) = parse_top_level(&body).expect("valid container");
        let chunk = chunks
            .iter()
            .find(|c| c.name == "WorldInfo")
            .expect("present");
        let info = read_world_info(&body, chunk, &names).expect("valid world info");

        assert_eq!(
            info.properties.get("weather"),
            Some(&crate::dictionary::Value::Int(2))
        );
        assert_eq!(info.properties.len(), 2);
    }
}
