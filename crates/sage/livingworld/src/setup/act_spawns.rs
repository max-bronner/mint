use mint_sage_ini::living_world::campaign::{ActNugget, LivingWorldCampaignDef};
use mint_sage_ini::living_world::players::SpawnArmyDef;
use mint_sage_ini::living_world::Coord2;
use mint_sage_ini::living_world::LivingWorldData;

use crate::model::{PlayerId, RegionId, UnitId};
use crate::rules::army_roster;
use crate::state::{LivingWorldState, PalantirMarker};

const MAX_ACT_DEPTH: usize = 16;
const MAX_ACTS_BEGUN: usize = 256;

pub(super) type Registered = (usize, PlayerId);

pub(crate) struct ActRecords<'a> {
    records: Vec<(usize, &'a SpawnArmyDef)>,
    begun: Vec<usize>,
}

impl<'a> ActRecords<'a> {
    pub(crate) fn new(campaign: &'a LivingWorldCampaignDef) -> Self {
        let mut records = Vec::new();
        for (act, def) in campaign.acts.iter().enumerate() {
            for nugget in &def.nuggets {
                if let ActNugget::SpawnArmy(spawn) = nugget {
                    records.push((act, &**spawn));
                }
            }
        }
        let mut begun = Vec::new();
        if !campaign.acts.is_empty() {
            begin(campaign, 0, 0, &mut begun);
        }
        Self { records, begun }
    }

    pub(crate) fn palantir_markers(
        &self,
        campaign: &LivingWorldCampaignDef,
    ) -> Vec<PalantirMarker> {
        let mut markers: Vec<PalantirMarker> = Vec::new();
        for act in &self.begun {
            let Some(def) = campaign.acts.get(*act) else {
                continue;
            };
            for nugget in &def.nuggets {
                let ActNugget::ForceBattle(force) = nugget else {
                    continue;
                };
                let Some(movie) = force.movie.as_deref().filter(|m| !m.is_empty()) else {
                    continue;
                };
                let event = force.event_name.clone().unwrap_or_default();
                if force.remove_event == Some(true) {
                    markers.retain(|m| !m.event.eq_ignore_ascii_case(&event));
                } else if !markers.iter().any(|m| m.event.eq_ignore_ascii_case(&event)) {
                    let at = force.position.unwrap_or_default();
                    markers.push(PalantirMarker {
                        event,
                        movie: movie.to_owned(),
                        position: [at.x, at.y],
                        play_next_act_after_movie: force.play_next_act_after_movie == Some(true),
                    });
                }
            }
        }
        markers
    }

    pub(super) fn named(&self, name: &str, template: &str) -> Option<usize> {
        self.records.iter().position(|(_, def)| {
            def.scripting_name
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
                && super::listed(&def.spawn_for_templates.0, template)
        })
    }
}

fn begin(campaign: &LivingWorldCampaignDef, act: usize, depth: usize, begun: &mut Vec<usize>) {
    if depth > MAX_ACT_DEPTH || begun.len() >= MAX_ACTS_BEGUN {
        return;
    }
    begun.push(act);
    let Some(def) = campaign.acts.get(act) else {
        return;
    };
    for called in &def.call_act_subroutine.0 {
        if let Some(next) = campaign
            .acts
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(called))
        {
            begin(campaign, next, depth.saturating_add(1), begun);
        }
    }
}

fn has_position(def: &SpawnArmyDef) -> bool {
    def.position
        .is_some_and(|p| p.x.abs() > 0.0 || p.y.abs() > 0.0)
}

fn region_at_point(_point: Coord2) -> Option<RegionId> {
    None
}

impl LivingWorldState {
    fn spawn_record(
        &mut self,
        data: &LivingWorldData,
        def: &SpawnArmyDef,
        player: PlayerId,
        region: Option<RegionId>,
    ) {
        if def.is_city == Some(true) {
            return;
        }
        let initial = def
            .initial_region
            .as_deref()
            .and_then(|name| self.rules.region(name).ok());
        let Some(region) = initial.or(region) else {
            return;
        };
        let (hero, entries) = army_roster(data, def);
        let hero = hero.map(|h| self.rules.intern(&h));
        let roster: Vec<(UnitId, u32)> = entries
            .iter()
            .map(|(name, count)| (self.rules.intern(name), *count))
            .collect();
        let army = self.spawn_merging(player, region, hero, &roster);
        if let (Some(scripted), Some(name)) = (self.scripted.as_mut(), def.scripting_name.as_ref())
        {
            scripted.armies.push((name.clone(), army));
        }
    }

    fn template_name(&self, player: PlayerId) -> Option<String> {
        let template = self.player(player)?.template;
        self.rules
            .templates
            .get(usize::from(template.0))
            .map(|t| t.name.clone())
    }

    pub(super) fn spawn_named(
        &mut self,
        data: &LivingWorldData,
        records: &ActRecords<'_>,
        registered: &mut Vec<Registered>,
        player: PlayerId,
        region: RegionId,
        name: &str,
    ) {
        let Some(template) = self.template_name(player) else {
            return;
        };
        let Some(index) = records.named(name, &template) else {
            return;
        };
        registered.push((index, player));
        if let Some((_, def)) = records.records.get(index) {
            self.spawn_record(data, def, player, Some(region));
        }
    }

    pub(crate) fn spawn_act_armies(
        &mut self,
        data: &LivingWorldData,
        records: &ActRecords<'_>,
        registered: &[Registered],
        has_spawn_point: &dyn Fn(RegionId) -> bool,
    ) {
        for &act in &records.begun {
            for (index, (owner, def)) in records.records.iter().enumerate() {
                if *owner != act || def.spawn_at_act_start == Some(false) {
                    continue;
                }
                for template in &def.spawn_for_templates.0 {
                    let players: Vec<(PlayerId, Option<RegionId>)> = self
                        .players
                        .iter()
                        .filter(|p| {
                            self.rules
                                .templates
                                .get(usize::from(p.template.0))
                                .is_some_and(|t| t.name.eq_ignore_ascii_case(template))
                        })
                        .map(|p| (p.id, p.base_region))
                        .collect();
                    for (player, base) in players {
                        if registered.contains(&(index, player)) {
                            continue;
                        }
                        let region = match def.position.filter(|_| has_position(def)) {
                            Some(point) => region_at_point(point),
                            None => {
                                let Some(base) = base else { continue };
                                if !has_spawn_point(base) {
                                    return;
                                }
                                Some(base)
                            }
                        };
                        self.spawn_record(data, def, player, region);
                    }
                }
            }
        }
    }
}
