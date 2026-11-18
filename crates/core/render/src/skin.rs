use crate::math::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct SkinVertex {
    pub bones: [u16; 2],
    pub weights: [f32; 2],
    pub position2: [f32; 3],
    pub normal2: [f32; 3],
}

impl SkinVertex {
    pub const SIZE: usize = 36;

    #[must_use]
    pub fn bone_span(&self) -> u32 {
        let mut span = 0u32;
        for (bone, weight) in self.bones.iter().zip(self.weights) {
            if weight > 0.0 {
                span = span.max(u32::from(*bone).saturating_add(1));
            }
        }
        span
    }
}

#[must_use]
pub(crate) fn skin_bytes(skin: &[SkinVertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(skin.len().saturating_mul(SkinVertex::SIZE));
    for vertex in skin {
        for bone in vertex.bones {
            out.extend_from_slice(&bone.to_le_bytes());
        }
        for value in vertex
            .weights
            .iter()
            .chain(&vertex.position2)
            .chain(&vertex.normal2)
        {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

pub(crate) const BONE_SIZE: usize = 48;

pub(crate) fn write_bone(bone: &Mat4, out: &mut Vec<u8>) {
    out.extend_from_slice(&packed_bone(bone));
}

pub(crate) fn packed_bone(bone: &Mat4) -> [u8; BONE_SIZE] {
    let cols = bone.to_cols();
    let mut packed = [0u8; BONE_SIZE];
    for (row, texels) in packed.chunks_exact_mut(16).enumerate() {
        for (column, dst) in texels.chunks_exact_mut(4).enumerate() {
            let value = cols
                .get(column)
                .and_then(|col| col.get(row))
                .copied()
                .unwrap_or(0.0);
            dst.copy_from_slice(&value.to_le_bytes());
        }
    }
    packed
}

#[must_use]
pub fn skin_point(
    bones: &[Mat4],
    position: Vec3,
    normal: Vec3,
    tangent: Vec3,
    skin: &SkinVertex,
) -> (Vec3, Vec3, Vec3) {
    let position2 = Vec3::from_array(skin.position2);
    let normal2 = Vec3::from_array(skin.normal2);
    let mut moved = Vec3::ZERO;
    let mut turned = Vec3::ZERO;
    let mut swung = Vec3::ZERO;
    let mut applied = 0.0f32;
    for ((bone, weight), (source, source_normal)) in skin
        .bones
        .iter()
        .zip(skin.weights)
        .zip([(position, normal), (position2, normal2)])
    {
        if weight <= 0.0 {
            continue;
        }
        let Some(matrix) = bones.get(usize::from(*bone)) else {
            continue;
        };
        let p = matrix.transform_point(source);
        let n = matrix.transform_vector(source_normal);
        let t = matrix.transform_vector(tangent);
        moved = Vec3::new(
            moved.x + p.x * weight,
            moved.y + p.y * weight,
            moved.z + p.z * weight,
        );
        turned = Vec3::new(
            turned.x + n.x * weight,
            turned.y + n.y * weight,
            turned.z + n.z * weight,
        );
        swung = Vec3::new(
            swung.x + t.x * weight,
            swung.y + t.y * weight,
            swung.z + t.z * weight,
        );
        applied += weight;
    }
    if applied > 0.0 {
        (
            Vec3::new(moved.x / applied, moved.y / applied, moved.z / applied),
            turned.normalize(),
            swung.normalize(),
        )
    } else {
        (position, normal, tangent)
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_skin_stride_matches_the_encoding() {
        let bytes = skin_bytes(&[SkinVertex::default(); 3]);
        assert_eq!(bytes.len(), 3 * SkinVertex::SIZE);
        assert_eq!(std::mem::size_of::<SkinVertex>(), SkinVertex::SIZE);
    }

    #[test]
    fn the_bone_span_counts_only_weighted_bones() {
        let skin = SkinVertex {
            bones: [3, 9],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        };
        assert_eq!(skin.bone_span(), 4);
        let both = SkinVertex {
            weights: [0.5, 0.5],
            ..skin
        };
        assert_eq!(both.bone_span(), 10);
        assert_eq!(SkinVertex::default().bone_span(), 0);
    }

    #[test]
    fn each_bone_transforms_its_own_copy_of_the_vertex() {
        let bones = [
            Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(0.0, 0.0, 20.0)),
        ];
        let skin = SkinVertex {
            bones: [0, 1],
            weights: [0.5, 0.5],
            position2: [0.0, 4.0, 0.0],
            normal2: [0.0, 0.0, 1.0],
        };
        let (p, n, _) = skin_point(
            &bones,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::ZERO,
            &skin,
        );
        assert_eq!(p.to_array(), [6.0, 2.0, 10.0]);
        assert_eq!(n.to_array(), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_vertex_with_nothing_to_apply_is_left_alone() {
        let stored = Vec3::new(1.0, 2.0, 3.0);
        let normal = Vec3::new(0.0, 1.0, 0.0);
        let none = SkinVertex::default();
        assert_eq!(
            skin_point(&[Mat4::IDENTITY], stored, normal, Vec3::ZERO, &none).0,
            stored
        );
        let past = SkinVertex {
            bones: [5, 0],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        };
        assert_eq!(
            skin_point(&[Mat4::IDENTITY], stored, normal, Vec3::ZERO, &past).0,
            stored
        );
        let posed = SkinVertex {
            bones: [0, 0],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        };
        let (_, _, tangent) = skin_point(&[Mat4::IDENTITY], stored, normal, Vec3::ZERO, &posed);
        assert_eq!(tangent, Vec3::ZERO);
    }

    #[test]
    fn a_bone_is_stored_as_its_top_three_rows() {
        let bone = Mat4::from_translation(Vec3::new(7.0, 8.0, 9.0));
        let mut out = Vec::new();
        write_bone(&bone, &mut out);
        assert_eq!(out.len(), BONE_SIZE);
        assert_eq!(&out[12..16], &7.0f32.to_le_bytes());
        assert_eq!(&out[28..32], &8.0f32.to_le_bytes());
        assert_eq!(&out[44..48], &9.0f32.to_le_bytes());
    }
}
