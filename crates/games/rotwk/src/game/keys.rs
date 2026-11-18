use super::*;

impl Game {
    pub(super) fn on_key(&mut self, key: Key, pressed: bool) {
        crate::diag::diag!("input: key {key:?} {}", if pressed { "down" } else { "up" });
        if pressed {
            if let Key::Character(c) = key {
                if let Some(action) = self.command_hotkey_action(c) {
                    self.slot_action(action);
                    return;
                }
            }
        }
        match key {
            Key::ArrowUp => self.pan_forward = pressed,
            Key::ArrowDown => self.pan_back = pressed,
            Key::ArrowLeft => self.pan_left = pressed,
            Key::ArrowRight => self.pan_right = pressed,
            Key::Shift => self.shift_held = pressed,
            Key::Alt if self.order_mode_key == Some(Key::Alt) => self.order_mode_waypoint = pressed,
            Key::Escape if pressed => {
                let store_open = self.mode == Mode::Match
                    && self
                        .apt_overlay
                        .as_ref()
                        .is_some_and(AptOverlay::spell_store_open);
                let objectives_open = self.mode == Mode::Match
                    && self
                        .apt_overlay
                        .as_ref()
                        .is_some_and(AptOverlay::objectives_open);
                match escape_action(
                    self.span.is_some(),
                    self.placement.is_some(),
                    self.power_target.is_some(),
                    self.mode == Mode::Match,
                    store_open,
                    objectives_open,
                ) {
                    EscapeAction::CancelSpan => {
                        self.span = None;
                        println!("wall: span cancelled");
                    }
                    EscapeAction::CancelPlacement => {
                        self.placement = None;
                        println!("placement: cancelled");
                    }
                    EscapeAction::CancelPowerTarget => {
                        self.power_target = None;
                        println!("power: target cancelled");
                    }
                    EscapeAction::CloseStore => {
                        if let Some(overlay) = self.apt_overlay.as_mut() {
                            overlay.escape_spell_store();
                        }
                    }
                    EscapeAction::CloseObjectives => {
                        if let Some(overlay) = self.apt_overlay.as_mut() {
                            overlay.escape_objectives();
                        }
                    }
                    EscapeAction::ToggleQuitMenu => self.toggle_quit_menu(),
                    EscapeAction::EndMatchOrReturnToShell => self.end_match_or_return_to_shell(),
                }
            }
            key if self
                .diplomacy_key
                .is_some_and(|bound| bound.key == key && bound.on_press == pressed)
                && self.mode == Mode::Match
                && self.overlay_enabled =>
            {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    overlay.toggle_objectives();
                }
            }
            key if self
                .spell_store_key
                .is_some_and(|bound| bound.key == key && bound.on_press == pressed)
                && self.mode == Mode::Match
                && self.overlay_enabled =>
            {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    overlay.request_spell_store_toggle();
                }
            }
            key if pressed && self.view_last_radar_event_key == Some(key) => {
                if let Some([x, y]) = self.audio.jump_press(self.ui_clock) {
                    let z = ground_at(&self.state.scene, x, y);
                    self.state.camera.jump_to(Vec3::new(x, y, z));
                    println!("radar: jump to ({x:.1}, {y:.1})");
                } else {
                    println!("radar: jump list empty");
                }
            }
            Key::Character(';') if pressed => {
                self.overlay_enabled = !self.overlay_enabled;
                if !self.overlay_enabled {
                    self.ui_router.release_capture();
                }
                println!(
                    "render: APT overlay (command bar/palantir) {} [{} of 2] - key ';'{}",
                    if self.overlay_enabled { "on" } else { "off" },
                    u8::from(self.overlay_enabled).saturating_add(1),
                    if self.apt_overlay.is_none() {
                        " - did not load; toggling changes nothing"
                    } else {
                        ""
                    }
                );
            }
            Key::Character('-') if pressed => {
                let mode = self.toggle_scale_mode();
                println!(
                    "--ui-scale {}{}",
                    mode.as_str(),
                    if self.apt_overlay.is_none() {
                        " - overlay did not load; toggling changes nothing yet"
                    } else {
                        ""
                    }
                );
            }
            Key::Character('/') if pressed => {
                let mode = self.toggle_image_modes();
                println!(
                    "image modes = {}{}",
                    mode.label(),
                    if self.apt_overlay.is_none() {
                        " - overlay did not load; toggling changes nothing yet"
                    } else {
                        ""
                    }
                );
            }
            Key::Character('0') if pressed => {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    let anchor = overlay.toggle_text_anchor();
                    println!("text anchor = {}", anchor.label());
                } else {
                    println!("text anchor - overlay did not load; toggling changes nothing yet");
                }
            }
            Key::Function(1) if pressed => {
                let next = Modes {
                    search: self.modes.search.next(),
                    ..self.modes
                };
                self.cycle_modes(next);
            }
            Key::Function(2) if pressed => {
                let next = Modes {
                    smoothing: self.modes.smoothing.next(),
                    ..self.modes
                };
                self.cycle_modes(next);
            }
            Key::Function(3) if pressed => {
                let next = Modes {
                    avoidance: self.modes.avoidance.next(),
                    ..self.modes
                };
                self.cycle_modes(next);
            }
            Key::Function(5) if pressed => {
                let next = self.state.scene.terrain_shading.modulation.shroud.next();
                self.state.scene.terrain_shading.modulation.shroud = next;
                let rung = TerrainShroud::ALL
                    .iter()
                    .position(|mode| *mode == next)
                    .map_or(0, |index| index.saturating_add(1));
                println!(
                    "render: shroud {} [{rung} of {}]",
                    next.label(),
                    TerrainShroud::ALL.len()
                );
            }
            Key::Function(6) if pressed => {
                let players = self.state.simulation.rules().player_count();
                let next = self
                    .state
                    .shroud_viewer
                    .get()
                    .checked_add(1)
                    .filter(|slot| *slot < players)
                    .unwrap_or(0);
                self.state.shroud_viewer = PlayerId::new(next);
                println!(
                    "render: shroud viewer is player {next} [{} of {players}] - \
                     the simulation computed every slot either way",
                    next.saturating_add(1)
                );
            }
            Key::Character('j') if pressed => self.cycle_passability(),
            Key::Function(9) if pressed => {
                self.obstacle_overlay = !self.obstacle_overlay;
                println!(
                    "render: obstacle overlay {} [{} of 2] - amber static, red dynamic",
                    if self.obstacle_overlay { "on" } else { "off" },
                    u8::from(self.obstacle_overlay).saturating_add(1)
                );
            }
            Key::Character(c) if pressed && queue_button(c).is_some() => {
                if let Some(button) = queue_button(c) {
                    self.digit_pressed(button);
                }
            }
            Key::CtrlCharacter('f') if pressed => {
                self.preview_decals = self.preview_decals.next();
                println!("formation: {}", self.preview_decals.label());
            }
            Key::CtrlCharacter('z') if pressed => {
                self.submit(RotwkCommand::CancelProduction);
                self.submit(RotwkCommand::CancelConstruction);
            }
            Key::Function(10) if pressed => self.audio.cycle_rolloff(),
            Key::Function(11) if pressed && self.shift_held => self.toggle_mirror_skip(),
            Key::Function(11) if pressed => self.cycle_water_reflection(),
            Key::Character('`' | '^') if pressed => self.cycle_fx_detail(),
            Key::Function(8) if pressed => self.toggle_surface_overlays(),
            Key::Character('+') if pressed => self.cycle_sun_convention(),
            Key::Character('\\' | '#') if pressed => self.cycle_waves(),
            Key::Character('\'') if pressed => {
                let next = self.state.visuals.bar_style.next();
                self.state.visuals.bar_style = next;
                println!(
                    "render: health bar fill {} [{} of 2]",
                    next.label(),
                    next.rung()
                );
            }
            Key::Character(c) if pressed && stance_for_key(c).is_some() => {
                if let Some(stance) = stance_for_key(c) {
                    self.set_stance(stance);
                }
            }
            Key::Character('s') if pressed => self.submit(RotwkCommand::Stop),
            Key::Character('a') if pressed => self.slot_action(SlotAction::ArmAttackMove),
            Key::Character('.') if pressed => {
                let next = self.state.units.blending().next();
                self.state.units.set_blending(next);
                let rung = mint_sage_scene::Blending::ALL
                    .iter()
                    .position(|blending| *blending == next)
                    .map_or(0, |index| index.saturating_add(1));
                println!(
                    "render: animation blending {} [{rung} of {}]",
                    next.label(),
                    mint_sage_scene::Blending::ALL.len()
                );
            }
            Key::CtrlCharacter('g') if pressed => {
                let next = self.state.scene.particle_scene.flat_size().next();
                self.state.scene.particle_scene.set_flat_size(next);
                println!("render: flat particles drawn {}", next.label());
            }
            Key::CtrlCharacter('h') if pressed => {
                let next = self.state.scene.particle_scene.smudge_jitter().next();
                self.state.scene.particle_scene.set_smudge_jitter(next);
                println!("render: heat haze shift redrawn {}", next.label());
            }
            Key::CtrlCharacter('t') if pressed => {
                let on = !self.state.units.transitions_enabled();
                self.state.units.set_transitions_enabled(on);
                println!(
                    "render: animation state scripts {}",
                    if on {
                        "run (transitions on)"
                    } else {
                        "not run (transitions off)"
                    }
                );
            }
            Key::CtrlCharacter('w') if pressed => {
                let next = self.state.scene.particle_scene.wind().next();
                let with_wind = self.state.scene.particle_scene.set_wind(next);
                println!(
                    "render: particle wind = {}, {with_wind} live system(s) have wind",
                    next.label()
                );
            }
            Key::CtrlCharacter('b') if pressed => {
                let next = self.state.scene.breeze.next();
                self.state.scene.breeze = next;
                let breeze = next.breeze();
                println!(
                    "render: tree breeze = {} (intensity {}, lean {}, period {} s)",
                    next.label(),
                    breeze.intensity,
                    breeze.lean,
                    breeze.period
                );
            }
            Key::CtrlCharacter('k') if pressed => {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    let covers_remaining = overlay.toggle_timer_overlay_direction();
                    println!(
                        "render: button clock covers the part {} [{}]",
                        if covers_remaining {
                            "still to go"
                        } else {
                            "already done"
                        },
                        if covers_remaining {
                            "retail"
                        } else {
                            "NOT retail, comparison only"
                        }
                    );
                }
            }
            Key::CtrlCharacter('y') if pressed => {
                if let Some(objects) = self.state.scene.object_scene.as_mut() {
                    objects.tainted_still = !objects.tainted_still;
                    println!(
                        "render: TaintedTree trees {}",
                        if objects.tainted_still {
                            "still"
                        } else {
                            "swaying"
                        }
                    );
                }
            }
            Key::CtrlCharacter('c') if pressed => {
                self.state.scene.cloud_darkening_on = !self.state.scene.cloud_darkening_on;
                println!(
                    "render: cloud darkening {} (level {})",
                    if self.state.scene.cloud_darkening_on {
                        "on"
                    } else {
                        "off"
                    },
                    self.state.scene.cloud_darkening.level()
                );
            }
            Key::CtrlCharacter('o') if pressed => {
                self.state.scene.fog_on = !self.state.scene.fog_on;
                println!(
                    "render: distance fog {} ({:?})",
                    if self.state.scene.fog_on { "on" } else { "off" },
                    self.state.scene.fog
                );
            }
            Key::CtrlCharacter('p') if pressed => {
                let next = self.state.scene.particle_scene.attached_frame().next();
                self.state.scene.particle_scene.set_attached_frame(next);
                println!(
                    "render: attached particle systems use {} axes",
                    next.label()
                );
            }
            Key::CtrlCharacter('j') if pressed => {
                let next = self.state.scene.particle_scene.bone_lookup().next();
                self.state.scene.particle_scene.set_bone_lookup(next);
                println!(
                    "render: particle bone lookup {} [{}]",
                    next.label(),
                    match next {
                        mint_sage_scene::BoneLookup::DrawableWide =>
                            "default, retail-unproven unless ParticleBonesCheckDrawable is set",
                        mint_sage_scene::BoneLookup::ModuleLocal => "alternative, retail-unproven",
                    }
                );
            }
            Key::Function(7) if pressed => {
                let next = self.state.scene.particle_scene.bone_frame().next();
                self.state.scene.particle_scene.set_bone_frame(next);
                println!("render: fx at bone oriented by {}", next.label());
            }
            Key::CtrlCharacter('e') if pressed => {
                let next = self.state.scene.particle_scene.additive_alpha().next();
                self.state.scene.particle_scene.set_additive_alpha(next);
                println!(
                    "render: an additive mesh particle's alpha is {}",
                    next.label()
                );
            }
            Key::CtrlCharacter('u') if pressed => {
                let report = self.state.scene.particle_scene.budget_report();
                println!("particles: {report:?}");
            }
            Key::CtrlCharacter('l') if pressed => {
                let next = self.state.scene.particle_scene.beam_length().next();
                self.state.scene.particle_scene.set_beam_length(next);
                println!("render: SetTargetMatrix beam length is {}", next.label());
            }
            Key::CtrlCharacter('m') if pressed => {
                let next = self.state.scene.border_in_mirror.next();
                self.state.scene.border_in_mirror = next;
                println!(
                    "water: border in mirror: {} [{} of 2]",
                    next.label(),
                    next.ordinal()
                );
            }
            Key::Character('[') if pressed => {
                let next = self.state.scene.particle_scene.flipbook().next();
                self.state.scene.particle_scene.set_flipbook(next);
                println!(
                    "render: particle flipbook {} [{} of {}]",
                    next.label(),
                    next.rung(),
                    mint_sage_scene::FlipbookMode::ALL.len()
                );
            }
            Key::Character(',') if pressed => {
                let shown = self.state.fx.cycle();
                println!(
                    "render: effects {} [{} of {}]",
                    shown.label(),
                    shown.rung(),
                    crate::fx_present::FxShown::ALL.len()
                );
            }
            Key::Function(12) if pressed => {
                self.box_mode = self.box_mode.next();
                let rung = BoxMode::ALL
                    .iter()
                    .position(|mode| *mode == self.box_mode)
                    .map_or(0, |index| index.saturating_add(1));
                println!(
                    "selection: {} [{rung} of {}]",
                    self.box_mode.label(),
                    BoxMode::ALL.len()
                );
            }
            Key::Character('<') if pressed => {
                let command = crate::test_controls::TestControls::reset_powers(self.acting_player);
                self.submit(command);
            }
            Key::Character('?') if pressed => {
                let command = crate::test_controls::TestControls::spell_points(
                    &self.state.simulation,
                    self.acting_player,
                    5,
                );
                self.submit(command);
            }
            Key::CtrlCharacter('d') if pressed => {
                self.submit(crate::test_controls::TestControls::damage(25));
            }
            Key::CtrlCharacter('i') if pressed => {
                if let Some(command) =
                    crate::test_controls::TestControls::toggle_dummies(&self.state.simulation)
                {
                    self.submit(command);
                }
            }
            Key::Character('>') | Key::CtrlCharacter('x') if pressed => {
                let quarters = if key == Key::Character('>') { 4 } else { 1 };
                if let Some(command) = self.test_controls.level_up(
                    &self.state.simulation,
                    self.acting_player,
                    quarters,
                ) {
                    self.submit(command);
                }
            }
            Key::CtrlShiftDigit(digit) if pressed => {
                let frame = self.state.simulation.frame();
                self.test_controls.set_speed(digit, frame);
            }
            Key::CtrlShiftCharacter('l') if pressed => {
                if crate::capture::is_capturing() {
                    self.capture_snapshot();
                    crate::capture::toggle();
                } else {
                    crate::capture::toggle();
                    self.capture_snapshot();
                }
            }
            Key::CtrlShiftCharacter('m') if pressed => crate::capture::mark(),
            Key::Enter
                if pressed
                    && self
                        .apt_overlay
                        .as_ref()
                        .is_some_and(AptOverlay::spell_store_open) =>
            {
                if let Some(overlay) = self.apt_overlay.as_mut() {
                    overlay.close_spell_store();
                }
            }
            Key::Enter if pressed => {
                let players = usize::from(self.state.simulation.rules().player_count());
                let next = self.control.next(self.acting_player, players);
                self.acting_player = next;
                println!(
                    "control: acting as player {} [--control {}]",
                    next.get(),
                    self.control.label()
                );
            }
            _ => {}
        }
    }

    fn cycle_passability(&mut self) {
        let next = self.passability_sources.next();
        match build_match(
            &self.state.scene,
            &self.data,
            &self.state.vfs,
            &self.config,
            next,
        ) {
            Ok(fresh) => {
                self.passability_sources = next;
                self.state.simulation = fresh.simulation;
                self.state.goal = None;
                self.state.previous.clear();
                self.state.snapshot.invalidate();
                self.state.shroud_source = None;
                self.modes = Modes::default();
                let rung = PassabilitySources::ALL
                    .iter()
                    .position(|rung| *rung == next)
                    .map_or(0, |index| index.saturating_add(1));
                println!(
                    "passability: {} [{rung} of {}] - match restarted, camera held",
                    next.label(),
                    PassabilitySources::ALL.len()
                );
            }
            Err(error) => {
                eprintln!("could not rebuild the passability grid: {error} - keeping the old one");
            }
        }
    }

    fn toggle_surface_overlays(&mut self) {
        let scene = &mut self.state.scene;
        let mut now = false;
        for slot in &scene.surface_overlays {
            if let Some(instance) = scene.drawn.get_mut(*slot) {
                instance.surface_overlay = !instance.surface_overlay;
                now = instance.surface_overlay;
            }
        }
        println!(
            "render: ground overlays {} [{} of 2] - {} instance(s)",
            if now { "in-surface" } else { "sort-level" },
            if now { 1 } else { 2 },
            scene.surface_overlays.len()
        );
    }

    fn cycle_sun_convention(&mut self) {
        let scene = &mut self.state.scene;
        scene.sun = scene.sun.next();
        scene.rig.mirror_azimuth();
        let d = scene.rig.lights[0].direction;
        println!(
            "render: sun {} [{} of 2] - travels ({:.2}, {:.2}, {:.2})",
            scene.sun.label(),
            if scene.sun == mint_sage_scene::SunConvention::default() {
                1
            } else {
                2
            },
            d.x,
            d.y,
            d.z
        );
    }

    fn cycle_fx_detail(&mut self) {
        let scene = &mut self.state.scene;
        scene.fx_water.detail = scene.fx_water.detail.next();
        crate::scene::apply_fx_visibility(scene, true);
        let drawn: Vec<String> = scene
            .fx_water
            .labels()
            .into_iter()
            .zip(scene.fx_water.visibility())
            .filter(|(_, v)| *v)
            .map(|(l, _)| l)
            .collect();
        println!(
            "water: fx detail {}, drawing {}",
            scene.fx_water.detail.label(),
            drawn.join(", ")
        );
    }

    fn toggle_mirror_skip(&mut self) {
        let scene = &mut self.state.scene;
        scene.mirror_skip = !scene.mirror_skip;
        println!(
            "mirror skip: {} - {} while it was {}",
            if scene.mirror_skip { "on" } else { "off" },
            scene.mirror_tally.line(),
            if scene.mirror_skip { "off" } else { "on" }
        );
        scene.mirror_tally.reset();
    }

    fn cycle_water_reflection(&mut self) {
        let leaving = self.state.scene.reflection;
        let next = leaving.next();
        self.state.scene.reflection = next;
        println!(
            "water: reflection {} [{} of 6] - planes {}, {} on '{}', {}",
            next.label(),
            next.ordinal(),
            self.state.scene.water_scene.planes.len(),
            self.state.scene.mirror_tally.line(),
            leaving.label(),
            crate::report::frame_line(
                &format!("frames on '{}'", leaving.label()),
                self.mode_frames.summary()
            )
        );
        self.mode_frames.reset();
        self.state.scene.mirror_tally.reset();
    }

    fn cycle_waves(&mut self) {
        let leaving = self.state.scene.wave_rung;
        let next = leaving.next();
        self.state.scene.wave_rung = next;
        println!(
            "water: waves {} [{} of 5] - {}",
            next.label(),
            next.ordinal(),
            crate::report::frame_line(
                &format!("frames on '{}'", leaving.label()),
                self.mode_frames.summary()
            )
        );
        self.mode_frames.reset();
    }
}

