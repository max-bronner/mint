use std::sync::Arc;
use std::time::Instant;

use mint_render::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, CustomCursor, Window, WindowId};

use crate::clock::FixedTimestep;
use crate::frame_log::{FrameLog, FrameMarks};
use crate::input::{lines_from_pixels, InputEvent, Key, PointerButton};
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub simulation_hz: u32,
    pub pipeline_cache_dir: Option<std::path::PathBuf>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "mint".to_owned(),
            width: 1280,
            height: 720,
            simulation_hz: crate::SIMULATION_HZ,
            pipeline_cache_dir: None,
        }
    }
}

pub trait App {
    fn input(&mut self, event: &InputEvent);

    fn update(&mut self, step: u64) {
        let _ = step;
    }

    fn frame(&mut self, dt: f32) {
        let _ = dt;
    }

    fn draw(&mut self, renderer: &mut Renderer, blend: f32) -> mint_render::Result<()>;

    fn cursor(&mut self) -> CursorRequest {
        CursorRequest::Unchanged
    }

    fn speed(&self) -> f64 {
        1.0
    }

    fn exit_requested(&self) -> bool {
        false
    }

    fn paused(&self) -> bool {
        false
    }

    fn frame_stages(&self, stage: &mut dyn FnMut(&'static str, f64)) {
        let _ = stage;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorRequest {
    Unchanged,
    Default,
    Hidden,
    Image {
        rgba: Vec<u8>,
        width: u16,
        height: u16,
        hotspot: (u16, u16),
    },
}

pub fn run<A, F>(config: &WindowConfig, build: F) -> Result<()>
where
    A: App,
    F: FnOnce(&mut Renderer) -> std::result::Result<A, Box<dyn std::error::Error + Send + Sync>>,
{
    let event_loop =
        EventLoop::new().map_err(|e| Error::event_loop("creating the event loop", e))?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut handler = Handler {
        config: config.clone(),
        build: Some(build),
        state: None,
        failure: None,
    };
    event_loop
        .run_app(&mut handler)
        .map_err(|e| Error::event_loop("running the event loop", e))?;

    match handler.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct State<A> {
    window: Arc<Window>,
    renderer: Renderer,
    app: A,
    clock: FixedTimestep,
    last_frame: Instant,
    frame_log: FrameLog,
    ctrl: bool,
    shift: bool,
}

struct Handler<A, F> {
    config: WindowConfig,
    build: Option<F>,
    state: Option<State<A>>,
    failure: Option<Error>,
}

impl<A, F> Handler<A, F>
where
    A: App,
    F: FnOnce(&mut Renderer) -> std::result::Result<A, Box<dyn std::error::Error + Send + Sync>>,
{
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Error) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        event_loop.exit();
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<State<A>> {
        let Some(build) = self.build.take() else {
            return Err(Error::AlreadyStarted);
        };
        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| Error::event_loop("creating the window", e))?,
        );

        let size = window.inner_size();
        let mut renderer = Renderer::for_window_with_pipeline_cache(
            Arc::clone(&window),
            size.width,
            size.height,
            self.config.pipeline_cache_dir.as_deref(),
        )?;
        let app = build(&mut renderer).map_err(Error::App)?;

        Ok(State {
            window,
            renderer,
            app,
            clock: FixedTimestep::new(self.config.simulation_hz),
            last_frame: Instant::now(),
            frame_log: FrameLog::from_env(),
            ctrl: false,
            shift: false,
        })
    }
}

impl<A, F> ApplicationHandler for Handler<A, F>
where
    A: App,
    F: FnOnce(&mut Renderer) -> std::result::Result<A, Box<dyn std::error::Error + Send + Sync>>,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match self.start(event_loop) {
            Ok(state) => self.state = Some(state),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                state.app.input(&InputEvent::Resized {
                    width: size.width,
                    height: size.height,
                });
            }
            WindowEvent::CursorMoved { position, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                state.app.input(&InputEvent::PointerMoved {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }
            WindowEvent::MouseInput {
                state: pressed,
                button,
                ..
            } => {
                if let Some(button) = translate_button(button) {
                    state.app.input(&InputEvent::PointerButton {
                        button,
                        pressed: pressed == ElementState::Pressed,
                    });
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let ctrl = modifiers.state().control_key();
                state.shift = modifiers.state().shift_key();
                if ctrl != state.ctrl {
                    state.ctrl = ctrl;
                    state.app.input(&InputEvent::Modifiers { ctrl });
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !event.repeat {
                    let key = physical_digit(event.physical_key)
                        .filter(|_| state.ctrl && state.shift)
                        .map_or_else(
                            || {
                                with_modifiers(
                                    translate_key(&event.logical_key),
                                    state.ctrl,
                                    state.shift,
                                )
                            },
                            Key::CtrlShiftDigit,
                        );
                    state.app.input(&InputEvent::Key {
                        key,
                        pressed: event.state == ElementState::Pressed,
                    });
                }
                if event.state == ElementState::Pressed {
                    let text = event.text.as_ref().map(|t| t.as_str());
                    for character in translate_text(text) {
                        state.app.input(&InputEvent::TextInput { character });
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => lines_from_pixels(position.y),
                };
                state.app.input(&InputEvent::Scroll { lines });
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_frame);
                state.last_frame = now;

                let steps = state.clock.advance_unless_paused(
                    elapsed,
                    state.app.speed(),
                    state.app.paused(),
                );
                let already = state.clock.steps_taken();
                for step in 0..u64::from(steps) {
                    let index = already
                        .saturating_sub(u64::from(steps))
                        .saturating_add(step);
                    state.app.update(index);
                }
                let stepped = Instant::now();

                state.app.frame(elapsed.as_secs_f32());
                let framed = Instant::now();

                let blend = state.clock.blend();
                let drawn = state.app.draw(&mut state.renderer, blend);
                let marks = FrameMarks {
                    start: now,
                    stepped,
                    framed,
                    drawn: Instant::now(),
                };
                let present = state.renderer.present_wait();
                state
                    .frame_log
                    .record(elapsed, steps, &marks, present, &state.app);

                apply_cursor_request(state.app.cursor(), event_loop, &state.window);
                if state.app.exit_requested() {
                    event_loop.exit();
                }

                if let Err(error) = drawn {
                    self.fail(event_loop, Error::Render(error));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

fn apply_cursor_request(request: CursorRequest, event_loop: &ActiveEventLoop, window: &Window) {
    match request {
        CursorRequest::Unchanged => {}
        CursorRequest::Hidden => window.set_cursor_visible(false),
        CursorRequest::Default => {
            window.set_cursor_visible(true);
            window.set_cursor(CursorIcon::Default);
        }
        CursorRequest::Image {
            rgba,
            width,
            height,
            hotspot,
        } => match CustomCursor::from_rgba(rgba, width, height, hotspot.0, hotspot.1) {
            Ok(source) => {
                let cursor = event_loop.create_custom_cursor(source);
                window.set_cursor_visible(true);
                window.set_cursor(cursor);
            }
            Err(error) => {
                tracing::warn!("cursor image rejected, leaving the previous one: {error}");
            }
        },
    }
}

fn with_modifiers(key: Key, ctrl: bool, shift: bool) -> Key {
    match key {
        Key::Character(c) if ctrl && ('\u{1}'..='\u{1a}').contains(&c) => {
            let letter = u8::try_from(u32::from(c)).map_or(b'?', |code| code.saturating_add(0x60));
            if shift {
                Key::CtrlShiftCharacter(char::from(letter))
            } else {
                Key::CtrlCharacter(char::from(letter))
            }
        }
        Key::Character(c) if ctrl && shift => Key::CtrlShiftCharacter(c.to_ascii_lowercase()),
        Key::Character(c) if ctrl => Key::CtrlCharacter(c.to_ascii_lowercase()),
        other => other,
    }
}

fn physical_digit(key: PhysicalKey) -> Option<u8> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    Some(match code {
        KeyCode::Digit0 => 0,
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return None,
    })
}

fn translate_key(key: &WinitKey) -> Key {
    match key {
        WinitKey::Character(text) => match text.chars().next() {
            Some(c) => Key::Character(c.to_ascii_lowercase()),
            None => Key::Other,
        },
        WinitKey::Named(NamedKey::Escape) => Key::Escape,
        WinitKey::Named(NamedKey::Space) => Key::Space,
        WinitKey::Named(NamedKey::ArrowUp) => Key::ArrowUp,
        WinitKey::Named(NamedKey::ArrowDown) => Key::ArrowDown,
        WinitKey::Named(NamedKey::ArrowLeft) => Key::ArrowLeft,
        WinitKey::Named(NamedKey::ArrowRight) => Key::ArrowRight,
        WinitKey::Named(NamedKey::Backspace) => Key::Backspace,
        WinitKey::Named(NamedKey::Enter) => Key::Enter,
        WinitKey::Named(NamedKey::Tab) => Key::Tab,
        WinitKey::Named(NamedKey::Shift) => Key::Shift,
        WinitKey::Named(NamedKey::Alt) => Key::Alt,
        WinitKey::Named(NamedKey::F1) => Key::Function(1),
        WinitKey::Named(NamedKey::F2) => Key::Function(2),
        WinitKey::Named(NamedKey::F3) => Key::Function(3),
        WinitKey::Named(NamedKey::F4) => Key::Function(4),
        WinitKey::Named(NamedKey::F5) => Key::Function(5),
        WinitKey::Named(NamedKey::F6) => Key::Function(6),
        WinitKey::Named(NamedKey::F7) => Key::Function(7),
        WinitKey::Named(NamedKey::F8) => Key::Function(8),
        WinitKey::Named(NamedKey::F9) => Key::Function(9),
        WinitKey::Named(NamedKey::F10) => Key::Function(10),
        WinitKey::Named(NamedKey::F11) => Key::Function(11),
        WinitKey::Named(NamedKey::F12) => Key::Function(12),
        _ => Key::Other,
    }
}

fn translate_text(text: Option<&str>) -> impl Iterator<Item = char> + '_ {
    text.into_iter().flat_map(str::chars)
}

fn translate_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        MouseButton::Back | MouseButton::Forward | MouseButton::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_buttons_mint_names_map_across_and_the_rest_are_dropped() {
        assert_eq!(
            translate_button(MouseButton::Left),
            Some(PointerButton::Primary)
        );
        assert_eq!(
            translate_button(MouseButton::Right),
            Some(PointerButton::Secondary)
        );
        assert_eq!(
            translate_button(MouseButton::Middle),
            Some(PointerButton::Middle)
        );
        assert_eq!(translate_button(MouseButton::Other(9)), None);
    }

    #[test]
    fn ctrl_turns_a_character_into_its_own_key_whether_reported_as_letter_or_control_code() {
        assert_eq!(
            with_modifiers(Key::Character('h'), true, false),
            Key::CtrlCharacter('h')
        );
        assert_eq!(
            with_modifiers(Key::Character('\u{8}'), true, false),
            Key::CtrlCharacter('h')
        );
        assert_eq!(
            with_modifiers(Key::Character('h'), false, false),
            Key::Character('h')
        );
        assert_eq!(with_modifiers(Key::Escape, true, false), Key::Escape);
    }

    #[test]
    fn ctrl_and_shift_together_turn_a_character_into_ctrl_shift_character() {
        assert_eq!(
            with_modifiers(Key::Character('l'), true, true),
            Key::CtrlShiftCharacter('l')
        );
    }

    #[test]
    fn ctrl_shift_lowercases_a_shift_applied_uppercase_letter_from_a_real_keyboard() {
        assert_eq!(
            with_modifiers(Key::Character('L'), true, true),
            Key::CtrlShiftCharacter('l')
        );
        assert_eq!(
            with_modifiers(Key::Character('M'), true, false),
            Key::CtrlCharacter('m')
        );
    }

    #[test]
    fn a_letter_key_arrives_lowercased_whatever_the_shift_state() {
        assert_eq!(
            translate_key(&WinitKey::Character("T".into())),
            Key::Character('t')
        );
        assert_eq!(
            translate_key(&WinitKey::Character("t".into())),
            Key::Character('t')
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::Escape)),
            Key::Escape
        );
        assert_eq!(translate_key(&WinitKey::Named(NamedKey::Space)), Key::Space);
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::ArrowUp)),
            Key::ArrowUp
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::ArrowDown)),
            Key::ArrowDown
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::ArrowLeft)),
            Key::ArrowLeft
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::ArrowRight)),
            Key::ArrowRight
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::F1)),
            Key::Function(1)
        );
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::Insert)),
            Key::Other
        );
    }

    #[test]
    fn backspace_and_enter_translate_distinctly_from_other_and_from_each_other() {
        assert_eq!(
            translate_key(&WinitKey::Named(NamedKey::Backspace)),
            Key::Backspace
        );
        assert_eq!(translate_key(&WinitKey::Named(NamedKey::Enter)), Key::Enter);
        assert_ne!(Key::Backspace, Key::Enter);
        assert_ne!(Key::Backspace, Key::Other);
        assert_ne!(Key::Enter, Key::Other);
        assert_eq!(translate_key(&WinitKey::Named(NamedKey::Tab)), Key::Tab);
    }

    #[test]
    fn the_default_window_asks_for_the_simulation_rate_the_data_assumes() {
        assert_eq!(WindowConfig::default().simulation_hz, crate::SIMULATION_HZ);
    }

    #[test]
    fn a_mixed_case_punctuated_string_round_trips_through_text_input_exactly() {
        let typed = "Hello, World! 123 <mixedCASE-\"quoted\">";
        let recorded: String = translate_text(Some(typed)).collect();
        assert_eq!(recorded, typed);
    }

    #[test]
    fn text_input_translation_is_empty_for_a_key_with_no_text_at_all() {
        assert_eq!(translate_text(None).count(), 0);
    }

    #[test]
    fn text_that_carries_more_than_one_character_is_split_in_order() {
        assert_eq!(translate_text(Some("^e")).collect::<Vec<_>>(), ['^', 'e']);
    }
}
