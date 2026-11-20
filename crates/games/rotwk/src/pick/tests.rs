use super::*;
use mint_movement::Vec2;
use mint_sage_modules::GeometryPiece;

pub(crate) fn view(entity_index: u32, x: f32, y: f32, kind_of: KindOf, owner: u8) -> UnitView {
    UnitView {
        entity: test_entity(entity_index),
        position: Vec2::new(x, y),
        ground: 0.0,
        facing: 0.0,
        radius: 5.0,
        height: 20.0,
        kind_of,
        targetable: false,
        pick_type: kind_of.pick_type(),
        owner: PlayerId::new(owner),
        selected: false,
        moving: false,
        health: None,
        dying: false,
        created: 0,
        visibility: mint_sage_shroud::ObjectVisibility::Clear,
        speed: 0.0,
        weapon_cycle: 0.0,
        attack_cycle: 0,
        attack_slot: None,
        kind: None,
        art_kind: None,
        told_kind: None,
        hover_kind: None,
        conditions: mint_sage_scene::Conditions::NONE,
        build_elapsed: None,
        build_total: None,
        under_repair: false,
        held: false,
    }
}

fn test_entity(index: u32) -> Entity {
    let mut world = mint_sim::World::new();
    let mut entity = world.spawn().unwrap();
    for _ in 0..index {
        entity = world.spawn().unwrap();
    }
    entity
}

fn undeclared(_: &UnitView) -> Declared<'static> {
    Declared::default()
}

fn piece(offset: (f32, f32), shape: PieceShape, name: Option<&str>, active: bool) -> GeometryPiece {
    GeometryPiece {
        offset,
        offset_z: 0.0,
        shape,
        name: name.map(str::to_owned),
        active,
        height: Some(20.0),
        for_health_box: true,
    }
}

fn camera_over(x: f32, y: f32, distance: f32) -> Camera {
    Camera {
        eye: Vec3::new(x, y - distance * 0.5, distance),
        target: Vec3::new(x, y, 0.0),
        up: Vec3::Z,
        near: 1.0,
        far: 10_000.0,
        ..Camera::default()
    }
}

#[test]
fn a_ray_down_the_axis_enters_the_cylinder_at_its_top_face() {
    let ray = Ray {
        origin: Vec3::new(0.0, 0.0, 100.0),
        direction: Vec3::new(0.0, 0.0, -1.0),
    };
    let t = ray_hits_cylinder(ray, Vec3::new(0.0, 0.0, 10.0), 5.0, 20.0).unwrap();
    assert!((t - 70.0).abs() < 1e-3, "{t}");
}

