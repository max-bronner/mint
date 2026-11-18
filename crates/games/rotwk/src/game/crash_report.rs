use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;

use super::Game;
use crate::crash::{Failure, MatchReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForcedCrash {
    Render,
    Panic,
    Simulation,
}

impl ForcedCrash {
    pub(super) fn parse(name: &str) -> Result<Self, String> {
        match name {
            "render" => Ok(Self::Render),
            "panic" => Ok(Self::Panic),
            "sim" => Ok(Self::Simulation),
            _ => Err(format!("crash: {name:?} is not render, panic or sim")),
        }
    }
}

impl Game {
    pub(super) fn object_counts(&self, map_players_only: bool) -> BTreeMap<(u8, String), usize> {
        let rules = self.state.simulation.rules();
        let mut counts = BTreeMap::new();
        let Some(ids) = rules.ids() else {
            return counts;
        };
        let world = self.state.simulation.state();
        if let Some(store) = world.store(ids.kind) {
            for (entity, kind) in store.iter() {
                let Some(owner) = world.get(ids.owner, entity).map(|owner| owner.0) else {
                    continue;
                };
                if map_players_only && rules.map_player_name(owner).is_none() {
                    continue;
                }
                let name = self
                    .state
                    .kinds
                    .get(*kind)
                    .map_or_else(|| format!("kind {}", kind.0), |e| e.record.name.clone());
                let count: &mut usize = counts.entry((owner.get(), name)).or_default();
                *count = count.saturating_add(1);
            }
        }
        counts
    }

    pub(super) fn force_update_crash(&mut self) {
        match self.forced_crash {
            Some(ForcedCrash::Panic) => {
                self.forced_crash = None;
                forced_panic();
            }
            Some(ForcedCrash::Simulation) => {
                self.forced_crash = None;
                self.failure = Some(Failure::message(
                    "simulation",
                    "forced by `do crash sim`; the simulation itself did not fail",
                ));
            }
            Some(ForcedCrash::Render) | None => {}
        }
    }

    pub(super) fn force_draw_crash(
        &mut self,
        renderer: &mut mint_render::Renderer,
    ) -> mint_render::Result<()> {
        if self.forced_crash != Some(ForcedCrash::Render) {
            return Ok(());
        }
        self.forced_crash = None;
        let triangle = mint_render::MeshData {
            vertices: vec![mint_render::Vertex::default(); 3],
            indices: vec![0, 1, 2],
        };
        let four = mint_render::MeshData {
            vertices: vec![mint_render::Vertex::default(); 4],
            indices: vec![0, 1, 2],
        };
        let mesh = renderer.upload_mesh(&triangle)?;
        crate::crash::event(format_args!(
            "mesh update: forced test mesh, 4 vertices into 3"
        ));
        renderer
            .update_mesh_vertices(mesh, &four)
            .map_err(|error| error.about("the test mesh `do crash render` uploaded"))
    }
}

#[allow(clippy::panic)]
fn forced_panic() {
    panic!("forced by `do crash panic`");
}

impl MatchReport for Game {
    fn setup(&self, out: &mut String) {
        let _ = writeln!(
            out,
            "seed: {:#018x}\ncontrol: {:?}\nacting player: {}\npassability: {:?}\n{:#?}",
            self.state.simulation.seed(),
            self.control,
            self.acting_player.get(),
            self.passability_sources,
            self.config
        );
    }

    fn simulation(&self, out: &mut String) {
        let simulation = &self.state.simulation;
        let recording = simulation.recording();
        let last = recording.hashes().last().map_or_else(
            || format!("{:#018x} (before tick 0)", recording.initial_hash()),
            |hash| format!("{hash:#018x}"),
        );
        let _ = writeln!(
            out,
            "tick: {}\nmode: {:?}\nstate hash after the last completed tick: {last}\n\
             commands queued, not yet run: {}",
            simulation.frame().get(),
            self.mode,
            simulation.pending_commands()
        );
        let _ = writeln!(out, "state hash now: {:#018x}", simulation.state_hash());
        let counts = self.object_counts(false);
        let _ = writeln!(
            out,
            "objects alive: {} in {} owner/template group(s)",
            counts.values().sum::<usize>(),
            counts.len()
        );
        for ((owner, name), count) in &counts {
            let _ = writeln!(out, "  player {owner}: {name} x{count}");
        }
    }

    fn view(&self, out: &mut String) {
        let camera = self.state.camera.camera();
        let _ = writeln!(
            out,
            "camera eye: {:.1}, {:.1}, {:.1}\ncamera target: {:.1}, {:.1}, {:.1}\nviewport: {}x{}",
            camera.eye.x,
            camera.eye.y,
            camera.eye.z,
            camera.target.x,
            camera.target.y,
            camera.target.z,
            self.viewport.0,
            self.viewport.1
        );
        let units = self.units();
        let selected = units.iter().filter(|unit| unit.selected);
        let _ = writeln!(out, "selected: {}", selected.clone().count());
        for unit in selected {
            let name = unit
                .kind
                .and_then(|kind| self.state.kinds.get(crate::unit::Kind(kind)))
                .map_or("?", |entry| entry.record.name.as_str());
            let _ = writeln!(out, "  {:?} {name} owner {}", unit.entity, unit.owner.get());
        }
    }

    fn replay(&self, out: &mut dyn io::Write) -> io::Result<String> {
        let recording = self.state.simulation.recording();
        writeln!(
            out,
            "mint-rotwk command log 1\nreplay-format {}\nseed {:#018x}\ninitial-hash {:#018x}\n\
             frames {}\ncommands {}",
            recording.format_version(),
            recording.seed(),
            recording.initial_hash(),
            recording.hashes().len(),
            recording.commands().len()
        )?;
        writeln!(out, "# c <frame> <player> <sequence> <command>")?;
        for envelope in recording.commands() {
            let key = envelope.key();
            writeln!(
                out,
                "c {} {} {} {:?}",
                key.frame.get(),
                key.player.get(),
                key.sequence,
                envelope.command()
            )?;
        }
        writeln!(out, "# h <frame> <state hash after it>")?;
        for (frame, hash) in recording.hashes().iter().enumerate() {
            writeln!(out, "h {frame} {hash:#018x}")?;
        }
        Ok(format!(
            "{}: {} command(s) over {} frame(s), seed {:#018x}\n\
             re-run it with the launch line plus `--replay <path>` (docs/design/crash-dumps.md, \"Replaying\")",
            crate::crash::REPLAY_FILE,
            recording.commands().len(),
            recording.hashes().len(),
            recording.seed()
        ))
    }
}
