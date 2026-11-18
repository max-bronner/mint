use mint_sage_apt::Character;

use crate::instance::{InstanceId, InstanceKind};
use crate::library::Library;
use crate::timeline::Stage;

use super::value::Value;

pub(crate) const PROPERTIES: [&str; 22] = [
    "_x",
    "_y",
    "_xscale",
    "_yscale",
    "_currentframe",
    "_totalframes",
    "_alpha",
    "_visible",
    "_width",
    "_height",
    "_rotation",
    "_target",
    "_framesloaded",
    "_name",
    "_droptarget",
    "_url",
    "_highquality",
    "_focusrect",
    "_soundbuftime",
    "_quality",
    "_xmouse",
    "_ymouse",
];

pub(crate) fn property_index(name: &str) -> Option<usize> {
    PROPERTIES.iter().position(|p| p.eq_ignore_ascii_case(name))
}

pub(crate) fn index_from_number(n: f64) -> Option<usize> {
    if !n.is_finite() || n <= -1.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = n as usize;
    (i < PROPERTIES.len()).then_some(i)
}

pub(crate) fn level_number(name: &str) -> Option<i32> {
    let prefix = name.get(..6)?;
    if !(prefix.eq_ignore_ascii_case("_level") || prefix.eq_ignore_ascii_case("_flash")) {
        return None;
    }
    name.get(6..)?.parse().ok().or(Some(0))
}

impl Stage {
    pub(crate) fn child_by_name(&self, parent: InstanceId, name: &str) -> Option<InstanceId> {
        let p = self.arena.get(parent)?;
        p.children.values().copied().find(|c| {
            self.arena
                .get(*c)
                .is_some_and(|i| match (&i.name, i.default_name) {
                    (Some(n), _) => n.eq_ignore_ascii_case(name),
                    (None, Some(n)) => {
                        name.get(..8)
                            .is_some_and(|pre| pre.eq_ignore_ascii_case("instance"))
                            && name.get(8..).and_then(|d| d.parse::<u32>().ok()) == Some(n)
                    }
                    (None, None) => false,
                })
        })
    }

    pub(crate) fn path_property(&self, inst: InstanceId, name: &str) -> Option<Value> {
        let obj = |id: Option<InstanceId>| {
            id.and_then(|i| self.script_object(i))
                .map_or(Value::Undefined, Value::Object)
        };
        if name.eq_ignore_ascii_case("_root") {
            return Some(obj(self.root));
        }
        if name.eq_ignore_ascii_case("_parent") {
            return Some(obj(self.arena.get(inst).and_then(|i| i.parent)));
        }
        if name.eq_ignore_ascii_case("_global") {
            return Some(Value::Object(self.heap.global));
        }
        if level_number(name) == Some(0) {
            return Some(obj(self.root));
        }
        if level_number(name).is_some() {
            return Some(Value::Undefined);
        }
        None
    }

