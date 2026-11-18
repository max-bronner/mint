use mint_sage_ini::living_world::players::{BuildingNuggetKind, CommandPointKind};
use mint_sage_ini::living_world::LivingWorldData;

use crate::model::{BuildingId, UnitId};
use crate::rules::Rules;

pub const TYPES: usize = 10;
pub const HERO: usize = 5;
pub const RATIOS: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTemplate {
    pub name: String,
    pub ratios: [i32; RATIOS],
    pub armory: i32,
    pub barracks: i32,
    pub castle: i32,
    pub farm: i32,
    pub preference: [i32; 7],
}

impl Default for AiTemplate {
    fn default() -> Self {
        Self {
            name: String::new(),
            ratios: [0; RATIOS],
            armory: 0,
            barracks: 0,
            castle: 0,
            farm: 0,
            preference: [10; 7],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryMember {
    pub unit: UnitId,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnEntry {
    pub members: Vec<EntryMember>,
    pub hero: Option<UnitId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Main {
    WorldCp,
    Strengthen,
    Upgrade,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildingAi {
    pub main: Option<Main>,
    pub spawn: Option<Vec<SpawnEntry>>,
    pub world_cp: i32,
    pub treasury: Option<i32>,
    pub strengthen_level_one: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiRules {
    pub templates: Vec<AiTemplate>,
    pub default_template: Option<usize>,
    pub buildings: Vec<BuildingAi>,
}

impl AiRules {
    #[must_use]
    pub fn template(&self, name: Option<&str>) -> AiTemplate {
        name.and_then(|n| {
            self.templates
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(n))
        })
        .or_else(|| self.default_template.and_then(|i| self.templates.get(i)))
        .cloned()
        .unwrap_or_default()
    }

    #[must_use]
    pub fn building(&self, id: BuildingId) -> Option<&BuildingAi> {
        self.buildings.get(usize::from(id.0))
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn build(data: &LivingWorldData, rules: &mut Rules) -> AiRules {
    let templates: Vec<AiTemplate> = data
        .ai_templates
        .iter()
        .map(|t| {
            let ratio = |v: Option<i32>| v.unwrap_or(0);
            let score = |v: Option<i32>| v.unwrap_or(0);
            let pref = |v: Option<i32>| v.unwrap_or(10);
            AiTemplate {
                name: t.name.clone(),
                ratios: [
                    ratio(t.desired_soldier_ratio),
                    ratio(t.desired_archer_ratio),
                    ratio(t.desired_pikemen_ratio),
                    ratio(t.desired_cavalry_ratio),
                    ratio(t.desired_monster_ratio),
                    ratio(t.desired_hero_ratio),
                    ratio(t.desired_fortress_ratio),
                    ratio(t.desired_siege_ratio),
                    ratio(t.desired_support_ratio),
                ],
                armory: score(t.building_score_armory),
                barracks: score(t.building_score_barracks),
                castle: score(t.building_score_castle),
                farm: score(t.building_score_farm),
                preference: [
                    pref(t.bonus_preference_resource),
                    pref(t.bonus_preference_army),
                    pref(t.bonus_preference_legendary),
                    pref(t.bonus_preference_attack),
                    pref(t.bonus_preference_defense),
                    pref(t.bonus_preference_experience),
                    pref(t.bonus_preference_treasury),
                ],
            }
        })
        .collect();
    let default_template = templates
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case("DefaultAITemplate"));

    let mut buildings = Vec::new();
    for b in data.buildings.iter() {
        let mut main = None;
        let mut spawn: Option<Vec<SpawnEntry>> = None;
        let (mut world_cp, mut treasury, mut level_one) = (0, None, 0.0f32);
        let mut strengthen = false;
        let mut upgrade = false;
        let mut is_world = false;
        for nugget in &b.nuggets {
            match &nugget.kind {
                BuildingNuggetKind::IncreaseCommandPoints(c)
                    if c.kind == Some(CommandPointKind::World) =>
                {
                    is_world = true;
                    world_cp = c.amount.unwrap_or(0);
                }
                BuildingNuggetKind::StrengthenArmy(s) => {
                    strengthen = true;
                    level_one = s
                        .bonuses
                        .iter()
                        .find(|x| x.level == 1)
                        .map_or(0.0, |x| (x.weapon - 1.0) * 100.0);
                }
                BuildingNuggetKind::UpgradeTroops(_) => upgrade = true,
                BuildingNuggetKind::IncreaseTreasury(t) if treasury.is_none() => {
                    treasury = Some(t.treasure_amount.unwrap_or(0));
                }
                BuildingNuggetKind::SpawnArmy(s) => {
                    let list = spawn.get_or_insert_with(Vec::new);
                    for army in &s.armies {
                        let members = army
                            .player_army
                            .as_deref()
                            .and_then(|n| data.armies.get(n))
                            .map(|def| {
                                def.entries
                                    .iter()
                                    .filter_map(|e| {
                                        let unit = rules.intern(e.thing_template.as_deref()?);
                                        let count =
                                            u32::try_from(e.quantity.unwrap_or(1)).unwrap_or(1);
                                        Some(EntryMember { unit, count })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let hero = army
                            .hero_template_name
                            .as_deref()
                            .filter(|n| !n.is_empty())
                            .map(|n| rules.intern(n));
                        list.push(SpawnEntry { members, hero });
                    }
                }
                _ => {}
            }
        }
        if is_world {
            main = Some(Main::WorldCp);
        } else if strengthen {
            main = Some(Main::Strengthen);
        } else if upgrade {
            main = Some(Main::Upgrade);
        }
        buildings.push(BuildingAi {
            main,
            spawn,
            world_cp,
            treasury,
            strengthen_level_one: level_one,
        });
    }
    AiRules {
        templates,
        default_template,
        buildings,
    }
}
