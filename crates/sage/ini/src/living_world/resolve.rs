use std::collections::BTreeSet;

use super::campaign::{ActNugget, Conditions, LivingWorldCampaignDef};
use super::data::LivingWorldData;
use super::players::{BuildingNuggetKind, SpawnArmyDef};
use super::region::RegionCampaignDef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub block: &'static str,
    pub name: String,
    pub field: &'static str,
    pub target: &'static str,
    pub missing: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ExternalNames<'a> {
    pub objects: &'a BTreeSet<String>,
    pub player_templates: &'a BTreeSet<String>,
    pub maps: Option<&'a BTreeSet<String>>,
}

struct Check<'a> {
    found: Vec<Unresolved>,
    block: &'static str,
    name: &'a str,
}

impl<'a> Check<'a> {
    fn names<'n>(
        &mut self,
        field: &'static str,
        target: &'static str,
        names: impl IntoIterator<Item = &'n str>,
        exists: impl Fn(&str) -> bool,
    ) {
        for missing in names {
            if !exists(missing) {
                self.found.push(Unresolved {
                    block: self.block,
                    name: self.name.to_owned(),
                    field,
                    target,
                    missing: missing.to_owned(),
                });
            }
        }
    }

    fn one(
        &mut self,
        field: &'static str,
        target: &'static str,
        name: Option<&String>,
        exists: impl Fn(&str) -> bool,
    ) {
        self.names(field, target, name.map(String::as_str), exists);
    }

    fn many(
        &mut self,
        field: &'static str,
        target: &'static str,
        names: &[String],
        exists: impl Fn(&str) -> bool,
    ) {
        self.names(field, target, names.iter().map(String::as_str), exists);
    }
}

fn lower_in(set: &BTreeSet<String>) -> impl Fn(&str) -> bool + '_ {
    move |name| set.contains(&name.to_ascii_lowercase())
}

#[must_use]
pub fn resolve(data: &LivingWorldData, external: &ExternalNames) -> Vec<Unresolved> {
    let object = lower_in(external.objects);
    let faction = lower_in(external.player_templates);
    let lw_player = |name: &str| data.player_templates.contains(name);
    let army = |name: &str| data.armies.contains(name);
    let army_icon = |name: &str| data.army_icons.contains(name);
    let mut found = Vec::new();

    for campaign in data.region_campaigns.iter() {
        let mut check = Check {
            found: Vec::new(),
            block: "LivingWorldRegionCampaign",
            name: &campaign.name,
        };
        check.one(
            "RegionEffectsManagerName",
            "LivingWorldRegionEffects",
            campaign.region_effects_manager_name.as_ref(),
            |n| data.region_effects.contains(n),
        );
        let region = |name: &str| campaign.region(name).is_some();
        for r in &campaign.regions {
            let mut check = Check {
                found: Vec::new(),
                block: "Region",
                name: &r.name,
            };
            check.names("Connection", "Region", r.connected(), region);
            if let Some(maps) = external.maps {
                check.one("MapName", "map", r.map_name.as_ref(), lower_in(maps));
            }
            found.append(&mut check.found);
        }
        for bonus in &campaign.concurrent_bonuses {
            let mut inner = Check {
                found: Vec::new(),
                block: "ConcurrentRegionBonus",
                name: &bonus.name,
            };
            inner.many("Regions", "Region", &bonus.regions.0, region);
            found.append(&mut inner.found);
        }
        found.append(&mut check.found);
    }

    for template in data.player_templates.iter() {
        let mut check = Check {
            found: Vec::new(),
            block: "LivingWorldPlayerTemplate",
            name: &template.name,
        };
        check.one(
            "Faction",
            "PlayerTemplate",
            template.faction.as_ref(),
            &faction,
        );
        check.one(
            "FactionDozerTemplateName",
            "Object",
            template.faction_dozer_template_name.as_ref(),
            &object,
        );
        check.one(
            "FactionInnUnitTemplateName",
            "Object",
            template.faction_inn_unit_template_name.as_ref(),
            &object,
        );
        check.one(
            "DefaultArmyIconName",
            "LivingWorldArmyIcon",
            template.default_army_icon_name.as_ref(),
            army_icon,
        );
        check.one(
            "BuildPlotIconName",
            "LivingWorldBuildPlotIcon",
            template.build_plot_icon_name.as_ref(),
            |n| data.build_plot_icons.contains(n),
        );
        found.append(&mut check.found);
    }

    for a in data.armies.iter() {
        let mut check = Check {
            found: Vec::new(),
            block: "LivingWorldPlayerArmy",
            name: a.army_name.as_deref().unwrap_or_default(),
        };
        for entry in &a.entries {
            check.one(
                "ArmyEntry/ThingTemplate",
                "Object",
                entry.thing_template.as_ref(),
                &object,
            );
        }
        found.append(&mut check.found);
    }

    for b in data.buildings.iter() {
        let mut check = Check {
            found: Vec::new(),
            block: "LivingWorldBuilding",
            name: &b.name,
        };
        check.many(
            "AvailableTo",
            "LivingWorldPlayerTemplate",
            &b.available_to.0,
            lw_player,
        );
        check.one(
            "BattleThingTemplate",
            "Object",
            b.battle_thing_template.as_ref(),
            &object,
        );
        check.one(
            "BuildingIcon",
            "LivingWorldBuildingIcon",
            b.building_icon.as_ref(),
            |n| data.building_icons.contains(n),
        );
        for nugget in &b.nuggets {
            match &nugget.kind {
                BuildingNuggetKind::SpawnArmy(spawn) => {
                    for to_spawn in &spawn.armies {
                        spawn_army_refs(&mut check, to_spawn, &army, &object, &army_icon);
                    }
                }
                BuildingNuggetKind::UpgradeTroops(upgrade) => {
                    check.many(
                        "UpgradeableUnits",
                        "Object",
                        &upgrade.upgradeable_units.0,
                        &object,
                    );
                }
                _ => {}
            }
        }
        found.append(&mut check.found);
    }

    for campaign in data.campaigns.iter() {
        found.extend(campaign_refs(data, campaign, external));
    }

    for (block, name, sides) in [
        (
            "LivingWorldAutoResolveResourceBonus",
            "",
            data.resource_bonus.as_ref().map(|b| &b.sides.0),
        ),
        (
            "LivingWorldAutoResolveSciencePurchasePointBonus",
            "",
            data.science_point_bonus.as_ref().map(|b| &b.sides.0),
        ),
    ] {
        if let Some(sides) = sides {
            let mut check = Check {
                found: Vec::new(),
                block,
                name,
            };
            check.many("Sides", "LivingWorldPlayerTemplate", sides, lw_player);
            found.append(&mut check.found);
        }
    }
    found
}

