use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mint_render::Renderer;
use mint_vfs::Vfs;

use crate::game::{ControlSet, Game, GameBuild, MatchConfig, PassabilitySources};
use crate::game_data::GameData;
use crate::scene::{MatchScene, SceneProgress, SceneStep};
use crate::ui_resources::UiResources;

const SCENE_STEPS: u8 = 6;

pub(crate) const FRAME_BUDGET: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    ReadMap,
    Data,
    Scene,
    Match,
    Art,
    Finish,
}

impl Stage {
    const ALL: [Self; 6] = [
        Self::ReadMap,
        Self::Data,
        Self::Scene,
        Self::Match,
        Self::Art,
        Self::Finish,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::ReadMap => "map",
            Self::Data => "game data",
            Self::Scene => "scene",
            Self::Match => "match",
            Self::Art => "unit art",
            Self::Finish => "finish",
        }
    }

    const fn weight(self, data_reused: bool) -> u32 {
        match self {
            Self::ReadMap => 2,
            Self::Data => {
                if data_reused {
                    0
                } else {
                    25
                }
            }
            Self::Scene => 15,
            Self::Match => 5,
            Self::Art => 50,
            Self::Finish => 3,
        }
    }
}

fn percent_at(stage: Stage, fraction: f32, data_reused: bool) -> u8 {
    let total: u32 = Stage::ALL
        .iter()
        .map(|s| s.weight(data_reused))
        .sum::<u32>()
        .max(1);
    let before: u32 = Stage::ALL
        .iter()
        .take_while(|s| **s != stage)
        .map(|s| s.weight(data_reused))
        .sum();
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss, clippy::arithmetic_side_effects)]
    let within = (stage.weight(data_reused) as f32 * fraction.clamp(0.0, 1.0)) as u32;
    #[allow(clippy::arithmetic_side_effects)]
    let percent = (before + within) * 100 / total;
    u8::try_from(percent.min(99)).unwrap_or(99)
}

pub(crate) struct MatchLoader {
    vfs: Arc<Vfs>,
    install_roots: Vec<PathBuf>,
    sources: PassabilitySources,
    config: MatchConfig,
    map_label: String,
    viewport: (u32, u32),
    scale_mode: mint_sage_apt_viewer::ScaleMode,
    control: ControlSet,
    data_reused: bool,
    stage: Stage,
    map_bytes: Option<Vec<u8>>,
    data: Option<GameData>,
    ui: Option<UiResources>,
    scene: Option<MatchScene>,
    scene_step: Option<SceneStep>,
    scene_steps: u8,
    build: Option<Box<GameBuild>>,
    spent: [Duration; 6],
    started: Instant,
    reported: Option<u8>,
}

