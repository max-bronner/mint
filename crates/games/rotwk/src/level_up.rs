use std::collections::HashMap;

use mint_render::{Renderer, Vec3};
use mint_sim::Entity;

use crate::audio::GameAudio;
use crate::game::{fire_at_drawables, report_second, MatchState};
use crate::render_state::UnitView;
use crate::report::SecondReport;

#[derive(Debug, Default)]
pub(crate) struct LevelWatch {
    seen: HashMap<Entity, usize>,
    next: HashMap<Entity, usize>,
    pending: Vec<(u32, String)>,
    tints: Vec<(u32, mint_sage_modules::LevelUpTint)>,
    pub(crate) rises: u64,
    pub(crate) fired: u64,
    pub(crate) systems: u64,
    pub(crate) unknown: u64,
    pub(crate) tinted: u64,
}

impl LevelWatch {
    pub(crate) fn observe(
        &mut self,
        current: impl IntoIterator<Item = (Entity, usize)>,
    ) -> Vec<(Entity, usize)> {
        let mut rose = Vec::new();
        self.next.clear();
        for (entity, level) in current {
            if self.seen.get(&entity).is_some_and(|old| level > *old) {
                rose.push((entity, level));
            }
            self.next.insert(entity, level);
        }
        std::mem::swap(&mut self.seen, &mut self.next);
        self.rises = self
            .rises
            .saturating_add(u64::try_from(rose.len()).unwrap_or(u64::MAX));
        rose
    }

    pub(crate) fn queue(&mut self, id: u32, list: &str) {
        self.pending.push((id, list.to_owned()));
    }

    pub(crate) fn take(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending)
    }

    pub(crate) fn queue_tint(&mut self, id: u32, tint: mint_sage_modules::LevelUpTint) {
        self.tints.push((id, tint));
    }

    pub(crate) fn take_tints(&mut self) -> Vec<(u32, mint_sage_modules::LevelUpTint)> {
        std::mem::take(&mut self.tints)
    }
}

pub(crate) fn observe(state: &mut MatchState, audio: &mut GameAudio, units: &[UnitView]) {
    let gate = crate::game::FxShroud::of(state);
    let simulation = &state.simulation;
    let rules = simulation.rules();
    let Some(ids) = rules.ids() else {
        return;
    };
    let world = simulation.state();
    let rose = state.level_ups.observe(units.iter().filter_map(|unit| {
        rules
            .experience_of(world, ids, unit.entity)
            .map(|(level, _)| (unit.entity, level))
    }));
    for (entity, level) in &rose {
        if let Some(tint) = rules
            .level_of(world, ids, *entity, *level)
            .and_then(|level| level.level_up_tint)
        {
            state.level_ups.queue_tint(entity.index(), tint);
        }
    }
    if state.fx.shown() == crate::fx_present::FxShown::Off {
        return;
    }
    for (entity, level) in rose {
        let Some(list) = rules
            .level_of(world, ids, entity, level)
            .and_then(|level| level.level_up_fx.as_deref())
        else {
            continue;
        };
        if let Some(unit) = units.iter().find(|unit| unit.entity == entity) {
            let at = Vec3::new(unit.position.x, unit.position.y, unit.ground);
            let particles = &state.scene.particle_scene;
            if let Some(fx_list) = particles
                .fx_list(list)
                .filter(|_| !gate.skips(simulation, particles, list, Some(entity), at))
            {
                let weather = if state.scene.snowy {
                    mint_sage_ini::NuggetWeather::Snowy
                } else {
                    mint_sage_ini::NuggetWeather::Normal
                };
                let primary = rules.fx_facts(world, entity, crate::game::LOCAL_PLAYER);
                let primary = primary
                    .as_ref()
                    .map(crate::fx_present::ObjectFacts::as_facts);
                let kinds = &state.kinds;
                let equivalent = |a: &str, b: &str| {
                    kinds
                        .kind(a)
                        .zip(kinds.kind(b))
                        .is_some_and(|(a, b)| kinds.equivalent(a, b))
                };
                let mask = crate::fx_present::admit(
                    &fx_list.nuggets,
                    primary.as_ref(),
                    None,
                    weather,
                    &equivalent,
                );
                for (index, nugget) in fx_list.nuggets.iter().enumerate() {
                    if !mask.admits(index) {
                        continue;
                    }
                    if nugget.keyword.eq_ignore_ascii_case("Sound") {
                        if let Some(sound) = nugget.name.as_deref() {
                            audio.play(sound, Some(at), None);
                        }
                    } else if nugget.eva_event.is_some() {
                        audio.report_fx_eva_event(
                            rules,
                            world,
                            &state.scene.particle_scene,
                            list,
                            true,
                            entity,
                            crate::game::LOCAL_PLAYER,
                            simulation.frame(),
                            [at.x, at.y, at.z],
                        );
                    }
                }
            }
        }
        state.level_ups.queue(entity.index(), list);
    }
}

pub(crate) fn play_fx(
    renderer: &mut Renderer,
    state: &mut MatchState,
    seconds: f32,
    report: &mut SecondReport,
) {
    let events = state.level_ups.take();
    if !events.is_empty() && state.fx.shown() != crate::fx_present::FxShown::Off {
        let (fired, systems, unknown) = fire_at_drawables(renderer, state, events);
        let watch = &mut state.level_ups;
        watch.fired = watch.fired.saturating_add(fired);
        watch.systems = watch.systems.saturating_add(systems);
        watch.unknown = watch.unknown.saturating_add(unknown);
    }
    let watch = &state.level_ups;
    if report_second(seconds) && watch.rises > 0 {
        report.line(format!(
            "level up: {} rise(s) seen, {} LevelUpFx list(s) played ({} particle system(s) \
             started), {} named no FXList [{}]",
            watch.rises,
            watch.fired,
            watch.systems,
            watch.unknown,
            state.fx.shown().label()
        ));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn entity(index: u32) -> Entity {
        let mut world = mint_sim::World::new();
        let mut last = world.spawn().unwrap();
        for _ in 0..index {
            last = world.spawn().unwrap();
        }
        last
    }

    #[test]
    fn only_a_rise_of_an_object_already_watched_is_reported() {
        let (a, b) = (entity(0), entity(1));
        let mut watch = LevelWatch::default();
        assert!(
            watch.observe([(a, 1)]).is_empty(),
            "first sight is a baseline"
        );
        assert_eq!(watch.observe([(a, 2), (b, 3)]), vec![(a, 2)]);
        assert!(watch.observe([(a, 2), (b, 3)]).is_empty(), "no change");
        assert!(
            watch.observe([(a, 1), (b, 3)]).is_empty(),
            "a fall is silent"
        );
        assert!(watch.observe([(b, 3)]).is_empty());
        assert!(watch.observe([(a, 4), (b, 3)]).is_empty(), "back in view");
        assert_eq!(watch.rises, 1);
    }
}
