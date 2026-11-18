use super::*;

pub(crate) fn report_offscreen_depth(
    camera: &Camera,
    bounds: &Aabb,
    aspect: f32,
    direction: DepthDirection,
    lift: f32,
) {
    println!(
        "depth = {}  near = {:.4}  far = {:.1}  far/near = {:.0}  lift = {lift}",
        match direction {
            DepthDirection::Standard => "standard-Z",
            DepthDirection::Reversed => "reversed-Z",
        },
        camera.near,
        camera.far,
        camera.far / camera.near.max(f32::MIN_POSITIVE),
    );
    for (label, d) in depth_probes(camera, bounds) {
        println!(
            "  {label:<10} d = {d:>9.1}   ulps: standard {:>9}   reversed {:>9}",
            lift_separation_ulps(camera, DepthDirection::Standard, aspect, d, lift),
            lift_separation_ulps(camera, DepthDirection::Reversed, aspect, d, lift),
        );
    }
}

pub(crate) fn lift_separation_ulps(
    camera: &Camera,
    direction: DepthDirection,
    aspect: f32,
    depth: f32,
    lift: f32,
) -> u64 {
    let projection = camera.projection_with_depth(direction, aspect);
    let at = |d: f32| {
        projection
            .transform_point(Vec3::new(0.0, 0.0, -d))
            .z
            .to_bits()
    };
    i64::from(at(depth)).abs_diff(i64::from(at(depth + lift)))
}

pub(crate) fn depth_probes(camera: &Camera, bounds: &Aabb) -> Vec<(&'static str, f32)> {
    let range = |to: Vec3| {
        let (dx, dy, dz) = (
            to.x - camera.eye.x,
            to.y - camera.eye.y,
            to.z - camera.eye.z,
        );
        (dx * dx + dy * dy + dz * dz).sqrt()
    };
    let mut nearest = f32::INFINITY;
    let mut furthest = 0.0f32;
    if !bounds.is_empty() {
        let (min, max) = (bounds.min(), bounds.max());
        for x in [min.x, max.x] {
            for y in [min.y, max.y] {
                for z in [min.z, max.z] {
                    let d = range(Vec3::new(x, y, z));
                    nearest = nearest.min(d);
                    furthest = furthest.max(d);
                }
            }
        }
    }
    let mut probes = vec![("focus", range(camera.target))];
    if furthest > 0.0 {
        probes.push(("map near", nearest.max(camera.near)));
        probes.push(("map far", furthest));
    }
    probes
}
