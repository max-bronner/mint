use crate::model::{bonus, Battle, BuildingId, PlayerId, PlayerKind, RegionId};
use crate::state::LivingWorldState;
use crate::victory::names_team;

pub const AUTO_RESOLVE: u8 = 2;
pub const RETREAT: u8 = 4;
pub const REAL_TIME: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BattleType {
    #[default]
    Both,
    AutoResolveOnly,
    RealTimeOnly,
}

impl BattleType {
    #[must_use]
    pub fn ordinal(self) -> u8 {
        match self {
            Self::Both => 0,
            Self::AutoResolveOnly => 1,
            Self::RealTimeOnly => 2,
        }
    }

    #[must_use]
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        match ordinal {
            0 => Some(Self::Both),
            1 => Some(Self::AutoResolveOnly),
            2 => Some(Self::RealTimeOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskInputs {
    pub battle_type: BattleType,
    pub campaign_map: bool,
    pub local_game: bool,
    pub every_human_in_battle: bool,
    pub can_retreat: bool,
}

#[must_use]
pub fn button_mask(i: MaskInputs) -> u8 {
    if i.campaign_map {
        return REAL_TIME;
    }
    let mut mask = if i.battle_type == BattleType::RealTimeOnly {
        0
    } else {
        AUTO_RESOLVE
    };
    if i.can_retreat {
        mask |= RETREAT;
    }
    if i.battle_type != BattleType::AutoResolveOnly {
        if i.local_game || i.every_human_in_battle {
            mask |= REAL_TIME;
        } else {
            mask |= AUTO_RESOLVE;
        }
    }
    mask
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerStats {
    pub treasury: i32,
    pub income: i32,
    pub bonuses: [i32; 3],
    pub resource_multiplier: f32,
    pub power_points: i32,
}

impl LivingWorldState {
    #[must_use]
    pub fn player_stats(&self, player: PlayerId) -> Option<PlayerStats> {
        let p = self.player(player)?;
        let income = self
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.owner == Some(player))
            .map(|(i, r)| {
                let fertile = u16::try_from(i)
                    .ok()
                    .and_then(|i| self.rules.region_rules(RegionId(i)))
                    .map_or(0, |rules| rules.bonus.get(bonus::FERTILE_TERRITORY));
                r.plots
                    .iter()
                    .flatten()
                    .filter(|plot| !plot.under_construction)
                    .filter_map(|plot| self.rules.building(plot.building))
                    .flat_map(|b| b.treasury.iter().copied())
                    .fold(fertile, i32::saturating_add)
            })
            .fold(0, i32::saturating_add);
        let effective = crate::economy::effective_resource_bonus(
            p.bonus_a.get(bonus::RESOURCE),
            self.rules.resource_multiplier_limit,
        );
        #[allow(clippy::cast_precision_loss)]
        let resource_multiplier = effective as f32 * 0.01f32 + 1.0f32;
        Some(PlayerStats {
            treasury: p.resources,
            income,
            bonuses: [
                p.bonus_a.get(bonus::ATTACK),
                p.bonus_a.get(bonus::DEFENSE),
                p.bonus_a.get(bonus::EXPERIENCE),
            ],
            resource_multiplier,
            power_points: p
                .bonus_a
                .get(bonus::LEGENDARY)
                .saturating_add(p.rank_points),
        })
    }

    #[must_use]
    pub fn buildable_here(&self, player: PlayerId) -> Vec<BuildingId> {
        let Some(template) = self.player(player).map(|p| p.template) else {
            return Vec::new();
        };
        self.rules
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.available_to.is_empty() || b.available_to.contains(&template))
            .filter_map(|(index, _)| u16::try_from(index).ok().map(BuildingId))
            .collect()
    }

    #[must_use]
    pub fn capital_decides(&self, player: PlayerId, region: RegionId) -> bool {
        self.player(player).is_some_and(|p| {
            p.base_region == Some(region)
                && self
                    .rules
                    .conditions
                    .player_defeat
                    .iter()
                    .any(|c| c.lose_if_capital_lost && names_team(&c.teams, p.team))
        })
    }

    #[must_use]
    pub fn has_hero_army_at(&self, player: PlayerId, region: RegionId) -> bool {
        self.armies_of(player)
            .any(|a| a.region == region && a.is_hero_army())
    }

    #[must_use]
    pub fn can_retreat_from(&self, battle: &Battle, player: PlayerId) -> bool {
        battle.players.contains(&player)
            && self.has_hero_army_at(player, battle.region)
            && self.rules.region_rules(battle.region).is_some_and(|r| {
                r.neighbours
                    .iter()
                    .any(|n| self.allied_owner(self.owner(*n), player))
            })
            && !self.capital_decides(player, battle.region)
    }

    #[must_use]
    pub fn battle_buttons(
        &self,
        battle: &Battle,
        player: PlayerId,
        battle_type: BattleType,
        local_game: bool,
    ) -> u8 {
        let humans = |list: &[PlayerId]| {
            list.iter()
                .filter(|p| self.kind(**p) == Some(PlayerKind::Human))
                .count()
        };
        let everyone: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        button_mask(MaskInputs {
            battle_type,
            campaign_map: self
                .rules
                .region_rules(battle.region)
                .is_some_and(|r| r.campaign_map),
            local_game,
            every_human_in_battle: humans(&everyone) == humans(&battle.players),
            can_retreat: self.can_retreat_from(battle, player),
        })
    }
}
