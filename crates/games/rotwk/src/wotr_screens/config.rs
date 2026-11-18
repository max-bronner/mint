use std::collections::BTreeMap;

use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_livingworld::screen_queries::BattleType;
use mint_sage_livingworld::{LivingWorldState, PlayerId};

use crate::wotr_dialog::DEFAULT_ROUNDS_PER_STEP;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Face {
    pub(crate) name: String,
    pub(crate) color: u32,
    pub(crate) faction_icon: Option<String>,
}

const DEFAULT_BANNER: &str = "BannerMen";

#[derive(Debug, Clone, Default)]
pub(crate) struct HudConfig {
    rounds_per_step: Option<u32>,
    faces: BTreeMap<u8, Face>,
    images: BTreeMap<&'static str, String>,
    building_images: BTreeMap<String, String>,
    banners: Banners,
    battle_type: BattleType,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Banners {
    by_hero: BTreeMap<String, String>,
    default: Option<String>,
}

impl Banners {
    pub(super) fn read(data: &LivingWorldData) -> Self {
        let icon = |name: &str| data.banner_types.get(name)?.icon.clone();
        Self {
            by_hero: data
                .spawn_armies()
                .into_iter()
                .filter_map(|army| {
                    let hero = army.hero_template_name.as_deref()?;
                    Some((hero.to_ascii_lowercase(), icon(army.banner.as_deref()?)?))
                })
                .collect(),
            default: icon(DEFAULT_BANNER),
        }
    }

    pub(super) fn icon(&self, hero: Option<&str>) -> Option<&str> {
        hero.and_then(|h| self.by_hero.get(&h.to_ascii_lowercase()))
            .or(self.default.as_ref())
            .map(String::as_str)
    }
}

pub(super) fn faction_icon(data: &LivingWorldData, template: &str) -> Option<String> {
    data.player_templates.get(template)?.faction_icon.clone()
}

fn byte(unit: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (unit.clamp(0.0, 1.0) * 255.0).round() as u32
    }
}

impl HudConfig {
    pub(crate) fn new(
        data: &LivingWorldData,
        state: &LivingWorldState,
        banner_colors: &[[f32; 3]],
        names: &[String],
    ) -> Self {
        let mut faces = BTreeMap::new();
        for player in state.players.iter() {
            let template = state.rules.templates.get(usize::from(player.template.0));
            let [r, g, b] = banner_colors
                .get(usize::from(player.id.0))
                .copied()
                .unwrap_or([1.0; 3]);
            faces.insert(
                player.id.0,
                Face {
                    name: names
                        .get(usize::from(player.id.0))
                        .cloned()
                        .unwrap_or_default(),
                    color: 0xFF00_0000 | (byte(r) << 16) | (byte(g) << 8) | byte(b),
                    faction_icon: template.and_then(|t| faction_icon(data, &t.name)),
                },
            );
        }
        let mut images = BTreeMap::new();
        if let Some(hud) = &data.strategic_hud {
            let pairs = [
                ("CancelArmyMoveButton", &hud.cancel_army_move_button),
                (
                    "CancelArmyMemberMoveButton",
                    &hud.cancel_army_member_move_button,
                ),
                ("DestroyBuildingButton", &hud.destroy_building_button),
                (
                    "CancelBuildingConstructionButton",
                    &hud.cancel_building_construction_button,
                ),
                ("DisbandArmyMemberButton", &hud.disband_army_member_button),
                ("UpgradeUnitButton", &hud.upgrade_unit_button),
                (
                    "ToggleSelectionDetailsButton",
                    &hud.toggle_selection_details_button,
                ),
            ];
            for (element, def) in pairs {
                if let Some(image) = def.as_ref().and_then(|d| d.image.clone()) {
                    images.insert(element, image);
                }
            }
            if let Some(types) = &hud.type_images {
                let named = [
                    ("BuildingFortress", &types.building_fortress),
                    ("BuildingArmory", &types.building_armory),
                    ("BuildingResource", &types.building_resource),
                    ("BuildingBarracks", &types.building_barracks),
                    ("UnitHero", &types.unit_hero),
                    ("UnitFortress", &types.unit_fortress),
                ];
                for (element, image) in named {
                    if let Some(image) = image {
                        images.insert(element, image.clone());
                    }
                }
            }
        }
        let rounds_per_step = data
            .strategic_hud
            .as_ref()
            .and_then(|h| h.dynamic_auto_resolve_dialog.as_ref())
            .and_then(|d| d.battle_rounds_per_step)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0);
        let building_images = data
            .buildings
            .iter()
            .filter_map(|b| {
                Some((
                    b.name.to_ascii_lowercase(),
                    b.construct_button_image.clone()?,
                ))
            })
            .collect();
        Self {
            rounds_per_step,
            faces,
            images,
            building_images,
            banners: Banners::read(data),
            battle_type: BattleType::default(),
        }
    }

    pub(crate) fn with_battle_type(mut self, battle_type: BattleType) -> Self {
        self.battle_type = battle_type;
        self
    }

    pub(crate) fn battle_type(&self) -> BattleType {
        self.battle_type
    }

    pub(crate) fn banner_icon(&self, hero: Option<&str>) -> Option<&str> {
        self.banners.icon(hero)
    }

    pub(crate) fn building_image(&self, building: &str) -> Option<&str> {
        self.building_images
            .get(&building.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub(crate) fn rounds_per_step(&self) -> u32 {
        self.rounds_per_step.unwrap_or(DEFAULT_ROUNDS_PER_STEP)
    }

    pub(crate) fn face(&self, player: PlayerId) -> Face {
        self.faces.get(&player.0).cloned().unwrap_or_else(|| Face {
            name: String::new(),
            color: 0xFFFF_FFFF,
            faction_icon: None,
        })
    }

    pub(crate) fn button_image(&self, element: &str) -> Option<&str> {
        self.images.get(element).map(String::as_str)
    }

    pub(crate) fn type_image(&self, element: &str) -> Option<&str> {
        self.button_image(element)
    }
}
