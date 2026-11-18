use std::collections::BTreeSet;

use mint_app::{App, InputEvent, Key, PointerButton};
use mint_render::{
    Blend, Instance, Mat4, MeshData, MeshHandle, Renderer, Scene, UiQuad, Vec3, Vertex,
};
use mint_sage_asset::Resolver;
use mint_sage_particle::FlatGround;
use mint_sage_scene::ParticleScene;
use mint_ui::{PointerButton as UiButton, PointerRouting, Rect};
use mint_vfs::Vfs;

use crate::camera::ViewCamera;
use crate::sidebar::{self, Layout, RowHit, Text};
use crate::subject::Subject;
use crate::tree::{FxTree, NodeKind};
use crate::Result;

const STEP_HZ: f32 = 30.0;

const GRID_HALF: f32 = 300.0;
const GRID_STEP: f32 = 20.0;
const GRID_LINE: f32 = 0.18;

const MARKER_STEP: f32 = 10.0;
const MARKER_MIN: f32 = 10.0;
const MARKER_MAX: f32 = 400.0;

const FAR: f32 = 4000.0;

#[derive(Debug, Clone, Copy, Default)]
struct Held([bool; 6]);

impl Held {
    fn slot(key: char) -> Option<usize> {
        match key {
            'w' => Some(0),
            'a' => Some(1),
            's' => Some(2),
            'd' => Some(3),
            'q' => Some(4),
            'e' => Some(5),
            _ => None,
        }
    }

    fn set(&mut self, key: char, down: bool) -> bool {
        let Some(slot) = Self::slot(key) else {
            return false;
        };
        if let Some(held) = self.0.get_mut(slot) {
            *held = down;
        }
        true
    }

    fn release_all(&mut self) {
        self.0 = [false; 6];
    }

    fn axes(self) -> (f32, f32, f32) {
        let on = |slot: usize| f32::from(u8::from(self.0.get(slot).copied().unwrap_or(false)));
        (on(0) - on(2), on(3) - on(1), on(5) - on(4))
    }
}

struct Control {
    key: Key,
    keycap: &'static str,
    label: &'static str,
    rect: Rect,
}

const CONTROL_ROWS: [&[(Key, &str, &str, f32)]; 3] = [
    &[
        (Key::Space, "Space", "replay", 108.0),
        (Key::Character('p'), "P", "pause", 76.0),
        (Key::Character('n'), "N", "step", 68.0),
        (Key::Character('l'), "L", "loop", 66.0),
        (Key::Character('x'), "X", "clear", 72.0),
    ],
    &[
        (Key::Character('1'), "1", "0.1x", 66.0),
        (Key::Character('2'), "2", "0.25x", 74.0),
        (Key::Character('3'), "3", "0.5x", 66.0),
        (Key::Character('4'), "4", "1x", 54.0),
        (Key::Character('5'), "5", "2x", 54.0),
    ],
    &[
        (Key::Character('m'), "M", "model", 80.0),
        (Key::Character('g'), "G", "grid", 66.0),
        (Key::Character('r'), "R", "camera", 86.0),
        (Key::Character('h'), "H", "hide", 66.0),
    ],
];

pub struct Viewer {
    scene: ParticleScene,
    tree: FxTree,
    vfs: Vfs,
    objects: mint_sage_ini::ObjectIndex,
    subject: Subject,
    bones: (usize, usize),
    resolver: Resolver,
    text: Text,
    router: mint_ui::InputRouter,
    widgets: mint_ui::WidgetState,
    filter: String,
    expanded: BTreeSet<usize>,
    hidden: BTreeSet<usize>,
    solo: Option<usize>,
    selected: Option<usize>,
    rows: Vec<usize>,
    camera: ViewCamera,
    viewport: (u32, u32),
    pointer: [f32; 2],
    panning: bool,
    looking: bool,
    help: bool,
    pending: Option<(f32, usize)>,
    held: Held,
    fast: bool,
    paused: bool,
    step_once: bool,
    speed: f32,
    looping: bool,
    marker_distance: f32,
    marker_bearing: f32,
    grid: bool,
    grid_mesh: Option<MeshHandle>,
    marker_mesh: Option<MeshHandle>,
    attached: Vec<mint_sage_particle::SystemId>,
    instances: Vec<Instance>,
    quads: Vec<UiQuad>,
    want_fire: bool,
    want_subject: bool,
}

