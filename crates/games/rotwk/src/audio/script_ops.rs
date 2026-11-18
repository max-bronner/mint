use std::collections::BTreeSet;
use std::sync::Arc;

use mint_render::Vec3;
use mint_sage_ini::audio::Submix;

use super::{owner_id, GameAudio};
use crate::diag::diag;
use crate::music_script::MusicCommand;
use crate::rules::map_scripts::{AudioCmd, AudioOp};

#[derive(Debug, Clone, Copy)]
struct Fade {
    from: f32,
    to: f32,
    ramp: f32,
    hold: f32,
    back: f32,
    elapsed: f32,
}

impl Fade {
    fn level(&self) -> Option<f32> {
        let t = self.elapsed;
        if t < self.ramp {
            return Some(self.from + (self.to - self.from) * (t / self.ramp));
        }
        let t = t - self.ramp;
        if t < self.hold {
            return Some(self.to);
        }
        let t = t - self.hold;
        if t < self.back {
            return Some(self.to + (self.from - self.to) * (t / self.back));
        }
        None
    }
}

#[derive(Debug)]
pub(super) struct ScriptState {
    seen: u32,
    pub(super) eva_on: bool,
    pub(super) disabled_objects: BTreeSet<u64>,
    pub(super) owns_music: bool,
    pub(super) music_gain: f32,
    pub(super) music_slider: f32,
    fade: Option<Fade>,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            seen: 0,
            eva_on: true,
            disabled_objects: BTreeSet::new(),
            owns_music: false,
            music_gain: 1.0,
            music_slider: 1.0,
            fade: None,
        }
    }
}

impl GameAudio {
    pub(crate) fn apply_script_audio(
        &mut self,
        pending: &[AudioCmd],
        ground: &dyn Fn(f32, f32) -> f32,
    ) {
        let seen = self.script.seen;
        for cmd in pending {
            if cmd.serial.wrapping_sub(seen).wrapping_sub(1) >= u32::MAX / 2 {
                continue;
            }
            self.script.seen = cmd.serial;
            self.apply_script_op(&cmd.op, ground);
        }
    }

    fn apply_script_op(&mut self, op: &AudioOp, ground: &dyn Fn(f32, f32) -> f32) {
        match op {
            AudioOp::Sound {
                name,
                at,
                owner,
                allow_overlap,
            } => {
                let at = at.map(|p| Vec3::new(p.x, p.y, ground(p.x, p.y)));
                let outcome = self.play(name, at, owner.map(owner_id));
                diag!(
                    "script: sound {name} (overlap {allow_overlap}) -> {}",
                    outcome.label()
                );
            }
            AudioOp::ObjectSound { entity, enabled } => {
                let owner = owner_id(*entity);
                if *enabled {
                    self.script.disabled_objects.remove(&owner);
                    if let Some(custom) = self.custom_ambients.get_mut(&owner) {
                        custom.enabled = true;
                    }
                } else {
                    self.script.disabled_objects.insert(owner);
                }
                diag!("script: object sound of {owner:#x} enabled {enabled}");
            }
            AudioOp::FadeImmunity { name, immune } => {
                diag!("script: sound {name} fade immunity {immune} (the pool fade has no per-event exemption)");
            }
            AudioOp::AllSubjectToFade => {}
            AudioOp::DisableEvent(name) => {
                self.audio.disable_event(name);
                diag!("script: sound {name} disabled");
            }
            AudioOp::EnableEvent(name) => {
                self.audio.enable_event(name);
                diag!("script: sound {name} enabled");
            }
            AudioOp::EnableAllEvents => {
                self.audio.enable_all_events();
                diag!("script: every sound enabled");
            }
            AudioOp::FadeVolume {
                from,
                to,
                ramp,
                hold,
                back,
            } => {
                self.script.fade = Some(Fade {
                    from: *from,
                    to: *to,
                    ramp: *ramp,
                    hold: *hold,
                    back: *back,
                    elapsed: 0.0,
                });
                diag!("script: fade audio {from}->{to} over {ramp}s, hold {hold}s, back {back}s");
            }
            AudioOp::MusicVolume(volume) => {
                self.script.music_gain = *volume;
                self.audio
                    .set_submix_gain(Submix::Music, self.script.music_slider * volume);
                diag!("script: music volume {volume}");
            }
            AudioOp::Background { suspended } => {
                if *suspended {
                    self.pause();
                } else {
                    self.resume();
                }
                diag!("script: background sounds suspended {suspended}");
            }
            AudioOp::Eva(on) => {
                self.script.eva_on = *on;
                diag!("script: EVA enabled {on}");
            }
            AudioOp::Music(command) => {
                diag!("script: map script asks music {command:?}");
                let vfs = Arc::clone(&self.vfs);
                match command.clone() {
                    MusicCommand::Replace(request) => self.audio.music_replace(&vfs, request),
                    MusicCommand::Push(request) => self.audio.music_push(&vfs, request),
                    MusicCommand::Pop { fade_out, fade_in } => {
                        self.audio.music_pop(fade_out, fade_in);
                    }
                }
            }
            AudioOp::MusicTurnOff { fade } => {
                self.script.owns_music = true;
                self.audio.music_stop(*fade);
                diag!("script: music scripting turned off");
            }
            AudioOp::MusicReturn { fade: _ } => {
                self.script.owns_music = false;
                diag!("script: music scripting returned");
            }
            AudioOp::MusicReset { fade } => {
                self.script.owns_music = false;
                self.audio.music_stop(*fade);
                diag!("script: music scripting reset");
            }
        }
    }

    pub(super) fn step_script_fade(&mut self, dt: f32) {
        let Some(fade) = self.script.fade.as_mut() else {
            return;
        };
        fade.elapsed += dt.max(0.0);
        match fade.level() {
            Some(level) => self.audio.set_pool_gain(level),
            None => {
                self.script.fade = None;
                self.audio.set_pool_gain(1.0);
            }
        }
    }
}
