use super::*;

const RESTRICTION_PROBE: f32 = 0.25;

pub(super) fn passes_input_lock(event: &InputEvent) -> bool {
    match event {
        InputEvent::Resized { .. } | InputEvent::Modifiers { .. } => true,
        InputEvent::Key { key, pressed } => !*pressed || matches!(key, Key::Escape),
        _ => false,
    }
}

impl Game {
    fn script_cine(&self) -> Option<&crate::rules::map_scripts::CineState> {
        self.state.cinematic.dev.as_ref().or_else(|| {
            self.state
                .simulation
                .rules()
                .script_cine(self.state.simulation.state())
        })
    }

    pub(super) fn script_input_disabled(&self) -> bool {
        self.mode == Mode::Match && self.script_cine().is_some_and(|cine| cine.input_disabled)
    }

    fn script_camera_ops(&self) -> Option<&crate::rules::map_scripts::CameraOps> {
        self.state
            .simulation
            .rules()
            .script_camera_ops(self.state.simulation.state())
    }

    fn camera_lock_all(&self) -> bool {
        self.script_camera_ops().is_some_and(|ops| ops.lock_all)
            && !self.script_cine().is_some_and(|cine| cine.letterbox)
    }

    fn camera_lock_angle_height(&self) -> bool {
        self.script_camera_ops()
            .is_some_and(|ops| ops.lock_angle_height)
            && !self.script_cine().is_some_and(|cine| cine.letterbox)
    }

    pub(super) fn camera_scroll_locked(&self) -> bool {
        self.camera_lock_all()
    }

    pub(super) fn camera_rotation_locked(&self) -> bool {
        self.camera_lock_all() || self.camera_lock_angle_height()
    }

    pub(super) fn camera_zoom_locked(&self) -> bool {
        self.state.camera.is_posed()
            || self.state.scene.script_camera.blending()
            || self.camera_lock_all()
    }

    pub(super) fn drive_camera_restriction(&mut self) {
        let command = self
            .script_camera_ops()
            .and_then(|ops| ops.restrict.clone())
            .filter(|c| self.state.scene.camera_area_seen != Some(c.seq));
        let focus = self.state.camera.focus();
        if let Some(command) = command {
            self.state.scene.camera_area_seen = Some(command.seq);
            crate::diag::diag!(
                "script: camera restriction seq={} {}",
                command.seq,
                command
                    .area
                    .as_ref()
                    .map_or("removed".to_owned(), |area| format!(
                        "area of {} vertices",
                        area.len()
                    ))
            );
            self.state.scene.camera_area = command
                .area
                .map(|area| crate::camera_area::AreaRestriction::new(area, [focus.x, focus.y]));
        }
        let camera = &self.state.camera;
        let free = !camera.is_scripted()
            && !camera.is_posed()
            && !self.state.scene.script_camera.busy()
            && !self.script_cine().is_some_and(|cine| cine.letterbox);
        if !free {
            return;
        }
        let Some(mut area) = self.state.scene.camera_area.take() else {
            return;
        };
        let eye = camera.eye();
        let probe = [
            focus.x + (eye.x - focus.x) * RESTRICTION_PROBE,
            focus.y + (eye.y - focus.y) * RESTRICTION_PROBE,
        ];
        let rules = self.state.simulation.rules();
        let world = self.state.simulation.state();
        let viewer = self.state.shroud_viewer;
        let shrouded = |p: [f32; 2]| {
            rules.ids().is_some_and(|ids| {
                crate::shroud::with_grid(world, RotwkRules::shroud_ids(ids), |grid| {
                    let (x, y) = grid.world_to_cell(p[0], p[1]);
                    grid.cell_state(viewer, x, y) == mint_sage_shroud::CellVisibility::Unexplored
                })
                .unwrap_or(false)
            })
        };
        let kept = area.clamp(probe, shrouded);
        self.state.scene.camera_area = Some(area);
        if kept != probe {
            self.state.camera.set_focus(Vec3::new(
                focus.x + kept[0] - probe[0],
                focus.y + kept[1] - probe[1],
                focus.z,
            ));
        }
    }

