use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use mint_app::Key;
use mint_sage_apt_player::{ImageKey, MovieEvent, RenderCommand, RenderList, RenderOptions};
use mint_sage_apt_viewer::gadget::gadget_key;
use mint_sage_apt_viewer::host::{Change, GadgetEvent, SurfaceContent, SurfaceIcon};
use mint_sage_asset::HeroStore;

use super::{Screen, ShellApp};
use crate::cah_screen::{CahHost, CahState, Request, HERO_LIST, STATS_LIST};
use crate::hud::{EntrySpec, ImageSource};

const NAME_ENTRY: (&str, &str) = ("CahAppearance::InitGadgets", "CreateAHero::HeroName");

impl ShellApp {
    pub(super) fn enter_create_a_hero(&mut self) {
        let Some(sys) = self.cah_system.clone() else {
            println!("shell: no CreateAHeroSystem in the data - the screen stays closed");
            return;
        };
        let store = match HeroStore::load(&self.vfs, crate::setup_heroes::own_dir(), &[]) {
            Ok(store) => store,
            Err(error) => {
                println!("shell: no hero list: {error}");
                return;
            }
        };
        for skipped in store.skipped() {
            println!("heroes: skipped {skipped}");
        }
        let state = CahState::open(
            sys,
            Arc::clone(&self.strings),
            store,
            self.cah_suppress_promo,
        );
        self.enter(Screen::CreateAHero);
        self.cah
            .view
            .player
            .set_sync_host(Some(Box::new(CahHost(Rc::clone(&state)))));
        self.cah_state = Some(state);
    }

    pub(super) fn cah_frame(&mut self, elapsed: Duration) {
        let Some(state) = self.cah_state.clone() else {
            self.cah.frame(elapsed);
            return;
        };
        {
            let mut st = state.borrow_mut();
            let list = Arc::new(st.hero_list());
            let (init, name) = HERO_LIST;
            self.cah.expose.image(
                ImageKey::Name(gadget_key(init, name)),
                Some(ImageSource::List(list)),
            );
            let (init, name) = STATS_LIST;
            self.cah.expose.image(
                ImageKey::Name(gadget_key(init, name)),
                Some(ImageSource::List(Arc::new(st.stats_list()))),
            );
            let entry = st.name_hint().then(|| {
                ImageSource::Entry(Arc::new(EntrySpec {
                    text: st.name_text().to_owned(),
                    focused: st.name_focused(),
                }))
            });
            self.cah.expose.image(
                ImageKey::Name(gadget_key(NAME_ENTRY.0, NAME_ENTRY.1)),
                entry,
            );
            st.publish(&mut self.cah.expose);
        }
        state.borrow_mut().tick_view(self.screen_update_due);
        self.cah.frame(elapsed);
        for event in self.cah.expose.take_events() {
            if !self.played(&event) && !matches!(event, MovieEvent::Load { .. }) {
                println!("cah: unhandled movie event {event:?}");
            }
        }
        let answer = self.message_box_frame(elapsed);
        let (log, request, boxes) = {
            let mut st = state.borrow_mut();
            if let Some(button) = answer {
                st.answer_box(button);
            }
            (st.take_log(), st.take_request(), st.take_boxes())
        };
        for request in &boxes {
            self.show_message_box(request);
        }
        for line in log {
            println!("{line}");
        }
        match request {
            Some(Request::MainMenu) => {
                self.leave_create_a_hero();
                println!("shell: CreateAHero -> MainMenu");
                self.enter(Screen::MainMenu);
            }
            None => {}
        }
    }

    fn leave_create_a_hero(&mut self) {
        if let Some(state) = &self.cah_state {
            let mut state = state.borrow_mut();
            state.close();
            self.cah_suppress_promo = state.suppress_promo();
        }
        self.cah.view.player.set_sync_host(None);
        self.cah_state = None;
    }

    pub(super) fn cah_key(&mut self, key: Key, pressed: bool) {
        let Some(state) = self.cah_state.clone() else {
            return;
        };
        if self.message_box.is_up() {
            return;
        }
        let mut st = state.borrow_mut();
        match key {
            Key::Shift => st.set_shift(pressed),
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                st.set_arrow(key, pressed);
            }
            _ if !pressed => {}
            Key::Character(c) => st.type_text(&c.to_string()),
            Key::Space => st.type_text(" "),
            Key::Backspace => st.backspace(),
            _ => {}
        }
    }

    pub(super) fn cah_gadget_changed(&mut self, event: &GadgetEvent) {
        let (init, name) = HERO_LIST;
        if let (Some(key), Change::Row(row)) = (event.key.as_deref(), event.change) {
            if key == gadget_key(init, name) {
                if let Some(state) = &self.cah_state {
                    state.borrow_mut().select_row(row);
                }
            }
        }
    }
}

const DRAW_MAP: &str = "CreateAHero::DrawMapComponent";
const PICTURE_GUARD: &str = "CreateAHero::RenderPictureGuard";

impl ShellApp {
    pub(super) fn scan_cah_surfaces(&mut self) {
        let Some(state) = self.cah_state.clone() else {
            return;
        };
        let view = &mut self.cah.view;
        let mut map = None;
        let mut list = RenderList::default();
        view.player
            .render_with(&mut list, RenderOptions { surfaces: true });
        for command in &list.commands {
            let RenderCommand::Surface { instance, .. } = command else {
                continue;
            };
            let kind = view.player.own_string(*instance, "_type");
            match kind.as_deref() {
                Some(PICTURE_GUARD) => {
                    state.borrow_mut().guard_drawn();
                    view.set_surface(*instance, Some(SurfaceContent::Hidden));
                }
                Some(DRAW_MAP) => {
                    map = view.slot_rect(*instance);
                    let content =
                        self.cah_scene_texture
                            .map_or(SurfaceContent::Hidden, |texture| {
                                SurfaceContent::Icon(SurfaceIcon {
                                    texture,
                                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                                    tint: [1.0, 1.0, 1.0, 1.0],
                                    premultiplied: false,
                                    rotated: false,
                                })
                            });
                    view.set_surface(*instance, Some(content));
                }
                _ => {}
            }
        }
        self.cah_map_rect = map;
    }

    pub(crate) fn cah_map_size(&self) -> Option<(f32, f32)> {
        self.cah_map_rect
            .map(|[l, t, r, b]| ((r - l).max(1.0), (b - t).max(1.0)))
    }

    pub(crate) fn cah_set_scene_texture(&mut self, texture: Option<mint_render::TextureHandle>) {
        self.cah_scene_texture = texture;
    }

    pub(crate) fn cah_scene_wanted(&self) -> bool {
        self.cah_state.is_some()
    }

    pub(crate) fn cah_preview(&self) -> Option<crate::cah_screen::Preview> {
        self.cah_state.as_ref()?.borrow_mut().preview()
    }

    pub(crate) fn cah_picture_request(&mut self, unix_seconds: u64) -> Option<String> {
        self.cah_state
            .as_ref()?
            .borrow_mut()
            .take_picture_request(unix_seconds)
    }

    pub(crate) fn cah_picture_taken(&mut self, path: &str) {
        if let Some(state) = &self.cah_state {
            state.borrow_mut().picture_taken(path);
        }
    }
}
