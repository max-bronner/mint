use super::*;

pub(crate) fn frame_terrain_camera(
    terrain: &Terrain,
    aspect: f32,
    pitch: f32,
    zoom: f32,
) -> OrbitCamera {
    let mut camera = OrbitCamera::new(Vec3::Z);
    camera.frame(&terrain.playable_bounds(), aspect);
    camera.pitch = pitch;
    camera.yaw = 0.6;
    camera.distance /= zoom;
    camera.min_distance = MIN_CAMERA_DISTANCE.min(camera.distance);
    camera
}

pub(crate) fn pan_limit(terrain: &Terrain) -> Aabb {
    let bounds = terrain.bounds();
    if bounds.is_empty() {
        return bounds;
    }
    let (min, max) = (bounds.min(), bounds.max());
    let margin = (max.x - min.x).max(max.y - min.y) * PAN_MARGIN;
    Aabb::from_corners(
        Vec3::new(min.x - margin, min.y - margin, min.z),
        Vec3::new(max.x + margin, max.y + margin, max.z),
    )
}

pub(crate) fn pan_camera(camera: &mut OrbitCamera, forward: f32, right: f32, limit: &Aabb) {
    if forward == 0.0 && right == 0.0 {
        return;
    }
    let eye = camera.eye();
    let facing = Vec3::new(camera.target.x - eye.x, camera.target.y - eye.y, 0.0);
    let length = facing.length();
    if !length.is_finite() || length <= 1e-4 {
        return;
    }
    let (fx, fy) = (facing.x / length, facing.y / length);
    let (rx, ry) = (fy, -fx);

    let step = (camera.distance * PAN_PER_SECOND).max(MIN_PAN_PER_SECOND);
    let mut target = Vec3::new(
        camera.target.x + (fx * forward + rx * right) * step,
        camera.target.y + (fy * forward + ry * right) * step,
        camera.target.z,
    );
    if !limit.is_empty() {
        target = Vec3::new(
            target.x.clamp(limit.min().x, limit.max().x),
            target.y.clamp(limit.min().y, limit.max().y),
            target.z,
        );
    }
    if target.x.is_finite() && target.y.is_finite() {
        camera.target = target;
    }
}
