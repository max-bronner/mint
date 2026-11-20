use super::*;

pub(super) fn scene(
    data: &str,
    spawns: &[(&str, Vec2, PlayerId)],
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_on(data, spawns, false)
}

pub(super) fn scene_on(
    data: &str,
    spawns: &[(&str, Vec2, PlayerId)],
    snowy: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names: Vec<String> = spawns.iter().map(|(name, ..)| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawned = spawns
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawned)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap)
        .with_players(2)
        .with_map_snowy(snowy);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

pub(super) fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

pub(super) fn damage_mult(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(1.0, |bonuses| bonuses.0.damage_mult)
}

pub(super) fn holds(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    entity: Entity,
    list: &str,
) -> bool {
    let id = simulation.rules().combat.modifiers.id(list).unwrap();
    simulation
        .state()
        .get(ids.timed_modifiers, entity)
        .is_some_and(|timed| timed.0.iter().any(|(held, _)| *held == id))
}

fn aura(name: &str, list: &str, extra: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
         Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
         Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
         BonusName = {list}\n    RefreshDelay = 2000\n    ObjectFilter = ANY +INFANTRY\n    \
         {extra}\n  End\n  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  \
         GeometryHeight = 20\nEnd\n"
    )
}

const SOLDIER: &str = "\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ExperienceLevel SoldierLevel1\n  TargetNames = Soldier\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel SoldierLevel2\n  TargetNames = Soldier\n  RequiredExperience = 5\n  Rank = 2\nEnd\n";

fn list(name: &str, category: &str, mult: u32, ms: u32, flags: &str) -> String {
    format!(
        "ModifierList {name}\n  Category = {category}\n  Modifier = DAMAGE_MULT {mult}%\n  \
         Duration = {ms}\n  {flags}\nEnd\n"
    )
}

fn data(lists: &[String], auras: &[String]) -> String {
    format!("{SOLDIER}{}{}", lists.concat(), auras.concat())
}

const AT: Vec2 = Vec2::new(100.0, 100.0);

#[test]
fn a_longer_list_of_the_same_category_replaces_the_held_one_and_a_shorter_is_refused() {
    let flag = "ReplaceInCategoryIfLongest = Yes";
    let text = data(
        &[
            list("Short", "LEADERSHIP", 150, 3000, flag),
            list("Long", "LEADERSHIP", 200, 9000, flag),
        ],
        &[
            aura("AuraShort", "Short", "Range = 100"),
            aura("AuraLong", "Long", "Range = 100"),
        ],
    );
    let (mut simulation, ids, e) = scene(
        &text,
        &[
            ("AuraShort", AT, PLAYER),
            ("AuraLong", AT, PLAYER),
            ("Soldier", AT, PLAYER),
        ],
    );
    run(&mut simulation, 3);
    assert_eq!(
        damage_mult(&simulation, ids, e[2]),
        2.0,
        "Long replaced Short"
    );
    run(&mut simulation, 100);
    assert_eq!(
        damage_mult(&simulation, ids, e[2]),
        2.0,
        "Short ends before Long and stays out"
    );
}

#[test]
fn a_list_ending_with_the_held_one_in_its_category_is_refused() {
    let flag = "ReplaceInCategoryIfLongest = Yes";
    let text = data(
        &[
            list("TieA", "SPELL", 150, 3000, flag),
            list("TieB", "SPELL", 200, 3000, flag),
        ],
        &[
            aura("AuraA", "TieA", "Range = 100"),
            aura("AuraB", "TieB", "Range = 100"),
        ],
    );
    let (mut simulation, ids, e) = scene(
        &text,
        &[
            ("AuraA", AT, PLAYER),
            ("AuraB", AT, PLAYER),
            ("Soldier", AT, PLAYER),
        ],
    );
    run(&mut simulation, 3);
    assert_eq!(damage_mult(&simulation, ids, e[2]), 1.5);
}

