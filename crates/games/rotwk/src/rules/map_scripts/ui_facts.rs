use std::collections::{BTreeMap, BTreeSet};

use mint_sim::{PlayerId, StateHasher};

use crate::rules::command::UiPanel;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct UiFacts {
    open: BTreeSet<(PlayerId, UiPanel)>,
    scroll: BTreeMap<PlayerId, f32>,
    formation_order: BTreeSet<PlayerId>,
    spell_store_disabled: bool,
    objectives_disabled: bool,
    planning: Option<bool>,
    objectives_closes: u32,
}

impl UiFacts {
    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        if *self == Self::default() {
            return;
        }
        hasher.write_usize(self.open.len());
        for (player, panel) in &self.open {
            hasher.write_u8(player.get());
            hasher.write_u8(panel.code());
        }
        hasher.write_usize(self.scroll.len());
        for (player, distance) in &self.scroll {
            hasher.write_u8(player.get());
            hasher.write_f32(*distance);
        }
        hasher.write_usize(self.formation_order.len());
        for player in &self.formation_order {
            hasher.write_u8(player.get());
        }
        hasher.write_bool(self.spell_store_disabled);
        hasher.write_bool(self.objectives_disabled);
        hasher.write_u8(match self.planning {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        hasher.write_u32(self.objectives_closes);
    }

    fn disabled(&self, panel: UiPanel) -> bool {
        match panel {
            UiPanel::SpellStore => self.spell_store_disabled,
            UiPanel::Objectives => self.objectives_disabled,
            UiPanel::StanceSubMenu => false,
        }
    }

    pub(crate) fn set_panel(&mut self, player: PlayerId, panel: UiPanel, open: bool) {
        if open && !self.disabled(panel) {
            self.open.insert((player, panel));
        } else {
            self.open.remove(&(player, panel));
        }
    }

    pub(crate) fn panel_open(&self, player: PlayerId, panel: UiPanel) -> bool {
        self.open.contains(&(player, panel))
    }

    pub(crate) fn scrolled(&mut self, player: PlayerId, distance: f32) {
        if distance.is_finite() && distance > 0.0 {
            let total = self.scroll.entry(player).or_insert(0.0);
            *total += distance;
        }
    }

    pub(crate) fn camera_reset(&mut self, player: PlayerId) {
        self.scroll.remove(&player);
    }

    pub(crate) fn scroll_distance(&self, player: PlayerId) -> f32 {
        self.scroll.get(&player).copied().unwrap_or(0.0)
    }

    pub(crate) fn note_formation_order(&mut self, player: PlayerId) {
        self.formation_order.insert(player);
    }

    pub(crate) fn formation_order_issued(&self, player: PlayerId) -> bool {
        self.formation_order.contains(&player)
    }

    fn close_everywhere(&mut self, panel: UiPanel) {
        self.open.retain(|(_, open)| *open != panel);
    }

    pub(crate) fn set_spell_store_enabled(&mut self, enabled: bool) {
        self.spell_store_disabled = !enabled;
        if !enabled {
            self.close_everywhere(UiPanel::SpellStore);
        }
    }

    pub(crate) fn set_objectives_enabled(&mut self, enabled: bool) {
        if !enabled && !self.objectives_disabled {
            self.objectives_closes = self.objectives_closes.wrapping_add(1);
        }
        self.objectives_disabled = !enabled;
        if !enabled {
            self.close_everywhere(UiPanel::Objectives);
        }
    }

    pub(crate) fn close_objectives(&mut self) {
        self.objectives_closes = self.objectives_closes.wrapping_add(1);
        self.close_everywhere(UiPanel::Objectives);
    }

    pub(crate) fn set_planning_allowed(&mut self, allowed: bool) {
        self.planning = Some(allowed);
    }

    pub(crate) fn planning_mode(&self) -> Option<bool> {
        self.planning
    }

    pub(crate) fn view(&self) -> UiView {
        UiView {
            spell_store_disabled: self.spell_store_disabled,
            objectives_disabled: self.objectives_disabled,
            objectives_closes: self.objectives_closes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct UiView {
    pub(crate) spell_store_disabled: bool,
    pub(crate) objectives_disabled: bool,
    pub(crate) objectives_closes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: PlayerId = PlayerId::new(0);

    fn hash(facts: &UiFacts) -> u64 {
        let mut hasher = StateHasher::new();
        hasher.write_u8(1);
        facts.hash_into(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn disabling_a_panel_closes_it_everywhere_and_the_switches_are_hashed() {
        let fresh = UiFacts::default();
        let mut facts = UiFacts::default();
        facts.set_panel(ME, UiPanel::SpellStore, true);
        facts.set_spell_store_enabled(false);
        assert!(!facts.panel_open(ME, UiPanel::SpellStore));
        assert_ne!(hash(&facts), hash(&fresh));
        facts.set_spell_store_enabled(true);
        assert_eq!(hash(&facts), hash(&fresh));
        facts.set_planning_allowed(false);
        assert_eq!(facts.planning_mode(), Some(false));
        assert_ne!(hash(&facts), hash(&fresh));
    }
}
