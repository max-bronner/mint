use mint_render::{RenderTargetHandle, Renderer};
use mint_sim::Entity;

use super::{viewport_aspect, Game};
use crate::rules::{CahPreview, RotwkCommand};

impl Game {
    pub(crate) fn draw_world_to_target(
        &mut self,
        renderer: &mut Renderer,
        blend: f32,
        target: RenderTargetHandle,
    ) -> mint_render::Result<()> {
        self.state.scene.render_target = Some(target);
        let drawn = self.draw_frame(renderer, blend, Some(&[]));
        self.state.scene.render_target = None;
        drawn
    }

    pub(crate) fn scene_poses(&self) -> Vec<(String, Entity, [f32; 2], f32)> {
        self.state
            .simulation
            .rules()
            .named_poses(self.state.simulation.state())
    }

    pub(crate) fn scene_position(&self, entity: Entity) -> Option<[f32; 2]> {
        self.state
            .simulation
            .rules()
            .position_of(self.state.simulation.state(), entity)
    }

    pub(crate) fn scene_lens(&self) -> (f32, f32, f32) {
        (
            self.data.camera.max_height,
            self.data.camera.pitch_degrees.to_radians(),
            viewport_aspect(self.viewport),
        )
    }

    pub(crate) fn scene_clip_names(&self, entity: Entity) -> Vec<String> {
        self.state.units.playing_clip_names(entity.index())
    }

    pub(crate) fn scene_clip_frames(&self, entity: Entity, name: &str) -> Option<u32> {
        self.state.units.clip_frames_named(entity.index(), name)
    }

    pub(crate) fn scene_submit(&mut self, command: CahPreview) {
        if let Err(error) = self
            .state
            .simulation
            .submit(self.acting_player, RotwkCommand::CahPreview(command))
        {
            eprintln!("cah scene: command refused: {error}");
        }
    }
}