fn anti_scene() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let text = data(
        &[
            list("Curse", "DEBUFF", 50, 3000, ""),
            list("Plain", "BUFF", 150, 3000, ""),
            list(
                "Guarded",
                "BUFF",
                120,
                3000,
                "IgnoreIfAnticategoryActive = Yes",
            ),
        ],
        &[
            aura(
                "AuraAnti",
                "Curse",
                "Range = 30\n    AntiCategory = BUFF\n    TargetEnemy = No",
            ),
            aura("AuraPlain", "Plain", "Range = 100"),
            aura("AuraGuarded", "Guarded", "Range = 100"),
        ],
    );
    scene(
        &text,
        &[
            ("AuraAnti", AT, PLAYER),
            ("AuraPlain", AT, PLAYER),
            ("AuraGuarded", AT, PLAYER),
            ("Soldier", AT, PLAYER),
            ("Soldier", Vec2::new(100.0, 180.0), PLAYER),
        ],
    )
}

#[test]
fn an_aura_anti_category_switches_the_recipients_lists_of_that_category_off() {
    let (mut simulation, ids, e) = anti_scene();
    run(&mut simulation, 3);
    assert_eq!(
        damage_mult(&simulation, ids, e[3]),
        0.5,
        "only the curse counts"
    );
    assert!(
        (damage_mult(&simulation, ids, e[4]) - 1.8).abs() < 1e-5,
        "out of the anti aura both BUFF lists count"
    );
}

#[test]
fn a_list_that_ignores_an_active_anti_category_is_not_given_by_an_aura() {
    let (mut simulation, ids, e) = anti_scene();
    run(&mut simulation, 3);
    assert!(!holds(&simulation, ids, e[3], "Guarded"));
    assert!(
        holds(&simulation, ids, e[3], "Plain"),
        "unflagged lists are still given"
    );
    assert!(holds(&simulation, ids, e[4], "Guarded"));
}

#[test]
fn an_aura_with_a_max_active_rank_skips_objects_above_it() {
    let text = data(
        &[list("Ranked", "BUFF", 150, 3000, "")],
        &[aura(
            "AuraRank",
            "Ranked",
            "Range = 100\n    MaxActiveRank = 1",
        )],
    );
    let (mut simulation, ids, e) = scene(
        &text,
        &[
            ("AuraRank", AT, PLAYER),
            ("Soldier", AT, PLAYER),
            ("Soldier", AT, PLAYER),
        ],
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![e[2]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 10 })
        .unwrap();
    run(&mut simulation, 5);
    assert_eq!(damage_mult(&simulation, ids, e[1]), 1.5, "rank 1");
    assert_eq!(damage_mult(&simulation, ids, e[2]), 1.0, "rank 2");
}

#[test]
fn an_aura_gift_lasts_the_lists_own_duration_after_the_source_is_gone() {
    let beacon = "Object Beacon\n  Side = Men\n  KindOf = IMMOBILE UNATTACKABLE\n  \
        Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
        BonusName = Linger\n    RefreshDelay = 100\n    Range = 100\n    \
        ObjectFilter = ANY +INFANTRY\n  End\n  \
        Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 1000\n    MaxLifetime = 1000\n  End\nEnd\n"
        .to_owned();
    let text = data(&[list("Linger", "BUFF", 150, 3000, "")], &[beacon]);
    let (mut simulation, ids, e) = scene(&text, &[("Beacon", AT, PLAYER), ("Soldier", AT, PLAYER)]);
    run(&mut simulation, 100);
    assert_eq!(
        simulation.state().get(ids.owner, e[0]).map(|_| ()),
        None,
        "the beacon is gone by now"
    );
    assert_eq!(damage_mult(&simulation, ids, e[1]), 1.5, "still held");
    run(&mut simulation, 40);
    assert_eq!(
        damage_mult(&simulation, ids, e[1]),
        1.0,
        "ended with the list"
    );
}

#[test]
fn a_wall_aura_does_not_reach_a_unit_on_the_ground() {
    let wall = "Object Wall\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE WALK_ON_TOP_OF_WALL\n  \
        Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
        Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
        BonusName = WallBonus\n    RefreshDelay = 2000\n    ObjectFilter = ANY +INFANTRY\n  End\n  \
        Geometry = CYLINDER\n  GeometryMajorRadius = 60\n  GeometryHeight = 20\nEnd\n"
        .to_owned();
    let text = data(&[list("WallBonus", "BUFF", 150, 3000, "")], &[wall]);
    let (mut simulation, ids, e) = scene(
        &text,
        &[
            ("Wall", AT, PLAYER),
            ("Soldier", Vec2::new(130.0, 100.0), PLAYER),
        ],
    );
    run(&mut simulation, 5);
    assert_eq!(damage_mult(&simulation, ids, e[1]), 1.0);
}