    pub(crate) fn target_path_slash(&self, inst: InstanceId) -> String {
        let names = self.path_names(inst);
        if names.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", names.join("/"))
        }
    }

    pub(crate) fn target_path_dot(&self, inst: InstanceId) -> String {
        let mut out = "_level0".to_owned();
        for n in self.path_names(inst) {
            out.push('.');
            out.push_str(&n);
        }
        out
    }

    fn pointer_position(&self) -> [f32; 2] {
        self.mouse.position.unwrap_or(self.pointer)
    }

    fn path_names(&self, inst: InstanceId) -> Vec<String> {
        let mut names = Vec::new();
        let mut at = Some(inst);
        while let Some(id) = at {
            let Some(i) = self.arena.get(id) else {
                break;
            };
            if i.parent.is_none() || names.len() > 64 {
                break;
            }
            names.push(i.name());
            at = i.parent;
        }
        names.reverse();
        names
    }

    pub(crate) fn bounds_in_parent(&self, lib: &Library, inst: InstanceId) -> Option<[f32; 4]> {
        let i = self.arena.get(inst)?;
        let local = self.local_bounds(lib, inst, 0)?;
        Some(i.matrix.transform_rect(local))
    }

    pub(crate) fn local_bounds(
        &self,
        lib: &Library,
        inst: InstanceId,
        depth: u32,
    ) -> Option<[f32; 4]> {
        if depth > 64 {
            return None;
        }
        let i = self.arena.get(inst)?;
        match i.kind {
            InstanceKind::Clip(_) | InstanceKind::Button(_) => {
                let mut acc: Option<[f32; 4]> = None;
                for child in i.children.values() {
                    let Some(c) = self.arena.get(*child) else {
                        continue;
                    };
                    if let Some(b) = self.local_bounds(lib, *child, depth.saturating_add(1)) {
                        let b = c.matrix.transform_rect(b);
                        acc = Some(match acc {
                            None => b,
                            Some(a) => [
                                a[0].min(b[0]),
                                a[1].min(b[1]),
                                a[2].max(b[2]),
                                a[3].max(b[3]),
                            ],
                        });
                    }
                }
                acc
            }
            _ => match lib.character(i.character)? {
                Character::Shape { bounds, .. }
                | Character::EditText { bounds, .. }
                | Character::Text { bounds, .. } => {
                    Some([bounds.left, bounds.top, bounds.right, bounds.bottom])
                }
                _ => None,
            },
        }
    }

    pub(crate) fn get_display_property(
        &mut self,
        lib: &Library,
        inst: InstanceId,
        index: usize,
    ) -> Value {
        let frames = self.frame_and_total(lib, inst);
        let Some(i) = self.arena.get_mut(inst) else {
            return Value::Undefined;
        };
        let num = |v: f32| Value::Number(f64::from(v));
        match index {
            0 => num(i.x()),
            1 => num(i.y()),
            2 => num(i.x_scale()),
            3 => num(i.y_scale()),
            4 => Value::Number(f64::from(frames.map_or(1, |f| f.0.max(1)))),
            5 | 12 => Value::Number(f64::from(frames.map_or(1, |f| f.1))),
            6 => num(i.alpha()),
            7 => Value::Bool(i.visible()),
            8 | 9 => {
                let b = self.bounds_in_parent(lib, inst);
                b.map_or(Value::Number(0.0), |b| {
                    num(if index == 8 { b[2] - b[0] } else { b[3] - b[1] })
                })
            }
            10 => num(i.rotation()),
            11 => Value::str(&self.target_path_slash(inst)),
            13 => Value::str(&i.name()),
            14 | 15 => Value::str(""),
            16 => Value::Number(1.0),
            17 => Value::Bool(true),
            18 => Value::Number(5.0),
            19 => Value::str("HIGH"),
            20 => Value::Number(f64::from(self.pointer_position()[0])),
            21 => Value::Number(f64::from(self.pointer_position()[1])),
            _ => Value::Undefined,
        }
    }

    pub(crate) fn set_display_property(
        &mut self,
        lib: &Library,
        inst: InstanceId,
        index: usize,
        value: &Value,
    ) {
        let n = value.to_number();
        let local_size = if index == 8 || index == 9 {
            self.local_bounds(lib, inst, 0)
        } else {
            None
        };
        let name = if index == 13 {
            Some(self.string_of(lib, value).to_string())
        } else {
            None
        };
        let Some(i) = self.arena.get_mut(inst) else {
            return;
        };
        match index {
            0 if n.is_finite() => i.set_x(narrow(n)),
            1 if n.is_finite() => i.set_y(narrow(n)),
            2 if n.is_finite() => i.set_x_scale(narrow(n)),
            3 if n.is_finite() => i.set_y_scale(narrow(n)),
            6 if n.is_finite() => i.set_alpha(narrow(n)),
            7 => i.set_visible(value.to_bool()),
            8 | 9 if n.is_finite() => {
                if let Some(b) = local_size {
                    let size = if index == 8 { b[2] - b[0] } else { b[3] - b[1] };
                    if size > 0.0 {
                        let percent = narrow(n) / size * 100.0;
                        if index == 8 {
                            i.set_x_scale(percent);
                        } else {
                            i.set_y_scale(percent);
                        }
                    }
                }
            }
            10 if n.is_finite() => i.set_rotation(narrow(n)),
            13 => {
                if let Some(name) = name {
                    i.set_name(name);
                }
            }
            _ => {}
        }
    }

    fn frame_and_total(&self, lib: &Library, inst: InstanceId) -> Option<(u16, u16)> {
        let clip = self.arena.get(inst)?.clip()?;
        let total = lib.timeline(clip.timeline)?.frame_count();
        Some((clip.current_frame, total))
    }
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(n: f64) -> f32 {
    n as f32
}
