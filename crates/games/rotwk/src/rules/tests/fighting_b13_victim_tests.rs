use super::fighting_b4_tests::{attack, scene, TOGGLES};
use super::fighting_b6_tests::{current_slot, run_attack, weapon};
use super::*;

const PLAIN_VICTIM: &str = "Object Dummy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
    Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
    Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

mod choice {
    use super::*;

    const UPGRADE: &str = "Upgrade Upgrade_NoParalyse\n  Type = OBJECT\n  BuildCost = 0\n  \
        BuildTime = 0\nEnd\n";

    fn special(nugget: &str, extra: &str) -> String {
        format!(
            "Weapon Special\n  AttackRange = 300\n  DelayBetweenShots = 500\n  {extra}\n  \
             {nugget}\nEnd\n"
        )
    }

    fn victim(extra: &str) -> String {
        format!(
            "Object Dummy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  {extra}\n  \
               Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
        )
    }

    fn probe(behaviors: &str) -> String {
        format!(
            "Object Probe\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
               Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
               WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Special\n    \
                 Weapon = SECONDARY Hit\n  End\n  {behaviors}\n  \
               LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
        )
    }

    fn chosen(special: &str, victim_extra: &str, probe_behaviors: &str) -> u8 {
        let data = format!(
            "{TOGGLES}{UPGRADE}{}{special}{}{}",
            weapon("Hit", 300, ""),
            victim(victim_extra),
            probe(probe_behaviors),
        );
        let (simulation, ids, units) = run_attack(&data, &["Probe"], 200.0, 6);
        current_slot(&simulation, ids, units[0])
    }

    const SHOVE: &str =
        "MetaImpactNugget\n    ShockWaveAmount = 50\n    ShockWaveRadius = 50\n    \
        ShockWaveTaperOff = 1.0\n    ShockWaveZMult = 1.0\n  End";
    const NO_PARALYSE: &str = "Behavior = GrantUpgradeCreate Tag_Grant\n    \
        UpgradeToGrant = Upgrade_NoParalyse\n  End\n  \
        Behavior = StatusBitsUpgrade Tag_Status\n    StatusToSet = IGNORE_PARALYZE_NUGGET\n    \
        TriggeredBy = Upgrade_NoParalyse\n  End";

    #[test]
    fn a_slot_whose_victim_test_refuses_is_not_chosen() {
        let proportional = "PassengerProportionalAttack = Yes\n  MaxAttackPassengers = 4";
        let damage = "DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End";
        let carrier = "Behavior = TransportContain Tag_Carry\n    Slots = 4\n    \
            PassengerFilter = ANY +INFANTRY\n  End";
        let rows: [(&str, String, &str, &str, u8); 11] = [
            (
                "a shove at a victim that resists it",
                special(SHOVE, ""),
                "ShockwaveResistance = 9999",
                "",
                1,
            ),
            (
                "a shove at a victim that does not",
                special(SHOVE, ""),
                "",
                "",
                0,
            ),
            (
                "a grab the shooter's contain refuses",
                special("GrabNugget\n    ContainTargetOnEffect = Yes\n  End", ""),
                "",
                "",
                1,
            ),
            (
                "a paralysis the victim ignores",
                special("ParalyzeNugget\n    Duration = 1000\n  End", ""),
                NO_PARALYSE,
                "",
                1,
            ),
            (
                "a paralysis of a victim that does not ignore it",
                special("ParalyzeNugget\n    Duration = 1000\n  End", ""),
                "",
                "",
                0,
            ),
            (
                "a garrison-killer at something that holds nothing",
                special(
                    "DamageContainedNugget\n    KillKindof = INFANTRY\n  End",
                    "",
                ),
                "",
                "",
                1,
            ),
            (
                "a slave attack with no watched slave",
                special("SlaveAttackNugget\n  End", ""),
                "",
                "",
                1,
            ),
            (
                "a passenger-proportional weapon with no passengers",
                special(damage, proportional),
                "",
                carrier,
                1,
            ),
            (
                "the same weapon on a firer with no contain",
                special(damage, proportional),
                "",
                "",
                0,
            ),
            (
                "an ordinary damage weapon on a carrier",
                special(damage, ""),
                "",
                carrier,
                0,
            ),
            (
                "an attribute modifier",
                special(
                    "AttributeModifierNugget\n    AttributeModifier = Hexed\n  End",
                    "",
                ),
                "",
                "",
                0,
            ),
        ];
        for (label, weapon_text, victim_extra, behaviors, expected) in rows {
            let weapon_text = if label == "an attribute modifier" {
                format!("ModifierList Hexed\n  Category = BUFF\n  Modifier = SPEED 50%\nEnd\n{weapon_text}")
            } else {
                weapon_text
            };
            assert_eq!(
                chosen(&weapon_text, victim_extra, behaviors),
                expected,
                "{label}"
            );
        }
    }
}

mod modifier {
    use super::*;

    const DATA: &str = "\
ModifierList Hexed\n  Category = BUFF\n  Modifier = SPEED 50%\n  Duration = 10000\nEnd\n\
Weapon Hex\n  AttackRange = 300\n  DelayBetweenShots = 5000\n  \
  AttributeModifierNugget\n    AttributeModifier = Hexed\n    Radius = 120\n    DamageArc = 45\n    \
    AntiCategories = LEADERSHIP\n  End\nEnd\n\
Object Hexer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hex\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

    fn fired() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
        let data = format!("{TOGGLES}{DATA}{PLAIN_VICTIM}");
        let (mut simulation, ids, units) = scene(
            &data,
            &[
                ("Hexer", Vec2::new(300.0, 300.0), PLAYER),
                ("Dummy", Vec2::new(340.0, 300.0), OTHER),
                ("Dummy", Vec2::new(250.0, 300.0), OTHER),
            ],
        );
        simulation.tick().unwrap();
        attack(&mut simulation, units[0], units[1]);
        for _ in 0..12 {
            simulation.tick().unwrap();
        }
        (simulation, ids, units[1], units[2])
    }

    fn lists(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> usize {
        simulation
            .state()
            .get(ids.timed_modifiers, entity)
            .map_or(0, |held| held.0.len())
    }

    #[test]
    fn a_modifier_arc_spares_a_victim_behind_the_firer() {
        let (simulation, ids, ahead, behind) = fired();
        assert_eq!(lists(&simulation, ids, ahead), 1, "the victim ahead");
        assert_eq!(lists(&simulation, ids, behind), 0, "the victim behind");
    }

    #[test]
    fn a_timed_modifier_suppresses_its_anti_categories_on_the_victim() {
        let (simulation, ids, ahead, _) = fired();
        assert!(simulation
            .state()
            .get(ids.suppressed, ahead)
            .is_some_and(|held| held.hides(Some("LEADERSHIP"))));
    }
}