fn stance_for_key(key: char) -> Option<mint_sage_ini::Stance> {
    match key {
        'f' => Some(mint_sage_ini::Stance::Battle),
        'g' => Some(mint_sage_ini::Stance::HoldGround),
        'd' => Some(mint_sage_ini::Stance::Aggressive),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EscapeAction {
    CancelSpan,
    CancelPlacement,
    CancelPowerTarget,
    CloseStore,
    CloseObjectives,
    ToggleQuitMenu,
    EndMatchOrReturnToShell,
}

pub(crate) fn escape_action(
    span_active: bool,
    placement_active: bool,
    power_target_active: bool,
    in_match: bool,
    store_open: bool,
    objectives_open: bool,
) -> EscapeAction {
    if span_active {
        EscapeAction::CancelSpan
    } else if placement_active {
        EscapeAction::CancelPlacement
    } else if power_target_active {
        EscapeAction::CancelPowerTarget
    } else if in_match && objectives_open {
        EscapeAction::CloseObjectives
    } else if in_match && store_open {
        EscapeAction::CloseStore
    } else if in_match {
        EscapeAction::ToggleQuitMenu
    } else {
        EscapeAction::EndMatchOrReturnToShell
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_action, stance_for_key, EscapeAction};
    use mint_sage_ini::Stance;

    #[test]
    fn d_orders_aggressive_not_the_retired_c_stand_in() {
        assert_eq!(stance_for_key('d'), Some(Stance::Aggressive));
        assert_eq!(stance_for_key('c'), None);
    }

    #[test]
    fn f_and_g_order_battle_and_hold_ground() {
        assert_eq!(stance_for_key('f'), Some(Stance::Battle));
        assert_eq!(stance_for_key('g'), Some(Stance::HoldGround));
    }

    #[test]
    fn a_letter_with_no_stance_orders_nothing() {
        assert_eq!(stance_for_key('q'), None);
    }

    #[test]
    fn a_second_escape_opens_the_quit_menu_once_the_store_has_closed() {
        let first = escape_action(false, false, false, true, true, false);
        assert_eq!(first, EscapeAction::CloseStore);
        let second = escape_action(false, false, false, true, false, false);
        assert_eq!(second, EscapeAction::ToggleQuitMenu);
    }

    #[test]
    fn an_in_progress_placement_outranks_the_store_and_the_quit_menu() {
        assert_eq!(
            escape_action(true, false, false, true, true, false),
            EscapeAction::CancelSpan
        );
        assert_eq!(
            escape_action(false, true, false, true, true, false),
            EscapeAction::CancelPlacement
        );
        assert_eq!(
            escape_action(false, false, true, true, true, false),
            EscapeAction::CancelPowerTarget
        );
    }

    #[test]
    fn escape_closes_the_objectives_screen_before_the_quit_menu() {
        assert_eq!(
            escape_action(false, false, false, true, false, true),
            EscapeAction::CloseObjectives
        );
        assert_eq!(
            escape_action(false, false, false, true, false, false),
            EscapeAction::ToggleQuitMenu
        );
    }

    #[test]
    fn escape_outside_a_match_returns_to_the_shell() {
        assert_eq!(
            escape_action(false, false, false, false, false, false),
            EscapeAction::EndMatchOrReturnToShell
        );
    }
}
