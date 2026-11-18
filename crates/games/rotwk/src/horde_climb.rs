use mint_movement::Vec2;
use mint_sage_passability::WallGeometry;

const GAP_CEILING: f32 = 90.0;
const WAIT_SPEED_CAP: f32 = 6.0;
const WAIT_DECAY: f32 = 0.9;
const WAIT_SLOWING: f32 = 0.01;
const AIRBORNE: f32 = 0.01;
const FAR_RADII: f32 = 2.0;
const STEP_CAP: f32 = 10.0;
const FAR_STEP_FLOOR: f32 = 9.0;
const FAR_RISE: f32 = 0.8;
const NEAR_RISE: f32 = 0.6;
const BANNER_SCALE: f32 = 0.8;
const SPEED_SHARE: f32 = 0.7;
const SPEED_CAP: f32 = 7.0;
const DISTANCE_RANGE: (f32, f32) = (1.0, 200.0);
const DISTANCE_FLOOR_SHARE: f32 = 0.2;
const DISTANCE_SLOPE: f32 = 0.8 * 0.005;
const PLACEMENT_EPSILON: f32 = 0.5;

pub(crate) struct ClimbInput<'a> {
    pub(crate) member: Vec2,
    pub(crate) member_z: f32,
    pub(crate) member_height: f32,
    pub(crate) ground: f32,
    pub(crate) slot: Vec2,
    pub(crate) horde_speed: f32,
    pub(crate) horde_radius: f32,
    pub(crate) horde_distance: f32,
    pub(crate) banner: bool,
    pub(crate) climbing_or_rappelling: bool,
    pub(crate) wall: &'a WallGeometry,
    pub(crate) wall_ground: f32,
    pub(crate) wall_under: &'a dyn Fn(Vec2) -> bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClimbPoint {
    pub(crate) position: Vec2,
    pub(crate) z: f32,
    pub(crate) gap: f32,
}

fn to_wall_frame(wall: &WallGeometry, point: Vec2) -> Vec2 {
    let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
    let (dx, dy) = (point.x - wall.centre.x, point.y - wall.centre.y);
    Vec2::new(dx * cos + dy * sin, -dx * sin + dy * cos)
}

fn from_wall_frame(wall: &WallGeometry, local: Vec2) -> Vec2 {
    let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
    Vec2::new(
        wall.centre.x + local.x * cos - local.y * sin,
        wall.centre.y + local.x * sin + local.y * cos,
    )
}

pub(crate) fn climb_point(input: &ClimbInput<'_>) -> ClimbPoint {
    let offset = input.slot.minus(input.member);
    let gap = offset.length();
    let direction = if gap > 0.0 {
        offset.scale(1.0 / gap)
    } else {
        Vec2::ZERO
    };
    let far = gap > FAR_RADII * input.horde_radius;
    let speed = (SPEED_SHARE * input.horde_speed.min(gap)).min(SPEED_CAP);
    let distance = input
        .horde_distance
        .clamp(DISTANCE_RANGE.0, DISTANCE_RANGE.1);
    let mut step = (speed
        * (DISTANCE_FLOOR_SHARE + DISTANCE_SLOPE * (DISTANCE_RANGE.1 - distance)))
        .min(STEP_CAP);
    if input.banner {
        step *= BANNER_SCALE;
    }
    if far {
        step = step.max(FAR_STEP_FLOOR);
    }
    let rise = if far { FAR_RISE } else { NEAR_RISE } * step;
    let ahead = input.member.plus(direction.scale(step));
    let answer = |position: Vec2, z: f32| ClimbPoint { position, z, gap };
    let busy = input.climbing_or_rappelling || input.member_z > input.ground + AIRBORNE;
    let midpoint = |point: Vec2| {
        Vec2::new(
            (input.member.x + point.x) * 0.5,
            (input.member.y + point.y) * 0.5,
        )
    };
    let descend = || {
        if input.member_z > input.ground {
            answer(input.member, (input.member_z - rise).max(input.ground))
        } else {
            answer(ahead, input.member_z)
        }
    };

    let mut point = ahead;
    if !(input.wall_under)(point) {
        if busy {
            return descend();
        }
        let halfway = midpoint(point);
        if !(input.wall_under)(halfway) {
            return descend();
        }
        point = halfway;
    }

    let top = input.wall_ground + input.wall.height;
    let fits = |at: Vec2| {
        to_wall_frame(input.wall, at).x.abs() <= input.wall.major_radius && input.member_z <= top
    };
    if !fits(point) {
        let halfway = midpoint(point);
        if busy || !fits(halfway) {
            return answer(point, input.member_z);
        }
        point = halfway;
    }
    if input.member_z >= top {
        return answer(point, input.member_z);
    }
    let mut local = to_wall_frame(input.wall, input.member);
    local.x = local
        .x
        .clamp(-input.wall.major_radius, input.wall.major_radius);
    let held = from_wall_frame(input.wall, local);
    let lifted = input.wall_ground + 0.5 * input.member_height;
    let z = if lifted > input.member_z {
        lifted
    } else {
        input.member_z + rise
    };
    answer(held, z.min(top))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) horizontal: bool,
    pub(crate) rising: bool,
    pub(crate) descending: bool,
}

