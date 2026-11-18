use crate::effect::{kind, Class, Parameter, Type};
use crate::shader::Constant;

impl Parameter {
    #[must_use]
    pub fn floats(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.words.len());
        let mut words = self.words.iter();
        leaves(&self.ty, &mut words, &mut out, 0);
        out
    }
}

fn leaves<'a>(
    ty: &Type,
    words: &mut impl Iterator<Item = &'a u32>,
    out: &mut Vec<f32>,
    depth: u32,
) {
    if depth > 8 {
        return;
    }
    for _ in 0..ty.elements.max(1) {
        match ty.class {
            Class::Struct => {
                for member in &ty.members {
                    leaves(member, words, out, depth.saturating_add(1));
                }
            }
            Class::Object => {
                if !kind::SAMPLERS.contains(&ty.kind) {
                    words.next();
                }
            }
            _ => {
                for _ in 0..ty.rows.saturating_mul(ty.columns) {
                    let Some(word) = words.next() else { return };
                    out.push(match ty.kind {
                        kind::FLOAT => f32::from_bits(*word),
                        kind::BOOL => f32::from(u8::from(*word != 0)),
                        _ => *word as i32 as f32,
                    });
                }
            }
        }
    }
}

pub fn write_constant(constant: &Constant, value: &[f32], out: &mut [f32]) {
    let shapes: Vec<(u16, u16, u16, u16)> = if constant.members.is_empty() {
        vec![(
            constant.class,
            constant.rows,
            constant.columns,
            constant.elements,
        )]
    } else {
        constant
            .members
            .iter()
            .map(|(_, class, rows, columns, elements)| (*class, *rows, *columns, *elements))
            .collect()
    };
    let mut register = u32::from(constant.register_index);
    let end = register.saturating_add(u32::from(constant.register_count));
    let mut cursor = 0usize;
    let elements = if constant.members.is_empty() {
        1
    } else {
        constant.elements.max(1)
    };
    for _ in 0..elements {
        for (class, rows, columns, member_elements) in &shapes {
            let (rows, columns) = (usize::from(*rows), usize::from(*columns));
            for _ in 0..(*member_elements).max(1) {
                let cell = |r: usize, c: usize| {
                    value
                        .get(
                            cursor
                                .saturating_add(r.saturating_mul(columns))
                                .saturating_add(c),
                        )
                        .copied()
                        .unwrap_or(0.0)
                };
                let (count, pick): (usize, &dyn Fn(usize, usize) -> Option<f32>) = match class {
                    2 => (rows, &|k, i| (i < columns).then(|| cell(k, i))),
                    3 => (columns, &|k, i| (i < rows).then(|| cell(i, k))),
                    _ => (1, &|_, i| {
                        (i < columns.saturating_mul(rows)).then(|| cell(0, i))
                    }),
                };
                for k in 0..count {
                    if register >= end {
                        return;
                    }
                    for i in 0..4 {
                        if let Some(v) = pick(k, i) {
                            let at = (register as usize).saturating_mul(4).saturating_add(i);
                            if let Some(slot) = out.get_mut(at) {
                                *slot = v;
                            }
                        }
                    }
                    register = register.saturating_add(1);
                }
                cursor = cursor.saturating_add(rows.saturating_mul(columns));
            }
        }
    }
}

pub fn fill(table: &[Constant], lookup: &dyn Fn(&str) -> Option<Vec<f32>>, out: &mut [f32]) {
    for constant in table.iter().filter(|c| c.register_set == 2) {
        if let Some(value) = lookup(&constant.name) {
            write_constant(constant, &value, out);
        }
    }
}

pub fn fill_bools(table: &[Constant], lookup: &dyn Fn(&str) -> Option<Vec<f32>>, out: &mut [f32]) {
    for constant in table.iter().filter(|c| c.register_set == 0) {
        let Some(value) = lookup(&constant.name) else {
            continue;
        };
        for (k, v) in value
            .iter()
            .take(usize::from(constant.register_count))
            .enumerate()
        {
            if let Some(slot) = out.get_mut(usize::from(constant.register_index).saturating_add(k))
            {
                *slot = f32::from(u8::from(*v != 0.0));
            }
        }
    }
}
