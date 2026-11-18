use crate::array::Element;
use crate::chunk::Chunk;
use crate::material::need;
use crate::reader::Reader;
use crate::{Array, ChunkType, Quat, Result, Vec3};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HierarchyHeader {
    pub version: u32,
    pub name: String,
    pub num_pivots: u32,
    pub center: Vec3,
}

impl HierarchyHeader {
    pub const SIZE: usize = 36;

    fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "HIERARCHY_HEADER")?;
        let mut r = Reader::new(payload);
        Ok(Self {
            version: r.u32()?,
            name: r.fixed_name(16)?,
            num_pivots: r.u32()?,
            center: Vec3::read(&mut r)?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pivot {
    pub name: String,
    pub parent: Option<u32>,
    pub translation: Vec3,
    pub euler_angles: Vec3,
    pub rotation: Quat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct RawPivot {
    name: [u8; 16],
    parent_idx: u32,
    translation: Vec3,
    euler_angles: Vec3,
    rotation: Quat,
}

impl Element for RawPivot {
    const SIZE: usize = 60;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r.array::<16>()?,
            parent_idx: r.u32()?,
            translation: Vec3::read(r)?,
            euler_angles: Vec3::read(r)?,
            rotation: Quat::read(r)?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hierarchy {
    pub header: HierarchyHeader,
    pub pivots: Vec<Pivot>,
}

impl Hierarchy {
    pub fn parse(chunk: &Chunk<'_>) -> Result<Self> {
        let mut out = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::HIERARCHY_HEADER => out.header = HierarchyHeader::parse(kid.payload())?,
                ChunkType::PIVOTS => {
                    let raw: Array<'_, RawPivot> = kid.array("PIVOTS")?;
                    let mut pivots = Vec::with_capacity(raw.len());
                    for p in raw.iter() {
                        let p = p?;
                        pivots.push(Pivot {
                            name: crate::reader::decode_name(&p.name),
                            parent: (p.parent_idx != u32::MAX).then_some(p.parent_idx),
                            translation: p.translation,
                            euler_angles: p.euler_angles,
                            rotation: p.rotation,
                        });
                    }
                    out.pivots = pivots;
                }
                _ => {}
            }
        }
        Ok(out)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pivots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pivots.is_empty()
    }

    #[must_use]
    pub fn counts_agree(&self) -> bool {
        usize::try_from(self.header.num_pivots).is_ok_and(|n| n == self.pivots.len())
    }

    #[must_use]
    pub fn pivot(&self, index: usize) -> Option<&Pivot> {
        self.pivots.get(index)
    }

    pub fn children_of(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let index = u32::try_from(index).ok();
        self.pivots
            .iter()
            .enumerate()
            .filter_map(move |(i, p)| (index.is_some() && p.parent == index).then_some(i))
    }

    #[must_use]
    pub fn root(&self) -> Option<usize> {
        self.pivots.iter().position(|p| p.parent.is_none())
    }

    #[must_use]
    pub fn unusable_parents(&self) -> Vec<usize> {
        (0..self.pivots.len())
            .filter(|&i| !self.parent_chain_terminates(i))
            .collect()
    }

    fn parent_chain_terminates(&self, start: usize) -> bool {
        let mut current = start;
        for _ in 0..=self.pivots.len() {
            let Some(pivot) = self.pivots.get(current) else {
                return false;
            };
            let Some(parent) = pivot.parent else {
                return true;
            };
            let Ok(parent) = usize::try_from(parent) else {
                return false;
            };
            if parent >= self.pivots.len() {
                return false;
            }
            current = parent;
        }
        false
    }

    #[must_use]
    pub fn parents_precede_children(&self) -> bool {
        self.pivots.iter().enumerate().all(|(i, p)| match p.parent {
            None => true,
            Some(parent) => usize::try_from(parent).is_ok_and(|parent| parent < i),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{chunk, concat, hierarchy};
    use crate::Chunks;

    fn parse(bytes: &[u8]) -> Hierarchy {
        let chunk = Chunks::new(bytes)
            .next()
            .and_then(Result::ok)
            .expect("one chunk");
        Hierarchy::parse(&chunk).expect("valid hierarchy")
    }

    #[test]
    fn builds_a_tree() {
        let h = parse(&hierarchy(
            "SKEL",
            &[("ROOT", u32::MAX), ("TORSO", 0), ("L_ARM", 1), ("R_ARM", 1)],
        ));
        assert_eq!(h.header.name, "SKEL");
        assert_eq!(h.len(), 4);
        assert!(h.counts_agree());
        assert_eq!(h.root(), Some(0));
        assert_eq!(h.children_of(1).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(h.pivot(2).map(|p| p.name.as_str()), Some("L_ARM"));
        assert!(h.unusable_parents().is_empty());
        assert!(h.parents_precede_children());
    }

    #[test]
    fn an_out_of_range_parent_is_unusable() {
        let h = parse(&hierarchy("SKEL", &[("ROOT", u32::MAX), ("A", 99)]));
        assert_eq!(h.unusable_parents(), vec![1]);
    }

    #[test]
    fn a_cycle_is_unusable_and_does_not_hang() {
        let h = parse(&hierarchy(
            "SKEL",
            &[("ROOT", u32::MAX), ("A", 2), ("B", 1)],
        ));
        assert_eq!(h.unusable_parents(), vec![1, 2]);
        assert!(!h.parents_precede_children());
    }

    #[test]
    fn a_self_parent_is_unusable() {
        let h = parse(&hierarchy("SKEL", &[("ROOT", u32::MAX), ("A", 1)]));
        assert_eq!(h.unusable_parents(), vec![1]);
    }

    #[test]
    fn forward_references_are_resolvable_but_not_in_order() {
        let h = parse(&hierarchy(
            "SKEL",
            &[("ROOT", u32::MAX), ("CHILD", 2), ("PARENT", 0)],
        ));
        assert!(h.unusable_parents().is_empty());
        assert!(!h.parents_precede_children());
    }

    #[test]
    fn a_lying_pivot_count_does_not_size_the_allocation() {
        let mut bytes = hierarchy("SKEL", &[("ROOT", u32::MAX)]);
        let offset = 8 + 8 + 20;
        bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let h = parse(&bytes);
        assert_eq!(h.len(), 1);
        assert!(!h.counts_agree());
    }

    #[test]
    fn a_ragged_pivot_array_is_an_error() {
        let body = concat(&[
            chunk(ChunkType::HIERARCHY_HEADER, false, &[0u8; 36]),
            chunk(ChunkType::PIVOTS, false, &[0u8; 59]),
        ]);
        let file = chunk(ChunkType::HIERARCHY, true, &body);
        let top = Chunks::new(&file)
            .next()
            .and_then(Result::ok)
            .expect("outer chunk parses");
        assert!(matches!(
            Hierarchy::parse(&top),
            Err(crate::Error::RaggedArray {
                what: "PIVOTS",
                stride: 60,
                actual: 59
            })
        ));
    }
}
