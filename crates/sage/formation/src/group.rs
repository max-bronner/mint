use mint_movement::Vec2;

pub const NO_FORMATION: u8 = 0;

const PRIORITIES: u8 = 13;
const MAX_SQUADS: u32 = 12;
const MAX_SQUAD_UNITS: usize = 36;
const MAX_ROWS: usize = 10;
const SWAP_TRIES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupSpacing {
    pub column_width: f32,
    pub row_depth: f32,
    pub squad_spacing: f32,
    pub columns: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupMember {
    pub position: Vec2,
    pub priority: Option<u8>,
    pub width: u32,
    pub depth: u32,
}

fn squad_count(size_total: u32, columns: i32) -> u32 {
    let capacity = u32::try_from(columns.max(1))
        .unwrap_or(1)
        .saturating_mul(12);
    size_total.div_ceil(capacity).clamp(1, MAX_SQUADS)
}

#[must_use]
pub fn lay_out(members: &[GroupMember], from: Vec2, to: Vec2, spacing: GroupSpacing) -> Vec<Vec2> {
    let mut slots = vec![Vec2::ZERO; members.len()];
    let width = u32::try_from(spacing.columns.saturating_mul(2).clamp(2, 6)).unwrap_or(2);
    let size_total = members.iter().fold(0u32, |sum, member| {
        sum.saturating_add(member.depth.saturating_mul(member.width).max(1))
    });
    let squad_count = squad_count(size_total, spacing.columns);

    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|a, b| {
        let near = |i: usize| {
            members
                .get(i)
                .map_or(f32::INFINITY, |m| m.position.minus(to).length_squared())
        };
        near(*a).total_cmp(&near(*b)).then(a.cmp(b))
    });

    let mut squads: Vec<Vec<usize>> = (0..squad_count).map(|_| Vec::new()).collect();
    let mut next = 0usize;
    for class in 0..PRIORITIES {
        for &index in &order {
            if members.get(index).and_then(|m| m.priority) != Some(class) {
                continue;
            }
            if let Some(squad) = squads.get_mut(next) {
                if squad.len() < MAX_SQUAD_UNITS {
                    squad.push(index);
                }
            }
            next = next
                .saturating_add(1)
                .checked_rem(squads.len())
                .unwrap_or(0);
        }
    }

    let (mut left, mut right) = (0.0f32, 0.0f32);
    for (number, squad) in squads.iter().enumerate() {
        let rows = rows_of(members, squad, width);
        let size_x = rows
            .rows
            .iter()
            .map(|row| depth_of(members, row))
            .sum::<f32>()
            * spacing.row_depth;
        let size_y = rows.width_units * spacing.column_width;
        let half_gap = spacing.squad_spacing * 0.5;
        let mut origin = if number % 2 == 1 {
            let at = Vec2::new(left, half_gap);
            left += size_x + spacing.squad_spacing;
            at
        } else {
            let at = Vec2::new(right, -size_y - half_gap);
            right += size_x + spacing.squad_spacing;
            at
        };
        if squad_count == 1 {
            origin.y = -size_y * 0.5;
        }
        let mut rows_before = 0.0f32;
        for row in &rows.rows {
            let row_x = origin.x - rows_before * spacing.row_depth + spacing.row_depth * 0.5;
            let row_y = origin.y + (size_y - width_of(members, row) * spacing.column_width) * 0.5;
            let mut across = 0.0f32;
            for &index in row {
                let Some(member) = members.get(index) else {
                    continue;
                };
                let (w, d) = units(member);
                let mut slot = Vec2::new(
                    row_x - d * spacing.row_depth * 0.5,
                    row_y + (across + w * 0.5) * spacing.column_width,
                );
                if member.priority == Some(NO_FORMATION) {
                    slot = slot.scale(0.5);
                }
                if let Some(at) = slots.get_mut(index) {
                    *at = slot;
                }
                across += w;
            }
            rows_before += depth_of(members, row);
        }
    }

    let direction = to.minus(from).normalised().unwrap_or(Vec2::new(1.0, 0.0));
    let anchor = order
        .first()
        .and_then(|i| members.get(*i))
        .map_or(from, |front| {
            from.plus(direction.scale(front.position.minus(from).dot(direction)))
                .plus(direction.scale(spacing.row_depth * 0.5))
        });
    swap_pass(members, &order, &mut slots, anchor, direction);
    final_shift(members, &mut slots, spacing);
    slots
}

#[must_use]
pub fn rotate_offset(from: Vec2, to: Vec2, offset: Vec2) -> Vec2 {
    let Some(forward) = to.minus(from).normalised() else {
        return Vec2::ZERO;
    };
    let left = Vec2::new(-forward.y, forward.x);
    forward.scale(offset.x).plus(left.scale(offset.y))
}

