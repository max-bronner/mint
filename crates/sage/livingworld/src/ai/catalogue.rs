use std::collections::BTreeMap;

use crate::ai::data::{Main, TYPES};
use crate::model::{BuildingId, TemplateId};
use crate::rules::Rules;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Roles {
    pub farm: Option<BuildingId>,
    pub fortress: Option<BuildingId>,
    pub armory: Option<BuildingId>,
    pub barracks: Option<BuildingId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawner {
    pub by_type: Vec<Vec<usize>>,
    pub counters: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalogue {
    pub roles: Roles,
    pub spawners: BTreeMap<BuildingId, Spawner>,
}

impl Catalogue {
    #[must_use]
    pub fn build(rules: &Rules, template: TemplateId) -> Self {
        let mut out = Self::default();
        for (index, b) in rules.buildings.iter().enumerate() {
            if !b.available_to.is_empty() && !b.available_to.contains(&template) {
                continue;
            }
            let Some(id) = u16::try_from(index).ok().map(BuildingId) else {
                break;
            };
            let Some(ai) = rules.ai.building(id) else {
                continue;
            };
            match ai.main {
                Some(Main::WorldCp) => out.roles.farm = Some(id),
                Some(Main::Strengthen) => out.roles.fortress = Some(id),
                Some(Main::Upgrade) => out.roles.armory = Some(id),
                None if ai.spawn.is_some() => out.roles.barracks = Some(id),
                None => {}
            }
            let Some(entries) = &ai.spawn else { continue };
            let mut spawner = Spawner {
                by_type: vec![Vec::new(); TYPES],
                counters: vec![Vec::new(); TYPES],
            };
            for (e, entry) in entries.iter().enumerate() {
                for member in &entry.members {
                    let info = rules.unit(member.unit);
                    let t = info.unit_type.map_or(TYPES - 1, usize::from);
                    if let Some(list) = spawner.by_type.get_mut(t) {
                        list.push(e);
                    }
                    if let Some(list) = info
                        .counter_target
                        .and_then(|c| spawner.counters.get_mut(usize::from(c)))
                    {
                        list.push(e);
                    }
                }
            }
            out.spawners.insert(id, spawner);
        }
        out
    }
}