    fn drive_script_hud(&mut self) {
        let hud = self.state.scene.script_view.hud.clone();
        self.state.player_colors.house_off = hud.house_color_off;
        let object_seen = self.state.cinematic.object_flash_seen;
        let newer = |seq: u32| newer_than(seq, object_seen);
        let ids = self.state.simulation.rules().ids();
        for flash in hud.object_flashes.iter().filter(|f| newer(f.seq)) {
            self.state.cinematic.object_flash_seen = flash.seq;
            let color = if flash.white {
                [1.0; 3]
            } else {
                let owner = ids
                    .and_then(|ids| self.state.simulation.state().get(ids.owner, flash.entity))
                    .map(|owner| owner.0);
                owner.map_or([1.0; 3], |owner| {
                    let [r, g, b, _] = self.state.player_colors.of_or_white(owner);
                    [r, g, b]
                })
            };
            let count = u32::try_from(flash.count).unwrap_or(0);
            self.state
                .selection_flash
                .queue_script_flash(flash.entity.index(), color, count);
            crate::diag::diag!(
                "script: drawable {} flashes {count} time(s)",
                flash.entity.index()
            );
        }
    }

    pub(super) fn drive_button_flashes(&mut self, hud: &mut crate::hud::HudState) {
        let frame = self.state.simulation.frame().get();
        let seen = self.state.cinematic.ui_flash_seen;
        let flashes = &self.state.scene.script_view.hud.ui_flashes;
        let memory = &mut self.state.cinematic;
        for flash in flashes.iter().filter(|f| newer_than(f.seq, seen)) {
            memory.ui_flash_seen = flash.seq;
            memory
                .button_flashes
                .raise(&flash.target, flash.amount, frame, &hud.fielded_heroes);
            crate::diag::diag!(
                "script: ui flash {:?} amount {}",
                flash.target,
                flash.amount
            );
        }
        let bar = &hud.command_bar;
        let shown = bar
            .ring
            .iter()
            .flatten()
            .chain(&bar.bar)
            .chain(bar.radial.iter().map(|(_, slot)| slot))
            .chain(&bar.stance_flyout)
            .map(|slot| slot.command_button.as_str())
            .chain(
                hud.spell_bar
                    .iter()
                    .map(|slot| slot.command_button.as_str()),
            );
        memory.button_flashes.advance(frame, shown);
        hud.flash = memory.button_flashes.view(frame);
    }

    pub(crate) fn take_script_movie(&mut self) -> Option<crate::rules::map_scripts::MovieCmd> {
        let movie = self.state.scene.script_view.hud.movie.clone()?;
        if movie.seq == self.state.cinematic.movie_seen {
            return None;
        }
        self.state.cinematic.movie_seen = movie.seq;
        Some(movie)
    }

    pub(super) fn drive_cinematics(&mut self, dt: f32) {
        self.drive_script_hud();
        let menu_up = self.menu_open();
        let Some(cine) = self.script_cine().cloned() else {
            return;
        };
        let cine = &cine;
        let strings = &self.state.strings;
        let clicks = self.state.cinematic.drive(
            cine,
            &self.data.military_caption,
            |label| strings.fetch(label),
            &mut self.state.camera,
            dt,
            menu_up,
        );
        for _ in 0..clicks {
            self.audio
                .play_misc("MissionBriefingCharacterClick", None, None);
        }
        self.take_script_radar_events();
        self.play_script_audio();
        self.play_objective_sounds();
        self.drive_notification();
    }

    fn play_script_audio(&mut self) {
        let pending = self
            .state
            .simulation
            .rules()
            .script_audio(self.state.simulation.state())
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let scene = &self.state.scene;
        self.audio
            .apply_script_audio(&pending, &|x, y| ground_at(scene, x, y));
    }

