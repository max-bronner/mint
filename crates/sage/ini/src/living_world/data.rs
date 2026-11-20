use crate::tree::Block;

use super::auto_resolve::{
    AutoResolveArmorDef, AutoResolveBodyDef, AutoResolveCombatChainDef, AutoResolveHandicapDef,
    AutoResolveLeadershipDef, AutoResolveWeaponDef, ReinforcementScheduleDef, ResourceBonusDef,
    SciencePointBonusDef,
};
use super::campaign::LivingWorldCampaignDef;
use super::effects::RegionEffectsDef;
use super::grammar::{label, Cx, Store};
use super::hud::StrategicHudDef;
use super::players::{AiTemplateDef, BuildingDef, PlayerArmyDef, PlayerTemplateDef};
use super::presentation::{
    AnimObjectDef, ArmyIconDef, BannerTypeDef, BuildPlotIconDef, BuildingIconDef, MapInfoDef,
    WorldObjectDef, WorldSoundDef,
};
use super::region::RegionCampaignDef;

pub const ROOT_KEYWORDS: [&str; 25] = [
    "LivingWorldMapInfo",
    "LivingWorldObject",
    "LivingWorldSound",
    "BannerType",
    "LivingWorldAnimObject",
    "LivingWorldArmyIcon",
    "LivingWorldBuildingIcon",
    "LivingWorldBuildPlotIcon",
    "LivingWorldPlayerTemplate",
    "LivingWorldAITemplate",
    "LivingWorldPlayerArmy",
    "LivingWorldBuilding",
    "LivingWorldCampaign",
    "LivingWorldRegionCampaign",
    "LivingWorldRegionEffects",
    "StrategicHUD",
    "AutoResolveWeapon",
    "AutoResolveArmor",
    "AutoResolveBody",
    "AutoResolveCombatChain",
    "AutoResolveLeadership",
    "AutoResolveHandicapLevel",
    "LivingWorldAutoResolveResourceBonus",
    "LivingWorldAutoResolveSciencePurchasePointBonus",
    "AutoResolveReinforcementSchedule",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockCount {
    pub keyword: &'static str,
    pub read: usize,
    pub held: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LivingWorldData {
    pub map_info: Option<MapInfoDef>,
    pub objects: Store<WorldObjectDef>,
    pub sounds: Store<WorldSoundDef>,
    pub banner_types: Store<BannerTypeDef>,
    pub anim_objects: Store<AnimObjectDef>,
    pub army_icons: Store<ArmyIconDef>,
    pub building_icons: Store<BuildingIconDef>,
    pub build_plot_icons: Store<BuildPlotIconDef>,
    pub player_templates: Store<PlayerTemplateDef>,
    pub ai_templates: Store<AiTemplateDef>,
    pub armies: Store<PlayerArmyDef>,
    pub buildings: Store<BuildingDef>,
    pub campaigns: Store<LivingWorldCampaignDef>,
    pub region_campaigns: Store<RegionCampaignDef>,
    pub region_effects: Store<RegionEffectsDef>,
    pub strategic_hud: Option<StrategicHudDef>,
    pub auto_resolve_weapons: Store<AutoResolveWeaponDef>,
    pub auto_resolve_armors: Store<AutoResolveArmorDef>,
    pub auto_resolve_bodies: Store<AutoResolveBodyDef>,
    pub auto_resolve_combat_chains: Store<AutoResolveCombatChainDef>,
    pub auto_resolve_leaderships: Store<AutoResolveLeadershipDef>,
    pub auto_resolve_handicaps: Vec<AutoResolveHandicapDef>,
    pub resource_bonus: Option<ResourceBonusDef>,
    pub science_point_bonus: Option<SciencePointBonusDef>,
    pub reinforcement_schedules: Vec<ReinforcementScheduleDef>,
}

impl LivingWorldData {
    pub(crate) fn read_root(&mut self, block: &Block, cx: &Cx) -> bool {
        match block.keyword.as_str() {
            "LivingWorldMapInfo" => self.map_info = Some(MapInfoDef::read(block, cx)),
            "LivingWorldObject" => self
                .objects
                .insert(WorldObjectDef::read_plain_block(block, cx)),
            "LivingWorldSound" => self
                .sounds
                .insert(WorldSoundDef::read_plain_block(block, cx)),
            "BannerType" => self
                .banner_types
                .insert(BannerTypeDef::read_plain_block(block, cx)),
            "LivingWorldAnimObject" => self
                .anim_objects
                .insert(AnimObjectDef::read_plain_block(block, cx)),
            "LivingWorldArmyIcon" => self.army_icons.insert(ArmyIconDef::read(block, cx)),
            "LivingWorldBuildingIcon" => {
                self.building_icons.insert(BuildingIconDef::read(block, cx))
            }
            "LivingWorldBuildPlotIcon" => self
                .build_plot_icons
                .insert(BuildPlotIconDef::read(block, cx)),
            "LivingWorldPlayerTemplate" => self
                .player_templates
                .insert(PlayerTemplateDef::read(block, cx)),
            "LivingWorldAITemplate" => self.ai_templates.insert(AiTemplateDef::read(block, cx)),
            "LivingWorldPlayerArmy" => self.armies.insert(PlayerArmyDef::read(block, cx)),
            "LivingWorldBuilding" => self.buildings.insert(BuildingDef::read(block, cx)),
            "LivingWorldCampaign" => self
                .campaigns
                .insert(LivingWorldCampaignDef::read(block, cx)),
            "LivingWorldRegionCampaign" => self
                .region_campaigns
                .insert(RegionCampaignDef::read(block, cx)),
            "LivingWorldRegionEffects" => {
                let effects = RegionEffectsDef::read(block, cx);
                if self.region_effects.contains(&effects.name) {
                    cx.block_error(&label(block), block, "a region-effects name used once");
                } else {
                    self.region_effects.insert(effects);
                }
            }
            "StrategicHUD" => self.strategic_hud = Some(StrategicHudDef::read(block, cx)),
            "AutoResolveWeapon" => self
                .auto_resolve_weapons
                .insert(AutoResolveWeaponDef::read(block, cx)),
            "AutoResolveArmor" => self
                .auto_resolve_armors
                .insert(AutoResolveArmorDef::read(block, cx)),
            "AutoResolveBody" => self
                .auto_resolve_bodies
                .insert(AutoResolveBodyDef::read(block, cx)),
            "AutoResolveCombatChain" => self
                .auto_resolve_combat_chains
                .insert(AutoResolveCombatChainDef::read(block, cx)),
            "AutoResolveLeadership" => self
                .auto_resolve_leaderships
                .insert(AutoResolveLeadershipDef::read(block, cx)),
            "AutoResolveHandicapLevel" => self
                .auto_resolve_handicaps
                .push(AutoResolveHandicapDef::read(block, cx)),
            "LivingWorldAutoResolveResourceBonus" => {
                self.resource_bonus = Some(ResourceBonusDef::read(block, cx));
            }
            "LivingWorldAutoResolveSciencePurchasePointBonus" => {
                self.science_point_bonus = Some(SciencePointBonusDef::read(block, cx));
            }
            "AutoResolveReinforcementSchedule" => {
                if let Some(schedule) = ReinforcementScheduleDef::read(block, cx) {
                    self.reinforcement_schedules.push(schedule);
                }
            }
            _ => return false,
        }
        true
    }

    #[must_use]
    pub fn spawn_armies(&self) -> Vec<&super::players::SpawnArmyDef> {
        use super::campaign::ActNugget;
        use super::players::{BuildingNuggetKind, SpawnArmyDef};
        let mut out: Vec<&SpawnArmyDef> = Vec::new();
        for campaign in self.campaigns.iter() {
            for act in &campaign.acts {
                for nugget in &act.nuggets {
                    if let ActNugget::SpawnArmy(army) = nugget {
                        out.push(&**army);
                    }
                }
            }
        }
        for building in self.buildings.iter() {
            for nugget in &building.nuggets {
                if let BuildingNuggetKind::SpawnArmy(spawn) = &nugget.kind {
                    out.extend(spawn.armies.iter());
                }
            }
        }
        out
    }

    #[must_use]
    pub fn counts(&self) -> Vec<BlockCount> {
        let single = |keyword, present: bool| BlockCount {
            keyword,
            read: usize::from(present),
            held: usize::from(present),
        };
        let store = |keyword, read, held| BlockCount {
            keyword,
            read,
            held,
        };
        vec![
            single("LivingWorldMapInfo", self.map_info.is_some()),
            store(
                "LivingWorldObject",
                self.objects.defined(),
                self.objects.len(),
            ),
            store("LivingWorldSound", self.sounds.defined(), self.sounds.len()),
            store(
                "BannerType",
                self.banner_types.defined(),
                self.banner_types.len(),
            ),
            store(
                "LivingWorldAnimObject",
                self.anim_objects.defined(),
                self.anim_objects.len(),
            ),
            store(
                "LivingWorldArmyIcon",
                self.army_icons.defined(),
                self.army_icons.len(),
            ),
            store(
                "LivingWorldBuildingIcon",
                self.building_icons.defined(),
                self.building_icons.len(),
            ),
            store(
                "LivingWorldBuildPlotIcon",
                self.build_plot_icons.defined(),
                self.build_plot_icons.len(),
            ),
            store(
                "LivingWorldPlayerTemplate",
                self.player_templates.defined(),
                self.player_templates.len(),
            ),
            store(
                "LivingWorldAITemplate",
                self.ai_templates.defined(),
                self.ai_templates.len(),
            ),
            store(
                "LivingWorldPlayerArmy",
                self.armies.defined(),
                self.armies.len(),
            ),
            store(
                "LivingWorldBuilding",
                self.buildings.defined(),
                self.buildings.len(),
            ),
            store(
                "LivingWorldCampaign",
                self.campaigns.defined(),
                self.campaigns.len(),
            ),
            store(
                "LivingWorldRegionCampaign",
                self.region_campaigns.defined(),
                self.region_campaigns.len(),
            ),
            store(
                "LivingWorldRegionEffects",
                self.region_effects.defined(),
                self.region_effects.len(),
            ),
            single("StrategicHUD", self.strategic_hud.is_some()),
            store(
                "AutoResolveWeapon",
                self.auto_resolve_weapons.defined(),
                self.auto_resolve_weapons.len(),
            ),
            store(
                "AutoResolveArmor",
                self.auto_resolve_armors.defined(),
                self.auto_resolve_armors.len(),
            ),
            store(
                "AutoResolveBody",
                self.auto_resolve_bodies.defined(),
                self.auto_resolve_bodies.len(),
            ),
            store(
                "AutoResolveCombatChain",
                self.auto_resolve_combat_chains.defined(),
                self.auto_resolve_combat_chains.len(),
            ),
            store(
                "AutoResolveLeadership",
                self.auto_resolve_leaderships.defined(),
                self.auto_resolve_leaderships.len(),
            ),
            store(
                "AutoResolveHandicapLevel",
                self.auto_resolve_handicaps.len(),
                self.auto_resolve_handicaps.len(),
            ),
            single(
                "LivingWorldAutoResolveResourceBonus",
                self.resource_bonus.is_some(),
            ),
            single(
                "LivingWorldAutoResolveSciencePurchasePointBonus",
                self.science_point_bonus.is_some(),
            ),
            store(
                "AutoResolveReinforcementSchedule",
                self.reinforcement_schedules.len(),
                self.reinforcement_schedules.len(),
            ),
        ]
    }
}
