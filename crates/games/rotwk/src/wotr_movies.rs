use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mint_app::InputEvent;
use mint_render::{Renderer, ScreenDraw};
use mint_sage_apt_player::expose::{resolve_path, ImageKey};
use mint_sage_apt_player::{InstanceId, InstanceKind, Limits, MovieEvent, SharedGlobal, Value};
use mint_sage_apt_viewer::host::MovieView;
use mint_vfs::Vfs;

use crate::apt_overlay::{Hosted, ImageResolver, GUI_FX_READY};
use crate::hud::ImageSource;
use crate::in_game_menu::{InGameMenus, MENU_READY, QUIT_MENU_MOVIE};
use crate::ui_resources::UiResources;
use crate::wotr_hud::ClipEvent;
use crate::wotr_narration::Narration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Hud,
    Prompt,
    Award,
    Dialog,
    Swapper,
    Veterancy,
    Message,
}

impl Kind {
    pub(crate) const ALL: [Kind; 7] = [
        Kind::Hud,
        Kind::Prompt,
        Kind::Award,
        Kind::Dialog,
        Kind::Swapper,
        Kind::Veterancy,
        Kind::Message,
    ];

    pub(crate) fn movie(self) -> &'static str {
        match self {
            Kind::Hud => "StrategicHUD",
            Kind::Prompt => "StrategicBattlePrompt",
            Kind::Award => "StrategicRegionAward",
            Kind::Dialog => "StrategicDynamicAutoResolve",
            Kind::Swapper => "StrategicArmyUnitSwapper",
            Kind::Veterancy => "StrategicVeterancy",
            Kind::Message => "GuiFX",
        }
    }

    fn ready_event(self) -> &'static str {
        match self {
            Kind::Message => GUI_FX_READY,
            _ => "",
        }
    }

    pub(crate) fn lookup(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "hud" => Some(Kind::Hud),
            "prompt" => Some(Kind::Prompt),
            "award" => Some(Kind::Award),
            "dialog" => Some(Kind::Dialog),
            "swapper" => Some(Kind::Swapper),
            "veterancy" => Some(Kind::Veterancy),
            "message" => Some(Kind::Message),
            other => Self::ALL
                .into_iter()
                .find(|k| k.movie().eq_ignore_ascii_case(other)),
        }
    }

    fn modal(self) -> bool {
        !matches!(self, Kind::Hud)
    }

    fn in_dialog_frame(self) -> bool {
        matches!(
            self,
            Kind::Prompt | Kind::Dialog | Kind::Award | Kind::Swapper
        )
    }

    fn is_gui_fx(self) -> bool {
        matches!(self, Kind::Message)
    }
}

const IN_GAME: (&str, &str) = ("InGame", "1");

pub(crate) const MESSAGE_CLIP: &str = "StrategicMessageBox";

pub(crate) const OK_BUTTON_CLIP: &str = "StrategicMessageBox.ButtonsOk.MessageBoxOk";

const DIALOG_FRAME: &str = "dlgFrame.content";

const MESSAGE_LARGE_ABOVE_CHARS: usize = 256;

fn message_key(part: &str) -> String {
    format!("_level0.{MESSAGE_CLIP}_{part}")
}

fn declare_externs(movie: &mut Hosted) {
    movie.expose.set_extern(IN_GAME.0, Value::str(IN_GAME.1));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) kind: Kind,
    pub(crate) event: ClipEvent,
}

pub(crate) struct Movies {
    hosted: Vec<(Kind, Hosted, bool)>,
    strings: Arc<mint_sage_str::StringTable>,
    fonts: Arc<std::collections::HashMap<String, mint_ui::Font>>,
    pub(crate) narration: Narration,
    pub(crate) menus: InGameMenus,
    images: ImageResolver,
    pointer: (f32, f32),
}

