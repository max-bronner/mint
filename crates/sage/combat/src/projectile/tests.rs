use super::*;

fn arc() -> BezierArc {
    BezierArc {
        first_height: 24.0,
        second_height: 24.0,
        first_percent_indent: 0.3,
        second_percent_indent: 0.7,
        ignore_terrain_height: false,
        first_percent_height: DEFAULT_FIRST_PERCENT_HEIGHT,
        second_percent_height: DEFAULT_SECOND_PERCENT_HEIGHT,
        curve_flatten_min_dist: 0.0,
    }
}

#[test]
fn a_hill_between_the_ends_lifts_both_control_points_above_it() {
    let start = Point3::new(0.0, 0.0, 0.0);
    let end = Point3::new(300.0, 0.0, 0.0);
    let over_flat = arc().control_points(start, end, None);
    let over_hill = arc().control_points(start, end, Some(120.0));
    assert_eq!(over_flat[1].z, 24.0);
    assert_eq!(over_flat[2].z, 24.0);
    assert_eq!(over_hill[1].z, 144.0);
    assert_eq!(over_hill[2].z, 144.0);
    assert_eq!((over_hill[1].x, over_hill[2].x), (90.0, 210.0));
}

#[test]
fn ignoring_terrain_puts_the_control_points_at_the_percent_heights() {
    let mut ignoring = arc();
    ignoring.ignore_terrain_height = true;
    ignoring.first_percent_height = 0.2;
    ignoring.second_percent_height = 0.66;
    let start = Point3::new(0.0, 0.0, 100.0);
    let end = Point3::new(300.0, 0.0, 0.0);
    let points = ignoring.control_points(start, end, Some(500.0));
    assert_eq!((points[1].x, points[1].z), (90.0, 80.0));
    assert_eq!((points[2].x, points[2].z), (210.0, 34.0));
}

#[test]
fn the_higher_of_the_two_ends_is_the_floor_when_no_terrain_is_known() {
    let start = Point3::new(0.0, 0.0, 80.0);
    let end = Point3::new(300.0, 0.0, 0.0);
    let points = arc().control_points(start, end, None);
    assert_eq!(points[1].z, 104.0);
    assert_eq!(points[2].z, 104.0);
}

#[test]
fn a_flattening_arc_stands_on_the_chord_and_fades_out_when_short() {
    let mut flattening = arc();
    flattening.curve_flatten_min_dist = 100.0;
    flattening.first_percent_indent = 0.25;
    flattening.second_percent_indent = 0.75;
    let start = Point3::new(0.0, 0.0, 40.0);
    let far = flattening.control_points(start, Point3::new(300.0, 0.0, 10.0), None);
    assert_eq!((far[1].z, far[2].z), (32.5 + 24.0, 17.5 + 24.0));
    let flat = |x: f32| {
        flattening.control_points(Point3::new(0.0, 0.0, 0.0), Point3::new(x, 0.0, 0.0), None)
    };
    assert_eq!(flat(40.0)[1].z, 0.0);
    assert_eq!(flat(75.0)[1].z, 12.0);
    assert_eq!(flat(100.0)[1].z, 24.0);
    let over_hill = flattening.control_points(start, Point3::new(300.0, 0.0, 10.0), Some(50.0));
    assert_eq!((over_hill[1].z, over_hill[2].z), (74.0, 74.0));
}

#[test]
fn the_flight_time_is_the_curve_length_and_not_the_chord() {
    let start = Point3::new(0.0, 0.0, 0.0);
    let end = Point3::new(300.0, 0.0, 0.0);
    let flight = ProjectileFlight::launch(start, end, arc(), None, 300.0, 30.0);
    let per_frame = 300.0 / 30.0;
    let curve = flight.curve_length();
    let chord = start.distance_squared_2d(end).sqrt();
    assert!(curve > chord, "{curve} vs {chord}");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let expected = (curve / per_frame).ceil() as u32;
    assert_eq!(flight.ticks_to_arrival(), expected);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let from_chord = (chord / per_frame).ceil() as u32;
    assert_ne!(flight.ticks_to_arrival(), from_chord);
}

#[test]
fn the_arc_starts_at_the_launch_point_and_ends_at_the_target() {
    let start = Point3::new(0.0, 0.0, 10.0);
    let end = Point3::new(100.0, 50.0, 5.0);
    let flight = ProjectileFlight::launch(start, end, arc(), Some(30.0), 100.0, 30.0);
    assert_eq!(flight.at(0.0), start);
    assert_eq!(flight.at(1.0), end);
    assert_eq!(flight.at(-3.0), start);
    assert_eq!(flight.at(7.0), end);
}

#[test]
fn the_terrain_walk_finds_a_hill_the_chord_crosses() {
    let start = Point3::new(0.0, 0.0, 0.0);
    let end = Point3::new(300.0, 0.0, 0.0);
    let hill = |x: f32, _y: f32| {
        Some(if (140.0..160.0).contains(&x) {
            90.0
        } else {
            5.0
        })
    };
    assert_eq!(highest_terrain_along(start, end, hill), Some(90.0));
    assert_eq!(highest_terrain_along(start, end, |_, _| None), None);
}