impl Viewer {
    pub fn new(
        renderer: &mut Renderer,
        scene: ParticleScene,
        tree: FxTree,
        vfs: Vfs,
        objects: mint_sage_ini::ObjectIndex,
        viewport: (u32, u32),
        select: Option<&str>,
    ) -> Result<Self> {
        let text = Text::new(renderer)?;
        let grid_mesh = renderer.upload_mesh(&grid_mesh()).ok();
        let marker_mesh = renderer.upload_mesh(&marker_mesh()).ok();
        let mut viewer = Self {
            scene,
            tree,
            vfs,
            objects,
            subject: Subject::new(),
            bones: (0, 0),
            resolver: Resolver::new(),
            text,
            router: mint_ui::InputRouter::new(),
            widgets: mint_ui::WidgetState::new(),
            filter: String::new(),
            expanded: BTreeSet::new(),
            hidden: BTreeSet::new(),
            solo: None,
            selected: None,
            rows: Vec::new(),
            camera: ViewCamera::default(),
            viewport,
            pointer: [0.0, 0.0],
            panning: false,
            looking: false,
            help: true,
            pending: None,
            held: Held::default(),
            fast: false,
            paused: false,
            step_once: false,
            speed: 1.0,
            looping: false,
            marker_distance: 60.0,
            marker_bearing: 0.0,
            grid: true,
            grid_mesh,
            marker_mesh,
            attached: Vec::new(),
            instances: Vec::new(),
            quads: Vec::new(),
            want_fire: false,
            want_subject: false,
        };
        for category in crate::tree::Category::ALL {
            if let Some(header) = viewer.tree.category(category) {
                viewer.expanded.insert(header);
            }
        }
        if let Some(name) = select {
            viewer.select_by_name(name);
        }
        Ok(viewer)
    }

    fn select_by_name(&mut self, name: &str) {
        let Some(root) = self.tree.find_root(name) else {
            println!("select: no FXList or particle system named {name}");
            return;
        };
        self.selected = Some(root);
        let mut current = self.tree.node(root).and_then(|node| node.parent);
        for _ in 0..8 {
            let Some(index) = current else {
                break;
            };
            self.expanded.insert(index);
            current = self.tree.node(index).and_then(|node| node.parent);
        }
        self.print_status();
    }

    fn select_row_containing(&mut self, text: &str) -> bool {
        let Some(root) = self.selected else {
            return false;
        };
        let needle = text.to_ascii_lowercase();
        let mut pending = vec![root];
        while let Some(index) = pending.pop() {
            let Some(node) = self.tree.node(index) else {
                continue;
            };
            if index != root && node.line().to_ascii_lowercase().contains(&needle) {
                self.selected = Some(index);
                return true;
            }
            pending.extend(node.children.iter().rev().copied());
        }
        false
    }

    pub fn play_headless(
        &mut self,
        renderer: &mut Renderer,
        name: &str,
        row: Option<&str>,
        frames: u32,
    ) -> mint_render::Result<()> {
        self.select_by_name(name);
        if let Some(row) = row {
            if !self.select_row_containing(row) {
                println!("play: no row under {name} contains {row}");
            }
        }
        self.want_subject = true;
        self.want_fire = true;
        for frame in 0..frames {
            self.draw(renderer, 0.0)?;
            if frame % 15 == 0 {
                println!("frame {frame}: {}", self.status());
                for (name, count, mean) in self.scene.live_centroids_by_name() {
                    println!(
                        "  {name}: {count} at ({:.1}, {:.1}, {:.1})",
                        mean.x, mean.y, mean.z
                    );
                }
            }
        }
        Ok(())
    }

    fn marker(&self) -> Vec3 {
        let (sin, cos) = mint_math::f32::sin_cos(self.marker_bearing);
        Vec3::new(self.marker_distance * cos, self.marker_distance * sin, 0.0)
    }

    fn firable(&self, index: usize) -> Option<usize> {
        let mut current = Some(index);
        for _ in 0..32 {
            let node = self.tree.node(current?)?;
            if matches!(node.kind, NodeKind::FxList | NodeKind::System) {
                return current;
            }
            current = node.parent;
        }
        None
    }

    fn state_row(&self, index: usize) -> Option<usize> {
        let mut current = Some(index);
        for _ in 0..32 {
            let node = self.tree.node(current?)?;
            if !node.conditions.is_empty() {
                return current;
            }
            current = node.parent;
        }
        None
    }

