use crate::actions::{clip_event, ActionSource, QueuedAction};
use crate::geom::Matrix;
use crate::instance::{ButtonVisual, InstanceId, InstanceKind};
use crate::library::Library;
use crate::timeline::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HitMode {
    #[default]
    Records,
    MeshBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseEvent {
    Press,
    Release,
    ReleaseOutside,
    RollOver,
    RollOut,
    DragOver,
    DragOut,
}

impl MouseEvent {
    fn button(self) -> (ButtonVisual, u32) {
        match self {
            Self::DragOut => (ButtonVisual::Over, 0x10),
            Self::DragOver => (ButtonVisual::Down, 0x20),
            Self::Press => (ButtonVisual::Down, 0x04),
            Self::Release => (ButtonVisual::Over, 0x08),
            Self::ReleaseOutside => (ButtonVisual::Up, 0x40),
            Self::RollOut => (ButtonVisual::Up, 0x02),
            Self::RollOver => (ButtonVisual::Over, 0x01),
        }
    }

    fn clip_flag(self) -> u32 {
        match self {
            Self::Press => clip_event::PRESS,
            Self::Release => clip_event::RELEASE,
            Self::ReleaseOutside => clip_event::RELEASE_OUTSIDE,
            Self::RollOver => clip_event::ROLL_OVER,
            Self::RollOut => clip_event::ROLL_OUT,
            Self::DragOver => clip_event::DRAG_OVER,
            Self::DragOut => clip_event::DRAG_OUT,
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Press => "onPress",
            Self::Release => "onRelease",
            Self::ReleaseOutside => "onReleaseOutside",
            Self::RollOver => "onRollOver",
            Self::RollOut => "onRollOut",
            Self::DragOver => "onDragOver",
            Self::DragOut => "onDragOut",
        }
    }
}

const MOUSE_CLIP_EVENTS: u32 = clip_event::PRESS
    | clip_event::RELEASE
    | clip_event::RELEASE_OUTSIDE
    | clip_event::ROLL_OVER
    | clip_event::ROLL_OUT
    | clip_event::DRAG_OVER
    | clip_event::DRAG_OUT;

const MOUSE_METHODS: [&str; 7] = [
    "onPress",
    "onRelease",
    "onReleaseOutside",
    "onRollOver",
    "onRollOut",
    "onDragOver",
    "onDragOut",
];

#[derive(Debug, Clone, Default)]
pub(crate) struct MouseData {
    pub(crate) position: Option<[f32; 2]>,
    pub(crate) down: bool,
    pub(crate) hovered: Option<InstanceId>,
    pub(crate) pressed: Option<InstanceId>,
    pub(crate) hit_mode: HitMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitArea {
    pub button: InstanceId,
    pub corners: [[f32; 2]; 4],
}

impl Stage {
    pub(crate) fn update_mouse(&mut self, lib: &Library, moved: bool, button: Option<bool>) {
        if let Some(h) = self.mouse.hovered {
            if self.arena.get(h).is_none_or(|i| i.removed) {
                self.mouse.hovered = None;
            }
        }
        if let Some(p) = self.mouse.pressed {
            if self.arena.get(p).is_none_or(|i| i.removed) {
                self.mouse.pressed = None;
            }
        }
        let hovered_visible = self
            .mouse
            .hovered
            .and_then(|h| self.arena.get(h))
            .is_some_and(|i| i.visible());
        let skip_hover =
            !moved && button.is_none() && self.mouse.hovered.is_some() && hovered_visible;
        let new_over = match (skip_hover, self.mouse.position, self.root) {
            (false, Some(point), Some(root)) => self.pick(lib, root, &Matrix::IDENTITY, point, 0),
            _ => self.mouse.hovered,
        };
        let mut events: Vec<(InstanceId, MouseEvent)> = Vec::new();
        let current = self.mouse.hovered;
        if !skip_hover && current != new_over {
            if self.mouse.down {
                if let Some(pressed) = self.mouse.pressed {
                    if Some(pressed) == current {
                        events.push((pressed, MouseEvent::DragOut));
                    } else if Some(pressed) == new_over {
                        events.push((pressed, MouseEvent::DragOver));
                    }
                }
                if let Some(c) = current.filter(|c| Some(*c) != self.mouse.pressed) {
                    events.push((c, MouseEvent::DragOut));
                }
                if let Some(n) = new_over.filter(|n| Some(*n) != self.mouse.pressed) {
                    events.push((n, MouseEvent::DragOver));
                }
            } else {
                if let Some(c) = current {
                    events.push((c, MouseEvent::RollOut));
                }
                if let Some(n) = new_over {
                    events.push((n, MouseEvent::RollOver));
                }
            }
        }
        if !skip_hover {
            self.mouse.hovered = new_over;
        }
        match button {
            Some(true) => {
                if let Some(h) = self.mouse.hovered {
                    events.push((h, MouseEvent::Press));
                }
                self.mouse.pressed = self.mouse.hovered;
            }
            Some(false) => {
                let pressed = self.mouse.pressed.take();
                match pressed {
                    Some(p) if Some(p) == self.mouse.hovered => {
                        events.push((p, MouseEvent::Release));
                    }
                    Some(p) => {
                        events.push((p, MouseEvent::ReleaseOutside));
                        if let Some(h) = self.mouse.hovered {
                            events.push((h, MouseEvent::RollOver));
                        }
                    }
                    None => {}
                }
            }
            None => {}
        }
        for (object, event) in events {
            if self.arena.get(object).is_some_and(|i| !i.removed) {
                self.mouse_event(lib, object, event);
            }
        }
    }

    fn mouse_event(&mut self, lib: &Library, object: InstanceId, event: MouseEvent) {
        let Some(inst) = self.arena.get(object) else {
            return;
        };
        let character = inst.character;
        match inst.kind {
            InstanceKind::Button(current) => {
                let (visual, condition) = event.button();
                if self.enabled(object) {
                    if let Some(parent) = inst.parent {
                        for action in lib.button_actions(character) {
                            if action.condition & condition != 0 {
                                self.push_action(QueuedAction {
                                    clip: parent,
                                    movie: character.movie,
                                    source: ActionSource::Button {
                                        condition: action.condition,
                                        block: action.block_offset,
                                    },
                                    is_unload: false,
                                });
                            }
                        }
                    }
                    self.queue_mouse_method(object, event);
                    if current != visual {
                        self.set_button_state(lib, object, visual);
                    }
                } else if visual != ButtonVisual::Over && current != ButtonVisual::Up {
                    self.set_button_state(lib, object, ButtonVisual::Up);
                }
            }
            InstanceKind::Clip(_) => {
                self.queue_clip_event(lib, object, event.clip_flag());
                self.queue_mouse_method(object, event);
            }
            _ => {}
        }
    }

    fn enabled(&self, object: InstanceId) -> bool {
        self.script_object(object)
            .and_then(|o| self.heap.objects.get(o))
            .and_then(|o| o.props.get("enabled").cloned())
            .is_none_or(|v| v.to_bool())
    }

    fn queue_mouse_method(&mut self, object: InstanceId, event: MouseEvent) {
        self.queue_method(object, event.method());
    }

    pub(crate) fn queue_method(&mut self, object: InstanceId, name: &'static str) {
        let defined = self
            .script_object(object)
            .is_some_and(|o| self.has_property(o, name));
        if defined {
            let movie = self
                .arena
                .get(object)
                .map_or(crate::library::MovieId(0), |i| i.character.movie);
            self.push_action(QueuedAction {
                clip: object,
                movie,
                source: ActionSource::Method { name },
                is_unload: false,
            });
        }
    }

    fn button_mode(&self, lib: &Library, id: InstanceId) -> bool {
        let Some(inst) = self.arena.get(id) else {
            return false;
        };
        let handlers = inst
            .clip()
            .and_then(|c| c.clip_actions)
            .is_some_and(|(movie, offset)| {
                lib.clip_actions(movie, offset)
                    .iter()
                    .any(|h| h.events & MOUSE_CLIP_EVENTS != 0)
            });
        handlers
            || inst
                .script
                .is_some_and(|o| MOUSE_METHODS.iter().any(|name| self.has_property(o, name)))
    }

    fn pick(
        &self,
        lib: &Library,
        id: InstanceId,
        parent: &Matrix,
        point: [f32; 2],
        depth: u16,
    ) -> Option<InstanceId> {
        let inst = self.arena.get(id)?;
        if !inst.visible() || inst.removed || depth > self.limits.max_tree_depth {
            return None;
        }
        let m = parent.concat(&inst.matrix);
        match inst.kind {
            InstanceKind::Button(_) => {
                for (_, child) in inst.children.iter().rev() {
                    if let Some(hit) = self.pick(lib, *child, &m, point, depth.saturating_add(1)) {
                        return Some(hit);
                    }
                }
                self.button_contains(lib, id, &m, point).then_some(id)
            }
            InstanceKind::Clip(_) => {
                if self.button_mode(lib, id) && self.hit_test(lib, id, &m, point, depth) {
                    return Some(id);
                }
                let mut result = None;
                let mut found_depth = 0i32;
                for (child_depth, child) in inst.children.iter().rev() {
                    let Some(c) = self.arena.get(*child) else {
                        continue;
                    };
                    if c.clip_depth > 0 {
                        if result.is_some() && c.clip_depth >= found_depth {
                            let mask_m = m.concat(&c.matrix);
                            if self.hit_test(lib, *child, &mask_m, point, depth.saturating_add(1)) {
                                return result;
                            }
                            result = None;
                        }
                    } else if result.is_none() {
                        result = self.pick(lib, *child, &m, point, depth.saturating_add(1));
                        if result.is_some() {
                            found_depth = *child_depth;
                        }
                    }
                }
                result
            }
            _ => None,
        }
    }

    fn hit_test(
        &self,
        lib: &Library,
        id: InstanceId,
        m: &Matrix,
        point: [f32; 2],
        depth: u16,
    ) -> bool {
        let Some(inst) = self.arena.get(id) else {
            return false;
        };
        if depth > self.limits.max_tree_depth {
            return false;
        }
        match inst.kind {
            InstanceKind::Clip(_) | InstanceKind::Button(_) => inst.children.values().any(|c| {
                self.arena.get(*c).is_some_and(|ci| {
                    ci.visible()
                        && ci.clip_depth == 0
                        && self.hit_test(
                            lib,
                            *c,
                            &m.concat(&ci.matrix),
                            point,
                            depth.saturating_add(1),
                        )
                })
            }),
            _ => lib
                .character_bounds(inst.character, 0)
                .is_some_and(|b| m.rect_contains(b, point)),
        }
    }

    fn button_contains(&self, lib: &Library, id: InstanceId, m: &Matrix, point: [f32; 2]) -> bool {
        let Some(hit) = self.arena.get(id).and_then(|i| lib.button_hit(i.character)) else {
            return false;
        };
        match self.mouse.hit_mode {
            HitMode::Records => hit
                .records
                .iter()
                .any(|(rm, rect)| m.concat(rm).rect_contains(*rect, point)),
            HitMode::MeshBox => hit.mesh.is_some_and(|rect| m.rect_contains(rect, point)),
        }
    }

    pub(crate) fn hit_areas(&self, lib: &Library, out: &mut Vec<HitArea>) {
        out.clear();
        if let Some(root) = self.root {
            self.collect_hit_areas(lib, root, &Matrix::IDENTITY, 0, out);
        }
    }

    fn collect_hit_areas(
        &self,
        lib: &Library,
        id: InstanceId,
        parent: &Matrix,
        depth: u16,
        out: &mut Vec<HitArea>,
    ) {
        let Some(inst) = self.arena.get(id) else {
            return;
        };
        if !inst.visible() || depth > self.limits.max_tree_depth {
            return;
        }
        let m = parent.concat(&inst.matrix);
        if let (InstanceKind::Button(_), Some(hit)) = (&inst.kind, lib.button_hit(inst.character)) {
            let quad = |mm: &Matrix, r: [f32; 4]| {
                let corner = |x: f32, y: f32| {
                    let (px, py) = mm.apply(x, y);
                    [px, py]
                };
                HitArea {
                    button: id,
                    corners: [
                        corner(r[0], r[1]),
                        corner(r[2], r[1]),
                        corner(r[2], r[3]),
                        corner(r[0], r[3]),
                    ],
                }
            };
            match self.mouse.hit_mode {
                HitMode::Records => {
                    out.extend(hit.records.iter().map(|(rm, r)| quad(&m.concat(rm), *r)));
                }
                HitMode::MeshBox => out.extend(hit.mesh.map(|r| quad(&m, r))),
            }
        }
        for child in inst.children.values() {
            self.collect_hit_areas(lib, *child, &m, depth.saturating_add(1), out);
        }
    }
}
