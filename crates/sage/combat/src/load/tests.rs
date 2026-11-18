use super::*;
use mint_sage_ini::BlockTable;

fn parse(text: &str) -> Document {
    Document::parse("test.ini", text, &BlockTable::priority_blocks_path()).unwrap()
}

fn load(text: &str) -> CombatData {
    let document = parse(text);
    let mut macros = MacroTable::new();
    macros.insert_defines(document.defines());
    let mut data = CombatData::new();
    data.load_document(&document, &macros);
    data
}

#[test]
fn a_weapon_block_loads_its_timing_range_and_nuggets() {
    let data = load(
        "Weapon TestBow\n\
             \tAttackRange = 300\n\
             \tMinimumAttackRange = 40\n\
             \tDelayBetweenShots = 1000\n\
             \tPreAttackDelay = 500\n\
             \tPreAttackType = PER_POSITION\n\
             \tClipSize = 2\n\
             \tClipReloadTime = 2000\n\
             \tAutoReloadsClip = Yes\n\
             \tWeaponSpeed = 321\n\
             \tDamageNugget\n\
             \t\tDamage = 40\n\
             \t\tRadius = 5\n\
             \t\tDelayTime = 0\n\
             \t\tDamageType = PIERCE\n\
             \t\tDeathType = NORMAL\n\
             \tEnd\n\
             End\n",
    );
    let id = data.weapons.id("TestBow").unwrap();
    let weapon = data.weapons.get(id).unwrap();
    assert!((weapon.attack_range - 300.0).abs() < 1e-3);
    assert!((weapon.minimum_attack_range - 40.0).abs() < 1e-3);
    assert_eq!(
        weapon.delay_between_shots,
        FrameRange::fixed(LogicFrames::from_frames(30))
    );
    assert_eq!(weapon.pre_attack_delay, LogicFrames::from_frames(15));
    assert_eq!(
        weapon.clip_reload_time,
        FrameRange::fixed(LogicFrames::from_frames(60))
    );
    assert_eq!(weapon.pre_attack_type, PreAttackType::PerPosition);
    assert_eq!(weapon.clip_size, 2);
    assert!(weapon.auto_reloads_clip);
    assert_eq!(weapon.nuggets.len(), 1);
    let Nugget::Damage(nugget) = &weapon.nuggets[0] else {
        panic!("expected a damage nugget, got {:?}", weapon.nuggets[0]);
    };
    assert!((nugget.damage - 40.0).abs() < 1e-3);
    assert_eq!(data.damage_types.name(nugget.damage_type), Some("PIERCE"));
    assert_eq!(
        data.death_types.name(nugget.death_type.unwrap()),
        Some("NORMAL")
    );
    assert_eq!(data.report.weapons, 1);
    assert_eq!(data.report.modelled_nuggets, 1);
    assert_eq!(data.report.weapons_without_effect, 0);
}

#[test]
fn a_weapon_with_no_anti_field_defaults_to_ground_only() {
    let data = load("Weapon PlainBow\n\tAttackRange = 100\nEnd\n");
    let id = data.weapons.id("PlainBow").unwrap();
    let mask = data.weapons.get(id).unwrap().anti_mask;
    assert!(mask.contains(AntiMask::GROUND));
    assert!(!mask.contains(AntiMask::STRUCTURE));
    assert!(!mask.contains(AntiMask::AIRBORNE_VEHICLE));
}

#[test]
fn an_airborne_bit_adds_to_the_ground_default_rather_than_replacing_it() {
    let data = load(
        "Weapon EagleClaw\n\
             \tAttackRange = 100\n\
             \tAntiAirborneVehicle = Yes\n\
             \tAntiAirborneMonster = Yes\n\
             End\n",
    );
    let id = data.weapons.id("EagleClaw").unwrap();
    let mask = data.weapons.get(id).unwrap().anti_mask;
    assert!(mask.contains(AntiMask::GROUND), "default must survive");
    assert!(mask.contains(AntiMask::AIRBORNE_VEHICLE));
    assert!(mask.contains(AntiMask::AIRBORNE_MONSTER));
    assert!(!mask.contains(AntiMask::AIRBORNE_INFANTRY));
}

