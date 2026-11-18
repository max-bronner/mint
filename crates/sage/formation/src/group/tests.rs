use super::*;

const SHIPPED: GroupSpacing = GroupSpacing {
    column_width: 60.0,
    row_depth: 55.0,
    squad_spacing: 30.0,
    columns: 2,
};

fn member(x: f32, y: f32, priority: u8, width: u32) -> GroupMember {
    GroupMember {
        position: Vec2::new(x, y),
        priority: Some(priority),
        width,
        depth: 1,
    }
}

#[test]
fn the_squad_count_divides_by_the_unclamped_columns() {
    assert_eq!(squad_count(40, 4), 1);
    assert_eq!(squad_count(40, 2), 2);
    assert_eq!(squad_count(40, 0), 4);
}

fn near(a: Vec2, b: Vec2) -> bool {
    a.minus(b).length() < 1e-3
}

#[test]
fn two_squads_of_hordes_end_with_their_inner_fronts_behind_and_right_of_the_click() {
    #[allow(clippy::cast_precision_loss)]
    let hordes: Vec<GroupMember> = (0..13)
        .map(|i| member(-500.0 - 10.0 * i as f32, 0.0, 4, 2))
        .collect();
    let slots = lay_out(&hordes, Vec2::new(-500.0, 0.0), Vec2::ZERO, SHIPPED);
    assert!(
        slots.iter().any(|s| near(*s, Vec2::new(-13.75, 15.0))),
        "{slots:?}"
    );
    assert!(
        slots.iter().any(|s| near(*s, Vec2::new(-13.75, -135.0))),
        "{slots:?}"
    );
    assert!(slots.iter().all(|s| s.x <= -13.75 + 1e-3), "{slots:?}");
}

#[test]
fn cavalry_rows_lead_and_ranged_rows_trail() {
    let group = [
        member(0.0, 0.0, 7, 2),
        member(0.0, 1.0, 4, 2),
        member(0.0, 2.0, 1, 2),
    ];
    let slots = lay_out(&group, Vec2::new(-100.0, 0.0), Vec2::ZERO, SHIPPED);
    assert!(
        slots[2].x > slots[1].x && slots[1].x > slots[0].x,
        "{slots:?}"
    );
}

#[test]
fn two_members_of_one_class_trade_slots_to_keep_their_sides() {
    let group = [member(10.0, 50.0, 4, 1), member(0.0, -50.0, 4, 1)];
    let slots = lay_out(&group, Vec2::ZERO, Vec2::new(100.0, 0.0), SHIPPED);
    assert!(slots[0].y > slots[1].y, "{slots:?}");
}