    fn play_objective_sounds(&mut self) {
        let Some(cine) = self.script_cine() else {
            return;
        };
        let seen = self.state.cinematic.objective_sound_seen;
        let fresh: Vec<_> = cine
            .objective_sounds
            .iter()
            .filter(|sound| sound.serial.wrapping_sub(seen).wrapping_sub(1) < u32::MAX / 2)
            .copied()
            .collect();
        let Some(last) = fresh.last() else {
            return;
        };
        self.state.cinematic.objective_sound_seen = last.serial;
        let template = self
            .state
            .local_template
            .as_deref()
            .and_then(|name| self.data.players.get(name));
        let events: Vec<String> = fresh
            .iter()
            .filter_map(|sound| match sound.kind {
                crate::rules::map_scripts::ObjectiveSoundKind::Added => {
                    template?.objective_added_sound.clone()
                }
                crate::rules::map_scripts::ObjectiveSoundKind::Completed => {
                    template?.objective_completed_sound.clone()
                }
            })
            .collect();
        for event in events {
            crate::diag::diag!("script: objective sound {event}");
            self.audio.play(&event, None, None);
        }
    }

    pub(super) fn drive_notification(&mut self) {
        use crate::notification_box::Effect;
        use crate::notification_host::Callback;
        let now_ms = u64::try_from(self.ui_clock.as_millis()).unwrap_or(u64::MAX);
        let letterboxed = self.script_cine().is_some_and(|cine| cine.letterbox);
        let command = self
            .script_cine()
            .and_then(|cine| cine.notification.clone())
            .filter(|command| command.seq != self.state.cinematic.notification_seen);
        if !self.overlay_enabled {
            return;
        }
        let Some(overlay) = self.apt_overlay.as_mut() else {
            return;
        };
        if let Some(command) = command {
            self.state.cinematic.notification_seen = command.seq;
            match self.data.notification_box.get(&command.kind) {
                Some(kind) => {
                    let strings = &self.state.strings;
                    let icon = crate::notification_box::icon_for(
                        kind.icon.as_deref(),
                        &command.object_type,
                        |name| {
                            self.data
                                .templates
                                .get(name)
                                .map(|record| record.button_image.clone())
                        },
                    );
                    let entry = crate::notification_box::Entry {
                        title: kind
                            .title
                            .as_deref()
                            .map(|t| strings.fetch(t))
                            .unwrap_or_default(),
                        icon,
                        open_audio: kind.open_audio.clone(),
                        message: strings.fetch(&command.label),
                        font: kind.font.clone(),
                        colour: kind.colour,
                        ms: u32::try_from(command.ms).unwrap_or(0),
                        anchor: overlay.notification_anchor(),
                    };
                    crate::diag::diag!(
                        "script: notification {} - {:?}: {:?} ({} ms, {})",
                        command.kind,
                        entry.title,
                        entry.message,
                        entry.ms,
                        entry.anchor.name()
                    );
                    self.state.cinematic.notification.submit(entry);
                }
                None => crate::diag::diag!(
                    "script: notification type {} is not declared - nothing shown",
                    command.kind
                ),
            }
        }
        let (width, callbacks) = overlay.take_notification_state();
        let manager = &mut self.state.cinematic.notification;
        manager.set_message_width(width);
        for callback in callbacks {
            match callback {
                Callback::Initialized => manager.on_initialized(),
                Callback::Opened => manager.on_open(now_ms),
                Callback::Closed => manager.on_closed(),
                Callback::CloseClicked => {
                    if let Some(effect) = manager.on_close_button() {
                        overlay.apply_notification(effect);
                    }
                }
            }
        }
        for effect in manager.update(now_ms, letterboxed) {
            if let Effect::Present(entry) = &effect {
                if let Some(event) = entry.open_audio.as_deref() {
                    self.audio.play(event, None, None);
                }
            }
            overlay.apply_notification(effect);
        }
    }