struct Rows {
    rows: Vec<Vec<usize>>,
    width_units: f32,
}

fn rows_of(members: &[GroupMember], squad: &[usize], width: u32) -> Rows {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row = 0usize;
    let mut filled = 0u32;
    let mut widest = 0u32;
    'classes: for class in 0..PRIORITIES {
        if row >= MAX_ROWS {
            break;
        }
        for &index in squad {
            let Some(member) = members.get(index).filter(|m| m.priority == Some(class)) else {
                continue;
            };
            filled = filled.saturating_add(member.width);
            if filled > width {
                row = row.saturating_add(1);
                if row >= MAX_ROWS {
                    break 'classes;
                }
                filled = member.width;
            }
            while rows.len() <= row {
                rows.push(Vec::new());
            }
            if let Some(at) = rows.get_mut(row) {
                at.push(index);
            }
            widest = widest.max(filled);
        }
        if class % 3 == 0 && filled > 0 {
            row = row.saturating_add(1);
            filled = 0;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    Rows {
        rows,
        width_units: widest as f32,
    }
}

#[allow(clippy::cast_precision_loss)]
fn units(member: &GroupMember) -> (f32, f32) {
    (member.width as f32, member.depth as f32)
}

fn depth_of(members: &[GroupMember], row: &[usize]) -> f32 {
    row.iter()
        .filter_map(|i| members.get(*i))
        .map(|m| units(m).1)
        .fold(0.0, f32::max)
}

fn width_of(members: &[GroupMember], row: &[usize]) -> f32 {
    row.iter()
        .filter_map(|i| members.get(*i))
        .map(|m| units(m).0)
        .sum()
}

fn swap_pass(
    members: &[GroupMember],
    order: &[usize],
    slots: &mut [Vec2],
    anchor: Vec2,
    direction: Vec2,
) {
    let local = |index: usize| {
        let v = members
            .get(index)
            .map_or(Vec2::ZERO, |m| m.position.minus(anchor));
        Vec2::new(v.dot(direction), direction.x * v.y - direction.y * v.x)
    };
    for (rank, &a) in order.iter().enumerate() {
        let Some(class) = members.get(a).and_then(|m| m.priority) else {
            continue;
        };
        let la = local(a);
        for _ in 0..SWAP_TRIES {
            let mut swapped = false;
            for &b in order.iter().skip(rank.saturating_add(1)) {
                if members.get(b).and_then(|m| m.priority) != Some(class) {
                    continue;
                }
                let (Some(&sa), Some(&sb)) = (slots.get(a), slots.get(b)) else {
                    continue;
                };
                let lb = local(b);
                let (ay, ax) = ((sa.y - la.y).abs(), (sa.x - la.x).abs());
                let (by, bx) = ((sb.y - lb.y).abs(), (sb.x - lb.x).abs());
                let current = 2.0 * (ay + by) + ax + bx + 0.25 * (ax - bx).abs();
                let (ty, tx) = ((sb.y - la.y).abs(), (sb.x - la.x).abs());
                let (uy, ux) = ((sa.y - lb.y).abs(), (sa.x - lb.x).abs());
                let swapped_cost = 2.0 * (ty + uy) + tx + ux + 0.25 * (tx - ux).abs();
                let gain = (ay + ax) - (ty + tx);
                if gain < 0.0 {
                    continue;
                }
                if current + 0.3 * gain > swapped_cost {
                    slots.swap(a, b);
                    swapped = true;
                    break;
                }
            }
            if !swapped {
                break;
            }
        }
    }
}

fn final_shift(members: &[GroupMember], slots: &mut [Vec2], spacing: GroupSpacing) {
    let mut front: Option<(Vec2, u32)> = None;
    for (member, slot) in members.iter().zip(slots.iter()) {
        if member.priority.is_none() || slot.y < 0.0 {
            continue;
        }
        let better =
            front.is_none_or(|(best, _)| slot.x > best.x || (slot.x == best.x && slot.y < best.y));
        if better {
            front = Some((*slot, member.width));
        }
    }
    let Some((at, width)) = front else {
        return;
    };
    let shift = if width > 1 {
        let dy = if at.y == 0.0 {
            0.0
        } else {
            -at.y + spacing.column_width * 0.25
        };
        Vec2::new(-at.x - spacing.row_depth * 0.25, dy)
    } else {
        Vec2::new(0.0, -at.y)
    };
    for slot in slots.iter_mut() {
        *slot = slot.plus(shift);
    }
}

#[cfg(test)]
mod tests;