impl Movies {
    pub(crate) fn load(
        vfs: &Arc<Vfs>,
        installs: &[PathBuf],
        mapped_images: Arc<mint_sage_ini::MappedImageIndex>,
        window: (f32, f32),
        renderer: &mut Renderer,
    ) -> Result<Self, String> {
        let ui = UiResources::load(installs);
        println!(
            "wotr-hud: {} movie(s) decoded, {} string file(s) ({} failed)",
            ui.movies, ui.string_files, ui.string_failures
        );
        let mut hosted = Vec::new();
        let shared = SharedGlobal::new();
        for kind in Kind::ALL {
            let mut view = MovieView::load(
                Arc::clone(&ui.library),
                installs.to_vec(),
                kind.movie(),
                Arc::clone(&ui.fonts),
                Arc::clone(&ui.strings),
                window,
                Limits::default(),
                renderer,
            )
            .map_err(|error| format!("{}: {error}", kind.movie()))?;
            view.share_global(&shared);
            let mut movie = Hosted::new(view, kind.movie(), kind.ready_event());
            if !kind.is_gui_fx() {
                declare_externs(&mut movie);
            }
            hosted.push((kind, movie, kind == Kind::Hud));
        }
        let mut menu = |name: &'static str| {
            MovieView::load(
                Arc::clone(&ui.library),
                installs.to_vec(),
                name,
                Arc::clone(&ui.fonts),
                Arc::clone(&ui.strings),
                window,
                Limits::default(),
                renderer,
            )
            .map_err(|error| println!("wotr-hud: {name} will not open - {error}"))
            .ok()
            .map(|view| Hosted::new(view, name, MENU_READY))
        };
        let menus = InGameMenus::new(menu(QUIT_MENU_MOVIE), None, None, &ui.strings)
            .with_save_load(
                menu(crate::save_load::MOVIE),
                Arc::clone(vfs),
                Arc::clone(&ui.strings),
            )
            .for_session();
        Ok(Self {
            hosted,
            menus,
            strings: Arc::clone(&ui.strings),
            fonts: Arc::clone(&ui.fonts),
            narration: Narration::default(),
            images: ImageResolver::new(
                Arc::clone(vfs),
                mapped_images,
                Arc::new(mint_sage_ini::CommandButtonIndex::new()),
            ),
            pointer: (0.0, 0.0),
        })
    }

    fn get(&mut self, kind: Kind) -> Option<&mut (Kind, Hosted, bool)> {
        self.hosted.iter_mut().find(|(k, _, _)| *k == kind)
    }

    pub(crate) fn hosted(&mut self, kind: Kind) -> Option<&mut Hosted> {
        self.get(kind).map(|entry| &mut entry.1)
    }

    pub(crate) fn is_up(&self, kind: Kind) -> bool {
        self.hosted.iter().any(|(k, _, up)| *k == kind && *up)
    }

    pub(crate) fn show(&mut self, kind: Kind, up: bool) {
        let Some(entry) = self.get(kind) else { return };
        if entry.2 == up {
            return;
        }
        entry.2 = up;
        if up {
            if let Err(error) = entry.1.restart() {
                println!("wotr-hud: {error}");
            }
            if !kind.is_gui_fx() {
                declare_externs(&mut entry.1);
            }
        }
    }

    pub(crate) fn show_message(&mut self, title: &str, text: &str) {
        self.show(Kind::Message, true);
        let Some(hosted) = self.hosted(Kind::Message) else {
            return;
        };
        let key = |part: &str| message_key(part);
        hosted.expose.string(&key("Title"), Some(title.to_owned()));
        hosted.expose.string(&key("Text"), Some(text.to_owned()));
        hosted.expose.fire(
            &format!("{MESSAGE_CLIP}.Show"),
            vec![
                Value::str("Ok"),
                Value::Bool(text.chars().count() > MESSAGE_LARGE_ABOVE_CHARS),
            ],
        );
    }

    pub(crate) fn hide_message(&mut self) {
        self.show(Kind::Message, false);
    }

    pub(crate) fn frame(&mut self, dt: Duration) -> Vec<Sent> {
        self.narration.advance(dt);
        let mut sent = Vec::new();
        for (kind, hosted, up) in &mut self.hosted {
            if !*up {
                continue;
            }
            hosted.frame(dt);
            for event in hosted.expose.take_events() {
                let MovieEvent::Command {
                    qualified,
                    name,
                    args,
                } = &event
                else {
                    continue;
                };
                match ClipEvent::parse(qualified, args) {
                    Some(event) => sent.push(Sent { kind: *kind, event }),
                    None if *kind == Kind::Veterancy => sent.push(Sent {
                        kind: *kind,
                        event: ClipEvent {
                            clip: String::new(),
                            suffix: name.clone(),
                            args: args.clone(),
                        },
                    }),
                    None => println!("wotr-hud: {} sent {qualified:?} {args:?}", kind.movie()),
                }
            }
        }
        self.menus.frame(dt, &mut Vec::new());
        sent
    }

    fn dialog_frame(&self) -> Option<mint_sage_apt_viewer::Letterbox> {
        let (_, hud, _) = self.hosted.iter().find(|(k, _, _)| *k == Kind::Hud)?;
        let player = &hud.view.player;
        let m = player.world_matrix(resolve_path(player, DIALOG_FRAME)?)?;
        if m.b != 0.0 || m.c != 0.0 {
            println!(
                "wotr-hud: {DIALOG_FRAME} is rotated or skewed ({m:?}); its turn is not drawn"
            );
        }
        Some(mint_sage_apt_viewer::Letterbox {
            scale: [m.a, m.d],
            offset: [m.tx, m.ty],
        })
    }

    pub(crate) fn build(&mut self, renderer: &mut Renderer) {
        let frame = self.dialog_frame();
        for (kind, hosted, _) in &mut self.hosted {
            if kind.in_dialog_frame() && hosted.view.host != frame {
                println!(
                    "wotr-hud: {} placed by {DIALOG_FRAME} at {frame:?}",
                    kind.movie()
                );
                hosted.view.host = frame;
            }
        }
        for (_, hosted, up) in &mut self.hosted {
            if *up {
                hosted.apply_images(&mut self.images, renderer);
                hosted.view.build(renderer);
            }
        }
        if let Some(menu) = self.menus.shown_mut() {
            menu.apply_images(&mut self.images, renderer);
            menu.view.build(renderer);
        }
        let font = mint_sage_apt_viewer::resolved_font_file(crate::wotr_narration::FONT)
            .and_then(|file| self.fonts.get(file));
        self.narration.build(renderer, font);
    }

    pub(crate) fn draws(&self) -> Vec<ScreenDraw<'_>> {
        let mut all = Vec::new();
        for kind in Kind::ALL {
            if let Some((_, hosted, true)) = self.hosted.iter().find(|(k, _, _)| *k == kind) {
                all.extend(hosted.view.draws());
            }
            if kind == Kind::Hud {
                all.extend(self.narration.quads().iter().copied().map(ScreenDraw::Quad));
            }
        }
        if let Some(menu) = self.menus.shown() {
            all.extend(menu.view.draws());
        }
        all
    }

    pub(crate) fn modal(&self) -> Option<Kind> {
        self.hosted
            .iter()
            .rev()
            .find(|(k, _, up)| *up && k.modal())
            .map(|(k, _, _)| *k)
    }

    pub(crate) fn input(&mut self, event: &InputEvent) -> bool {
        if self.menus.is_open() {
            match event {
                InputEvent::PointerMoved { x, y } => self.menus.pointer_moved([*x, *y]),
                InputEvent::PointerButton {
                    button: mint_app::PointerButton::Primary,
                    pressed,
                } => self.menus.pointer_button(*pressed),
                InputEvent::Key {
                    key: mint_app::Key::Escape,
                    pressed: true,
                } => self.menus.toggle(),
                InputEvent::Key { key, pressed } => self.menus.key(*key, *pressed),
                _ => {}
            }
            return true;
        }
        let target = self.modal().unwrap_or(Kind::Hud);
        match event {
            InputEvent::PointerMoved { x, y } => {
                self.pointer = (*x, *y);
                let Some((_, hosted, _)) = self.get(target) else {
                    return false;
                };
                let stage = hosted.view.resolve_stage_point([*x, *y]);
                let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
                false
            }
            InputEvent::PointerButton {
                button: mint_app::PointerButton::Primary,
                pressed,
            } => {
                let at = self.pointer;
                let Some((_, hosted, _)) = self.get(target) else {
                    return false;
                };
                let stage = hosted.view.resolve_stage_point([at.0, at.1]);
                let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
                let over = hosted.view.player.hovered().is_some();
                let _ = hosted.view.player.pointer_button(*pressed);
                over || target.modal()
            }
            _ => false,
        }
    }

    pub(crate) fn press(&mut self, kind: Kind, path: &str) -> Result<String, String> {
        let hosted = self
            .hosted(kind)
            .ok_or_else(|| format!("{}: not loaded", kind.movie()))?;
        press_button(&mut hosted.view.player, path)
    }

    pub(crate) fn click(&mut self, kind: Kind, path: &str) -> Result<String, String> {
        let hosted = self
            .hosted(kind)
            .ok_or_else(|| format!("{}: not loaded", kind.movie()))?;
        click_button(&mut hosted.view.player, path)
    }

    pub(crate) fn call(
        &mut self,
        kind: Kind,
        path: &str,
        function: &str,
        args: Vec<Value>,
    ) -> Result<(), String> {
        let hosted = self
            .hosted(kind)
            .ok_or_else(|| format!("{}: not loaded", kind.movie()))?;
        let player = &mut hosted.view.player;
        let at = resolve_path(player, path).ok_or_else(|| format!("no clip {path}"))?;
        player
            .call(at, function, args)
            .map(|_| ())
            .map_err(|halt| format!("{path}.{function}: {halt:?}"))
    }

    pub(crate) fn set_value(&mut self, kind: Kind, path: &str, member: &str, text: &str) {
        if let Some(hosted) = self.hosted(kind) {
            hosted.expose.value(path, member, Value::str(text));
        }
    }

    pub(crate) fn global(&mut self, kind: Kind, name: &str) -> Value {
        self.hosted(kind).map_or(Value::Undefined, |h| {
            let root = h.view.player.root();
            h.view.player.variable(root, &format!("_global.{name}"))
        })
    }

    pub(crate) fn exists(&mut self, kind: Kind, path: &str) -> bool {
        self.hosted(kind)
            .is_some_and(|h| resolve_path(&h.view.player, path).is_some())
    }

    pub(crate) fn label(&self, label: &str) -> Option<String> {
        self.strings.get(label).map(str::to_owned)
    }

    pub(crate) fn formatted(&self, label: &str, value: i64) -> Option<String> {
        self.strings.get(label).map(|template| {
            mint_sage_str::format_placeholder(template, mint_sage_str::Placeholder::Int(value))
        })
    }

    pub(crate) fn set_text(&mut self, kind: Kind, key: &str, text: &str) {
        if let Some(hosted) = self.hosted(kind) {
            hosted.expose.string(key, Some(text.to_owned()));
        }
    }

    pub(crate) fn set_image(&mut self, kind: Kind, name: &str, image: ImageSource) {
        if let Some(hosted) = self.hosted(kind) {
            hosted
                .expose
                .image(ImageKey::Name(name.to_owned()), Some(image));
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn read(&mut self, kind: Kind, path: &str, member: &str) -> Option<f32> {
        let player = &mut self.hosted(kind)?.view.player;
        let at = resolve_path(player, path)?;
        match player.variable(at, member) {
            Value::Number(n) => Some(n as f32),
            _ => None,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn extern_number(&mut self, kind: Kind, name: &str) -> Option<f32> {
        match self.hosted(kind)?.view.player.extern_member(name)? {
            Value::Number(n) => Some(n as f32),
            _ => None,
        }
    }

    pub(crate) fn texts(&mut self, kind: Kind, needle: &str) -> Vec<String> {
        let Some(hosted) = self.hosted(kind) else {
            return Vec::new();
        };
        texts_of(&hosted.view.player, needle)
    }

    pub(crate) fn tree(&mut self, kind: Kind, needle: &str) -> Vec<String> {
        let Some(hosted) = self.hosted(kind) else {
            return Vec::new();
        };
        tree_of(&hosted.view.player, needle)
    }
}

pub(crate) fn texts_of(player: &mint_sage_apt_player::Player, needle: &str) -> Vec<String> {
    let needle = needle.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut stack = vec![(player.root(), 0u8)];
    while let Some((id, depth)) = stack.pop() {
        let Some(instance) = player.instance(id) else {
            continue;
        };
        if let InstanceKind::EditText { text } = instance.kind() {
            let path = player.target_path(id);
            if path.to_ascii_lowercase().contains(&needle) && !text.is_empty() {
                out.push(format!("{path}: {text:?}"));
            }
        }
        if depth < 14 {
            stack.extend(instance.children().map(|(_, c)| (c, depth + 1)));
        }
    }
    out.sort();
    out
}

pub(crate) fn press_button(
    player: &mut mint_sage_apt_player::Player,
    path: &str,
) -> Result<String, String> {
    button_actions(player, path, &[BUTTON_PRESS])
}

pub(crate) fn click_button(
    player: &mut mint_sage_apt_player::Player,
    path: &str,
) -> Result<String, String> {
    button_actions(player, path, &[BUTTON_PRESS, BUTTON_RELEASE])
}

const BUTTON_PRESS: u32 = 0x0004;
const BUTTON_RELEASE: u32 = 0x0008;

fn button_actions(
    player: &mut mint_sage_apt_player::Player,
    path: &str,
    conditions: &[u32],
) -> Result<String, String> {
    let at = resolve_path(player, path).ok_or_else(|| format!("no clip {path}"))?;
    let button = first_button(player, at).ok_or_else(|| format!("{path}: no button"))?;
    let mut queued = 0;
    for condition in conditions {
        queued += player
            .queue_button_actions(button, *condition)
            .map_err(|e| e.to_string())?;
    }
    if queued == 0 {
        return Err(format!("{path}: the button has no press action"));
    }
    Ok(format!(
        "{} action(s) queued on {}",
        queued,
        player.target_path(button)
    ))
}

pub(crate) fn children_of(
    player: &mint_sage_apt_player::Player,
    path: &str,
) -> Result<String, String> {
    let at = resolve_path(player, path).ok_or_else(|| format!("no clip {path}"))?;
    let instance = player
        .instance(at)
        .ok_or_else(|| format!("{path}: not live"))?;
    let mut rows: Vec<(i32, String)> = instance
        .children()
        .map(|(depth, child)| {
            let look = player.instance(child).map_or_else(String::new, |c| {
                format!(
                    " char {} at {:.0},{:.0}{}{}",
                    c.character().id,
                    c.x(),
                    c.y(),
                    if c.visible() { "" } else { " hidden" },
                    match c.clip_depth() {
                        0 => String::new(),
                        depth => format!(" masks to depth {depth}"),
                    }
                )
            });
            (depth, format!("{}{look}", player.target_path(child)))
        })
        .collect();
    rows.sort();
    Ok(rows
        .iter()
        .map(|(depth, line)| format!("#{depth} {line}"))
        .collect::<Vec<_>>()
        .join("\n  "))
}

pub(crate) fn var_of(
    player: &mut mint_sage_apt_player::Player,
    args: &str,
) -> Result<String, String> {
    let (path, name) = args.split_once(' ').ok_or("var CLIP NAME")?;
    let path = if path == "-" { "" } else { path };
    let at = resolve_path(player, path).ok_or_else(|| format!("no clip {path}"))?;
    Ok(format!("{:?}", player.variable(at, name.trim())))
}

pub(crate) fn tree_of(player: &mint_sage_apt_player::Player, needle: &str) -> Vec<String> {
    let needle = needle.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut stack = vec![(player.root(), 0u8)];
    while let Some((id, depth)) = stack.pop() {
        let Some(instance) = player.instance(id) else {
            continue;
        };
        let tag = match instance.kind() {
            InstanceKind::Clip(_) => "clip",
            InstanceKind::Button(_) => "button",
            _ => continue,
        };
        let path = player.target_path(id);
        if path.to_ascii_lowercase().contains(&needle) {
            out.push(format!("{tag} {path}"));
        }
        if depth < 12 {
            stack.extend(instance.children().map(|(_, c)| (c, depth + 1)));
        }
    }
    out.sort();
    out
}

fn first_button(player: &mint_sage_apt_player::Player, at: InstanceId) -> Option<InstanceId> {
    let instance = player.instance(at)?;
    if matches!(instance.kind(), InstanceKind::Button(_)) {
        return Some(at);
    }
    instance
        .children()
        .find_map(|(_, child)| first_button(player, child))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pop_up_is_the_strategic_message_box_instance_and_its_text_keys_follow_its_path() {
        assert_eq!(MESSAGE_CLIP, "StrategicMessageBox");
        assert_eq!(message_key("Title"), "_level0.StrategicMessageBox_Title");
        assert_eq!(message_key("Text"), "_level0.StrategicMessageBox_Text");
        assert!(OK_BUTTON_CLIP.starts_with(MESSAGE_CLIP));
        assert!(
            !OK_BUTTON_CLIP.ends_with("bttn"),
            "the strategic frame's button is unnamed"
        );
    }
}