    pub(super) fn feed_objectives_overlay(&mut self) {
        let frame = self.state.simulation.frame().get();
        let campaign = matches!(self.config, MatchConfig::Campaign(_));
        let Some((objectives, input_disabled)) = self
            .script_cine()
            .map(|cine| (cine.objectives.clone(), cine.input_disabled))
        else {
            return;
        };
        let feed = crate::objectives_host::Feed {
            rows: crate::objectives::extern_values(&objectives),
            input_enabled: !(campaign && input_disabled),
        };
        let memory = &mut self.state.cinematic;
        let new_objective =
            crate::objectives::note_newly_shown(&mut memory.objectives_shown, &objectives);
        if new_objective {
            crate::diag::diag!(
                "script: objective shown - {} of {} shown",
                objectives.iter().filter(|o| o.shown).count(),
                objectives.len()
            );
        }
        let timed = memory.button_flashes.objectives_on(frame);
        let Some(overlay) = self.apt_overlay.as_mut() else {
            return;
        };
        overlay.set_campaign(campaign);
        overlay.set_objectives_feed(feed);
        if new_objective {
            overlay.note_new_objective();
        }
        overlay.set_timed_objectives_flash(timed);
    }

    fn take_script_radar_events(&mut self) {
        let Some(cine) = self.script_cine() else {
            return;
        };
        let seen = self.state.cinematic.radar_seen;
        let fresh: Vec<_> = cine
            .radar
            .iter()
            .filter(|event| event.serial.wrapping_sub(seen).wrapping_sub(1) < u32::MAX / 2)
            .copied()
            .collect();
        let Some(last) = fresh.last() else {
            return;
        };
        self.state.cinematic.radar_seen = last.serial;
        let now = self.ui_clock.as_secs_f32();
        for event in fresh {
            if let Some(kind) = crate::radar_events::RadarEventType::from_index(event.kind) {
                self.radar_events
                    .create(kind, (event.at.x, event.at.y), now);
            }
            self.audio.push_jump([event.at.x, event.at.y]);
            crate::diag::diag!(
                "script: radar event type {} at ({:.0}, {:.0})",
                event.kind,
                event.at.x,
                event.at.y
            );
        }
    }

    pub(super) fn dev_flash(&mut self, argument: &str) -> Result<String, String> {
        use crate::rules::map_scripts::UiFlashTarget;
        let words: Vec<&str> = argument.split_whitespace().collect();
        let usage = || format!("do flash: {argument:?} is not KIND [NAME] AMOUNT");
        let (kind, name, amount) = match words.as_slice() {
            [kind, amount] => (*kind, None, *amount),
            [kind, name, amount] => (*kind, Some(*name), *amount),
            _ => return Err(usage()),
        };
        let amount: i32 = amount.parse().map_err(|_| usage())?;
        let target = match (kind, name) {
            ("cameo", Some(name)) => UiFlashTarget::Cameo(name.to_owned()),
            ("hero", Some(name)) => UiFlashTarget::HeroSelect(name.to_owned()),
            ("builder", None) => UiFlashTarget::SelectBuilder,
            ("spellstore", None) => UiFlashTarget::SpellStore,
            ("objectives", None) => UiFlashTarget::Objectives,
            _ => return Err(usage()),
        };
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        let frame = self.state.simulation.frame().get();
        self.state
            .cinematic
            .button_flashes
            .raise(&target, amount, frame, &hud.fielded_heroes);
        Ok(format!("flash {target:?} {amount} at frame {frame}"))
    }