    fn select_subject(&mut self, renderer: &mut Renderer, index: usize) {
        let Some(name) = self.tree.object_of(index).map(str::to_owned) else {
            return;
        };
        self.subject
            .show(renderer, &self.vfs, &self.resolver, &self.objects, &name);
        let conditions = self
            .state_row(index)
            .and_then(|row| self.tree.node(row))
            .map(|node| node.conditions.clone())
            .unwrap_or_default();
        self.subject.set_conditions(&conditions);
    }

    fn fire(&mut self, renderer: &mut Renderer) {
        let Some(selected) = self.selected else {
            println!("fire: nothing selected");
            return;
        };
        self.attached.clear();
        self.scene.clear();
        self.subject.restart();
        self.pending = None;
        if let Some(pose) = self.state_row(selected) {
            if let Some(node) = self.tree.node(pose) {
                if node.kind == NodeKind::Weapon {
                    self.pending = Some((node.fire_after.unwrap_or(0.0), selected));
                    println!(
                        "fire: pose {} now, {} after {:.2} s",
                        node.conditions.join(" "),
                        node.name,
                        node.fire_after.unwrap_or(0.0)
                    );
                } else {
                    println!("fire: the object replays its state");
                }
            }
            self.apply_visibility();
            self.print_status();
            return;
        }
        self.fire_row(renderer, selected);
        self.apply_visibility();
        self.print_status();
    }

    fn fire_pending(&mut self, renderer: &mut Renderer, selected: usize) {
        let is_weapon = self
            .tree
            .node(selected)
            .is_some_and(|node| node.kind == NodeKind::Weapon);
        let rows = if is_weapon {
            self.tree
                .node(selected)
                .map(|node| node.children.clone())
                .unwrap_or_default()
        } else {
            vec![selected]
        };
        for row in rows {
            self.fire_row(renderer, row);
        }
        self.apply_visibility();
    }

    fn fire_row(&mut self, renderer: &mut Renderer, row: usize) {
        let Some(index) = self.firable(row) else {
            return;
        };
        let Some(node) = self.tree.node(index).cloned() else {
            return;
        };
        let ground = FlatGround(0.0);
        match node.kind {
            NodeKind::FxList => {
                let caller = self.subject.drawable();
                let at = caller.map_or(Vec3::ZERO, |caller| {
                    caller.placement().transform_point(Vec3::ZERO)
                });
                let report = self.scene.fire_list(
                    renderer,
                    &self.vfs,
                    &self.resolver,
                    &node.name,
                    at,
                    Some(self.marker()),
                    &ground,
                    caller,
                    None,
                    &|_, _| false,
                    mint_sage_ini::NuggetWeather::Normal,
                );
                if let Some(report) = report.as_ref() {
                    self.bones = (report.at_bone_pos, report.bone_missing);
                }
                match report {
                    Some(report) => println!(
                        "fire: FXList {} - {} system(s), {} FXListAtBonePos ({} with no bone, \
                         {} naming no list), {} camera shake(s)",
                        node.name,
                        report.systems,
                        report.at_bone_pos,
                        report.bone_missing,
                        report.nested_list_missing,
                        report.camera_shakes.len()
                    ),
                    None => println!("fire: no FXList named {}", node.name),
                }
            }
            NodeKind::System => {
                match self.scene.attach_named(
                    renderer,
                    &self.vfs,
                    &self.resolver,
                    &node.name,
                    Mat4::IDENTITY,
                ) {
                    Some(id) => {
                        self.attached.push(id);
                        println!("fire: particle system {}", node.name);
                    }
                    None => println!("fire: no particle system named {}", node.name),
                }
            }
            _ => {}
        }
    }

    fn apply_visibility(&mut self) {
        self.scene.clear_hidden();
        if let Some(solo) = self.solo {
            let kept = self.tree.subtree_systems(solo);
            for (name, _) in self.scene.live_systems_by_name() {
                if !kept.contains(&name.to_ascii_lowercase()) {
                    self.scene.set_system_hidden(&name, true);
                }
            }
        }
        for row in &self.hidden {
            for name in self.tree.subtree_systems(*row) {
                self.scene.set_system_hidden(&name, true);
            }
        }
    }

    fn status(&self) -> String {
        self.status_lines().join(" | ")
    }

