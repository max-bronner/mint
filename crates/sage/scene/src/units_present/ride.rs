use mint_render::Mat4;

use super::{Ride, UnitDrawable, UnitPose};

pub(super) fn carry_riders(
    drawables: &mut [UnitDrawable],
    paired: &[Option<usize>],
    poses: &[UnitPose],
) {
    let rides: Vec<(usize, Ride)> = paired
        .iter()
        .enumerate()
        .filter_map(|(at, pose)| Some((at, poses.get((*pose)?)?.ride?)))
        .collect();
    for (at, ride) in rides {
        let motion = drawables
            .iter()
            .find(|drawable| drawable.id == ride.container)
            .and_then(|container| bone_motion(container, usize::from(ride.pivot)));
        if let (Some(motion), Some(rider)) = (motion, drawables.get_mut(at)) {
            rider.placement = motion.mul_mat4(rider.placement);
        }
    }
}

fn bone_motion(container: &UnitDrawable, pivot: usize) -> Option<Mat4> {
    let posed = container.bones().get(pivot).copied()?;
    let rest = container.model.bind_bone(pivot)?;
    let world = container.placement();
    Some(
        world
            .mul_mat4(posed)
            .mul_mat4(affine_inverse(rest)?)
            .mul_mat4(affine_inverse(world)?),
    )
}

fn affine_inverse(m: Mat4) -> Option<Mat4> {
    let a = |row: usize, col: usize| m.get(row, col);
    let cofactor =
        |r0: usize, r1: usize, c0: usize, c1: usize| a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0);
    let adjugate = [
        [
            cofactor(1, 2, 1, 2),
            -cofactor(0, 2, 1, 2),
            cofactor(0, 1, 1, 2),
        ],
        [
            -cofactor(1, 2, 0, 2),
            cofactor(0, 2, 0, 2),
            -cofactor(0, 1, 0, 2),
        ],
        [
            cofactor(1, 2, 0, 1),
            -cofactor(0, 2, 0, 1),
            cofactor(0, 1, 0, 1),
        ],
    ];
    let det = a(0, 0) * adjugate[0][0] + a(0, 1) * adjugate[1][0] + a(0, 2) * adjugate[2][0];
    if !det.is_finite() || det.abs() <= f32::EPSILON {
        return None;
    }
    let linear = adjugate.map(|row| row.map(|value| value / det));
    let translation = [a(0, 3), a(1, 3), a(2, 3)];
    let moved = linear
        .map(|row| -(row[0] * translation[0] + row[1] * translation[1] + row[2] * translation[2]));
    Some(Mat4::from_cols([
        [linear[0][0], linear[1][0], linear[2][0], 0.0],
        [linear[0][1], linear[1][1], linear[2][1], 0.0],
        [linear[0][2], linear[1][2], linear[2][2], 0.0],
        [moved[0], moved[1], moved[2], 1.0],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_render::Vec3;

    #[test]
    fn an_affine_inverse_undoes_its_transform() {
        let turn = Mat4::from_cols([
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let m = Mat4::from_translation(Vec3::new(3.0, -2.0, 7.0))
            .mul_mat4(turn)
            .mul_mat4(Mat4::from_scale(Vec3::splat(2.0)));
        let back = affine_inverse(m).unwrap().mul_mat4(m);
        let point = back.transform_point(Vec3::new(1.5, 4.0, -2.5));
        assert!((point.x - 1.5).abs() < 1e-5, "{point:?}");
        assert!((point.y - 4.0).abs() < 1e-5, "{point:?}");
        assert!((point.z + 2.5).abs() < 1e-5, "{point:?}");
    }
}