impl MatchLoader {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vfs: Arc<Vfs>,
        install_roots: Vec<PathBuf>,
        sources: PassabilitySources,
        config: MatchConfig,
        map_label: String,
        viewport: (u32, u32),
        scale_mode: mint_sage_apt_viewer::ScaleMode,
        control: ControlSet,
        data: Option<GameData>,
        ui: Option<UiResources>,
    ) -> Self {
        Self {
            vfs,
            install_roots,
            sources,
            config,
            map_label,
            viewport,
            scale_mode,
            control,
            data_reused: data.is_some(),
            stage: Stage::ReadMap,
            map_bytes: None,
            data,
            ui,
            scene: None,
            scene_step: None,
            scene_steps: 0,
            build: None,
            spent: [Duration::ZERO; 6],
            started: Instant::now(),
            reported: None,
        }
    }

    fn fraction(&self) -> f32 {
        match (&self.build, self.stage) {
            (Some(build), Stage::Art) => {
                let (done, total) = build.art_progress();
                #[allow(clippy::cast_precision_loss)]
                {
                    done as f32 / total.max(1) as f32
                }
            }
            (_, Stage::Scene) => f32::from(self.scene_steps) / f32::from(SCENE_STEPS),
            _ => 0.0,
        }
    }

    pub(crate) fn progress_change(&mut self) -> Option<(u8, &'static str)> {
        let percent = percent_at(self.stage, self.fraction(), self.data_reused);
        if self.reported.is_some_and(|last| last >= percent) {
            return None;
        }
        self.reported = Some(percent);
        Some((percent, self.stage.label()))
    }

    pub(crate) fn advance(
        &mut self,
        renderer: &mut Renderer,
        budget: Duration,
    ) -> Result<Option<Box<Game>>, String> {
        let begun = Instant::now();
        loop {
            let step_started = Instant::now();
            let stage = self.stage;
            let game = self.step(renderer)?;
            let index = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(0);
            if let Some(spent) = self.spent.get_mut(index) {
                *spent = spent.saturating_add(step_started.elapsed());
            }
            if let Some(mut game) = game {
                game.set_scale_mode(self.scale_mode);
                self.report_timings();
                crate::startup::mark("match built");
                return Ok(Some(game));
            }
            if begun.elapsed() >= budget {
                return Ok(None);
            }
        }
    }

    fn step(&mut self, renderer: &mut Renderer) -> Result<Option<Box<Game>>, String> {
        match self.stage {
            Stage::ReadMap => {
                let bytes = self
                    .vfs
                    .read(&self.map_label)
                    .map_err(|error| format!("reading {}: {error}", self.map_label))?;
                self.map_bytes = Some(bytes);
                self.stage = Stage::Data;
            }
            Stage::Data => {
                if self.data.is_none() {
                    let (data, line) = crate::load_game_data_or_exit(&self.vfs);
                    println!("{line}");
                    self.data = Some(data);
                }
                self.stage = Stage::Scene;
            }
            Stage::Scene => self.step_scene(renderer)?,
            Stage::Match => self.step_match(renderer)?,
            Stage::Art => {
                let Some(build) = self.build.as_mut() else {
                    return Err("load: unit art stage without a match".to_owned());
                };
                if build.art_done() {
                    self.stage = Stage::Finish;
                } else {
                    build.step_art(renderer);
                }
            }
            Stage::Finish => return self.step_finish(renderer).map(Some),
        }
        Ok(None)
    }

    fn step_scene(&mut self, renderer: &mut Renderer) -> Result<(), String> {
        self.scene_steps = self.scene_steps.saturating_add(1);
        let Some(next) = self.scene_step.take() else {
            let (Some(map_bytes), Some(data)) = (self.map_bytes.take(), self.data.as_mut()) else {
                return Err("load: scene stage without its map and data".to_owned());
            };
            crate::game_data::compose_for_match(data, &self.vfs, &self.map_label)?;
            let object_index = data.object_index.take();
            let (skirmish, allow_ring_heroes) = self.config.scene_filter();
            self.scene_step = Some(crate::scene::begin_match_scene(
                &self.vfs,
                &map_bytes,
                self.map_label.clone(),
                &data.templates,
                skirmish,
                allow_ring_heroes,
                object_index,
                Some(crate::scene::game_tuning_file()),
            )?);
            return Ok(());
        };
        match next(renderer, self.vfs.as_ref())? {
            SceneProgress::More(step) => self.scene_step = Some(step),
            SceneProgress::Done(scene) => {
                self.scene = Some(*scene);
                self.stage = Stage::Match;
            }
        }
        Ok(())
    }

    fn step_match(&mut self, renderer: &mut Renderer) -> Result<(), String> {
        let (Some(scene), Some(data)) = (self.scene.take(), self.data.take()) else {
            return Err("load: match stage without its scene and data".to_owned());
        };
        let build = GameBuild::begin(
            renderer,
            scene,
            data,
            self.viewport,
            Arc::clone(&self.vfs),
            self.sources,
            self.config.clone(),
            self.install_roots.clone(),
            self.control.clone(),
            self.ui.take(),
        )?;
        self.build = Some(build);
        self.stage = Stage::Art;
        Ok(())
    }

    fn step_finish(&mut self, renderer: &mut Renderer) -> Result<Box<Game>, String> {
        let Some(build) = self.build.take() else {
            return Err("load: finish stage without a match".to_owned());
        };
        println!("load: first-frame state hash {:#018x}", build.state_hash());
        let game = build.finish(renderer)?;
        if let Err(error) = renderer.save_pipeline_cache() {
            println!("load: pipeline cache not saved: {error}");
        }
        Ok(game)
    }

    fn report_timings(&self) {
        let ms = |stage: Stage| {
            let index = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(0);
            self.spent
                .get(index)
                .copied()
                .unwrap_or_default()
                .as_secs_f64()
                * 1000.0
        };
        println!(
            "load: map {:.0} ms, game data {:.0} ms ({}), scene {:.0} ms, match {:.0} ms, unit art {:.0} ms, finish {:.0} ms, {:.0} ms from the first step to the game",
            ms(Stage::ReadMap),
            ms(Stage::Data),
            if self.data_reused { "reused from the shell" } else { "parsed" },
            ms(Stage::Scene),
            ms(Stage::Match),
            ms(Stage::Art),
            ms(Stage::Finish),
            self.started.elapsed().as_secs_f64() * 1000.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_never_goes_back_and_stays_under_100_until_the_game_exists() {
        for reused in [false, true] {
            let mut last = 0;
            for stage in Stage::ALL {
                for step in 0..=10u8 {
                    let percent = percent_at(stage, f32::from(step) / 10.0, reused);
                    assert!(percent >= last, "{stage:?} {step}: {percent} < {last}");
                    assert!(percent <= 99);
                    last = percent;
                }
            }
        }
    }
}
