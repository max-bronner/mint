use std::sync::{Arc, OnceLock};

use mint_sim::ai_power::{
    AiPowerCandidate, AiPowerContext, AiPowerKind, AiPowerRegistry, AiPowerTarget,
};

struct StanceAggressive;

pub const AGGRESSIVE_MODE: u8 = 5;

pub const STANCE_HEALTH_FLOOR: f32 = 0.35;

impl AiPowerKind for StanceAggressive {
    fn should_activate(&self, ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
        ctx.current_mode != Some(AGGRESSIVE_MODE) && ctx.health_ratio > STANCE_HEALTH_FLOOR
    }
}

struct SelfBuff;

pub const SELF_BUFF_SCAN_RADIUS: f32 = 50.0;

const SELF_BUFF_HEALTH_CEILING: f32 = 0.7;

const SELF_BUFF_HEALTH_FLOOR: f32 = 0.4;

impl AiPowerKind for SelfBuff {
    fn should_activate(&self, ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
        if !ctx.in_combat {
            return false;
        }
        if ctx.health_ratio >= SELF_BUFF_HEALTH_CEILING {
            return false;
        }
        if ctx.health_ratio < SELF_BUFF_HEALTH_FLOOR {
            return true;
        }
        ctx.nearby_enemies > ctx.nearby_allies
    }
}

struct StanceBattle;

impl AiPowerKind for StanceBattle {
    fn should_activate(&self, ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
        ctx.health_ratio <= STANCE_HEALTH_FLOOR
    }
}

struct EnemyTypeKiller;

impl AiPowerKind for EnemyTypeKiller {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        ctx.in_combat && !candidates.is_empty()
    }

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let mut best: Option<&AiPowerCandidate> = None;
        for candidate in candidates {
            if best.is_none_or(|current| candidate.health_ratio < current.health_ratio) {
                best = Some(candidate);
            }
        }
        best.map(|candidate| AiPowerTarget::Object(candidate.entity))
    }
}

struct Charge;

impl AiPowerKind for Charge {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        ctx.in_combat && !candidates.is_empty()
    }

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let mut iter = candidates.iter();
        let mut current = iter.next()?;
        for candidate in iter {
            if candidate.health_ratio < current.health_ratio {
                current = candidate;
                break;
            }
        }
        Some(AiPowerTarget::Object(current.entity))
    }
}

struct RangedAoeAttack;

impl AiPowerKind for RangedAoeAttack {
    fn should_activate(&self, _ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        !candidates.is_empty()
    }

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let nearest = candidates.first()?;
        Some(AiPowerTarget::Position(
            nearest.position.0,
            nearest.position.1,
        ))
    }
}

struct TargetAoeSummon;

impl AiPowerKind for TargetAoeSummon {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        ctx.in_combat && !candidates.is_empty()
    }

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let best = candidates.first()?;
        Some(AiPowerTarget::Position(best.position.0, best.position.1))
    }
}

struct GiveXpAoe;

impl AiPowerKind for GiveXpAoe {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        !ctx.in_combat && !candidates.is_empty()
    }

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let best = candidates.first()?;
        Some(AiPowerTarget::Position(best.position.0, best.position.1))
    }
}

struct SelfAoeHealHeroes;

const SELF_HEAL_THRESHOLD_CENTRE: f32 = 0.5;

const SELF_HEAL_THRESHOLD_SPREAD: f32 = 0.1;

impl AiPowerKind for SelfAoeHealHeroes {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool {
        let unit = ctx.rng_unit.clamp(0.0, 1.0);
        let threshold =
            SELF_HEAL_THRESHOLD_CENTRE + (unit * 2.0 - 1.0) * SELF_HEAL_THRESHOLD_SPREAD;
        if ctx.health_ratio < threshold {
            return true;
        }
        candidates
            .iter()
            .any(|candidate| candidate.health_ratio < threshold)
    }
}

struct ToggleMeleeAndRange;

const TOGGLE_MELEE_NEAR: f32 = 50.0;

const TOGGLE_MELEE_FAR: f32 = 150.0;

impl AiPowerKind for ToggleMeleeAndRange {
    fn should_activate(&self, ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
        let Some(distance) = ctx.victim_distance else {
            return false;
        };
        if ctx.weapon_is_melee {
            distance >= TOGGLE_MELEE_FAR
        } else {
            distance <= TOGGLE_MELEE_NEAR
        }
    }
}