pub(crate) fn placement(from: Vec2, from_z: f32, point: &ClimbPoint) -> Placement {
    let horizontal = point.position.minus(from).length() > PLACEMENT_EPSILON;
    let rise = point.z - from_z;
    Placement {
        horizontal,
        rising: !horizontal && rise > PLACEMENT_EPSILON,
        descending: !horizontal && rise < -PLACEMENT_EPSILON,
    }
}

pub(crate) fn waits_for_climbers(speed: f32, radius: f32, gap: f32) -> (f32, f32) {
    if gap > radius {
        (
            WAIT_SPEED_CAP.min(speed * (1.0 - WAIT_SLOWING * gap)),
            gap * WAIT_DECAY,
        )
    } else {
        (speed, gap)
    }
}

pub(crate) fn record_gap(recorded: f32, gap: f32) -> f32 {
    recorded.max(gap).min(GAP_CEILING)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALL: WallGeometry = WallGeometry {
        id: 0,
        centre: Vec2::new(100.0, 50.0),
        angle: 0.0,
        major_radius: 40.0,
        minor_radius: 10.0,
        height: 30.0,
    };

    fn on_wall(point: Vec2) -> bool {
        (point.x - 100.0).abs() <= 40.0 && (point.y - 50.0).abs() <= 10.0
    }

    fn input(member: Vec2, member_z: f32, ground: f32) -> ClimbInput<'static> {
        ClimbInput {
            member,
            member_z,
            member_height: 16.0,
            ground,
            slot: Vec2::new(member.x, 80.0),
            horde_speed: 6.0,
            horde_radius: 30.0,
            horde_distance: 100.0,
            banner: false,
            climbing_or_rappelling: false,
            wall: &WALL,
            wall_ground: 0.0,
            wall_under: &on_wall,
        }
    }

    #[test]
    fn a_climber_is_lifted_to_half_its_height_then_rises_by_the_step_with_x_and_y_fixed() {
        let start = Vec2::new(100.0, 38.0);
        let first = climb_point(&input(start, 0.0, 0.0));
        assert_eq!((first.position, first.z), (start, 8.0));
        let second = climb_point(&input(start, 8.0, 0.0));
        let step = 4.2 * (0.2 + 0.8 * 0.005 * 100.0);
        assert_eq!(second.position, start);
        assert!((second.z - (8.0 + 0.6 * step)).abs() < 1e-4, "{}", second.z);
    }

    #[test]
    fn a_climber_off_the_wall_above_the_ground_descends_in_place() {
        let start = Vec2::new(100.0, 10.0);
        let point = climb_point(&input(start, 12.0, 0.0));
        assert_eq!(point.position, start);
        assert!(point.z < 12.0 && point.z >= 0.0, "{}", point.z);
        let moved = placement(start, 12.0, &point);
        assert!(moved.descending && !moved.rising && !moved.horizontal);
    }

    #[test]
    fn a_horde_waits_for_climbers_trailing_by_more_than_its_radius_and_the_gap_decays() {
        assert_eq!(waits_for_climbers(10.0, 30.0, 50.0), (5.0, 45.0));
        assert_eq!(waits_for_climbers(20.0, 30.0, 40.0).0, 6.0);
        assert_eq!(waits_for_climbers(10.0, 30.0, 30.0), (10.0, 30.0));
    }
}