fn condition(simulation: &Simulation<RotwkRules>, entity: Entity, name: &str) -> bool {
    simulation
        .rules()
        .holds_condition_in(simulation.state(), entity, name)
}

fn looks_data(lists: &[&str]) -> (String, Vec<(String, Vec2, PlayerId)>) {
    let mut auras = Vec::new();
    let mut spawns = Vec::new();
    for (index, name) in lists.iter().enumerate() {
        let object = format!("Aura{index}");
        auras.push(aura(&object, name, "Range = 100"));
        spawns.push((object, AT, PLAYER));
    }
    let text = format!(
        "{SOLDIER}Upgrade Upgrade_Drafted\n  Type = OBJECT\nEnd\n{}{}",
        [
            "ModifierList Raise\n  Category = SPELL\n  Modifier = DAMAGE_MULT 110%\n  Duration = 1000\n  \
             ModelCondition = USER_3\nEnd\n",
            "ModifierList Seed\n  Category = BUFF\n  Modifier = DAMAGE_MULT 110%\n  Duration = 0\n  \
             ModelCondition = USER_4\nEnd\n",
            "ModifierList Stop\n  Category = DEBUFF\n  Modifier = DAMAGE_MULT 110%\n  Duration = 1000\n  \
             ClearModelCondition = USER_4\nEnd\n",
            "ModifierList Glow\n  Category = LEADERSHIP\n  Modifier = DAMAGE_MULT 110%\n  Duration = 1000\n  \
             FX = FX_Glow\n  EndFX = FX_GlowEnd\nEnd\n",
            "ModifierList Draft\n  Category = FORMATION\n  Modifier = DAMAGE_MULT 110%\n  Duration = 0\n  \
             Upgrade = Upgrade_Drafted Delay:1000\nEnd\n",
        ]
        .concat(),
        auras.concat()
    );
    (text, spawns)
}

fn looks_scene(lists: &[&str]) -> (Simulation<RotwkRules>, Ids, Entity) {
    let (text, spawns) = looks_data(lists);
    let mut placed: Vec<(&str, Vec2, PlayerId)> = spawns
        .iter()
        .map(|(name, at, owner)| (name.as_str(), *at, *owner))
        .collect();
    placed.push(("Soldier", AT, PLAYER));
    let (simulation, ids, entities) = scene(&text, &placed);
    let soldier = *entities.last().unwrap();
    (simulation, ids, soldier)
}

#[test]
fn a_lists_model_condition_is_set_at_its_gain_and_cleared_at_its_end() {
    let (mut simulation, _ids, soldier) = looks_scene(&["Raise"]);
    run(&mut simulation, 3);
    assert!(condition(&simulation, soldier, "USER_3"), "set at the gain");
    run(&mut simulation, 40);
    assert!(
        !condition(&simulation, soldier, "USER_3"),
        "cleared when the list ended, before the next pulse"
    );
}

#[test]
fn a_lists_clear_model_condition_clears_what_is_set() {
    let (mut simulation, _ids, soldier) = looks_scene(&["Seed", "Stop"]);
    run(&mut simulation, 3);
    assert!(!condition(&simulation, soldier, "USER_4"));
}

#[test]
fn a_lists_fx_plays_when_given_and_its_end_fx_when_it_ends() {
    let (mut simulation, _ids, _soldier) = looks_scene(&["Glow"]);
    let mut played: Vec<String> = Vec::new();
    for _ in 0..45 {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            played.extend(simulation.rules().fx_name(event.list).map(str::to_owned));
        }
    }
    assert!(played.iter().any(|name| name == "FX_Glow"), "{played:?}");
    assert!(played.iter().any(|name| name == "FX_GlowEnd"), "{played:?}");
}

#[test]
fn a_lists_upgrade_is_given_after_its_delay() {
    let (mut simulation, ids, soldier) = looks_scene(&["Draft"]);
    let bit = simulation
        .rules()
        .upgrade_bits
        .bit("Upgrade_Drafted")
        .unwrap();
    let held = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.object_upgrades, soldier)
            .is_some_and(|held| held.0.contains(bit))
    };
    run(&mut simulation, 5);
    assert!(!held(&simulation), "not yet");
    run(&mut simulation, 40);
    assert!(held(&simulation), "after the delay");
}