struct Inert;

impl AiPowerKind for Inert {
    fn should_activate(&self, _ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
        false
    }
}

const INERT_TYPES: &[&str] = &[
    "AI_SPECIAL_POWER_AOE_AND_BUFF",
    "AI_SPECIAL_POWER_ATTACK_HEAL_AOE",
    "AI_SPECIAL_POWER_BASIC_SELF_DEBUFF",
    "AI_SPECIAL_POWER_CAPTURE_BUILDING",
    "AI_SPECIAL_POWER_DOMINATE_ENEMY",
    "AI_SPECIAL_POWER_DOMINATE_TROLL",
    "AI_SPECIAL_POWER_ELENDIL",
    "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER_STRUCTURES",
    "AI_SPECIAL_POWER_GANDALF_WIZARD_BLAST",
    "AI_SPECIAL_POWER_GOBLINKING_CALLOFTHEDEEP",
    "AI_SPECIAL_POWER_GOBLINKING_MOUNTED",
    "AI_SPECIAL_POWER_HEAL_AOE",
    "AI_SPECIAL_POWER_LEGOLAS_ARROWWIND",
    "AI_SPECIAL_POWER_LEGOLAS_TRAINARCHERS",
    "AI_SPECIAL_POWER_RANGED_AOE_ATTACK_UNITS",
    "AI_SPECIAL_POWER_SOUL_FREEZE",
    "AI_SPECIAL_POWER_STANCEHOLDGROUND",
    "AI_SPECIAL_POWER_TAME_THE_BEAST",
    "AI_SPECIAL_POWER_TOGGLE_MOUNTED",
    "AI_SPECIAL_POWER_TOGGLE_SIEGE",
    "AI_SPELLBOOK_ALWAYS_FIRE",
    "AI_SPELLBOOK_ARMY_BREAKER",
    "AI_SPELLBOOK_ASSIST_BATTLE_BUFF",
    "AI_SPELLBOOK_ASSIST_BATTLE_DEBUFF",
    "AI_SPELLBOOK_BUFFECONOMYBUILDING",
    "AI_SPELLBOOK_BUFFTERRAIN",
    "AI_SPELLBOOK_CALLTHEHORDE",
    "AI_SPELLBOOK_CAPTURE_CREEP",
    "AI_SPELLBOOK_CITADEL",
    "AI_SPELLBOOK_DEBUFFECONOMYBUILDING",
    "AI_SPELLBOOK_DEBUFFPRODUCTIONBUILDING",
    "AI_SPELLBOOK_ENSHROUDINGMIST",
    "AI_SPELLBOOK_HEAL",
    "AI_SPELLBOOK_REBUILD",
    "AI_SPELLBOOK_SHROUD_REVEAL",
    "AI_SPELLBOOK_STRUCTURE_BASEKILL",
    "AI_SPELLBOOK_STRUCTURE_BREAKER",
    "AI_SPELLBOOK_STRUCTURE_BREAKER_PREF_WALLS",
    "AI_SPELLBOOK_TREE_KILLER",
];

fn build_registry() -> AiPowerRegistry {
    let mut registry = AiPowerRegistry::default();
    registry.register(
        "AI_SPECIAL_POWER_STANCEAGGRESSIVE",
        Arc::new(StanceAggressive),
    );
    registry.register("AI_SPECIAL_POWER_BASIC_SELF_BUFF", Arc::new(SelfBuff));
    registry.register("AI_SPECIAL_POWER_STANCEBATTLE", Arc::new(StanceBattle));
    registry.register(
        "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER",
        Arc::new(EnemyTypeKiller),
    );
    registry.register(
        "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER_RANGED",
        Arc::new(EnemyTypeKiller),
    );
    registry.register("AI_SPECIAL_POWER_CHARGE", Arc::new(Charge));
    registry.register(
        "AI_SPECIAL_POWER_RANGED_AOE_ATTACK",
        Arc::new(RangedAoeAttack),
    );
    registry.register(
        "AI_SPECIAL_POWER_TARGETAOE_SUMMON",
        Arc::new(TargetAoeSummon),
    );
    registry.register("AI_SPECIAL_POWER_GIVEXP_AOE", Arc::new(GiveXpAoe));
    registry.register(
        "AI_SPECIAL_POWER_SELFAOEHEALHEROS",
        Arc::new(SelfAoeHealHeroes),
    );
    registry.register(
        "AI_SPECIAL_POWER_TOGGLE_MELEE_AND_RANGE",
        Arc::new(ToggleMeleeAndRange),
    );
    for name in INERT_TYPES {
        registry.register(name, Arc::new(Inert));
    }
    registry
}

