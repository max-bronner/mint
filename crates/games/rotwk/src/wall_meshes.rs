use mint_render::Mat4;
use mint_sage_asset::Resolver;
use mint_sage_model::convert::pivot_world_transforms;
use mint_sage_model::ObjectIndex;
use mint_sage_w3d::{Model, Triangle, Vec3};
use mint_vfs::Vfs;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WallBounds {
    pub(crate) perimeter: Vec<(f32, f32)>,
    pub(crate) height: f32,
}

impl WallBounds {
    pub(crate) fn placed(
        &self,
        centre: mint_movement::Vec2,
        angle: f32,
    ) -> crate::wall_layer::Polygon {
        let (sin, cos) = mint_math::f32::sin_cos(angle);
        let perimeter = self
            .perimeter
            .iter()
            .map(|&(x, y)| {
                mint_movement::Vec2::new(centre.x + x * cos - y * sin, centre.y + x * sin + y * cos)
            })
            .collect();
        crate::wall_layer::Polygon {
            perimeter,
            height: self.height,
        }
    }
}

pub(crate) fn wall_bounds(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
) -> Option<WallBounds> {
    let (positions, triangles, transform) = named_mesh(vfs, resolver, index, template, |module| {
        module.wall_bounds_mesh
    })?;
    bounds_of(&positions, &triangles, transform)
}

pub(crate) fn ramp_mesh_1(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
) -> Option<RampMesh> {
    let (positions, triangles, transform) =
        named_mesh(vfs, resolver, index, template, |module| module.ramp_mesh_1)?;
    ramp_of(&positions, &triangles, transform)
}

pub(crate) fn ramp_mesh_2(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
) -> Option<RampMesh> {
    let (positions, triangles, transform) =
        named_mesh(vfs, resolver, index, template, |module| module.ramp_mesh_2)?;
    ramp_of(&positions, &triangles, transform)
}

pub(crate) fn has_raised_wall_mesh(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
) -> bool {
    named_mesh(vfs, resolver, index, template, |module| {
        module.raised_wall_mesh
    })
    .is_some()
}