#[test]
fn explicit_anti_ground_no_clears_the_default_for_a_structure_only_weapon() {
    let data = load(
        "Weapon SiegeRam\n\
             \tAttackRange = 20\n\
             \tAntiStructure = Yes\n\
             \tAntiGround = No\n\
             End\n",
    );
    let id = data.weapons.id("SiegeRam").unwrap();
    let mask = data.weapons.get(id).unwrap().anti_mask;
    assert!(mask.contains(AntiMask::STRUCTURE));
    assert!(
        !mask.contains(AntiMask::GROUND),
        "No must clear the default"
    );
}

#[test]
fn armour_rows_reach_the_template_in_file_order() {
    let mut data = load(
        "Armor TestArmor\n\
             \tArmor = FLAME 100%\n\
             \tArmor = DEFAULT 50%\n\
             \tArmor = PIERCE 25%\n\
             \tFlankedPenalty = 20%\n\
             End\n",
    );
    let id = data.armors.id("TestArmor").unwrap();
    let template = data.armors.get(id).unwrap();
    let flame = data.damage_types.lookup("FLAME").unwrap();
    let pierce = data.damage_types.lookup("PIERCE").unwrap();
    assert!((template.coefficient(flame) - 0.5).abs() < 1e-6);
    assert!((template.coefficient(pierce) - 0.25).abs() < 1e-6);
    assert!((template.flanked_penalty().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(data.report.armor_rows, 3);
    let magic = data.damage_types.intern("MAGIC").unwrap();
    assert!((data.armors.get(id).unwrap().coefficient(magic) - 0.5).abs() < 1e-6);
}

#[test]
fn unresolvable_inline_arithmetic_is_counted_rather_than_hidden() {
    let data = load(
        "Weapon Broken\n\
             \tAttackRange = #SUBTRACT( SOMETHING 75 )\n\
             \tDelayBetweenShots = NOT_A_DEFINE\n\
             \tDamageNugget\n\
             \t\tDamage = 10\n\
             \t\tDamageType = SLASH\n\
             \tEnd\n\
             End\n",
    );
    assert_eq!(data.report.unresolved_fields, 2);
    let weapon = data
        .weapons
        .get(data.weapons.id("Broken").unwrap())
        .unwrap();
    assert_eq!(weapon.attack_range, 0.0);
    assert_eq!(weapon.delay_between_shots, FrameRange::ZERO);
}

#[test]
fn inline_arithmetic_over_a_macro_is_evaluated() {
    let data = load(
        "#define BOW_RANGE 300
         Weapon Rangefinder
             	AttackRange = #SUBTRACT( BOW_RANGE 100 )
             	MinimumAttackRange = #MULTIPLY( 20 2 )
             	DamageNugget
             		Damage = 10
             		DamageType = SLASH
             	End
             End
",
    );
    assert_eq!(data.report.unresolved_fields, 0);
    let weapon = data
        .weapons
        .get(data.weapons.id("Rangefinder").unwrap())
        .unwrap();
    assert!((weapon.attack_range - 200.0).abs() < 1e-6);
    assert!((weapon.minimum_attack_range - 40.0).abs() < 1e-6);
}

#[test]
fn an_emotion_duration_is_whole_seconds_in_authored_frames() {
    let document = parse(
        "Weapon Scare\n\
         \tEmotionWeaponNugget\n\
         \t\tEmotionType = FEAR\n\
         \t\tDuration = 2.5\n\
         \tEnd\n\
         End\n",
    );
    let mut data = CombatData::new();
    data.authored_logic_hz = core::num::NonZeroU32::new(5);
    data.load_document(&document, &MacroTable::new());
    let id = data.weapons.id("Scare").unwrap();
    let Some(EffectNugget::EmotionWeapon(nugget)) = data.effects.get(id.index(), 0) else {
        panic!("expected an emotion nugget");
    };
    assert_eq!(nugget.duration, LogicFrames::from_frames(60));
}

#[test]
fn a_nugget_flag_that_is_neither_yes_nor_no_is_counted() {
    let data = load(
        "Weapon Blast\n\
             \tMetaImpactNugget\n\
             \t\tShockWaveAmount = 50\n\
             \t\tShockWaveRadius = 100\n\
             \t\tInvertShockWave = Maybe\n\
             \tEnd\n\
             End\n",
    );
    assert_eq!(data.report.unresolved_fields, 1);
}

#[test]
fn a_min_max_reload_time_keeps_both_ends_of_the_band() {
    let data = load(
        "Weapon Ranged\n\
             \tClipReloadTime = Min:1000 Max:3000\n\
             \tDamageNugget\n\
             \t\tDamage = 1\n\
             \t\tDamageType = PIERCE\n\
             \tEnd\n\
             End\n",
    );
    let weapon = data
        .weapons
        .get(data.weapons.id("Ranged").unwrap())
        .unwrap();
    assert_eq!(
        weapon.clip_reload_time,
        FrameRange::new(LogicFrames::from_frames(30), LogicFrames::from_frames(90))
    );
}

#[test]
fn durations_round_up_to_whole_frames_of_the_authored_rate() {
    let document = parse(
        "Weapon Quick\n\
             \tDelayBetweenShots = Min:5 Max:10\n\
             \tPreAttackDelay = 0\n\
             \tClipReloadTime = Min:1500 Max:2000\n\
             \tDamageNugget\n\
             \t\tDamage = 1\n\
             \t\tDamageType = PIERCE\n\
             \tEnd\n\
             End\n",
    );
    let mut data = CombatData::new();
    data.authored_logic_hz = core::num::NonZeroU32::new(5);
    data.load_document(&document, &MacroTable::new());
    let weapon = data.weapons.get(data.weapons.id("Quick").unwrap()).unwrap();
    assert_eq!(
        (
            weapon.delay_between_shots,
            weapon.pre_attack_delay,
            weapon.clip_reload_time
        ),
        (
            FrameRange::fixed(LogicFrames::from_frames(6)),
            LogicFrames::ZERO,
            FrameRange::new(LogicFrames::from_frames(48), LogicFrames::from_frames(60)),
        )
    );
}

#[test]
fn a_nugget_requiring_an_upgrade_is_marked_and_one_forbidding_it_is_not() {
    let data = load(
        "Weapon Bow\n\
             \tProjectileNugget\n\
             \t\tProjectileTemplateName = Arrow\n\
             \t\tWarheadTemplateName = Head\n\
             \t\tForbiddenUpgradeNames = Upgrade_FireArrows\n\
             \tEnd\n\
             \tProjectileNugget\n\
             \t\tProjectileTemplateName = FireArrow\n\
             \t\tWarheadTemplateName = FireHead\n\
             \t\tRequiredUpgradeNames = Upgrade_FireArrows\n\
             \tEnd\n\
             End\n",
    );
    let weapon = data.weapons.get(data.weapons.id("Bow").unwrap()).unwrap();
    assert_eq!(weapon.nuggets.len(), 2);
    assert!(!weapon.nuggets[0].is_gated());
    assert!(weapon.nuggets[1].is_gated());
}

#[test]
fn resolved_filters_pick_the_arrow_the_shooters_upgrades_select() {
    let mut data = load(
        "Weapon Bow\n\
             \tProjectileNugget\n\
             \t\tWarheadTemplateName = Head\n\
             \t\tForbiddenUpgradeNames = Upgrade_FireArrows\n\
             \tEnd\n\
             \tProjectileNugget\n\
             \t\tWarheadTemplateName = FireHead\n\
             \t\tRequiredUpgradeNames = Upgrade_FireArrows\n\
             \tEnd\n\
             \tDamageNugget\n\
             \t\tDamage = 1\n\
             \t\tDamageType = PIERCE\n\
             \tEnd\n\
             End\n",
    );
    let bow = data.weapons.id("Bow").unwrap();
    let none = UpgradeMask::EMPTY;
    let mut fire = UpgradeMask::EMPTY;
    fire.set(UpgradeBit::new(9));
    assert!(data.nugget_passes(bow, 0, &none));
    assert!(
        !data.nugget_passes(bow, 1, &fire),
        "unresolved: gated means never"
    );

    data.resolve_upgrade_filters(|name| {
        name.eq_ignore_ascii_case("Upgrade_FireArrows")
            .then_some(UpgradeBit::new(9))
    });
    assert!(data.nugget_passes(bow, 0, &none));
    assert!(!data.nugget_passes(bow, 1, &none));
    assert!(!data.nugget_passes(bow, 0, &fire));
    assert!(data.nugget_passes(bow, 1, &fire));
    assert!(
        data.nugget_passes(bow, 2, &fire),
        "an unfiltered nugget always runs"
    );
    assert_eq!(data.filters.len(), 2);
}

#[test]
fn accept_damage_add_is_read_and_defaults_to_yes() {
    let data = load(
        "Weapon AddTest
         	DamageNugget
		Damage = 1
		DamageType = SLASH
		AcceptDamageAdd = No
	End
         	DamageNugget
		Damage = 1
		DamageType = SLASH
	End
         	DOTNugget
		Damage = 1
		DamageType = POISON
		AcceptDamageAdd = No
	End
         	DOTNugget
		Damage = 1
		DamageType = POISON
	End
         End
",
    );
    let weapon = data
        .weapons
        .get(data.weapons.id("AddTest").unwrap())
        .unwrap();
    let accepts: Vec<bool> = weapon
        .nuggets
        .iter()
        .map(|nugget| match nugget {
            Nugget::Damage(damage) => damage.accepts_add,
            Nugget::Dot(dot) => dot.accepts_add,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(accepts, vec![false, true, false, true]);
}

#[test]
fn a_spell_weapon_loads_its_affects_fx_scalars_and_new_nuggets() {
    let data = load(
        "Weapon RainOfFirePhaseOneWeapon\n\
             \tRadiusDamageAffects = ENEMIES NEUTRALS\n\
             \tDamageDealtAtSelfPosition = Yes\n\
             \tFireFX = FX_RainOfFirePhaseOne\n\
             \tDamageNugget\n\
             \t\tDamage = 7250\n\t\tDamageScalar = 50% NONE +COMMANDCENTER\n\
             \t\tRadius = 200\n\t\tDamageType = SIEGE\n\
             \tEnd\n\
             \tDOTNugget\n\
             \t\tDamage = 5\n\t\tRadius = 50\n\t\tDamageInterval = 500\n\
             \t\tDamageDuration = 3000\n\t\tDamageType = FLAME\n\
             \tEnd\n\
             \tAttributeModifierNugget\n\
             \t\tAttributeModifier = SunFlareTrollToStone\n\t\tRadius = 150\n\
             \tEnd\n\
             End\n\
             Weapon Plain\n\tDamageNugget\n\t\tDamage = 1\n\t\tDamageType = SLASH\n\tEnd\nEnd\n",
    );
    let id = data.weapons.id("RainOfFirePhaseOneWeapon").unwrap();
    let weapon = data.weapons.get(id).unwrap();
    assert_eq!(
        weapon.radius_affects,
        RadiusAffects::parse(&["ENEMIES", "NEUTRALS"])
    );
    assert!(weapon.damage_at_self);
    assert_eq!(weapon.fire_fx.as_deref(), Some("FX_RainOfFirePhaseOne"));
    assert_eq!(data.report.unresolved_fields, 0, "the scalar is a rule now");
    let rules = data.victims.get(id.index(), 0).unwrap();
    assert!((rules.scalars[0].0 - 0.5).abs() < 1e-6);
    let Nugget::Dot(dot) = &weapon.nuggets[1] else {
        panic!("{:?}", weapon.nuggets[1]);
    };
    assert_eq!(dot.interval, LogicFrames::from_frames(15));
    assert_eq!(dot.duration, LogicFrames::from_frames(90));
    let Nugget::Modifier(modifier) = &weapon.nuggets[2] else {
        panic!("{:?}", weapon.nuggets[2]);
    };
    assert_eq!(modifier.list, "SunFlareTrollToStone");
    assert_eq!(data.report.modelled_nuggets, 4);
    let plain = data.weapons.get(data.weapons.id("Plain").unwrap()).unwrap();
    assert_eq!(plain.radius_affects, RadiusAffects::default());
    assert!(!plain.damage_at_self && plain.fire_fx.is_none());
}

#[test]
fn the_seven_round_2b_nugget_kinds_are_read() {
    let data = load(
        "Weapon Ocl\n\tWeaponOCLNugget\n\t\tWeaponOCLName = OCL_FiresOfDoom\n\tEnd\nEnd\n\
             Weapon Fire\n\
             \tFireLogicNugget\n\t\tLogicType = INCREASE_BURN_RATE\n\
             \t\tRadius = 10\n\t\tDamage = 5\n\tEnd\n\
             End\n\
             Weapon Horde\n\tHordeAttackNugget\n\t\tLockWeaponSlot = TERTIARY\n\tEnd\nEnd\n\
             Weapon Slave\n\tSlaveAttackNugget\n\tEnd\nEnd\n\
             Weapon Sting\n\
             \tParalyzeNugget\n\t\tRadius = 0\n\t\tDuration = 4000\n\
             \t\tSpecialObjectFilter = CRIPPLE_STRIKE_OBJECT_FILTER\n\
             \t\tParalyzeFX = FX_PoisonedStunEffect\n\tEnd\n\
             End\n\
             Weapon Grab\n\
             \tGrabNugget\n\t\tSpecialObjectFilter = NONE +INFANTRY -HERO\n\
             \t\tContainTargetOnEffect = Yes\n\t\tRemoveTargetFromOtherContain = Yes\n\
             \t\tImpactTargetOnEffect = Yes\n\t\tShockWaveAmount = 50.0\n\
             \t\tShockWaveRadius = 16.0\n\t\tShockWaveTaperOff = 0.75\n\
             \t\tShockWaveZMult = 1.1\n\tEnd\n\
             End\n\
             Weapon Mine\n\
             \tSpawnAndFadeNugget\n\t\tObjectTargetFilter = ANY +STRUCTURE\n\
             \t\tSpawnedObjectName = IsengardDeployedExplosiveMine\n\
             \t\tSpawnOffset = X:8 Y:1 Z:0\n\tEnd\n\
             End\n",
    );
    let effect = |name: &str| {
        let id = data.weapons.id(name).unwrap();
        assert!(matches!(
            data.weapons.get(id).unwrap().nuggets[0],
            Nugget::Unmodelled(_)
        ));
        data.effects.get(id.index(), 0).unwrap().clone()
    };
    let EffectNugget::WeaponOcl(nugget) = effect("Ocl") else {
        panic!()
    };
    assert_eq!(nugget.list, "OCL_FiresOfDoom");

    let EffectNugget::FireLogic(nugget) = effect("Fire") else {
        panic!()
    };
    assert_eq!(nugget.logic_type, "INCREASE_BURN_RATE");
    assert_eq!(nugget.radius, 10.0);
    assert_eq!(nugget.damage, 5.0);

    let EffectNugget::HordeAttack(nugget) = effect("Horde") else {
        panic!()
    };
    assert_eq!(nugget.lock_weapon_slot.as_deref(), Some("TERTIARY"));

    assert!(matches!(effect("Slave"), EffectNugget::SlaveAttack));

    let EffectNugget::Paralyze(nugget) = effect("Sting") else {
        panic!()
    };
    assert_eq!(nugget.duration, LogicFrames::from_frames(120));
    assert_eq!(
        nugget.special_object_filter,
        vec!["CRIPPLE_STRIKE_OBJECT_FILTER"]
    );
    assert_eq!(nugget.fx.as_deref(), Some("FX_PoisonedStunEffect"));

    let EffectNugget::Grab(nugget) = effect("Grab") else {
        panic!()
    };
    assert!(nugget.contain_target_on_effect);
    assert!(nugget.remove_target_from_other_contain);
    assert!(nugget.impact_target_on_effect);
    assert_eq!(nugget.shock_wave_amount, 50.0);

    let EffectNugget::SpawnAndFade(nugget) = effect("Mine") else {
        panic!()
    };
    assert_eq!(nugget.spawned_object, "IsengardDeployedExplosiveMine");
    assert_eq!(nugget.offset, [8.0, 1.0, 0.0]);

    assert_eq!(data.report.modelled_nuggets, 7);
    assert_eq!(data.effects.len(), 7);
}

#[test]
fn a_nuggets_damage_fx_type_overrides_the_weapons() {
    let data = load(
        "Weapon Torch
	DamageFXType = FLAME
	DamageNugget
		Damage = 10
             		DamageType = FLAME
		DamageFXType = SWORD_SLASH
	End
             	DamageNugget
		Damage = 5
		DamageType = FLAME
	End
End
",
    );
    let id = data.weapons.id("Torch").unwrap();
    let sword = data.damage_fx_type_names.lookup("SWORD_SLASH");
    let flame = data.damage_fx_type_names.lookup("FLAME");
    assert_eq!(data.damage_fx_types.for_hit(id.index(), 0), sword);
    assert_eq!(data.damage_fx_types.for_hit(id.index(), 1), flame);
}

#[test]
fn an_unmodelled_nugget_keeps_its_keyword() {
    let data = load(
        "Weapon Odd\n\
             \tLuaEventNugget\n\
             \t\tLuaEvent = BeUncontrollablyAfraid\n\
             \tEnd\n\
             End\n",
    );
    let weapon = data.weapons.get(data.weapons.id("Odd").unwrap()).unwrap();
    assert_eq!(weapon.nuggets.len(), 1);
    assert_eq!(weapon.nuggets[0].keyword(), "LuaEventNugget");
    assert_eq!(data.report.nuggets, 1);
    assert_eq!(data.report.modelled_nuggets, 0);
    assert_eq!(data.report.weapons_without_effect, 1);
}

#[test]
fn a_damage_nugget_without_a_damage_type_does_not_invent_one() {
    let data = load(
        "Weapon Vague\n\
             \tDamageNugget\n\
             \t\tDamage = 10\n\
             \tEnd\n\
             End\n",
    );
    let weapon = data.weapons.get(data.weapons.id("Vague").unwrap()).unwrap();
    assert_eq!(weapon.nuggets[0].keyword(), "DamageNugget");
    assert!(matches!(weapon.nuggets[0], Nugget::Unmodelled(_)));
    assert!(data.damage_types.is_empty());
}

#[test]
fn hostile_numbers_load_without_panicking_and_without_absurd_durations() {
    let data = load(
        "Weapon Hostile\n\
             \tAttackRange = -1e30\n\
             \tDelayBetweenShots = -5000\n\
             \tPreAttackDelay = 99999999999999\n\
             \tClipSize = -3\n\
             \tClipReloadTime = Min:-1 Max:2\n\
             \tWeaponSpeed = 1e40\n\
             \tDamageNugget\n\
             \t\tDamage = 1e40\n\
             \t\tDamageType = PIERCE\n\
             \t\tRadius = -50\n\
             \t\tDamageScalar = -1\n\
             \t\tDelayTime = -1\n\
             \tEnd\n\
             End\n",
    );
    let weapon = data
        .weapons
        .get(data.weapons.id("Hostile").unwrap())
        .unwrap();
    assert_eq!(weapon.delay_between_shots, FrameRange::ZERO);
    assert_eq!(
        weapon.clip_reload_time,
        FrameRange::new(LogicFrames::ZERO, LogicFrames::from_frames(1))
    );
    assert_eq!(weapon.clip_size, 0);
    assert_eq!(
        weapon.pre_attack_delay,
        LogicFrames::from_engine_milliseconds(86_400_000)
    );
    assert!(weapon.attack_range.is_finite());
    assert!(weapon.weapon_speed.is_finite());
}

#[test]
fn a_block_with_no_name_is_counted_rather_than_stored() {
    let data = load("Weapon\n\tAttackRange = 10\nEnd\n");
    assert_eq!(data.report.unnamed_blocks, 1);
    assert!(data.weapons.is_empty());
}

#[test]
fn a_wrong_typed_hit_field_is_refused() {
    for (field, refused) in [
        ("HitPercentage = 70", false),
        ("HitPercentage = 70%", false),
        ("HitPercentage = often", true),
        ("HitPercentage = -5", true),
        ("CanBeDodged = Maybe", true),
        ("PreAttackRandomAmount = -200", true),
        ("ScatterRadius = wide", true),
        (
            "ProjectileCollidesWith = ENEMIES NEUTRAL STRUCTURES WALLS",
            false,
        ),
        ("ProjectileCollidesWith = ENEMIES NEUTRALS", true),
        ("RangeBonusPerFoot = #MULTIPLY( 300 .01 )", false),
        ("RangeBonus = high", true),
        ("ProjectileFilterInContainer = NONE +INFANTRY", false),
        (
            "ProjectileFilterInContainer = ANY +ROCK -HERO ENEMIES",
            false,
        ),
        ("ProjectileFilterInContainer = INFANTRY", true),
        ("ProjectileFilterInContainer = NONE +", true),
    ] {
        let data = load(&format!("Weapon Bow\n\t{field}\nEnd\n"));
        assert_eq!(data.value_errors.len() == 1, refused, "{field}");
        if !refused && field.starts_with("Hit") {
            let id = data.weapons.id("Bow").unwrap();
            assert!((data.weapons.get(id).unwrap().hit_chance - 0.7).abs() < 1e-6);
        }
    }
}