    fn status_lines(&self) -> Vec<String> {
        let Some(selected) = self.selected else {
            return vec!["nothing selected - click a row to play it".to_owned()];
        };
        let live: std::collections::BTreeMap<String, usize> = self
            .scene
            .live_systems_by_name()
            .into_iter()
            .map(|(name, count)| (name.to_ascii_lowercase(), count))
            .collect();
        let layers: Vec<String> = self
            .tree
            .node(selected)
            .map(|node| node.children.clone())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|child| {
                let node = self.tree.node(child)?;
                let count: usize = self
                    .tree
                    .subtree_systems(child)
                    .iter()
                    .filter_map(|name| live.get(name))
                    .sum();
                Some(format!("{} {count}", node.name, count = count))
            })
            .collect();
        vec![
            format!("selected: {}", self.tree.path(selected)),
            self.subject.status(),
            format!(
                "bones {} found, {} missing | live: {} particles in {} systems | layers: {}",
                self.bones.0.saturating_sub(self.bones.1),
                self.bones.1,
                self.scene.live_particles(),
                self.scene.live_systems(),
                if layers.is_empty() {
                    "(none)".to_owned()
                } else {
                    layers.join(", ")
                }
            ),
        ]
    }

    fn print_status(&self) {
        println!("{}", self.status());
    }

    fn print_mode(&self) {
        println!(
            "mode: {} speed {}x {} marker {:.0} units bearing {:.2} rad grid {} | {}",
            if self.paused { "paused" } else { "running" },
            self.speed,
            if self.looping { "loop" } else { "once" },
            self.marker_distance,
            self.marker_bearing,
            if self.grid { "on" } else { "off" },
            self.camera.report()
        );
    }

    fn hotkey(&mut self, key: Key, pressed: bool) {
        if let Key::Character(c) = key {
            if self.held.set(c, pressed) {
                return;
            }
        }
        if key == Key::Shift {
            self.fast = pressed;
            return;
        }
        if !pressed {
            return;
        }
        match key {
            Key::Space => self.want_fire = true,
            Key::Character('r') => {
                self.camera.reset();
                println!("{}", self.camera.report());
            }
            Key::Character('h') => self.help = !self.help,
            Key::Character('x') => {
                self.attached.clear();
                self.scene.clear();
                println!("clear: every effect removed");
            }
            Key::Character('1') => self.set_speed(0.1),
            Key::Character('2') => self.set_speed(0.25),
            Key::Character('3') => self.set_speed(0.5),
            Key::Character('4') => self.set_speed(1.0),
            Key::Character('5') => self.set_speed(2.0),
            Key::Character('[') => self.move_marker(-MARKER_STEP, 0.0),
            Key::Character(']') => self.move_marker(MARKER_STEP, 0.0),
            Key::Character(',') => self.move_marker(0.0, -0.2),
            Key::Character('.') => self.move_marker(0.0, 0.2),
            Key::CtrlCharacter('p') | Key::Character('p') => {
                self.paused = !self.paused;
                self.print_mode();
            }
            Key::CtrlCharacter('n') | Key::Character('n') => {
                self.paused = true;
                self.step_once = true;
                println!("step: one simulation frame");
            }
            Key::CtrlCharacter('l') | Key::Character('l') => {
                self.looping = !self.looping;
                self.print_mode();
            }
            Key::CtrlCharacter('g') | Key::Character('g') => {
                self.grid = !self.grid;
                self.print_mode();
            }
            Key::CtrlCharacter('h') => {
                self.hidden.clear();
                self.apply_visibility();
                println!("hide: everything shown again");
            }
            Key::CtrlCharacter('j') => {
                self.solo = None;
                self.apply_visibility();
                println!("solo: cleared");
            }
            Key::CtrlCharacter('t') => {
                if let Some(selected) = self.selected {
                    println!(
                        "--- {} ---\n{}",
                        self.tree.path(selected),
                        self.tree.render(selected)
                    );
                }
            }
            Key::CtrlCharacter('m') | Key::Character('m') => {
                let visible = !self.subject.visible();
                self.subject.set_visible(visible);
                self.print_status();
            }
            Key::CtrlCharacter('k') => println!("{}", crate::cli::KEYS),
            Key::Escape => self.router.clear_focus(),
            _ => {}
        }
    }

    fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
        self.print_mode();
    }

    fn move_marker(&mut self, distance: f32, bearing: f32) {
        self.marker_distance = (self.marker_distance + distance).clamp(MARKER_MIN, MARKER_MAX);
        self.marker_bearing += bearing;
        self.print_mode();
    }

    fn click_row(&mut self, position: [f32; 2]) {
        let layout = Layout::new(self.height(), self.rows.len());
        let visible = layout.list.visible_rows(&self.widgets);
        for index in visible {
            let rect = layout.list.row_rect(index, &self.widgets);
            if !rect.contains(position) {
                continue;
            }
            let Some(node) = self.rows.get(index).copied() else {
                return;
            };
            match sidebar::row_hit(position[0]) {
                RowHit::Eye => {
                    if !self.hidden.remove(&node) {
                        self.hidden.insert(node);
                    }
                    self.apply_visibility();
                    println!(
                        "hide: {} {}",
                        self.tree.path(node),
                        if self.hidden.contains(&node) {
                            "hidden"
                        } else {
                            "shown"
                        }
                    );
                }
                RowHit::Solo => {
                    self.solo = if self.solo == Some(node) {
                        None
                    } else {
                        Some(node)
                    };
                    self.apply_visibility();
                    println!(
                        "solo: {}",
                        self.solo
                            .map_or_else(|| "cleared".to_owned(), |row| self.tree.path(row))
                    );
                }
                RowHit::Name => {
                    self.selected = Some(node);
                    self.want_subject = true;
                    self.want_fire = true;
                    if self
                        .tree
                        .node(node)
                        .is_some_and(|node| !node.children.is_empty())
                        && !self.expanded.remove(&node)
                    {
                        self.expanded.insert(node);
                    }
                    self.print_status();
                }
            }
            return;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn height(&self) -> f32 {
        self.viewport.1 as f32
    }

    fn build_sidebar(&mut self, renderer: &mut Renderer) {
        use sidebar::color;
        self.rows = self.tree.rows(&self.filter, &self.expanded);
        let layout = Layout::new(self.height(), self.rows.len());
        self.quads.clear();
        self.text.begin();
        self.quads.push(sidebar::fill(layout.panel, color::PANEL));
        self.quads.push(sidebar::fill(
            Rect {
                min: [layout.panel.max[0] - 1.0, 0.0],
                max: layout.panel.max,
            },
            color::EDGE,
        ));

        let focused = self.router.focused() == Some(sidebar::FILTER_ID);
        self.quads.push(sidebar::fill(
            Rect {
                min: [layout.filter.min[0] - 1.0, layout.filter.min[1] - 1.0],
                max: [layout.filter.max[0] + 1.0, layout.filter.max[1] + 1.0],
            },
            if focused { color::ACCENT } else { color::EDGE },
        ));
        self.quads.push(sidebar::fill(
            layout.filter,
            if focused {
                color::FIELD_FOCUS
            } else {
                color::FIELD
            },
        ));
        let (prompt, tint) = if self.filter.is_empty() && !focused {
            ("Search - click here and type".to_owned(), color::DIM)
        } else {
            (
                format!("{}{}", self.filter, if focused { "|" } else { "" }),
                color::TEXT,
            )
        };
        let quads = self.text.line(
            &prompt,
            [layout.filter.min[0] + 10.0, layout.filter.max[1] - 11.0],
            tint,
        );
        self.quads.extend(quads);

        let viewport = layout.list.viewport;
        let clip = [
            viewport.min[0],
            viewport.min[1],
            viewport.max[0],
            viewport.max[1],
        ];
        let text_clip = [clip[0], clip[1], sidebar::TEXT_RIGHT, clip[3]];
        let pointer_in_list = viewport.contains(self.pointer);
        for index in layout.list.visible_rows(&self.widgets) {
            let Some(node_index) = self.rows.get(index).copied() else {
                continue;
            };
            let Some(node) = self.tree.node(node_index).cloned() else {
                continue;
            };
            let rect = layout.list.row_rect(index, &self.widgets);
            let top = rect.min[1];
            let hovered = pointer_in_list && rect.contains(self.pointer);
            let selected = self.selected == Some(node_index);
            let mut row: Vec<UiQuad> = Vec::new();
            if selected {
                row.push(sidebar::fill(rect, color::SELECTED));
                row.push(sidebar::fill(
                    Rect {
                        min: rect.min,
                        max: [rect.min[0] + 3.0, rect.max[1]],
                    },
                    color::ACCENT,
                ));
            } else if hovered {
                row.push(sidebar::fill(rect, color::HOVER));
            } else if matches!(node.kind, NodeKind::Category(_)) {
                row.push(sidebar::fill(rect, color::HEADER));
            }
            row.extend(sidebar::guides(top, node.depth));
            if !node.children.is_empty() {
                row.extend(sidebar::chevron(
                    top,
                    node.depth,
                    self.expanded.contains(&node_index),
                ));
            }
            row.extend(sidebar::toggles(
                top,
                self.hidden.contains(&node_index),
                self.solo == Some(node_index),
                hovered,
            ));
            for quad in row {
                self.quads.push(quad.with_clip(clip));
            }

            let baseline = rect.max[1] - 7.0;
            let mut pen = sidebar::label_x(node.depth);
            let role = node.role.label();
            let name_tint = if self.hidden.contains(&node_index) {
                color::DIM
            } else {
                sidebar::row_color(node.kind)
            };
            let mut runs: Vec<(String, [f32; 4])> = Vec::new();
            if !role.is_empty() {
                runs.push((role.to_owned(), color::DIM));
            }
            runs.push((node.name.clone(), name_tint));
            if !node.detail.is_empty() {
                runs.push((node.detail.clone(), color::DIM));
            }
            for (text, tint) in runs {
                let (quads, end) = self.text.run(&text, [pen, baseline], tint);
                pen = end + 7.0;
                for quad in quads {
                    self.quads.push(quad.with_clip(text_clip));
                }
            }
        }

        self.build_overlays();
        self.text.flush(renderer, &mut self.quads);
    }

    #[allow(clippy::cast_precision_loss)]
    fn build_overlays(&mut self) {
        use sidebar::color;
        const LINE: f32 = 20.0;
        let (width, height) = (self.viewport.0 as f32, self.height());
        let lines = self.status_lines();
        let bar = LINE * lines.len() as f32 + 14.0;
        self.quads.push(sidebar::fill(
            Rect {
                min: [sidebar::WIDTH, height - bar],
                max: [width, height],
            },
            color::OVERLAY,
        ));
        let clip = [sidebar::WIDTH, 0.0, width, height];
        for (row, line) in lines.iter().enumerate() {
            let baseline = height - bar + 8.0 + LINE * (row as f32 + 1.0) - 6.0;
            let tint = if row == 0 { color::TEXT } else { color::DIM };
            for quad in self
                .text
                .line(line, [sidebar::WIDTH + 14.0, baseline], tint)
            {
                self.quads.push(quad.with_clip(clip));
            }
        }

        let pointer = self.pointer;
        for control in self.controls() {
            let lit = self.control_on(control.key);
            let hovered = control.rect.contains(pointer);
            self.quads.push(sidebar::fill(
                control.rect,
                if lit {
                    color::SELECTED
                } else if hovered {
                    color::FIELD_FOCUS
                } else {
                    color::OVERLAY
                },
            ));
            self.quads.push(sidebar::fill(
                Rect {
                    min: [control.rect.min[0], control.rect.max[1] - 2.0],
                    max: control.rect.max,
                },
                if lit { color::ACCENT } else { color::EDGE },
            ));
            let baseline = control.rect.max[1] - 9.0;
            let (quads, end) = self.text.run(
                control.keycap,
                [control.rect.min[0] + 8.0, baseline],
                color::ACCENT,
            );
            self.quads.extend(quads);
            let quads = self
                .text
                .line(control.label, [end + 6.0, baseline], color::TEXT);
            self.quads.extend(quads);
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn controls(&self) -> Vec<Control> {
        const HEIGHT: f32 = 28.0;
        const GAP: f32 = 6.0;
        let right = self.viewport.0 as f32 - 12.0;
        let mut out = Vec::new();
        let rows: &[&[(Key, &str, &str, f32)]] = if self.help {
            &CONTROL_ROWS
        } else {
            &[&[(Key::Character('h'), "H", "controls", 100.0)]]
        };
        for (row, buttons) in rows.iter().enumerate() {
            let top = 12.0 + (HEIGHT + GAP) * row as f32;
            let mut x = right;
            for (key, keycap, label, width) in buttons.iter().rev() {
                x -= width;
                out.push(Control {
                    key: *key,
                    keycap,
                    label,
                    rect: Rect {
                        min: [x, top],
                        max: [x + width, top + HEIGHT],
                    },
                });
                x -= GAP;
            }
        }
        out
    }

    fn control_at(&self, point: [f32; 2]) -> Option<Key> {
        self.controls()
            .into_iter()
            .find(|control| control.rect.contains(point))
            .map(|control| control.key)
    }

    #[allow(clippy::float_cmp)]
    fn control_on(&self, key: Key) -> bool {
        match key {
            Key::Character('p') => self.paused,
            Key::Character('l') => self.looping,
            Key::Character('m') => self.subject.visible(),
            Key::Character('g') => self.grid,
            Key::Character('1') => self.speed == 0.1,
            Key::Character('2') => self.speed == 0.25,
            Key::Character('3') => self.speed == 0.5,
            Key::Character('4') => self.speed == 1.0,
            Key::Character('5') => self.speed == 2.0,
            _ => false,
        }
    }

    fn build_instances(&mut self) {
        self.instances.clear();
        if self.grid {
            if let Some(mesh) = self.grid_mesh {
                self.instances.push(Instance {
                    mesh,
                    transform: Mat4::IDENTITY,
                    tint: [0.30, 0.33, 0.38, 1.0],
                    ..Instance::new(mesh)
                });
            }
        }
        if let Some(mesh) = self.marker_mesh {
            let at = self.marker();
            self.instances.push(Instance {
                transform: Mat4::from_translation(at),
                tint: [1.0, 0.65, 0.2, 1.0],
                blend: Blend::Opaque,
                ..Instance::new(mesh)
            });
        }
    }

    fn take_fire(&mut self) -> bool {
        std::mem::take(&mut self.want_fire)
    }

    fn play_events(&mut self, renderer: &mut Renderer, events: &[(u32, String)]) {
        if events.is_empty() {
            return;
        }
        let ground = FlatGround(0.0);
        for (_, name) in events {
            let caller = self.subject.drawable();
            let at = caller.map_or(Vec3::ZERO, |caller| {
                caller.placement().transform_point(Vec3::ZERO)
            });
            self.scene.fire_list(
                renderer,
                &self.vfs,
                &self.resolver,
                name,
                at,
                Some(self.marker()),
                &ground,
                caller,
                None,
                &|_, _| false,
                mint_sage_ini::NuggetWeather::Normal,
            );
        }
    }
}

impl App for Viewer {
    fn input(&mut self, event: &InputEvent) {
        let layout = Layout::new(self.height(), self.rows.len());
        let targets = layout.targets();
        match *event {
            InputEvent::Resized { width, height } => self.viewport = (width, height),
            InputEvent::PointerMoved { x, y } => {
                let last = self.pointer;
                self.pointer = [x, y];
                let _ = self.router.pointer_moved(&targets, self.pointer);
                let (dx, dy) = (x - last[0], y - last[1]);
                if self.looking {
                    self.camera.rotate(dx, dy);
                } else if self.panning {
                    self.camera.pan(dx, dy);
                }
            }
            InputEvent::PointerButton { button, pressed } => {
                let ui_button = match button {
                    PointerButton::Secondary => UiButton::Secondary,
                    PointerButton::Middle => UiButton::Middle,
                    _ => UiButton::Primary,
                };
                let routing =
                    self.router
                        .pointer_button(&targets, self.pointer, ui_button, pressed);
                if pressed && routing != PointerRouting::Widget(sidebar::FILTER_ID) {
                    self.router.clear_focus();
                }
                match routing {
                    PointerRouting::Widget(id) => {
                        if id == sidebar::LIST_ID && pressed && button == PointerButton::Primary {
                            self.click_row(self.pointer);
                        }
                    }
                    PointerRouting::World => match button {
                        PointerButton::Primary if pressed => match self.control_at(self.pointer) {
                            Some(key) => self.hotkey(key, true),
                            None => self.panning = true,
                        },
                        PointerButton::Primary => self.panning = false,
                        PointerButton::Secondary => self.looking = pressed,
                        _ => {}
                    },
                }
                if !pressed {
                    match button {
                        PointerButton::Primary => self.panning = false,
                        PointerButton::Secondary => self.looking = false,
                        _ => {}
                    }
                }
            }
            InputEvent::Scroll { lines } => {
                if layout.panel.contains(self.pointer) {
                    layout
                        .list
                        .scroll_by(&mut self.widgets, -lines * sidebar::ROW_HEIGHT * 3.0);
                } else {
                    self.camera.zoom(lines);
                }
            }
            InputEvent::Key { key, pressed } => {
                if self.router.focused() == Some(sidebar::FILTER_ID) {
                    self.held.release_all();
                    match key {
                        Key::Escape | Key::Enter if pressed => self.router.clear_focus(),
                        Key::Backspace if pressed => {
                            self.filter.pop();
                        }
                        _ => {}
                    }
                    return;
                }
                self.hotkey(key, pressed);
            }
            InputEvent::TextInput { character } => {
                if self.router.focused() == Some(sidebar::FILTER_ID) && !character.is_control() {
                    self.filter.push(character);
                }
            }
            _ => {}
        }
    }

    fn frame(&mut self, dt: f32) {
        if self.router.focused() != Some(sidebar::FILTER_ID) {
            self.camera.travel(self.held.axes(), self.fast, dt);
        }
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        if std::mem::take(&mut self.want_subject) {
            if let Some(selected) = self.selected {
                self.select_subject(renderer, selected);
            }
        }
        if self.take_fire() {
            self.fire(renderer);
        }
        let ground = FlatGround(0.0);
        let step = if self.step_once {
            self.step_once = false;
            1.0 / STEP_HZ
        } else if self.paused {
            0.0
        } else {
            self.speed / STEP_HZ
        };
        if let Some((left, row)) = self.pending {
            let left = left - step;
            if left <= 0.0 && step > 0.0 {
                self.pending = None;
                self.fire_pending(renderer, row);
            } else {
                self.pending = Some((left, row));
            }
        }
        self.scene.advance(step, &ground);
        if self.looping && self.pending.is_none() && self.scene.live_systems() == 0 {
            self.fire(renderer);
        }

        self.subject.face(self.marker_bearing);
        self.build_instances();
        let mut instances = std::mem::take(&mut self.instances);
        let events = self.subject.advance(
            renderer,
            &self.vfs,
            &self.resolver,
            &mut self.scene,
            step,
            &mut instances,
        );
        self.instances = instances;
        self.play_events(renderer, &events);
        renderer.set_bones(self.subject.bones())?;

        let camera = self.camera.camera(FAR);
        let mut particles = std::mem::take(&mut self.instances);
        self.scene.draw(renderer, &camera, &mut particles);
        self.instances = particles;

        self.build_sidebar(renderer);
        let scene = Scene::new(camera)
            .with_instances(&self.instances)
            .with_ui_quads(&self.quads);
        renderer.render(&scene)
    }
}

fn grid_mesh() -> MeshData {
    let mut mesh = MeshData::new();
    let mut line = |a: Vec3, b: Vec3, across: Vec3| {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
        for point in [
            Vec3::new(a.x - across.x, a.y - across.y, 0.0),
            Vec3::new(b.x - across.x, b.y - across.y, 0.0),
            Vec3::new(b.x + across.x, b.y + across.y, 0.0),
            Vec3::new(a.x + across.x, a.y + across.y, 0.0),
        ] {
            mesh.vertices.push(Vertex {
                position: point.to_array(),
                normal: [0.0, 0.0, 1.0],
                ..Vertex::default()
            });
        }
        mesh.indices.extend([
            base,
            base.saturating_add(1),
            base.saturating_add(2),
            base,
            base.saturating_add(2),
            base.saturating_add(3),
        ]);
    };
    let mut at = -GRID_HALF;
    while at <= GRID_HALF {
        line(
            Vec3::new(at, -GRID_HALF, 0.0),
            Vec3::new(at, GRID_HALF, 0.0),
            Vec3::new(GRID_LINE, 0.0, 0.0),
        );
        line(
            Vec3::new(-GRID_HALF, at, 0.0),
            Vec3::new(GRID_HALF, at, 0.0),
            Vec3::new(0.0, GRID_LINE, 0.0),
        );
        at += GRID_STEP;
    }
    mesh
}

fn marker_mesh() -> MeshData {
    let (hx, hy, height) = (1.2f32, 1.2f32, 26.0f32);
    let corners = [
        Vec3::new(-hx, -hy, 0.0),
        Vec3::new(hx, -hy, 0.0),
        Vec3::new(hx, hy, 0.0),
        Vec3::new(-hx, hy, 0.0),
    ];
    let mut mesh = MeshData::new();
    for side in 0..4usize {
        let Some(a) = corners.get(side) else { continue };
        let Some(b) = corners.get(side.saturating_add(1) % 4) else {
            continue;
        };
        let normal = Vec3::new(b.y - a.y, a.x - b.x, 0.0).normalize().to_array();
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
        for point in [
            Vec3::new(a.x, a.y, 0.0),
            Vec3::new(b.x, b.y, 0.0),
            Vec3::new(b.x, b.y, height),
            Vec3::new(a.x, a.y, height),
        ] {
            mesh.vertices.push(Vertex {
                position: point.to_array(),
                normal,
                ..Vertex::default()
            });
        }
        mesh.indices.extend([
            base,
            base.saturating_add(1),
            base.saturating_add(2),
            base,
            base.saturating_add(2),
            base.saturating_add(3),
        ]);
    }
    mesh
}
