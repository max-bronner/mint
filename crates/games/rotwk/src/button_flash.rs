use std::collections::{BTreeMap, BTreeSet};

use mint_sim::Entity;

use crate::hud::FieldedHero;
use crate::rules::map_scripts::UiFlashTarget;

const FRAMES_PER_SECOND: u64 = 30;

const FRAMES_PER_CAMEO_STEP: u64 = 10;

#[derive(Debug, Default)]
pub(crate) struct ButtonFlashes {
    cameo: BTreeMap<String, u64>,
    heroes: BTreeMap<Entity, u64>,
    builder: Option<u64>,
    player_magic: Option<u64>,
    objectives: Option<u64>,
    stepped_to: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FlashView {
    cameo: Option<BTreeSet<String>>,
    heroes: Option<BTreeSet<Entity>>,
    pub(crate) builder: Option<bool>,
    pub(crate) player_magic: Option<bool>,
    pub(crate) objectives: bool,
}

impl FlashView {
    pub(crate) fn cameo(&self, name: &str) -> Option<bool> {
        self.cameo
            .as_ref()
            .map(|on| on.contains(&name.to_ascii_lowercase()))
    }

    pub(crate) fn hero(&self, entity: Option<Entity>) -> Option<bool> {
        self.heroes
            .as_ref()
            .map(|on| entity.is_some_and(|entity| on.contains(&entity)))
    }
}

fn after_seconds(frame: u64, seconds: i32) -> u64 {
    let frames = u64::try_from(seconds)
        .unwrap_or(0)
        .saturating_mul(FRAMES_PER_SECOND);
    frame.saturating_add(frames)
}

impl ButtonFlashes {
    pub(crate) fn raise(
        &mut self,
        target: &UiFlashTarget,
        amount: i32,
        frame: u64,
        heroes: &[FieldedHero],
    ) {
        match target {
            UiFlashTarget::Cameo(button) => {
                let steps = u64::try_from(amount).unwrap_or(0);
                self.cameo.insert(button.to_ascii_lowercase(), steps);
            }
            UiFlashTarget::HeroSelect(template) => {
                let until = frame.saturating_add(u64::try_from(amount).unwrap_or(0));
                for hero in heroes
                    .iter()
                    .filter(|hero| hero.template.eq_ignore_ascii_case(template))
                {
                    let held = self.heroes.entry(hero.entity).or_insert(0);
                    *held = (*held).max(until);
                }
            }
            UiFlashTarget::SelectBuilder => {
                self.builder = Some(frame.saturating_add(u64::try_from(amount).unwrap_or(0)));
            }
            UiFlashTarget::SpellStore => {
                let until = after_seconds(frame, amount);
                self.player_magic = Some(self.player_magic.unwrap_or(0).max(until));
            }
            UiFlashTarget::Objectives => {
                let until = after_seconds(frame, amount);
                self.objectives = Some(self.objectives.unwrap_or(0).max(until));
            }
        }
    }

    pub(crate) fn advance<'a>(&mut self, frame: u64, shown: impl IntoIterator<Item = &'a str>) {
        let from = self.stepped_to.replace(frame).unwrap_or(frame);
        if self.cameo.is_empty() || frame <= from {
            return;
        }
        let steps = frame / FRAMES_PER_CAMEO_STEP - from / FRAMES_PER_CAMEO_STEP;
        let shown: BTreeSet<String> = shown.into_iter().map(str::to_ascii_lowercase).collect();
        for (button, left) in &mut self.cameo {
            if shown.contains(button) {
                *left = left.saturating_sub(steps);
            }
        }
    }

    pub(crate) fn pressed(&mut self, button: &str) {
        if let Some(left) = self.cameo.get_mut(&button.to_ascii_lowercase()) {
            *left = 0;
        }
    }

    pub(crate) fn objectives_on(&self, frame: u64) -> bool {
        self.objectives.is_some_and(|until| frame < until)
    }

    pub(crate) fn view(&self, frame: u64) -> FlashView {
        let running = |until: u64| frame < until;
        FlashView {
            cameo: (!self.cameo.is_empty()).then(|| {
                self.cameo
                    .iter()
                    .filter(|(_, left)| **left > 0)
                    .map(|(button, _)| button.clone())
                    .collect()
            }),
            heroes: (!self.heroes.is_empty()).then(|| {
                self.heroes
                    .iter()
                    .filter(|(_, until)| running(**until))
                    .map(|(entity, _)| *entity)
                    .collect()
            }),
            builder: self.builder.map(running),
            player_magic: self.player_magic.map(running),
            objectives: self.objectives_on(frame),
        }
    }
}

#[cfg(test)]
mod tests;