fn spawn_army_refs(
    check: &mut Check<'_>,
    spawn: &SpawnArmyDef,
    army: &impl Fn(&str) -> bool,
    object: &impl Fn(&str) -> bool,
    army_icon: &impl Fn(&str) -> bool,
) {
    check.one(
        "PlayerArmy",
        "LivingWorldPlayerArmy",
        spawn.player_army.as_ref(),
        army,
    );
    check.one(
        "HeroTemplateName",
        "Object",
        spawn.hero_template_name.as_ref(),
        object,
    );
    check.one(
        "Icon",
        "LivingWorldArmyIcon",
        spawn.icon.as_ref(),
        army_icon,
    );
}

fn condition_regions(
    check: &mut Check<'_>,
    conditions: &Conditions,
    region: &impl Fn(&str) -> bool,
) {
    for victory in &conditions.team_victory {
        check.many(
            "ControlledRegions",
            "Region",
            &victory.controlled_regions.0,
            region,
        );
    }
}

fn campaign_refs(
    data: &LivingWorldData,
    campaign: &LivingWorldCampaignDef,
    external: &ExternalNames,
) -> Vec<Unresolved> {
    let object = lower_in(external.objects);
    let faction = lower_in(external.player_templates);
    let army = |name: &str| data.armies.contains(name);
    let army_icon = |name: &str| data.army_icons.contains(name);
    let scripting: BTreeSet<String> = campaign
        .scripting_names()
        .map(str::to_ascii_lowercase)
        .collect();
    let scripted = lower_in(&scripting);
    let mut check = Check {
        found: Vec::new(),
        block: "LivingWorldCampaign",
        name: &campaign.name,
    };
    for player in &campaign.add_players {
        check.one(
            "AddPlayer/PlayerTemplate",
            "LivingWorldPlayerTemplate",
            player.player_template.as_ref(),
            |n| data.player_templates.contains(n),
        );
        check.one(
            "AddPlayer/AITemplate",
            "LivingWorldAITemplate",
            player.ai_template.as_ref(),
            |n| data.ai_templates.contains(n),
        );
    }
    let mut scenario_campaign: Option<&RegionCampaignDef> = None;
    for scenario in &campaign.scenarios {
        if let Some(wanted) = &scenario.region_campaign {
            scenario_campaign = data.region_campaigns.get(wanted);
            if scenario_campaign.is_none() {
                check.one(
                    "Scenario/RegionCampaign",
                    "LivingWorldRegionCampaign",
                    Some(wanted),
                    |_| false,
                );
            }
        }
        let region = |name: &str| scenario_campaign.is_none_or(|c| c.region(name).is_some());
        check.many(
            "Scenario/DisableRegions",
            "Region",
            &scenario.disable_regions.0,
            region,
        );
        check.many(
            "Scenario/DisallowStartInRegions",
            "Region",
            &scenario.disallow_start_in_regions.0,
            region,
        );
        check.many(
            "Scenario/DefaultStartSpots",
            "Region",
            &scenario.default_start_spots.0,
            region,
        );
        check.many(
            "Scenario/DisabledFactions",
            "PlayerTemplate",
            &scenario.disabled_factions.0,
            &faction,
        );
        condition_regions(&mut check, &scenario.conditions, &region);
        for set in &scenario.ownership_sets {
            check.many("OwnershipSet/Regions", "Region", &set.regions.0, region);
            check.one(
                "OwnershipSet/StartRegion",
                "Region",
                set.start_region.as_ref(),
                region,
            );
            for spawn in &set.spawn_buildings {
                check.many(
                    "SpawnBuildings/Buildings",
                    "LivingWorldBuilding",
                    &spawn.buildings.0,
                    |n| data.buildings.contains(n),
                );
                check.one(
                    "SpawnBuildings/Region",
                    "Region",
                    spawn.region.as_ref(),
                    region,
                );
            }
            for spawn in &set.spawn_armies {
                check.many(
                    "SpawnArmies/Armies",
                    "act SpawnArmy ScriptingName",
                    &spawn.armies.0,
                    &scripted,
                );
                check.one(
                    "SpawnArmies/Region",
                    "Region",
                    spawn.region.as_ref(),
                    region,
                );
            }
        }
        for restriction in &scenario.starting_restrictions {
            check.many(
                "StartingRestriction/Regions",
                "Region",
                &restriction.regions.0,
                region,
            );
            check.many(
                "StartingRestriction/Factions",
                "PlayerTemplate",
                &restriction.factions.0,
                &faction,
            );
        }
    }
    let region = |name: &str| scenario_campaign.is_none_or(|c| c.region(name).is_some());
    for victory in &campaign.victory_types {
        condition_regions(&mut check, &victory.conditions, &region);
    }
    for act in &campaign.acts {
        for nugget in &act.nuggets {
            match nugget {
                ActNugget::EnableRegion(n) => {
                    check.one("EnableRegion/Region", "Region", n.region.as_ref(), region)
                }
                ActNugget::ForceBattle(n) => {
                    check.one("ForceBattle/Region", "Region", n.region.as_ref(), region)
                }
                ActNugget::SpawnArmy(n) => {
                    spawn_army_refs(&mut check, n, &army, &object, &army_icon);
                    check.one(
                        "SpawnArmy/InitialRegion",
                        "Region",
                        n.initial_region.as_ref(),
                        region,
                    );
                    check.many(
                        "SpawnArmy/SpawnForTemplates",
                        "LivingWorldPlayerTemplate",
                        &n.spawn_for_templates.0,
                        |t| data.player_templates.contains(t),
                    );
                }
                ActNugget::MoveArmy(n) => {
                    check.one(
                        "MoveArmy/ArmyScriptingName",
                        "act SpawnArmy ScriptingName",
                        n.army_scripting_name.as_ref(),
                        &scripted,
                    );
                    check.one(
                        "MoveArmy/TargetRegionName",
                        "Region",
                        n.target_region_name.as_ref(),
                        region,
                    );
                }
                ActNugget::SpawnBuilding(n) => {
                    check.one(
                        "SpawnBuilding/Building",
                        "LivingWorldBuilding",
                        n.building.as_ref(),
                        |t| data.buildings.contains(t),
                    );
                    check.one("SpawnBuilding/Region", "Region", n.region.as_ref(), region);
                }
                ActNugget::SetPlayerControlOfArmy(n) => check.one(
                    "SetPlayerControlOfArmy/ArmyScriptingName",
                    "act SpawnArmy ScriptingName",
                    n.army_scripting_name.as_ref(),
                    &scripted,
                ),
                ActNugget::UpdateAnimObject(n) => check.one(
                    "UpdateAnimObject/AnimObjectName",
                    "LivingWorldAnimObject",
                    n.anim_object_name.as_ref(),
                    |t| data.anim_objects.contains(t),
                ),
                _ => {}
            }
        }
    }
    check.found
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionGraph {
    pub regions: usize,
    pub edges: usize,
    pub asymmetric: usize,
}

impl RegionCampaignDef {
    #[must_use]
    pub fn graph(&self) -> RegionGraph {
        let mut edges = 0usize;
        let mut asymmetric = 0usize;
        for from in &self.regions {
            for to in from.connected() {
                edges = edges.saturating_add(1);
                let mutual = self.region(to).is_some_and(|other| {
                    other
                        .connected()
                        .any(|back| back.eq_ignore_ascii_case(&from.name))
                });
                if !mutual {
                    asymmetric = asymmetric.saturating_add(1);
                }
            }
        }
        RegionGraph {
            regions: self.regions.len(),
            edges,
            asymmetric,
        }
    }
}
