use mint_app::{App, InputEvent};
use mint_render::{Camera, RenderTargetHandle, Renderer, Scene, ScreenDraw, TextureHandle};

use crate::cah_scene::CahScene;
use crate::cah_screen::Preview;
use crate::game::{Game, MatchConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellGameKind {
    ShellMap,
    CreateAHero,
}

impl ShellGameKind {
    pub(crate) fn config(self) -> MatchConfig {
        MatchConfig::ShellGame(self)
    }
}

pub(crate) fn start_needed(running: Option<ShellGameKind>, wanted: ShellGameKind) -> bool {
    running != Some(wanted)
}

pub(crate) struct ShellGame {
    kind: ShellGameKind,
    game: Box<Game>,
    frames: u64,
    cah: Option<CahScene>,
    target: Option<(RenderTargetHandle, u32, u32)>,
}

impl ShellGame {
    pub(crate) fn new(kind: ShellGameKind, game: Box<Game>, map: &str) -> Self {
        println!(
            "shell-game: kind={kind:?} map={map} objects={} scripts={}",
            game.object_count(),
            game.script_count()
        );
        Self {
            kind,
            game,
            frames: 0,
            cah: (kind == ShellGameKind::CreateAHero).then(CahScene::default),
            target: None,
        }
    }

    pub(crate) fn kind(&self) -> ShellGameKind {
        self.kind
    }

    #[allow(dead_code)]
    pub(crate) fn game_mut(&mut self) -> &mut Game {
        &mut self.game
    }

    pub(crate) fn update(&mut self, step: u64) {
        self.game.update(step);
        if let Some(scene) = self.cah.as_mut() {
            scene.after_step(&mut self.game);
        }
    }

    pub(crate) fn preview(&mut self, preview: &Preview) {
        if let Some(scene) = self.cah.as_mut() {
            scene.frame(&mut self.game, preview);
        }
    }

    pub(crate) fn frame(&mut self, dt: f32) {
        self.game.frame(dt);
        self.frames = self.frames.saturating_add(1);
        if self.frames % 60 == 0 {
            let at = self.game.camera_target();
            println!("camera: ({:.1}, {:.1}, {:.1})", at.x, at.y, at.z);
        }
    }

    pub(crate) fn resize(&mut self, event: &InputEvent) {
        if matches!(event, InputEvent::Resized { .. }) {
            self.game.input(event);
        }
    }

    pub(crate) fn draw(
        &mut self,
        renderer: &mut Renderer,
        blend: f32,
        extra: &[ScreenDraw<'_>],
    ) -> mint_render::Result<()> {
        if self.kind == ShellGameKind::CreateAHero {
            let mut scene = Scene::new(Camera::default()).with_ordered_screen(extra);
            scene.clear_color = crate::shell::SHELL_CLEAR;
            return renderer.render(&scene);
        }
        self.game.draw_world(renderer, blend, extra)
    }

    pub(crate) fn render_to_rect(
        &mut self,
        renderer: &mut Renderer,
        blend: f32,
        size: (f32, f32),
        window_width: f32,
    ) -> Option<TextureHandle> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            size.0.round().clamp(1.0, 4096.0) as u32,
            size.1.round().clamp(1.0, 4096.0) as u32,
        );
        let target = match self.target {
            Some((target, tw, th)) if (tw, th) == (w, h) => target,
            Some((target, _, _)) => {
                renderer.resize_render_target(target, w, h).ok()?;
                self.target = Some((target, w, h));
                target
            }
            None => {
                let target = renderer.create_render_target(w, h).ok()?;
                self.target = Some((target, w, h));
                target
            }
        };
        #[allow(clippy::cast_precision_loss)]
        if let Some(scene) = self.cah.as_mut() {
            scene.set_rect(w as f32 / h as f32, size.0 / window_width.max(1.0));
        }
        match self.game.draw_world_to_target(renderer, blend, target) {
            Ok(()) => renderer.render_target_texture(target),
            Err(error) => {
                println!("cah scene: the scene did not render ({error})");
                None
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowEvent {
    Boot { gate_on: bool },
    ShellGameLoaded,
    MatchStart,
    CreateAHeroStart,
    ReturnToShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowAction {
    StartShellGame,
    PushMainMenu,
    DropShellGame,
    RestartShellGame,
    ShowLowLodImage,
    HoldLoadScreen,
    DropLoadScreen,
}

#[derive(Debug, Default)]
pub(crate) struct ShellFlow {
    gate_on: bool,
    running: bool,
    menu_pushed: bool,
}

impl ShellFlow {
    pub(crate) fn on(&mut self, event: FlowEvent) -> Vec<FlowAction> {
        match event {
            FlowEvent::Boot { gate_on } => {
                self.gate_on = gate_on;
                if gate_on {
                    self.running = true;
                    vec![FlowAction::StartShellGame]
                } else {
                    self.menu_pushed = true;
                    vec![FlowAction::PushMainMenu, FlowAction::ShowLowLodImage]
                }
            }
            FlowEvent::ShellGameLoaded if self.gate_on && !self.menu_pushed => {
                self.menu_pushed = true;
                vec![FlowAction::PushMainMenu]
            }
            FlowEvent::ShellGameLoaded => Vec::new(),
            FlowEvent::MatchStart if self.running => {
                self.running = false;
                vec![FlowAction::DropShellGame]
            }
            FlowEvent::MatchStart => Vec::new(),
            FlowEvent::CreateAHeroStart => {
                let mut actions = self.on(FlowEvent::MatchStart);
                actions.push(FlowAction::HoldLoadScreen);
                actions
            }
            FlowEvent::ReturnToShell if self.gate_on => {
                self.running = true;
                vec![FlowAction::RestartShellGame]
            }
            FlowEvent::ReturnToShell => {
                vec![FlowAction::DropLoadScreen, FlowAction::ShowLowLodImage]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_game_has_no_setup_players_and_no_seat() {
        for kind in [ShellGameKind::ShellMap, ShellGameKind::CreateAHero] {
            let config = kind.config();
            assert_eq!(config.scene_filter(), (true, false));
            assert_eq!(config.shell_game_kind(), Some(kind));
        }
    }

    #[test]
    fn the_same_kind_is_not_started_twice() {
        assert!(start_needed(None, ShellGameKind::ShellMap));
        assert!(!start_needed(
            Some(ShellGameKind::ShellMap),
            ShellGameKind::ShellMap
        ));
        assert!(start_needed(
            Some(ShellGameKind::CreateAHero),
            ShellGameKind::ShellMap
        ));
    }

    #[test]
    fn with_the_map_on_the_menu_waits_for_the_load_and_a_match_drops_the_map() {
        let mut flow = ShellFlow::default();
        assert_eq!(
            flow.on(FlowEvent::Boot { gate_on: true }),
            vec![FlowAction::StartShellGame]
        );
        assert_eq!(
            flow.on(FlowEvent::ShellGameLoaded),
            vec![FlowAction::PushMainMenu]
        );
        assert_eq!(
            flow.on(FlowEvent::MatchStart),
            vec![FlowAction::DropShellGame]
        );
        assert_eq!(
            flow.on(FlowEvent::ReturnToShell),
            vec![FlowAction::RestartShellGame]
        );
    }

    #[test]
    fn with_the_map_off_the_menu_is_up_at_once_over_the_still_image() {
        let mut flow = ShellFlow::default();
        assert_eq!(
            flow.on(FlowEvent::Boot { gate_on: false }),
            vec![FlowAction::PushMainMenu, FlowAction::ShowLowLodImage]
        );
        assert!(flow.on(FlowEvent::MatchStart).is_empty());
        assert_eq!(
            flow.on(FlowEvent::ReturnToShell),
            vec![FlowAction::DropLoadScreen, FlowAction::ShowLowLodImage]
        );
    }

    #[test]
    fn the_hero_scene_loads_under_the_load_screen_and_the_shell_map_reloads_after_it() {
        for gate_on in [true, false] {
            let mut flow = ShellFlow::default();
            flow.on(FlowEvent::Boot { gate_on });
            flow.on(FlowEvent::ShellGameLoaded);
            let enter = flow.on(FlowEvent::CreateAHeroStart);
            assert_eq!(enter.last(), Some(&FlowAction::HoldLoadScreen));
            assert_eq!(enter.contains(&FlowAction::DropShellGame), gate_on);
            let leave = flow.on(FlowEvent::ReturnToShell);
            let expected = if gate_on {
                FlowAction::RestartShellGame
            } else {
                FlowAction::DropLoadScreen
            };
            assert_eq!(leave.first(), Some(&expected), "gate on: {gate_on}");
        }
    }
}