#[test]
fn a_hostile_speed_or_path_arrives_on_the_first_advance() {
    let start = Point3::new(0.0, 0.0, 0.0);
    let end = Point3::new(100.0, 0.0, 0.0);
    for speed in [0.0f32, -50.0, f32::NAN, f32::INFINITY] {
        let mut flight = ProjectileFlight::launch(start, end, arc(), None, speed, 30.0);
        assert!(flight.advance(), "speed {speed}");
        assert_eq!(flight.ticks_to_arrival(), 1, "speed {speed}");
    }
    let same = ProjectileFlight::launch(start, start, arc(), None, 300.0, 30.0);
    assert!(same.frames() < 30);
    let nan = Point3::new(f32::NAN, 0.0, 0.0);
    let mut broken = ProjectileFlight::launch(start, nan, arc(), None, 300.0, 30.0);
    assert!(broken.advance());
    let mut bad_hz = ProjectileFlight::launch(start, end, arc(), None, 300.0, 0.0);
    assert!(bad_hz.advance());
}

#[test]
fn a_hostile_arc_still_flies_between_two_finite_points() {
    let start = Point3::new(0.0, 0.0, 0.0);
    let end = Point3::new(100.0, 0.0, 0.0);
    let broken = BezierArc {
        first_height: f32::NAN,
        second_height: f32::INFINITY,
        first_percent_indent: f32::NAN,
        second_percent_indent: -5.0,
        ignore_terrain_height: false,
        first_percent_height: f32::NAN,
        second_percent_height: f32::INFINITY,
        curve_flatten_min_dist: f32::NAN,
    };
    let flight = ProjectileFlight::launch(start, end, broken, Some(f32::NAN), 300.0, 30.0);
    for step in 0..=10u8 {
        let t = f32::from(step) / 10.0;
        let point = flight.at(t);
        assert!(
            point.x.is_finite() && point.y.is_finite(),
            "{point:?} at {t}"
        );
    }
    assert_eq!(flight.at(0.0), start);
    assert_eq!(flight.at(1.0), end);
}

#[test]
fn a_flight_arrives_at_exactly_one_and_stays_there() {
    let mut flight = ProjectileFlight::launch(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(300.0, 0.0, 0.0),
        arc(),
        None,
        300.0,
        30.0,
    );
    let expected = flight.ticks_to_arrival();
    let mut ticks = 1;
    while !flight.advance() {
        ticks += 1;
        assert!(ticks < 1000, "the flight never arrived");
    }
    assert_eq!(ticks, expected);
    assert_eq!(flight.t(), 1.0);
    assert_eq!(flight.position(), flight.end());
    flight.advance();
    assert_eq!(flight.t(), 1.0);
}

#[test]
fn every_field_of_a_flight_changes_its_hash() {
    let base = ProjectileFlight::launch(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(100.0, 0.0, 0.0),
        arc(),
        None,
        300.0,
        30.0,
    );
    let hash_of = |f: &ProjectileFlight| {
        let mut hasher = StateHasher::new();
        f.hash_into(&mut hasher);
        hasher.finish()
    };
    let mut moved = base;
    moved.advance();
    let faster = ProjectileFlight::launch(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(100.0, 0.0, 0.0),
        arc(),
        None,
        600.0,
        30.0,
    );
    let elsewhere = ProjectileFlight::launch(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(100.0, 1.0, 0.0),
        arc(),
        None,
        300.0,
        30.0,
    );
    let higher = ProjectileFlight::launch(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(100.0, 0.0, 0.0),
        arc(),
        Some(50.0),
        300.0,
        30.0,
    );
    for f in [moved, faster, elsewhere, higher] {
        assert_ne!(hash_of(&base), hash_of(&f), "{f:?}");
    }
}

#[test]
fn tracking_moves_the_aim_by_at_most_the_cap_and_keeps_the_progress() {
    let start = Point3::new(0.0, 0.0, 10.0);
    let end = Point3::new(300.0, 0.0, 10.0);
    let mut flight = ProjectileFlight::launch(start, end, arc(), None, 300.0, 30.0);
    flight.advance();
    let (t, frames) = (flight.t(), flight.frames());
    assert!(!flight.track(Point3::new(300.1, 0.0, 10.0), 5.0, arc(), |_, _| None));
    assert!(flight.track(Point3::new(300.0, 40.0, 10.0), 5.0, arc(), |_, _| None));
    assert_eq!(flight.end(), Point3::new(300.0, 5.0, 10.0));
    assert_eq!(flight.start(), start);
    assert_eq!((flight.t(), flight.frames()), (t, frames));
    assert!(flight.track(Point3::new(300.0, 8.0, 10.0), 5.0, arc(), |_, _| None));
    assert_eq!(flight.end(), Point3::new(300.0, 8.0, 10.0));
    assert!(!flight.track(start, 0.0, arc(), |_, _| None));
}

#[test]
fn a_ballistic_flight_follows_the_parabola_tick_by_tick() {
    let (start, velocity, gravity) = (
        Point3::new(10.0, -4.0, 30.0),
        Point3::new(0.6, -0.3, 0.5),
        -0.07_f32,
    );
    let frames = 40_u16;
    let at = |tick: f32| {
        Point3::new(
            start.x + velocity.x * tick,
            start.y + velocity.y * tick,
            start.z + velocity.z * tick + 0.5 * gravity * tick * tick,
        )
    };
    let mut flight = ProjectileFlight::ballistic(start, velocity, at(f32::from(frames)), frames);
    for tick in 1..=frames {
        let arrived = flight.advance();
        let (was, want) = (flight.position(), at(f32::from(tick)));
        assert!(
            was.distance_squared_3d(want) < 1e-6,
            "tick {tick}: {was:?} against {want:?}"
        );
        assert_eq!(arrived, tick == frames);
    }
}