#[test]
fn a_ray_beside_the_cylinder_misses_and_one_through_its_waist_hits() {
    let side = |x: f32| Ray {
        origin: Vec3::new(x, -100.0, 15.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    assert!(ray_hits_cylinder(side(0.0), Vec3::ZERO, 5.0, 20.0).is_some());
    assert!(ray_hits_cylinder(side(4.9), Vec3::ZERO, 5.0, 20.0).is_some());
    assert!(ray_hits_cylinder(side(5.1), Vec3::ZERO, 5.0, 20.0).is_none());
}

#[test]
fn a_ray_passing_over_the_top_misses() {
    let over = Ray {
        origin: Vec3::new(0.0, -100.0, 30.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    assert!(ray_hits_cylinder(over, Vec3::ZERO, 5.0, 20.0).is_none());
    let under = Ray {
        origin: Vec3::new(0.0, -100.0, -5.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    assert!(ray_hits_cylinder(under, Vec3::ZERO, 5.0, 20.0).is_none());
}

#[test]
fn a_cylinder_behind_the_ray_is_not_hit() {
    let away = Ray {
        origin: Vec3::new(0.0, 0.0, 40.0),
        direction: Vec3::new(0.0, 0.0, 1.0),
    };
    assert!(ray_hits_cylinder(away, Vec3::ZERO, 5.0, 20.0).is_none());
}

#[test]
fn a_ray_starting_inside_the_cylinder_hits_at_no_distance() {
    let inside = Ray {
        origin: Vec3::new(0.0, 0.0, 10.0),
        direction: Vec3::new(0.0, 0.0, 1.0),
    };
    assert_eq!(ray_hits_cylinder(inside, Vec3::ZERO, 5.0, 20.0), Some(0.0));
}

#[test]
fn a_click_enters_each_shown_geometry_piece_by_its_own_shape() {
    let geometry = Geometry {
        pieces: vec![
            piece(
                (0.0, 0.0),
                PieceShape::Box {
                    major: 30.0,
                    minor: 10.0,
                },
                None,
                true,
            ),
            piece((50.0, 0.0), PieceShape::Disc { radius: 5.0 }, None, true),
            piece(
                (-50.0, 0.0),
                PieceShape::Box {
                    major: 5.0,
                    minor: 5.0,
                },
                Some("Upper"),
                false,
            ),
        ],
        ..Geometry::default()
    };
    let shown: [(Name, bool); 1] = [(Name::from("upper"), true)];
    let mut unit = view(0, 100.0, 100.0, KindOf::SELECTABLE, 0);
    unit.facing = std::f32::consts::FRAC_PI_2;
    let down = |dx: f32, dy: f32| Ray {
        origin: Vec3::new(100.0 + dx, 100.0 + dy, 100.0),
        direction: Vec3::new(0.0, 0.0, -1.0),
    };
    let cases: [(&str, Ray, &[(Name, bool)], Option<f32>); 8] = [
        (
            "box corner, outside its inscribed circle",
            down(9.0, 29.0),
            &[],
            Some(80.0),
        ),
        (
            "inside the circle, across the box's width",
            down(28.0, 8.0),
            &[],
            None,
        ),
        (
            "the additional disc, north",
            down(0.0, 50.0),
            &[],
            Some(80.0),
        ),
        ("just past the disc", down(0.0, 56.0), &[], None),
        ("a hidden group's box", down(0.0, -50.0), &[], None),
        (
            "the same group once shown",
            down(0.0, -50.0),
            &shown,
            Some(80.0),
        ),
        (
            "level with the box, from the south",
            Ray {
                origin: Vec3::new(100.0, 0.0, 10.0),
                direction: Vec3::new(0.0, 1.0, 0.0),
            },
            &[],
            Some(70.0),
        ),
        (
            "above the box",
            Ray {
                origin: Vec3::new(100.0, 0.0, 25.0),
                direction: Vec3::new(0.0, 1.0, 0.0),
            },
            &[],
            None,
        ),
    ];
    for (what, ray, overrides, expected) in cases {
        let declared = Declared {
            geometry: Some(&geometry),
            overrides,
        };
        let t = ray_hits_unit(ray, &unit, declared);
        match (t, expected) {
            (Some(t), Some(e)) => assert!((t - e).abs() < 1e-3, "{what}: {t} vs {e}"),
            (t, e) => assert_eq!(t, e, "{what}"),
        }
        assert_eq!(
            pick_unit(&[unit], ray, PlayerId::new(0), |_| declared),
            expected.map(|_| unit.entity),
            "{what}"
        );
    }
}

#[test]
fn the_nearest_of_two_stacked_units_takes_the_click() {
    let units = [
        view(0, 0.0, 0.0, KindOf::SELECTABLE, 0),
        view(1, 0.0, 60.0, KindOf::SELECTABLE, 0),
    ];
    let ray = Ray {
        origin: Vec3::new(0.0, -100.0, 10.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    assert_eq!(
        pick_unit(&units, ray, PlayerId::new(0), undeclared),
        Some(units[0].entity)
    );
}

#[test]
fn a_shrubbery_in_front_of_a_unit_does_not_take_the_click() {
    let units = [
        view(0, 0.0, 0.0, KindOf::SHRUBBERY, 0),
        view(1, 0.0, 60.0, KindOf::SELECTABLE, 0),
    ];
    let ray = Ray {
        origin: Vec3::new(0.0, -100.0, 10.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    assert_eq!(
        pick_unit(&units, ray, PlayerId::new(0), undeclared),
        Some(units[1].entity)
    );
}

#[test]
fn an_order_passes_over_its_own_and_its_allies_for_the_enemy_behind() {
    let units = [
        view(0, 0.0, 0.0, KindOf::SELECTABLE, 0),
        view(1, 0.0, 30.0, KindOf::SELECTABLE, 2),
        view(2, 0.0, 60.0, KindOf::SELECTABLE, 1),
    ];
    let ray = Ray {
        origin: Vec3::new(0.0, -100.0, 10.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    let allies = crate::alliance::Alliances::new(vec![Some(0), None, Some(0)]);
    let me = PlayerId::new(0);
    assert_eq!(
        pick_target(&units, ray, undeclared, |unit: &UnitView| allies
            .relation(unit.owner, me)),
        Some(units[2].entity)
    );
    let no_team = |unit: &UnitView| {
        let owner = unit.owner;
        if owner == me {
            mint_sage_combat::victim::Relation::Allies
        } else {
            mint_sage_combat::victim::Relation::Enemies
        }
    };
    assert_eq!(
        pick_target(&units, ray, undeclared, no_team),
        Some(units[1].entity),
        "without the team the ally in front is the target"
    );
}

#[test]
fn an_order_passes_over_an_unattackable_enemy() {
    let units = [
        view(
            0,
            0.0,
            0.0,
            KindOf::SELECTABLE.union(KindOf::UNATTACKABLE),
            1,
        ),
        view(1, 0.0, 30.0, KindOf::SELECTABLE, 1),
    ];
    let ray = Ray {
        origin: Vec3::new(0.0, -100.0, 10.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    let me = PlayerId::new(0);
    let no_team = |unit: &UnitView| {
        let owner = unit.owner;
        if owner == me {
            mint_sage_combat::victim::Relation::Allies
        } else {
            mint_sage_combat::victim::Relation::Enemies
        }
    };
    assert_eq!(
        pick_target(&units, ray, undeclared, no_team),
        Some(units[1].entity)
    );
    assert_eq!(pick_target(&units[..1], ray, undeclared, no_team), None);
}

#[test]
fn an_order_offers_a_flagged_neutral_object_but_not_an_ally() {
    let mut units = [
        view(
            0,
            0.0,
            0.0,
            KindOf::SELECTABLE.union(KindOf::LINKED_TO_FLAG),
            1,
        ),
        view(1, 0.0, 30.0, KindOf::SELECTABLE, 0),
    ];
    let ray = Ray {
        origin: Vec3::new(0.0, -100.0, 10.0),
        direction: Vec3::new(0.0, 1.0, 0.0),
    };
    let me = PlayerId::new(0);
    let relation = |unit: &UnitView| {
        let owner = unit.owner;
        if owner == me {
            mint_sage_combat::victim::Relation::Allies
        } else {
            mint_sage_combat::victim::Relation::Neutral
        }
    };
    assert_eq!(
        pick_target(&units[..1], ray, undeclared, relation),
        Some(units[0].entity)
    );
    assert_eq!(
        pick_target(&units, ray, undeclared, relation),
        Some(units[0].entity)
    );
    units[0].kind_of = KindOf::SELECTABLE;
    assert_eq!(pick_target(&units, ray, undeclared, relation), None);
    units[0].targetable = true;
    assert_eq!(
        pick_target(&units, ray, undeclared, relation),
        Some(units[0].entity)
    );
}

#[test]
fn a_drag_refuses_an_enemy_unit_that_a_click_would_take() {
    let enemy = view(0, 0.0, 0.0, KindOf::SELECTABLE, 1);
    assert!(can_select(&enemy, false, PlayerId::new(0)));
    assert!(!can_select(&enemy, true, PlayerId::new(0)));
}

#[test]
fn a_drag_refuses_a_structure_that_a_click_would_take() {
    let building = view(0, 0.0, 0.0, KindOf::SELECTABLE.union(KindOf::STRUCTURE), 0);
    assert!(can_select(&building, false, PlayerId::new(0)));
    assert!(!can_select(&building, true, PlayerId::new(0)));
}

#[test]
fn the_two_box_rules_disagree_about_a_unit_whose_feet_are_outside_the_box() {
    let viewport = (800u32, 600u32);
    let camera = camera_over(0.0, 0.0, 200.0);
    let units = [view(0, 0.0, 0.0, KindOf::SELECTABLE, 0)];
    let (feet_x, feet_y, _) = project(&camera, viewport, Vec3::new(0.0, 0.0, 0.0)).unwrap();
    let rect = ScreenRect {
        min_x: feet_x - 40.0,
        min_y: feet_y - 40.0,
        max_x: feet_x + 40.0,
        max_y: feet_y - 4.0,
    };
    assert!(!rect.contains(feet_x, feet_y));
    let centre = box_select(
        &units,
        &camera,
        viewport,
        rect,
        BoxMode::Centre,
        PlayerId::new(0),
    );
    let bounds = box_select(
        &units,
        &camera,
        viewport,
        rect,
        BoxMode::Bounds,
        PlayerId::new(0),
    );
    assert!(centre.is_empty(), "the faithful rule tests the feet only");
    assert_eq!(bounds.len(), 1, "the modern rule tests the body");
}

#[test]
fn a_box_over_empty_ground_selects_nothing_and_one_over_a_unit_selects_it() {
    let viewport = (800u32, 600u32);
    let camera = camera_over(0.0, 0.0, 200.0);
    let units = [view(0, 0.0, 0.0, KindOf::SELECTABLE, 0)];
    let (x, y, _) = project(&camera, viewport, Vec3::ZERO).unwrap();
    let over = ScreenRect::from_corners((x - 30.0, y - 30.0), (x + 30.0, y + 30.0));
    let elsewhere = ScreenRect::from_corners((0.0, 0.0), (10.0, 10.0));
    for mode in BoxMode::ALL {
        assert_eq!(
            box_select(&units, &camera, viewport, over, mode, PlayerId::new(0)).len(),
            1,
            "{}",
            mode.label()
        );
        assert!(
            box_select(&units, &camera, viewport, elsewhere, mode, PlayerId::new(0)).is_empty(),
            "{}",
            mode.label()
        );
    }
}

#[test]
fn a_box_selection_comes_back_newest_created_first() {
    let viewport = (800u32, 600u32);
    let camera = camera_over(0.0, 0.0, 300.0);
    let mut units = vec![
        view(4, 0.0, 0.0, KindOf::SELECTABLE, 0),
        view(1, 20.0, 0.0, KindOf::SELECTABLE, 0),
        view(7, -20.0, 0.0, KindOf::SELECTABLE, 0),
    ];
    for (unit, created) in units.iter_mut().zip([3, 9, 5]) {
        unit.created = created;
    }
    units.reverse();
    let rect = ScreenRect::from_corners((0.0, 0.0), (800.0, 600.0));
    let picked = box_select(
        &units,
        &camera,
        viewport,
        rect,
        BoxMode::Centre,
        PlayerId::new(0),
    );
    let slots: Vec<u32> = picked.iter().map(|entity| entity.index()).collect();
    assert_eq!(slots, [1, 7, 4]);
}

#[test]
fn a_unit_behind_the_camera_is_never_boxed() {
    let viewport = (800u32, 600u32);
    let camera = Camera {
        eye: Vec3::new(0.0, 0.0, 100.0),
        target: Vec3::new(0.0, 100.0, 100.0),
        up: Vec3::Z,
        near: 1.0,
        far: 10_000.0,
        ..Camera::default()
    };
    let units = [view(0, 0.0, -200.0, KindOf::SELECTABLE, 0)];
    let rect = ScreenRect::from_corners((0.0, 0.0), (800.0, 600.0));
    for mode in BoxMode::ALL {
        assert!(
            box_select(&units, &camera, viewport, rect, mode, PlayerId::new(0)).is_empty(),
            "{}",
            mode.label()
        );
    }
}

#[test]
fn a_pixel_over_a_unit_rays_back_to_that_unit() {
    let viewport = (1280u32, 720u32);
    let camera = camera_over(500.0, 500.0, 400.0);
    let units = [
        view(0, 500.0, 500.0, KindOf::SELECTABLE, 0),
        view(1, 620.0, 460.0, KindOf::SELECTABLE, 0),
    ];
    for unit in &units {
        let waist = Vec3::new(
            unit.position.x,
            unit.position.y,
            unit.ground + unit.height * 0.5,
        );
        let (px, py, _) = project(&camera, viewport, waist).unwrap();
        let ray = camera.ray_through_pixel(viewport, px, py).unwrap();
        assert_eq!(
            pick_unit(&units, ray, PlayerId::new(0), undeclared),
            Some(unit.entity),
            "pixel ({px:.1}, {py:.1}) should pick the unit it was computed from"
        );
    }
    let empty = camera.ray_through_pixel(viewport, 5.0, 5.0).unwrap();
    assert_eq!(pick_unit(&units, empty, PlayerId::new(0), undeclared), None);
}

#[test]
fn non_finite_input_selects_nothing_rather_than_everything() {
    let viewport = (800u32, 600u32);
    let camera = camera_over(0.0, 0.0, 200.0);
    let nan_ray = Ray {
        origin: Vec3::new(f32::NAN, 0.0, 100.0),
        direction: Vec3::new(0.0, 0.0, -1.0),
    };
    assert_eq!(
        ray_hits_cylinder(nan_ray, Vec3::ZERO, 5.0, 20.0),
        None,
        "a NaN ray must miss"
    );
    for bad in [f32::NAN, f32::INFINITY, 0.0, -5.0] {
        assert_eq!(
            ray_hits_cylinder(
                Ray {
                    origin: Vec3::new(0.0, 0.0, 100.0),
                    direction: Vec3::new(0.0, 0.0, -1.0),
                },
                Vec3::ZERO,
                bad,
                20.0
            ),
            None,
            "a radius of {bad} is not a volume"
        );
    }

    let mut broken = view(0, 0.0, 0.0, KindOf::SELECTABLE, 0);
    broken.position = Vec2::new(f32::NAN, f32::NAN);
    let units = [broken];
    let ray = camera.ray_through_pixel(viewport, 400.0, 300.0).unwrap();
    assert_eq!(pick_unit(&units, ray, PlayerId::new(0), undeclared), None);
    let whole_screen = ScreenRect::from_corners((0.0, 0.0), (800.0, 600.0));
    for mode in BoxMode::ALL {
        assert!(
            box_select(
                &units,
                &camera,
                viewport,
                whole_screen,
                mode,
                PlayerId::new(0)
            )
            .is_empty(),
            "{}",
            mode.label()
        );
    }

    let nan_rect = ScreenRect::from_corners((f32::NAN, f32::NAN), (f32::NAN, f32::NAN));
    let good = [view(0, 0.0, 0.0, KindOf::SELECTABLE, 0)];
    for mode in BoxMode::ALL {
        assert!(
            box_select(&good, &camera, viewport, nan_rect, mode, PlayerId::new(0)).is_empty(),
            "{}",
            mode.label()
        );
    }
}

#[test]
fn the_mode_labels_paste_into_a_report_and_cycle_through_every_mode() {
    let mut seen = Vec::new();
    let mut mode = BoxMode::default();
    for _ in 0..BoxMode::ALL.len() {
        seen.push(mode.label());
        mode = mode.next();
    }
    assert_eq!(mode, BoxMode::default(), "cycling must return to the start");
    assert_eq!(seen.len(), BoxMode::ALL.len());
    assert!(seen.contains(&"box=centre-point (faithful)"));
}