static REGISTRY: OnceLock<AiPowerRegistry> = OnceLock::new();

pub fn ai_power_registry() -> &'static AiPowerRegistry {
    REGISTRY.get_or_init(build_registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::{Entity, World};

    const BASE_CTX: AiPowerContext = AiPowerContext {
        health_ratio: 1.0,
        current_mode: None,
        nearby_enemies: 0,
        nearby_allies: 0,
        in_combat: false,
        rng_unit: 0.0,
        victim_distance: None,
        weapon_is_melee: false,
    };

    fn entities(n: u32) -> Vec<Entity> {
        let mut world = World::default();
        (0..n).map(|_| world.spawn().unwrap()).collect()
    }

    fn candidate(entity: Entity, distance: f32, health_ratio: f32) -> AiPowerCandidate {
        AiPowerCandidate {
            entity,
            distance,
            health_ratio,
            position: (distance, 0.0),
        }
    }

    #[test]
    fn every_inert_type_loads_but_never_activates() {
        let registry = ai_power_registry();
        let ctx = AiPowerContext {
            health_ratio: 0.0,
            nearby_enemies: 100,
            in_combat: true,
            ..BASE_CTX
        };
        for name in INERT_TYPES {
            let kind = registry
                .get(name)
                .unwrap_or_else(|| panic!("{name} not registered"));
            assert!(!kind.should_activate(ctx, &[]), "{name} should stay inert");
        }
    }

    #[test]
    fn aggressive_mode_matches_mints_own_stance_index() {
        assert_eq!(AGGRESSIVE_MODE, mint_sage_ini::Stance::Aggressive.index());
    }

    #[test]
    fn stance_aggressive_switches_above_the_health_floor_when_not_already_aggressive() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_STANCEAGGRESSIVE").unwrap();
        let healthy_other_stance = AiPowerContext {
            health_ratio: 0.5,
            current_mode: Some(0),
            ..BASE_CTX
        };
        assert!(kind.should_activate(healthy_other_stance, &[]));

        let already_aggressive = AiPowerContext {
            current_mode: Some(AGGRESSIVE_MODE),
            ..healthy_other_stance
        };
        assert!(!kind.should_activate(already_aggressive, &[]));

        let too_hurt = AiPowerContext {
            health_ratio: 0.1,
            ..healthy_other_stance
        };
        assert!(!kind.should_activate(too_hurt, &[]));
    }

    #[test]
    fn self_buff_fires_when_badly_hurt_or_outnumbered_but_not_when_healthy() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_BASIC_SELF_BUFF").unwrap();

        let healthy = AiPowerContext {
            health_ratio: 0.9,
            nearby_enemies: 10,
            in_combat: true,
            ..BASE_CTX
        };
        assert!(!kind.should_activate(healthy, &[]));

        let badly_hurt = AiPowerContext {
            health_ratio: 0.1,
            nearby_enemies: 0,
            nearby_allies: 10,
            ..healthy
        };
        assert!(kind.should_activate(badly_hurt, &[]));

        let badly_hurt_but_not_in_combat = AiPowerContext {
            in_combat: false,
            ..badly_hurt
        };
        assert!(!kind.should_activate(badly_hurt_but_not_in_combat, &[]));

        let outnumbered = AiPowerContext {
            health_ratio: 0.5,
            nearby_enemies: 3,
            nearby_allies: 1,
            ..healthy
        };
        assert!(kind.should_activate(outnumbered, &[]));

        let evenly_matched = AiPowerContext {
            health_ratio: 0.5,
            nearby_enemies: 2,
            nearby_allies: 2,
            ..healthy
        };
        assert!(!kind.should_activate(evenly_matched, &[]));
    }

    #[test]
    fn stance_battle_fires_at_or_below_the_health_floor_regardless_of_mode() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_STANCEBATTLE").unwrap();
        let at_floor = AiPowerContext {
            health_ratio: STANCE_HEALTH_FLOOR,
            current_mode: Some(AGGRESSIVE_MODE),
            ..BASE_CTX
        };
        assert!(kind.should_activate(at_floor, &[]));

        let above_floor = AiPowerContext {
            health_ratio: STANCE_HEALTH_FLOOR + 0.01,
            ..at_floor
        };
        assert!(!kind.should_activate(above_floor, &[]));
    }

    #[test]
    fn enemy_type_killer_targets_the_weakest_enemy_keeping_the_nearer_on_a_tie() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_ENEMY_TYPE_KILLER").unwrap();
        let ctx = AiPowerContext {
            in_combat: true,
            ..BASE_CTX
        };
        assert!(!kind.should_activate(ctx, &[]));

        let e = entities(4);
        let candidates = [candidate(e[0], 10.0, 0.5), candidate(e[1], 20.0, 0.2)];
        assert!(kind.should_activate(ctx, &candidates));
        assert_eq!(
            kind.choose_target(&candidates),
            Some(AiPowerTarget::Object(e[1]))
        );

        let tied = [candidate(e[2], 10.0, 0.2), candidate(e[3], 20.0, 0.2)];
        assert_eq!(kind.choose_target(&tied), Some(AiPowerTarget::Object(e[2])));
    }

    #[test]
    fn charge_stops_at_the_first_weaker_enemy_while_walking_outward() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_CHARGE").unwrap();
        let e = entities(5);
        let candidates = [
            candidate(e[0], 10.0, 0.5),
            candidate(e[1], 20.0, 0.4),
            candidate(e[2], 30.0, 0.1),
        ];
        assert_eq!(
            kind.choose_target(&candidates),
            Some(AiPowerTarget::Object(e[1]))
        );

        let no_weaker = [candidate(e[3], 10.0, 0.5), candidate(e[4], 20.0, 0.9)];
        assert_eq!(
            kind.choose_target(&no_weaker),
            Some(AiPowerTarget::Object(e[3]))
        );
    }

    #[test]
    fn ranged_aoe_attack_fires_at_the_nearest_enemys_position_once_far_enough() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_RANGED_AOE_ATTACK").unwrap();
        assert!(!kind.should_activate(BASE_CTX, &[]));
        let candidates = [candidate(entities(1)[0], 500.0, 0.5)];
        assert!(kind.should_activate(BASE_CTX, &candidates));
        assert_eq!(
            kind.choose_target(&candidates),
            Some(AiPowerTarget::Position(500.0, 0.0))
        );
    }

    #[test]
    fn self_aoe_heal_heroes_fires_on_its_own_low_health_or_another_heros() {
        let registry = ai_power_registry();
        let kind = registry.get("AI_SPECIAL_POWER_SELFAOEHEALHEROS").unwrap();
        let ctx = AiPowerContext {
            health_ratio: 0.9,
            rng_unit: 0.5,
            ..BASE_CTX
        };
        assert!(!kind.should_activate(ctx, &[]));

        let own_low = AiPowerContext {
            health_ratio: 0.3,
            ..ctx
        };
        assert!(kind.should_activate(own_low, &[]));

        let other_hero_low = [candidate(entities(1)[0], 0.0, 0.3)];
        assert!(kind.should_activate(ctx, &other_hero_low));
    }

    #[test]
    fn toggle_melee_and_range_switches_at_its_own_distance_thresholds() {
        let registry = ai_power_registry();
        let kind = registry
            .get("AI_SPECIAL_POWER_TOGGLE_MELEE_AND_RANGE")
            .unwrap();
        assert!(!kind.should_activate(BASE_CTX, &[]));

        let ranged_close = AiPowerContext {
            victim_distance: Some(49.0),
            weapon_is_melee: false,
            ..BASE_CTX
        };
        assert!(kind.should_activate(ranged_close, &[]));
        let ranged_far = AiPowerContext {
            victim_distance: Some(51.0),
            ..ranged_close
        };
        assert!(!kind.should_activate(ranged_far, &[]));

        let melee_far = AiPowerContext {
            victim_distance: Some(151.0),
            weapon_is_melee: true,
            ..BASE_CTX
        };
        assert!(kind.should_activate(melee_far, &[]));
        let melee_close = AiPowerContext {
            victim_distance: Some(149.0),
            ..melee_far
        };
        assert!(!kind.should_activate(melee_close, &[]));
    }
}
