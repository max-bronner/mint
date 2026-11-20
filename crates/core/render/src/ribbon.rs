use crate::mesh::{MeshData, Vertex};
use crate::{Blend, DepthTest, Instance, MeshHandle, Renderer, TextureHandle, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RibbonStyle {
    pub width: f32,
    pub tile: f32,
    pub color: [f32; 3],
    pub texture: Option<TextureHandle>,
    pub blend: Blend,
    pub uv_offset: [f32; 2],
    pub depth_test: DepthTest,
}

#[must_use]
pub fn ribbon_mesh(points: &[[f32; 3]], width: f32, tile: f32, eye: [f32; 3]) -> Option<MeshData> {
    if points.len() < 2 || !(width > 0.0 && width.is_finite()) || !tile.is_finite() {
        return None;
    }
    let half = width * 0.5;
    let mut vertices = Vec::with_capacity(points.len().saturating_mul(2));
    for (i, p) in points.iter().enumerate() {
        let before = points.get(i.saturating_sub(1)).unwrap_or(p);
        let after = points.get(i.saturating_add(1)).unwrap_or(p);
        let tangent = Vec3::new(
            after[0] - before[0],
            after[1] - before[1],
            after[2] - before[2],
        );
        let to_eye = Vec3::new(eye[0] - p[0], eye[1] - p[1], eye[2] - p[2]);
        let side = tangent.cross(to_eye).normalize();
        let v = f32::from(u16::try_from(i).unwrap_or(u16::MAX)) * tile;
        let normal = to_eye.normalize().to_array();
        for (sign, u) in [(-1.0f32, 0.0f32), (1.0, 1.0)] {
            vertices.push(Vertex {
                position: [
                    p[0] + side.x * half * sign,
                    p[1] + side.y * half * sign,
                    p[2] + side.z * half * sign,
                ],
                normal,
                uv: [u, v],
                ..Vertex::default()
            });
        }
    }
    let segments = u32::try_from(points.len().saturating_sub(1)).ok()?;
    let mut indices = Vec::new();
    for s in 0..segments {
        let base = s.saturating_mul(2);
        indices.extend_from_slice(&[
            base,
            base.saturating_add(1),
            base.saturating_add(2),
            base.saturating_add(1),
            base.saturating_add(3),
            base.saturating_add(2),
        ]);
    }
    Some(MeshData { vertices, indices })
}

struct Pooled {
    mesh: MeshHandle,
    vertices: usize,
    in_use: bool,
}

#[derive(Default)]
pub struct Ribbons {
    pool: Vec<Pooled>,
}

impl Ribbons {
    pub fn begin(&mut self) {
        for entry in &mut self.pool {
            entry.in_use = false;
        }
    }

    pub fn push(
        &mut self,
        renderer: &mut Renderer,
        points: &[[f32; 3]],
        eye: [f32; 3],
        style: &RibbonStyle,
        out: &mut Vec<Instance>,
    ) -> crate::Result<()> {
        let Some(data) = ribbon_mesh(points, style.width, style.tile, eye) else {
            return Ok(());
        };
        let vertices = data.vertices.len();
        let free = self
            .pool
            .iter_mut()
            .find(|e| !e.in_use && e.vertices == vertices);
        let handle = if let Some(entry) = free {
            entry.in_use = true;
            renderer.update_mesh_vertices(entry.mesh, &data)?;
            entry.mesh
        } else {
            let mesh = renderer.upload_mesh(&data)?;
            self.pool.push(Pooled {
                mesh,
                vertices,
                in_use: true,
            });
            mesh
        };
        let mut instance = Instance::new(handle);
        instance.texture = style.texture;
        instance.tint = [style.color[0], style.color[1], style.color[2], 1.0];
        instance.blend = style.blend;
        instance.uv_offset = style.uv_offset;
        instance.depth_test = style.depth_test;
        instance.unlit = true;
        instance.cull = false;
        out.push(instance);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ribbon_faces_the_eye_and_runs_its_texture_once_per_point() {
        let line = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]];
        let mesh = ribbon_mesh(&line, 6.0, 1.0, [10.0, 0.0, 100.0]).expect("ribbon");
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 12);
        let first = mesh.vertices.first().expect("v0");
        let second = mesh.vertices.get(1).expect("v1");
        assert!((first.position[1].abs() - 3.0).abs() < 1.0e-5);
        assert!((first.position[1] + second.position[1]).abs() < 1.0e-5);
        assert!(first.position[2].abs() < 1.0e-5);
        assert_eq!((first.uv, second.uv), ([0.0, 0.0], [1.0, 0.0]));
        assert_eq!(mesh.vertices.get(4).map(|v| v.uv), Some([0.0, 2.0]));
    }

    #[test]
    fn a_ribbon_needs_two_points_and_a_positive_width() {
        let p = [[0.0; 3], [1.0, 0.0, 0.0]];
        assert!(ribbon_mesh(&p[..1], 1.0, 1.0, [0.0, 0.0, 9.0]).is_none());
        assert!(ribbon_mesh(&p, 0.0, 1.0, [0.0, 0.0, 9.0]).is_none());
        assert!(ribbon_mesh(&p, 1.0, 1.0, [0.0, 0.0, 9.0]).is_some());
    }
}
