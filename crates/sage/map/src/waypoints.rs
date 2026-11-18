use crate::container::RawChunk;
use crate::objects::Object;
use crate::reader::Reader;
use crate::{Error, Result};

pub const WAYPOINTS_CHUNK: &str = "WaypointsList";

pub const MAX_WAYPOINT_LINKS: usize = 8;

const LINK_BYTES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaypointLink {
    pub from: i32,
    pub to: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Waypoint {
    pub id: i32,
    pub name: String,
    pub position: [f32; 3],
    pub labels: [Option<String>; 3],
    pub bidirectional: bool,
    pub kind: Option<i32>,
    pub kind_option: String,
}

impl Waypoint {
    pub fn from_object(object: &Object) -> Result<Option<Self>> {
        let d = &object.properties;
        let Some(id) = d.get_int(WAYPOINTS_CHUNK, "waypointID")? else {
            return Ok(None);
        };
        let label = |key: &str| {
            d.get_str(WAYPOINTS_CHUNK, key)
                .map(|t| t.filter(|t| !t.is_empty()).map(str::to_string))
        };
        Ok(Some(Self {
            id,
            name: d
                .get_str(WAYPOINTS_CHUNK, "waypointName")?
                .unwrap_or_default()
                .to_string(),
            position: [object.x, object.y, object.z],
            labels: [
                label("waypointPathLabel1")?,
                label("waypointPathLabel2")?,
                label("waypointPathLabel3")?,
            ],
            bidirectional: d
                .get_bool(WAYPOINTS_CHUNK, "waypointPathBiDirectional")?
                .unwrap_or(false),
            kind: d.get_int(WAYPOINTS_CHUNK, "waypointType")?,
            kind_option: d
                .get_str(WAYPOINTS_CHUNK, "waypointTypeOption")?
                .unwrap_or_default()
                .to_string(),
        }))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaypointNode {
    pub links: Vec<usize>,
    pub back_pointer: Option<usize>,
}

#[must_use]
pub fn build_links(waypoints: &[Waypoint], links: &[WaypointLink]) -> Vec<WaypointNode> {
    let mut nodes = vec![WaypointNode::default(); waypoints.len()];
    let find = |id: i32| waypoints.iter().position(|w| w.id == id);
    for link in links {
        let (Some(from), Some(to)) = (find(link.from), find(link.to)) else {
            continue;
        };
        if from == to || has_link(&nodes, from, to) {
            continue;
        }
        add_link(&mut nodes, from, to);
        let reverse = waypoints.get(from).is_some_and(|w| w.bidirectional);
        if reverse && !has_link(&nodes, to, from) {
            add_link(&mut nodes, to, from);
        }
    }
    nodes
}

fn has_link(nodes: &[WaypointNode], from: usize, to: usize) -> bool {
    nodes.get(from).is_some_and(|n| n.links.contains(&to))
}

fn add_link(nodes: &mut [WaypointNode], from: usize, to: usize) {
    if let Some(target) = nodes.get_mut(to) {
        target.back_pointer = Some(from);
    }
    if let Some(source) = nodes.get_mut(from) {
        if source.links.len() < MAX_WAYPOINT_LINKS {
            source.links.push(to);
        }
    }
}

pub(crate) fn read_waypoint_links(body: &[u8], chunk: &RawChunk) -> Result<Vec<WaypointLink>> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let declared = r.u32()?;
    let count = usize::try_from(declared).unwrap_or(usize::MAX);
    if count.saturating_mul(LINK_BYTES) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: WAYPOINTS_CHUNK,
            offset: r.position(),
            count: declared,
            item_size: LINK_BYTES,
            available: r.remaining(),
        });
    }
    let mut links = Vec::new();
    for _ in 0..count {
        links.push(WaypointLink {
            from: r.i32()?,
            to: r.i32()?,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: WAYPOINTS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(links)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn way(id: i32, bidirectional: bool) -> Waypoint {
        Waypoint {
            id,
            name: format!("W{id}"),
            position: [0.0; 3],
            labels: [None, None, None],
            bidirectional,
            kind: None,
            kind_option: String::new(),
        }
    }

    fn link(from: i32, to: i32) -> WaypointLink {
        WaypointLink { from, to }
    }

    fn payload(links: &[WaypointLink]) -> Vec<u8> {
        let mut out = u32::try_from(links.len()).unwrap().to_le_bytes().to_vec();
        for l in links {
            out.extend_from_slice(&l.from.to_le_bytes());
            out.extend_from_slice(&l.to.to_le_bytes());
        }
        out
    }

    fn read(bytes: &[u8]) -> Result<Vec<WaypointLink>> {
        let chunk = RawChunk {
            name: WAYPOINTS_CHUNK.into(),
            version: 1,
            body: 0,
            len: bytes.len(),
        };
        read_waypoint_links(bytes, &chunk)
    }

    #[test]
    fn missing_self_and_repeated_links_are_ignored() {
        let ways = [way(1, false), way(2, false)];
        let nodes = build_links(
            &ways,
            &[link(1, 9), link(9, 1), link(1, 1), link(1, 2), link(1, 2)],
        );
        assert_eq!(nodes[0].links, [1]);
        assert!(nodes[1].links.is_empty());
    }

    #[test]
    fn only_a_bidirectional_source_adds_the_reverse_link() {
        let ways = [way(1, true), way(2, false)];
        let nodes = build_links(&ways, &[link(1, 2)]);
        assert_eq!(nodes[1].links, [0]);
        let nodes = build_links(&[way(1, false), way(2, true)], &[link(1, 2)]);
        assert!(nodes[1].links.is_empty());
    }

    #[test]
    fn the_ninth_link_is_dropped_but_still_sets_the_back_pointer() {
        let mut ways = vec![way(0, false)];
        ways.extend((1..=9).map(|id| way(id, false)));
        let links: Vec<_> = (1..=9).map(|to| link(0, to)).collect();
        let nodes = build_links(&ways, &links);
        assert_eq!(nodes[0].links.len(), MAX_WAYPOINT_LINKS);
        assert_eq!(nodes[9].back_pointer, Some(0));
        assert!(!nodes[0].links.contains(&9));
    }

    #[test]
    fn links_read_back_and_trailing_bytes_are_refused() {
        let links = [link(1, 2), link(-3, 4)];
        let mut bytes = payload(&links);
        assert_eq!(read(&bytes).unwrap(), links);
        bytes.push(0);
        assert!(matches!(read(&bytes), Err(Error::TrailingBytes { .. })));
    }

    #[test]
    fn a_link_count_larger_than_the_bytes_present_is_refused() {
        let mut bytes = payload(&[link(1, 2)]);
        bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read(&bytes),
            Err(Error::CountExceedsPayload { .. })
        ));
    }

    #[test]
    fn truncation_errors_and_corruption_never_panics() {
        let bytes = payload(&[link(1, 2), link(2, 3)]);
        for len in 0..bytes.len() {
            assert!(read(&bytes[..len]).is_err(), "prefix {len}");
        }
        for at in 0..bytes.len() {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = bytes.clone();
                bad[at] ^= flip;
                let _ = read(&bad);
            }
        }
    }
}
