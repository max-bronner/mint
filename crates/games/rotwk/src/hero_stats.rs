use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use mint_sage_ini::ThingStatDef;
use mint_sim::{Frame, PlayerId, Simulation};

use crate::cah_edit::CahSystem;
use crate::game_mode::GameMode;
use crate::heroes::HeroState;
use crate::kill_credit::KillCredit;
use crate::rules::RotwkRules;
use crate::templates::KindTable;

fn stat_counts(stat: &ThingStatDef, template: &str, has_kind: impl Fn(&str) -> bool) -> bool {
    let named = |list: &[String]| list.iter().any(|name| name.eq_ignore_ascii_case(template));
    if stat.excluded_kind_of.iter().any(|kind| has_kind(kind)) || named(&stat.excluded_things) {
        return false;
    }
    stat.kind_of.iter().any(|kind| has_kind(kind)) || named(&stat.thing_template_names)
}

fn end_keyword(mode: GameMode, internet_unranked: bool, won: bool) -> Option<&'static str> {
    match (mode, internet_unranked, won) {
        (GameMode::Skirmish, _, true) => Some("SkirmishVictoryStatName"),
        (GameMode::Skirmish, _, false) => Some("SkirmishDefeatStatName"),
        (GameMode::Internet, true, true) => Some("OpenPlayVictoryStatName"),
        (GameMode::Internet, true, false) => Some("OpenPlayDefeatStatName"),
        _ => None,
    }
}

pub(crate) struct HeroMatchStats {
    sys: Arc<CahSystem>,
    kinds: Arc<KindTable>,
    dir: PathBuf,
    player: PlayerId,
    id: String,
    class: usize,
    subclass: usize,
    counters: BTreeMap<String, i32>,
    earned: Vec<String>,
    recruited: bool,
    alive: bool,
    mode: GameMode,
    dirty: bool,
}

impl HeroMatchStats {
    pub(crate) fn open(
        sys: Arc<CahSystem>,
        kinds: Arc<KindTable>,
        rules: &RotwkRules,
        player: PlayerId,
        mode: GameMode,
        dir: PathBuf,
    ) -> Option<Self> {
        let record = &rules.cah_plan_of(player)?.record;
        let id = record.id_str().filter(|id| !id.is_empty())?;
        let class = usize::try_from(record.class_index).ok()?;
        let subclass = usize::try_from(record.subclass_index).ok()?;
        let counters = mint_sage_asset::read_stats(&dir, &id);
        let earned = sys.earned_awards(class, subclass, &counters);
        Some(Self {
            sys,
            kinds,
            dir,
            player,
            id,
            class,
            subclass,
            counters,
            earned,
            recruited: false,
            alive: false,
            mode,
            dirty: false,
        })
    }

    pub(crate) fn after_tick(&mut self, simulation: &Simulation<RotwkRules>, frame: Frame) {
        let rules = simulation.rules();
        let world = simulation.state();
        let Some(ids) = rules.ids() else {
            return;
        };
        let alive = rules
            .roster_of(world, ids, self.player)
            .is_some_and(|roster| {
                roster.0.iter().any(|slot| {
                    matches!(slot.state, HeroState::Alive(entity)
                    if rules.cah_bound_player(world, ids, entity) == Some(self.player))
                })
            });
        if alive {
            self.recruited = true;
        } else if self.alive {
            self.save();
        }
        self.alive = alive;
        let kills: Vec<KillCredit> = rules
            .kill_credits(world, frame)
            .iter()
            .filter(|credit| rules.cah_bound_player(world, ids, credit.killer) == Some(self.player))
            .copied()
            .collect();
        for credit in kills {
            self.count_kill(credit);
        }
    }

    fn count_kill(&mut self, credit: KillCredit) {
        let Some(entry) = self.kinds.get(credit.victim) else {
            return;
        };
        let Some(sub) = self.sys.subclass(self.class, self.subclass) else {
            return;
        };
        let template = entry.record.name.as_str();
        let mut changed = false;
        for name in &sub.stats {
            let Some(stat) = self.sys.thing_stat(name) else {
                continue;
            };
            let Some(stat_name) = stat.stat_name.as_deref() else {
                continue;
            };
            if stat_counts(stat, template, |kind| entry.record.has_kind(kind)) {
                let count = self.counters.entry(stat_name.to_owned()).or_insert(0);
                *count = count.saturating_add(1);
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
            self.recheck_awards();
        }
    }

    fn add_stat(&mut self, name: &str, amount: i32) {
        if self.sys.thing_stat(name).is_none() {
            return;
        }
        let count = self.counters.entry(name.to_owned()).or_insert(0);
        *count = count.saturating_add(amount);
        self.dirty = true;
        self.recheck_awards();
    }

    fn recheck_awards(&mut self) {
        for name in self
            .sys
            .earned_awards(self.class, self.subclass, &self.counters)
        {
            if !self.earned.contains(&name) {
                println!("hero: {} earned award {name}", self.id);
                self.earned.push(name);
            }
        }
    }

    pub(crate) fn finish(&mut self, won: bool, internet_unranked: bool) {
        if !self.recruited {
            return;
        }
        let name = end_keyword(self.mode, internet_unranked, won).and_then(|keyword| {
            self.sys
                .def()
                .stat_names
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(keyword))
                .map(|(_, stat)| stat.clone())
        });
        if let Some(name) = name {
            self.add_stat(&name, 1);
        }
        self.save();
    }

    pub(crate) fn save(&mut self) {
        if !self.dirty {
            return;
        }
        match mint_sage_asset::write_stats(&self.dir, &self.id, &self.counters) {
            Ok(()) => {
                self.dirty = false;
                println!(
                    "hero: stats for {} saved ({} counter(s))",
                    self.id,
                    self.counters.len()
                );
            }
            Err(error) => println!("hero: stats for {} not saved: {error}", self.id),
        }
    }
}

impl Drop for HeroMatchStats {
    fn drop(&mut self) {
        self.save();
    }
}

#[cfg(test)]
mod tests;