fn named_mesh(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
    field: for<'a> fn(mint_sage_ini::DrawModuleView<'a>) -> Option<&'a str>,
) -> Option<(Vec<Vec3>, Vec<Triangle>, Mat4)> {
    let mesh_name = index
        .draw_modules(template)
        .into_iter()
        .find_map(|module| field(module).map(str::to_owned))?;
    let drawn = index.lookup(template);
    let model_name = drawn.model()?;
    let loaded = mint_sage_model::load_model(vfs, resolver, model_name).ok()?;
    let bytes = vfs.read_path(&loaded.path).ok()?;
    let raw = Model::parse(&bytes).ok()?;
    let bones = loaded
        .hierarchy
        .as_ref()
        .map(pivot_world_transforms)
        .unwrap_or_default();
    let mesh = raw
        .meshes
        .iter()
        .find(|mesh| mesh.name().eq_ignore_ascii_case(&mesh_name))?;
    let transform = bone_binding(&raw, &mesh_name, &bones);
    let positions = mesh.positions.to_vec().ok()?;
    let triangles = mesh.triangles.to_vec().ok()?;
    Some((positions, triangles, transform))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RampMesh {
    pub(crate) from_left: (f32, f32, f32),
    pub(crate) from_right: (f32, f32, f32),
    pub(crate) to_left: (f32, f32, f32),
    pub(crate) to_right: (f32, f32, f32),
}

impl RampMesh {
    pub(crate) fn placed(
        &self,
        centre: mint_movement::Vec2,
        angle: f32,
        ground: f32,
    ) -> crate::wall_layer::Ramp {
        let (sin, cos) = mint_math::f32::sin_cos(angle);
        let place = |(x, y, z): (f32, f32, f32)| {
            (
                centre.x + x * cos - y * sin,
                centre.y + x * sin + y * cos,
                z + ground,
            )
        };
        crate::wall_layer::Ramp {
            from_left: place(self.from_left),
            from_right: place(self.from_right),
            to_left: place(self.to_left),
            to_right: place(self.to_right),
        }
    }
}

fn bone_binding(model: &Model<'_>, mesh_name: &str, bones: &[Mat4]) -> Mat4 {
    for hlod in &model.hlods {
        let Some(lod) = hlod.finest_lod() else {
            continue;
        };
        for sub in &lod.sub_objects {
            let leaf = sub.name.rsplit('.').next().unwrap_or(&sub.name);
            if !(sub.name.eq_ignore_ascii_case(mesh_name) || leaf.eq_ignore_ascii_case(mesh_name)) {
                continue;
            }
            let Ok(index) = usize::try_from(sub.bone_index) else {
                return Mat4::IDENTITY;
            };
            let index = if index < bones.len() { index } else { 0 };
            return bones.get(index).copied().unwrap_or(Mat4::IDENTITY);
        }
    }
    Mat4::IDENTITY
}

fn bounds_of(positions: &[Vec3], triangles: &[Triangle], transform: Mat4) -> Option<WallBounds> {
    let loop_indices = perimeter_loop(positions, triangles)?;
    let mut perimeter = Vec::with_capacity(loop_indices.len());
    let mut height = f32::NAN;
    for index in loop_indices {
        let point = positions.get(index)?;
        let world = transform.transform_point(mint_render::Vec3 {
            x: point.x,
            y: point.y,
            z: point.z,
        });
        perimeter.push((world.x, world.y));
        height = world.z;
    }
    if !height.is_finite() {
        return None;
    }
    Some(WallBounds { perimeter, height })
}

fn ramp_of(positions: &[Vec3], triangles: &[Triangle], transform: Mat4) -> Option<RampMesh> {
    let loop_indices = perimeter_loop(positions, triangles)?;
    if loop_indices.len() != 4 {
        return None;
    }
    let corners: Vec<mint_render::Vec3> = loop_indices
        .iter()
        .map(|&index| {
            let point = positions.get(index)?;
            Some(transform.transform_point(mint_render::Vec3 {
                x: point.x,
                y: point.y,
                z: point.z,
            }))
        })
        .collect::<Option<_>>()?;
    let edges = [(0usize, 1usize), (1, 2), (2, 3), (3, 0)];
    let edge_z =
        |(a, b): (usize, usize)| -> Option<f32> { Some(corners.get(a)?.z + corners.get(b)?.z) };
    let (low, _) = edges
        .iter()
        .copied()
        .filter_map(|edge| Some((edge, edge_z(edge)?)))
        .min_by(|(_, a), (_, b)| a.total_cmp(b))?;
    let (high, _) = edges
        .iter()
        .copied()
        .filter_map(|edge| Some((edge, edge_z(edge)?)))
        .max_by(|(_, a), (_, b)| a.total_cmp(b))?;
    if low == high || low.0 == high.0 || low.0 == high.1 || low.1 == high.0 || low.1 == high.1 {
        return None;
    }
    let at = |index: usize| -> Option<(f32, f32, f32)> {
        let point = corners.get(index)?;
        Some((point.x, point.y, point.z))
    };
    let to_right = (low.1 + 1) % 4;
    let to_left = (low.0 + 3) % 4;
    if !((high.0 == to_right && high.1 == to_left) || (high.1 == to_right && high.0 == to_left)) {
        return None;
    }
    Some(RampMesh {
        from_left: at(low.0)?,
        from_right: at(low.1)?,
        to_left: at(to_left)?,
        to_right: at(to_right)?,
    })
}

fn perimeter_loop(positions: &[Vec3], triangles: &[Triangle]) -> Option<Vec<usize>> {
    use std::collections::BTreeMap;

    let vertex_count = positions.len();
    let mut canon: Vec<usize> = Vec::with_capacity(vertex_count);
    let mut by_bits: BTreeMap<(u32, u32, u32), usize> = BTreeMap::new();
    for (index, point) in positions.iter().enumerate() {
        let key = (point.x.to_bits(), point.y.to_bits(), point.z.to_bits());
        canon.push(*by_bits.entry(key).or_insert(index));
    }

    let mut edge_counts: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for triangle in triangles {
        let [a, b, c] = triangle.indices;
        for (x, y) in [(a, b), (b, c), (c, a)] {
            let (x, y) = (
                *canon.get(usize::try_from(x).ok()?)?,
                *canon.get(usize::try_from(y).ok()?)?,
            );
            let key = if x <= y { (x, y) } else { (y, x) };
            let key = (u32::try_from(key.0).ok()?, u32::try_from(key.1).ok()?);
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
    let mut edge_total = 0usize;
    for ((a, b), count) in &edge_counts {
        if *count != 1 {
            continue;
        }
        let (a, b) = (usize::try_from(*a).ok()?, usize::try_from(*b).ok()?);
        if a >= vertex_count || b >= vertex_count {
            return None;
        }
        adjacency.get_mut(a)?.push(b);
        adjacency.get_mut(b)?.push(a);
        edge_total += 1;
    }
    if edge_total == 0 {
        return None;
    }
    if adjacency
        .iter()
        .any(|edges| !edges.is_empty() && edges.len() != 2)
    {
        return None;
    }
    let start = adjacency.iter().position(|edges| edges.len() == 2)?;
    let mut order = vec![start];
    let mut previous = start;
    let mut current = *adjacency.get(start)?.first()?;
    loop {
        if current == start {
            break;
        }
        order.push(current);
        let next = adjacency
            .get(current)?
            .iter()
            .copied()
            .find(|&candidate| candidate != previous)?;
        previous = current;
        current = next;
    }
    if order.len() != edge_total {
        return None;
    }
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(a: u32, b: u32, c: u32) -> Triangle {
        Triangle {
            indices: [a, b, c],
            ..Triangle::default()
        }
    }

    fn distinct_positions(n: u32) -> Vec<Vec3> {
        (0..n).map(|i| vec3(i as f32, 0.0, 0.0)).collect()
    }

    #[test]
    fn a_quads_perimeter_excludes_its_shared_diagonal() {
        let triangles = [triangle(0, 1, 2), triangle(0, 2, 3)];
        let order = perimeter_loop(&distinct_positions(4), &triangles).unwrap();
        assert_eq!(order.len(), 4);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3]);
    }

    #[test]
    fn two_disjoint_triangles_are_not_one_perimeter() {
        let triangles = [triangle(0, 1, 2), triangle(3, 4, 5)];
        assert!(perimeter_loop(&distinct_positions(6), &triangles).is_none());
    }

    #[test]
    fn a_quads_perimeter_closes_when_shared_corners_are_unwelded_duplicates() {
        let positions = [
            vec3(0.0, 0.0, 0.0),
            vec3(10.0, 0.0, 0.0),
            vec3(10.0, 10.0, 0.0),
            vec3(10.0, 10.0, 0.0),
            vec3(0.0, 10.0, 0.0),
            vec3(0.0, 0.0, 0.0),
        ];
        let triangles = [triangle(0, 1, 2), triangle(3, 4, 5)];
        let order = perimeter_loop(&positions, &triangles).unwrap();
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn bounds_of_reports_the_last_perimeter_vertex() {
        let positions = [
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            Vec3 {
                x: 10.0,
                y: 0.0,
                z: 5.0,
            },
            Vec3 {
                x: 10.0,
                y: 10.0,
                z: 2.0,
            },
            Vec3 {
                x: 0.0,
                y: 10.0,
                z: 3.0,
            },
        ];
        let triangles = [triangle(0, 1, 2), triangle(0, 2, 3)];
        let bounds = bounds_of(&positions, &triangles, Mat4::IDENTITY).unwrap();
        assert_eq!(bounds.height, 3.0, "last visited, index 3, not the max");
        assert_eq!(bounds.perimeter.len(), 4);
    }

    fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn ramp_of_sorts_a_clean_quad_into_ground_and_top_ends() {
        let positions = [
            vec3(0.0, 0.0, 0.0),
            vec3(10.0, 0.0, 0.0),
            vec3(10.0, 20.0, 15.0),
            vec3(0.0, 20.0, 15.0),
        ];
        let triangles = [triangle(0, 1, 2), triangle(0, 2, 3)];
        let ramp = ramp_of(&positions, &triangles, Mat4::IDENTITY).unwrap();
        assert_eq!(ramp.from_left, (0.0, 0.0, 0.0));
        assert_eq!(ramp.from_right, (10.0, 0.0, 0.0));
        assert_eq!(ramp.to_right, (10.0, 20.0, 15.0));
        assert_eq!(ramp.to_left, (0.0, 20.0, 15.0));
    }

    #[test]
    #[ignore = "needs a retail installation; set MINT_CORPUS2 (BFME2) and MINT_CORPUS (ROTWK)"]
    fn wall_bounds_resolves_most_of_the_corpus() {
        let Some(rotwk) = std::env::var_os("MINT_CORPUS").map(std::path::PathBuf::from) else {
            println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
            return;
        };
        let bfme2 = std::env::var_os("MINT_CORPUS2").map(std::path::PathBuf::from);
        let mut vfs = Vfs::new();
        for root in [bfme2, Some(rotwk)].into_iter().flatten() {
            let mut stack = vec![root.clone()];
            while let Some(dir) = stack.pop() {
                let Ok(listing) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in listing.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        if let Ok(archive) = mint_sage_big::BigArchive::open(&path) {
                            vfs.mount(std::sync::Arc::new(archive));
                        }
                    }
                }
            }
            if let Ok(mount) = mint_vfs::DirMount::with_prefix(root.join("data"), "data") {
                vfs.mount(std::sync::Arc::new(mount));
            }
        }
        let (index, _report) = mint_sage_model::load_object_index(&vfs);
        let resolver = Resolver::new();
        let names: Vec<String> = index.names().map(str::to_owned).collect();
        let mut hits = 0usize;
        let mut resolved = 0usize;
        for name in &names {
            if index
                .draw_modules(name)
                .iter()
                .any(|module| module.wall_bounds_mesh.is_some())
            {
                hits += 1;
                if wall_bounds(&vfs, &resolver, &index, name).is_some() {
                    resolved += 1;
                } else {
                    println!("  no bounds: {name}");
                }
            }
        }
        println!("wall bounds: {resolved} of {hits} template(s) with a WallBoundsMesh resolved");
        assert!(
            resolved * 100 >= hits.saturating_mul(80),
            "only {resolved} of {hits} resolved - perimeter welding may have regressed"
        );
    }

    #[test]
    fn ramp_of_refuses_a_mesh_that_is_not_a_quad() {
        let positions = [
            vec3(0.0, 0.0, 0.0),
            vec3(10.0, 0.0, 0.0),
            vec3(5.0, 10.0, 10.0),
        ];
        let triangles = [triangle(0, 1, 2)];
        assert!(ramp_of(&positions, &triangles, Mat4::IDENTITY).is_none());
    }
}