    pub(super) fn dev_cine(&mut self, argument: &str) -> Result<String, String> {
        use crate::rules::map_scripts::CineState;
        let (verb, rest) = argument.split_once(' ').unwrap_or((argument, ""));
        let on = rest.trim() != "off";
        let dev = self.state.cinematic.dev.get_or_insert_with(|| {
            CineState::with_objectives(&[
                ("OBJ:One".to_owned(), false),
                ("OBJ:Two".to_owned(), false),
                ("OBJ:Bonus".to_owned(), true),
            ])
        });
        let frame = self.state.simulation.frame().get();
        match verb {
            "letterbox" => dev.letterbox = on,
            "input" => {
                dev.input_disabled = !on;
                dev.input_seq = dev.input_seq.wrapping_add(1);
            }
            "freeze" => dev.time_frozen = on,
            "caption" => {
                let seq = dev.caption.as_ref().map_or(1, |c| c.seq.wrapping_add(1));
                dev.caption = Some(crate::rules::map_scripts::CaptionCmd {
                    seq,
                    label: rest.to_owned(),
                    ms: 5000,
                });
            }
            "notify" => {
                let (kind, label) = rest.split_once(' ').unwrap_or((rest, ""));
                let seq = dev
                    .notification
                    .as_ref()
                    .map_or(1, |n| n.seq.wrapping_add(1));
                dev.notification = Some(crate::rules::map_scripts::NotificationCmd {
                    seq,
                    kind: kind.to_owned(),
                    label: label.to_owned(),
                    ms: 5000,
                    object_type: String::new(),
                });
            }
            "objective" => {
                dev.show_objective(rest.trim().parse().map_err(|_| "do cine objective N")?)
            }
            "complete" => dev.complete_objective(
                rest.trim().parse().map_err(|_| "do cine complete N")?,
                true,
                frame.max(10),
            ),
            "radar" => {
                let (xy, kind) = rest.split_once(' ').unwrap_or((rest, "0"));
                let (x, y) = xy.split_once(',').ok_or("do cine radar X,Y [TYPE]")?;
                dev.push_radar(
                    mint_movement::Vec2::new(
                        x.parse().map_err(|_| "do cine radar X,Y")?,
                        y.parse().map_err(|_| "do cine radar X,Y")?,
                    ),
                    kind.trim().parse().unwrap_or(0),
                );
            }
            _ => return Err(format!("do cine: unknown {verb}")),
        }
        Ok(format!("cine {argument}"))
    }

    pub(super) fn sync_script_input(&mut self) {
        let Some(cine) = self.script_cine() else {
            return;
        };
        let (seq, disabled) = (cine.input_seq, cine.input_disabled);
        if seq == self.script_input_seen {
            return;
        }
        self.script_input_seen = seq;
        if !disabled {
            return;
        }
        self.pointer.cancel();
        self.ui_router.release_capture();
        self.pan_forward = false;
        self.pan_back = false;
        self.pan_left = false;
        self.pan_right = false;
        self.placement = None;
        self.span = None;
        self.power_target = None;
        self.attack_move_armed = false;
        self.order_mode_waypoint = false;
        crate::diag::diag!("script: input disabled - selection cleared, orders dropped");
        self.submit(RotwkCommand::ClearSelection);
    }
}

fn newer_than(seq: u32, seen: u32) -> bool {
    seq.wrapping_sub(seen).wrapping_sub(1) < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_the_input_off_only_escape_releases_and_window_events_reach_the_match() {
        let key = |key, pressed| InputEvent::Key { key, pressed };
        assert!(passes_input_lock(&key(Key::Escape, true)));
        assert!(passes_input_lock(&key(Key::Character('a'), false)));
        assert!(passes_input_lock(&InputEvent::Resized {
            width: 10,
            height: 10
        }));
        assert!(!passes_input_lock(&key(Key::Character('a'), true)));
        assert!(!passes_input_lock(&key(Key::ArrowUp, true)));
        assert!(!passes_input_lock(&InputEvent::Scroll { lines: 1.0 }));
        assert!(!passes_input_lock(&InputEvent::PointerButton {
            button: mint_app::PointerButton::Secondary,
            pressed: true,
        }));
        assert!(!passes_input_lock(&InputEvent::PointerMoved {
            x: 1.0,
            y: 1.0
        }));
    }
}
