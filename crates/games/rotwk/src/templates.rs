use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, MotionLimits, Vec2};
use mint_sage_asset::KindOf;
use mint_sage_ini::{CommandButtonDef, CommandButtonIndex};
use mint_sage_modules::{ObjectRecord, UpgradeBits};
use mint_sim::{LogicFrames, PlayerId, UpgradeMask};

use crate::defeat::ObjectFilter;
use crate::economy::{Buildable, Output, UpgradeBuildable};
use crate::game_data::GameData;
use crate::unit::{Kind, Kinematics, ProducerSpawn, Shape, Spawn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetKind {
    None,
    Position,
    Object,
    Confirm,
}

pub(crate) fn target_kind(options: &[String]) -> TargetKind {
    let has = |option: &str| options.iter().any(|o| o.eq_ignore_ascii_case(option));
    if has("NEED_TARGET_POS") {
        TargetKind::Position
    } else if options
        .iter()
        .any(|o| o.to_ascii_uppercase().starts_with("NEED_TARGET_") && o.ends_with("OBJECT"))
    {
        TargetKind::Object
    } else if has("CONTEXTMODE_COMMAND") {
        TargetKind::Confirm
    } else {
        TargetKind::None
    }
}

pub(crate) fn target_relation(options: &[String]) -> ObjectFilter {
    let has = |option: &str| options.iter().any(|o| o.eq_ignore_ascii_case(option));
    let mut tokens: Vec<String> = Vec::new();
    if has("NEED_TARGET_ENEMY_OBJECT") {
        tokens.push(String::from("ENEMIES"));
    }
    if has("NEED_TARGET_ALLY_OBJECT") {
        tokens.push(String::from("ALLIES"));
    }
    if has("NEED_TARGET_NEUTRAL_OBJECT") {
        tokens.push(String::from("NEUTRAL"));
    }
    ObjectFilter::parse(&tokens)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Needs(Option<Arc<CommandButtonDef>>);

impl Needs {
    pub(crate) fn of(buttons: &CommandButtonIndex, name: &str) -> Self {
        Self(
            buttons
                .get(name)
                .filter(|button| !button.needs_met(|_| false))
                .map(|button| Arc::new(button.clone())),
        )
    }

    pub(crate) fn met(&self, held: &UpgradeMask, bits: &UpgradeBits) -> bool {
        self.0.as_ref().is_none_or(|button| {
            button.needs_met(|name| bits.bit(name).is_some_and(|bit| held.contains(bit)))
        })
    }
}

pub(crate) const FALLBACK_RADIUS: f32 = 5.0;
pub(crate) const FALLBACK_HEIGHT: f32 = 18.0;

pub(crate) fn pick_shape(
    kind_of: KindOf,
    radius: Option<f32>,
    height: Option<f32>,
    geometry_is_small: bool,
) -> Shape {
    Shape {
        kind_of,
        radius: radius
            .filter(|radius| *radius > 0.0 && *radius <= MAX_RADIUS)
            .unwrap_or(FALLBACK_RADIUS),
        height: height
            .filter(|height| *height > 0.0 && *height <= MAX_RADIUS)
            .unwrap_or(FALLBACK_HEIGHT),
        geometry_is_small,
    }
}

const MAX_RADIUS: f32 = 2_000.0;
const MAX_HEALTH: f32 = 10_000_000.0;
pub(crate) const MAX_BUILD_SECONDS: f32 = 86_400.0;
const MAX_WHOLE: f32 = 1_000_000.0;
const MAX_DEPOSIT_MS: f32 = 86_400_000.0;
const MAX_QUEUE: f32 = 64.0;

pub(crate) fn queue_depth(record: &ObjectRecord) -> u32 {
    record
        .max_queue_entries
        .filter(|depth| *depth >= 1.0 && *depth <= MAX_QUEUE)
        .map_or(crate::rules::DEFAULT_MAX_QUEUE_ENTRIES, |depth| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let depth = depth as u32;
            depth
        })
}

#[derive(Debug, Clone)]
pub(crate) struct KindEntry {
    pub(crate) record: ObjectRecord,
    pub(crate) formation: Option<u16>,
    pub(crate) member: Option<Kind>,
    pub(crate) build_variants: Vec<Kind>,
    pub(crate) builds: Vec<Kind>,
    pub(crate) build_needs: Vec<Needs>,
    pub(crate) constructs: Vec<Kind>,
    pub(crate) foundation_builds: Vec<Kind>,
    pub(crate) foundation_needs: Vec<Needs>,
    pub(crate) command_set_variants: Vec<CommandSetVariant>,
    pub(crate) power_buttons: Vec<(String, TargetKind)>,
    pub(crate) power_needs: Vec<Needs>,
    pub(crate) auto_power: Vec<AutoButton>,
    pub(crate) revive_buttons: Vec<Needs>,
    pub(crate) spell_buttons: Vec<SpellButton>,
    pub(crate) weapon_toggles: Vec<(String, Vec<String>)>,
    pub(crate) limits: Option<Kinematics>,
    pub(crate) movement_penalty: f32,
    pub(crate) flight: Option<FlightProfile>,
    pub(crate) locomotor_sets: Vec<(String, Kinematics)>,
    pub(crate) burning_run: Option<BurningRun>,
    pub(crate) mover: Option<mint_movement::MoverClass>,
    pub(crate) surfaces: Option<u16>,
    pub(crate) facts: KindFacts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct KindFacts {
    pub(crate) infantry: bool,
    pub(crate) heavy_melee_hitter: bool,
    pub(crate) path_through_infantry: bool,
    pub(crate) path_through_each_other: bool,
    pub(crate) submarine: bool,
    pub(crate) large_rectangle_pathfind: bool,
    pub(crate) hero: bool,
    pub(crate) dozer: bool,
    pub(crate) cavalry: bool,
    pub(crate) large_footprint: bool,
}

impl KindFacts {
    pub(crate) fn of(record: &ObjectRecord) -> Self {
        Self {
            infantry: record.has_kind("INFANTRY"),
            heavy_melee_hitter: record.has_kind("HEAVY_MELEE_HITTER"),
            path_through_infantry: record.has_kind("PATH_THROUGH_INFANTRY"),
            path_through_each_other: record.has_kind("PATH_THROUGH_EACH_OTHER"),
            submarine: record.has_kind("SUBMARINE"),
            large_rectangle_pathfind: record.has_kind("LARGE_RECTANGLE_PATHFIND"),
            hero: record.has_kind("HERO"),
            dozer: record.has_kind("DOZER"),
            cavalry: record.has_kind("CAVALRY"),
            large_footprint: record
                .kind_of
                .iter()
                .any(|flag| flag == "MONSTER" || flag == "HORDE"),
        }
    }
}

pub(crate) const SURFACE_TABLE: [&str; 9] = [
    "GROUND",
    "WATER",
    "CLIFF",
    "AIR",
    "RUBBLE",
    "OBSTACLE",
    "IMPASSABLE",
    "DEEP_WATER",
    "WALL_RAILING",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpellButton {
    pub(crate) power: String,
    pub(crate) target: TargetKind,
    pub(crate) passive: bool,
    pub(crate) shown: bool,
    pub(crate) button: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandSetVariant {
    pub(crate) command_set: String,
    pub(crate) builds: Vec<Kind>,
    pub(crate) build_needs: Vec<Needs>,
    pub(crate) power_buttons: Vec<(String, TargetKind)>,
    pub(crate) power_needs: Vec<Needs>,
    pub(crate) revive_buttons: Vec<Needs>,
    pub(crate) auto_power: Vec<AutoButton>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AutoButton {
    pub(crate) power: String,
    pub(crate) target: TargetKind,
    pub(crate) auto_delay: Option<f32>,
    pub(crate) disallowed: Vec<String>,
    pub(crate) relation: ObjectFilter,
    pub(crate) preset_range: Option<f32>,
    pub(crate) need_damaged_target: bool,
    pub(crate) enable_on_model_condition: Vec<String>,
    pub(crate) disable_on_model_condition: Vec<String>,
}

impl KindEntry {
    pub(crate) fn power_buttons_under(&self, set: Option<&str>) -> &[(String, TargetKind)] {
        set.and_then(|set| {
            self.command_set_variants
                .iter()
                .find(|variant| variant.command_set.eq_ignore_ascii_case(set))
        })
        .map_or(&self.power_buttons, |variant| &variant.power_buttons)
    }

    fn variant(&self, set: Option<&str>) -> Option<&CommandSetVariant> {
        set.and_then(|set| {
            self.command_set_variants
                .iter()
                .find(|variant| variant.command_set.eq_ignore_ascii_case(set))
        })
    }

    pub(crate) fn power_needs_under(&self, set: Option<&str>) -> &[Needs] {
        self.variant(set)
            .map_or(&self.power_needs, |variant| &variant.power_needs)
    }

    pub(crate) fn auto_power_under(&self, set: Option<&str>) -> &[AutoButton] {
        self.variant(set)
            .map_or(&self.auto_power, |variant| &variant.auto_power)
    }

    pub(crate) fn revive_buttons_under(&self, set: Option<&str>) -> &[Needs] {
        self.variant(set)
            .map_or(&self.revive_buttons, |variant| &variant.revive_buttons)
    }
}

fn power_set_of(
    set: &str,
    command_sets: &mint_sage_ini::CommandSetIndex,
    command_buttons: &mint_sage_ini::CommandButtonIndex,
) -> (Vec<(String, TargetKind)>, Vec<Needs>, Vec<AutoButton>) {
    let buttons = mint_sage_modules::special_power_buttons(set, command_sets, command_buttons);
    let auto_power = buttons
        .iter()
        .filter(|button| button.auto_ability)
        .map(|button| AutoButton {
            power: button.power.clone(),
            target: target_kind(&button.options),
            auto_delay: button.auto_delay,
            disallowed: button.auto_ability_disallowed_on_model_condition.clone(),
            relation: target_relation(&button.options),
            preset_range: button.preset_range,
            need_damaged_target: button.need_damaged_target,
            enable_on_model_condition: button.enable_on_model_condition.clone(),
            disable_on_model_condition: button.disable_on_model_condition.clone(),
        })
        .collect();
    let (power_buttons, power_needs) = buttons
        .into_iter()
        .map(|button| {
            let needs = Needs::of(command_buttons, &button.button);
            ((button.power, target_kind(&button.options)), needs)
        })
        .unzip();
    (power_buttons, power_needs, auto_power)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct KindTable {
    entries: Vec<KindEntry>,
    by_name: BTreeMap<String, u16>,
    power_button_index: BTreeMap<String, (String, TargetKind)>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Production {
    pub(crate) buildables: Vec<Buildable>,
    pub(crate) menus: Vec<Vec<u16>>,
    pub(crate) menu_needs: Vec<Vec<Needs>>,
    pub(crate) menu_of: Vec<Option<u16>>,
    pub(crate) lines: Vec<String>,
    pub(crate) command_set_variants: Vec<(Kind, String, u16)>,
}

impl Production {
    pub(crate) fn variant_menu(&self, kind: Kind, set: &str) -> Option<u16> {
        self.command_set_variants
            .iter()
            .find(|(held, name, _)| *held == kind && name.eq_ignore_ascii_case(set))
            .map(|(_, _, menu)| *menu)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UpgradeMenus {
    pub(crate) upgrades: Vec<UpgradeBuildable>,
    pub(crate) object_menus: Vec<Vec<u16>>,
    pub(crate) player_menus: Vec<Vec<u16>>,
    pub(crate) object_needs: Vec<Vec<Needs>>,
    pub(crate) player_needs: Vec<Vec<Needs>>,
    pub(crate) object_menu_of: Vec<Option<u16>>,
    pub(crate) player_menu_of: Vec<Option<u16>>,
    pub(crate) variant_menus: Vec<(Kind, String, Option<u16>, Option<u16>)>,
    pub(crate) lines: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConstructMenus {
    pub(crate) menus: Vec<Vec<Kind>>,
    pub(crate) menu_of: Vec<Option<u16>>,
}

impl KindTable {
    pub(crate) fn closure(data: &GameData, seeds: &[String]) -> (Self, Vec<String>) {
        let power_button_index = data
            .command_buttons
            .iter()
            .filter(|(_, button)| {
                button.command.as_deref().is_some_and(|command| {
                    command.eq_ignore_ascii_case("SPECIAL_POWER")
                        || command.eq_ignore_ascii_case("SPECIAL_POWER_TOGGLE")
                })
            })
            .filter_map(|(name, button)| {
                let power = button.special_power.clone()?;
                Some((name.to_owned(), (power, target_kind(&button.options))))
            })
            .collect();
        let mut table = Self {
            power_button_index,
            ..Self::default()
        };
        let mut missing: Vec<String> = Vec::new();
        let mut queue: std::collections::VecDeque<String> = seeds.iter().cloned().collect();
        queue.extend(
            data.drag_formation
                .facing_arrow_templates
                .iter()
                .flatten()
                .cloned(),
        );
        while let Some(name) = queue.pop_front() {
            let key = name.to_ascii_lowercase();
            if table.by_name.contains_key(&key)
                || missing.iter().any(|m| m.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            let Some(record) = data.templates.get(&name) else {
                missing.push(name);
                continue;
            };
            let Ok(index) = u16::try_from(table.entries.len()) else {
                break;
            };
            let formation = data.formations.index_of(&name);
            if let Some(member) = formation.and_then(|at| data.formations.member(at)) {
                queue.push_back(member.to_owned());
            }
            if let Some(banner) = formation.and_then(|at| data.formations.banner(at)) {
                queue.push_back(banner.template.clone());
            }
            queue.extend(record.build_variations.iter().cloned());
            if let Some(set) = record.command_set.as_deref() {
                for build in
                    mint_sage_modules::unit_builds(set, &data.command_sets, &data.command_buttons)
                {
                    queue.push_back(build.object);
                }
                for construct in mint_sage_modules::dozer_constructs(
                    set,
                    &data.command_sets,
                    &data.command_buttons,
                ) {
                    queue.push_back(construct.object);
                }
                for construct in mint_sage_modules::foundation_constructs(
                    set,
                    &data.command_sets,
                    &data.command_buttons,
                ) {
                    queue.push_back(construct.object);
                }
            }
            for set in &record.upgraded_command_sets {
                for build in
                    mint_sage_modules::unit_builds(set, &data.command_sets, &data.command_buttons)
                {
                    queue.push_back(build.object);
                }
            }
            for list in record.creation_lists() {
                if let Some(def) = data.ocls.get(&list) {
                    for entry in &def.create_object {
                        queue.extend(entry.names.iter().cloned());
                    }
                }
            }
            for list in weapon_creation_lists(data, record) {
                if let Some(def) = data.ocls.get(&list) {
                    for entry in &def.create_object {
                        queue.extend(entry.names.iter().cloned());
                    }
                }
            }
            queue.extend(record.created_templates());
            if record.formation_preview_decal.is_none() {
                if let Some(class) = data.drag_formation.classify(&record.kind_of, &name) {
                    if let Some(fallback) = data
                        .drag_formation
                        .definitions
                        .get(class)
                        .and_then(|definition| definition.preview_object.clone())
                    {
                        queue.push_back(fallback);
                    }
                }
            }
            table.by_name.insert(key, index);
            let limits = motion_limits(record, &data.locomotors, NORMAL_CONDITION);
            let flight = flight_profile(record, &data.locomotors, NORMAL_CONDITION);
            let locomotor_sets = other_locomotor_sets(record, &data.locomotors);
            let burning_run = burning_run(record, &data.locomotors);
            let mover = mover_class(record, &data.locomotors, NORMAL_CONDITION);
            let surfaces = surface_mask(record, &data.locomotors, NORMAL_CONDITION);
            let thresholds =
                crate::render_state::DamageThresholds::of(Some(record), data.damage_thresholds);
            let movement_penalty = match data.movement_penalty_state {
                crate::combat::DamageRung::Pristine => f32::INFINITY,
                crate::combat::DamageRung::Damaged => thresholds.damaged,
                crate::combat::DamageRung::ReallyDamaged => thresholds.really,
                crate::combat::DamageRung::Rubble => 0.0,
            };
            table.entries.push(KindEntry {
                record: record.clone(),
                formation,
                member: None,
                build_variants: Vec::new(),
                builds: Vec::new(),
                build_needs: Vec::new(),
                constructs: Vec::new(),
                foundation_builds: Vec::new(),
                foundation_needs: Vec::new(),
                command_set_variants: Vec::new(),
                power_buttons: Vec::new(),
                power_needs: Vec::new(),
                revive_buttons: Vec::new(),
                auto_power: Vec::new(),
                spell_buttons: Vec::new(),
                weapon_toggles: Vec::new(),
                limits,
                movement_penalty,
                flight,
                locomotor_sets,
                burning_run,
                mover,
                surfaces,
                facts: KindFacts::of(record),
            });
        }
        for at in 0..table.entries.len() {
            let Some(entry) = table.entries.get(at) else {
                continue;
            };
            let member = entry
                .formation
                .and_then(|formation| data.formations.member(formation))
                .and_then(|name| table.kind(name));
            let build_variants: Vec<Kind> = entry
                .record
                .build_variations
                .iter()
                .filter_map(|name| table.kind(name))
                .collect();
            let builds_of = |set: &str| -> (Vec<Kind>, Vec<Needs>) {
                mint_sage_modules::unit_builds(set, &data.command_sets, &data.command_buttons)
                    .iter()
                    .filter_map(|build| {
                        Some((
                            table.kind(&build.object)?,
                            Needs::of(&data.command_buttons, &build.button),
                        ))
                    })
                    .unzip()
            };
            let (builds, build_needs) = entry
                .record
                .command_set
                .as_deref()
                .map(builds_of)
                .unwrap_or_default();
            let constructs: Vec<Kind> = entry
                .record
                .command_set
                .as_deref()
                .map(|set| {
                    mint_sage_modules::dozer_constructs(
                        set,
                        &data.command_sets,
                        &data.command_buttons,
                    )
                    .iter()
                    .filter_map(|construct| table.kind(&construct.object))
                    .collect()
                })
                .unwrap_or_default();
            let (foundation_builds, foundation_needs): (Vec<Kind>, Vec<Needs>) = entry
                .record
                .command_set
                .as_deref()
                .map(|set| {
                    mint_sage_modules::foundation_constructs(
                        set,
                        &data.command_sets,
                        &data.command_buttons,
                    )
                    .iter()
                    .filter_map(|build| {
                        Some((
                            table.kind(&build.object)?,
                            Needs::of(&data.command_buttons, &build.button),
                        ))
                    })
                    .unzip()
                })
                .unwrap_or_default();
            let powers_of =
                |set: &str| power_set_of(set, &data.command_sets, &data.command_buttons);
            let revives_of = |set: &str| -> Vec<Needs> {
                mint_sage_modules::revive_buttons(set, &data.command_sets, &data.command_buttons)
                    .iter()
                    .map(|name| Needs::of(&data.command_buttons, name))
                    .collect()
            };
            let command_set_variants: Vec<CommandSetVariant> = entry
                .record
                .upgraded_command_sets
                .iter()
                .map(|set| {
                    let (builds, build_needs) = builds_of(set);
                    let (power_buttons, power_needs, auto_power) = powers_of(set);
                    CommandSetVariant {
                        command_set: set.clone(),
                        builds,
                        build_needs,
                        power_buttons,
                        power_needs,
                        revive_buttons: revives_of(set),
                        auto_power,
                    }
                })
                .collect();
            let (power_buttons, power_needs, auto_power) = entry
                .record
                .command_set
                .as_deref()
                .map(powers_of)
                .unwrap_or_default();
            let revive_buttons = entry
                .record
                .command_set
                .as_deref()
                .map(revives_of)
                .unwrap_or_default();
            let spell_buttons: Vec<SpellButton> = entry
                .record
                .command_set
                .as_deref()
                .map(|set| {
                    mint_sage_modules::spell_book_buttons(
                        set,
                        &data.command_sets,
                        &data.command_buttons,
                    )
                    .into_iter()
                    .map(|button| SpellButton {
                        target: target_kind(&button.options),
                        passive: button
                            .options
                            .iter()
                            .any(|o| o.eq_ignore_ascii_case("NONPRESSABLE")),
                        shown: button.show_button,
                        power: button.power,
                        button: button.button,
                    })
                    .collect()
                })
                .unwrap_or_default();
            let mut weapon_toggles: Vec<(String, Vec<String>)> = Vec::new();
            for set in entry
                .record
                .command_set
                .iter()
                .chain(&entry.record.upgraded_command_sets)
            {
                for toggle in mint_sage_modules::weapon_toggle_buttons(
                    set,
                    &data.command_sets,
                    &data.command_buttons,
                ) {
                    if !weapon_toggles.iter().any(|(name, _)| *name == toggle.0) {
                        weapon_toggles.push(toggle);
                    }
                }
            }
            if let Some(entry) = table.entries.get_mut(at) {
                entry.weapon_toggles = weapon_toggles;
                entry.spell_buttons = spell_buttons;
                entry.member = member;
                entry.build_variants = build_variants;
                entry.builds = builds;
                entry.build_needs = build_needs;
                entry.constructs = constructs;
                entry.foundation_builds = foundation_builds;
                entry.foundation_needs = foundation_needs;
                entry.command_set_variants = command_set_variants;
                entry.power_buttons = power_buttons;
                entry.power_needs = power_needs;
                entry.revive_buttons = revive_buttons;
                entry.auto_power = auto_power;
            }
        }
        (table, missing)
    }

    pub(crate) fn add_command_set_variant(
        &mut self,
        kind: Kind,
        set: &str,
        command_sets: &mint_sage_ini::CommandSetIndex,
        command_buttons: &mint_sage_ini::CommandButtonIndex,
    ) {
        let (power_buttons, power_needs, auto_power) =
            power_set_of(set, command_sets, command_buttons);
        let revive_buttons = mint_sage_modules::revive_buttons(set, command_sets, command_buttons)
            .iter()
            .map(|name| Needs::of(command_buttons, name))
            .collect();
        if let Some(entry) = self.entries.get_mut(usize::from(kind.0)) {
            entry.command_set_variants.push(CommandSetVariant {
                command_set: set.to_owned(),
                builds: Vec::new(),
                build_needs: Vec::new(),
                power_buttons,
                power_needs,
                revive_buttons,
                auto_power,
            });
        }
    }

    pub(crate) fn kind(&self, name: &str) -> Option<Kind> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .copied()
            .map(Kind)
    }

    pub(crate) fn power_button(&self, name: &str) -> Option<&(String, TargetKind)> {
        self.power_button_index.get(&name.to_ascii_lowercase())
    }

    pub(crate) fn get(&self, kind: Kind) -> Option<&KindEntry> {
        self.entries.get(usize::from(kind.0))
    }

    pub(crate) fn variant_count(&self, kind: Kind) -> usize {
        self.get(kind).map_or(0, |entry| entry.build_variants.len())
    }

    pub(crate) fn equivalent(&self, a: Kind, b: Kind) -> bool {
        if a == b {
            return true;
        }
        let Some(ea) = self.get(a) else {
            return false;
        };
        let Some(eb) = self.get(b) else {
            return false;
        };
        if ea.build_variants.contains(&b) || eb.build_variants.contains(&a) {
            return true;
        }
        let name_a = ea.record.name.as_str();
        let name_b = eb.record.name.as_str();
        ea.record
            .equivalent_to
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name_b))
            || eb
                .record
                .equivalent_to
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name_a))
            || ea.record.equivalent_to.iter().any(|n| {
                eb.record
                    .equivalent_to
                    .iter()
                    .any(|m| n.eq_ignore_ascii_case(m))
            })
    }

    pub(crate) fn resolve_build_variation(&self, kind: Kind, roll: u32) -> Kind {
        let Some(variants) = self.get(kind).map(|entry| &entry.build_variants) else {
            return kind;
        };
        let index = usize::try_from(roll).unwrap_or(0);
        variants.get(index).copied().unwrap_or(kind)
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.record.name.clone())
            .collect()
    }

    pub(crate) fn infantry(&self) -> Vec<bool> {
        self.has_kind_of("INFANTRY")
    }

    pub(crate) fn move_only(&self) -> Vec<bool> {
        self.has_kind_of("MOVE_ONLY")
    }

    fn has_kind_of(&self, name: &str) -> Vec<bool> {
        self.entries
            .iter()
            .map(|entry| entry.record.kind_of.iter().any(|flag| flag == name))
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &KindEntry> {
        self.entries.iter()
    }

    pub(crate) fn horde_kind(&self, formation: u16, member: Kind) -> Option<Kind> {
        self.entries
            .iter()
            .position(|entry| {
                entry.formation == Some(formation)
                    && entry.member.is_some_and(|base| {
                        base == member
                            || self.get(base).is_some_and(|base_entry| {
                                base_entry.build_variants.contains(&member)
                            })
                    })
            })
            .and_then(|index| u16::try_from(index).ok())
            .map(Kind)
    }

    pub(crate) fn spawn(
        &self,
        combat: &mint_sage_combat::CombatData,
        kind: Kind,
        position: Vec2,
        facing: Facing,
        owner: PlayerId,
    ) -> Option<Spawn> {
        let record = &self.get(kind)?.record;
        let (mut kind_of, _) = KindOf::parse(&record.kind_of.join(" "));
        if record.dozer {
            kind_of = kind_of.union(KindOf::DOZER);
        }
        Some(Spawn {
            health: record
                .max_health
                .filter(|health| *health > 0.0 && *health <= MAX_HEALTH),
            weapon: record
                .weapon
                .as_deref()
                .and_then(|weapon| combat.weapons.id(weapon)),
            armour: record
                .armor
                .as_deref()
                .and_then(|armour| combat.armors.id(armour)),
            kind: Some(kind),
            vision: vision(record),
            income: income(record),
            claim: claimant(record),
            upkeep: self.upkeep(kind),
            ..Spawn::inert(
                position,
                facing,
                pick_shape(
                    kind_of,
                    record.geometry.radius,
                    record.geometry.height,
                    record.geometry_is_small.unwrap_or(false),
                ),
                owner,
            )
        })
    }

    pub(crate) fn upkeep(&self, kind: Kind) -> Option<i32> {
        let record = &self.get(kind)?.record;
        if !record.has_kind("SELECTABLE")
            || record.has_kind("STRUCTURE")
            || record.has_kind("HORDE")
        {
            return None;
        }
        whole(record.command_points?)
            .and_then(|points| i32::try_from(points).ok())
            .filter(|points| *points > 0)
    }

    pub(crate) fn production(&self, data: &GameData) -> Production {
        let mut production = Production {
            menu_of: vec![None; self.entries.len()],
            ..Production::default()
        };
        let mut buildable_of: BTreeMap<u16, u16> = BTreeMap::new();
        let mut buildable = |production: &mut Production, product: Kind, owner: &str| {
            if let Some(index) = buildable_of.get(&product.0) {
                return Some(*index);
            }
            match self.buildable(data, product) {
                Ok(built) => {
                    let index = u16::try_from(production.buildables.len()).ok()?;
                    production.buildables.push(built);
                    buildable_of.insert(product.0, index);
                    Some(index)
                }
                Err(why) => {
                    production.lines.push(format!(
                        "production: {owner}'s button for {} is not offered - {why}",
                        self.get(product)
                            .map_or("?", |product| product.record.name.as_str())
                    ));
                    None
                }
            }
        };
        for (at, entry) in self.entries.iter().enumerate() {
            if !entry.builds.is_empty() {
                if !entry.record.produces {
                    production.lines.push(format!(
                        "production: {} has {} UNIT_BUILD button(s) and no ProductionUpdate - not a producer",
                        entry.record.name,
                        entry.builds.len()
                    ));
                } else {
                    let (menu, needs): (Vec<u16>, Vec<Needs>) = entry
                        .builds
                        .iter()
                        .zip(&entry.build_needs)
                        .filter_map(|(product, needs)| {
                            Some((
                                buildable(&mut production, *product, &entry.record.name)?,
                                needs.clone(),
                            ))
                        })
                        .unzip();
                    if !menu.is_empty() {
                        if let (Ok(menu_index), Some(slot)) = (
                            u16::try_from(production.menus.len()),
                            production.menu_of.get_mut(at),
                        ) {
                            *slot = Some(menu_index);
                            production.menus.push(menu);
                            production.menu_needs.push(needs);
                        }
                    }
                }
            }
            for variant in &entry.command_set_variants {
                if variant.builds.is_empty() {
                    continue;
                }
                let (menu, needs): (Vec<u16>, Vec<Needs>) = variant
                    .builds
                    .iter()
                    .zip(&variant.build_needs)
                    .filter_map(|(product, needs)| {
                        Some((
                            buildable(&mut production, *product, &entry.record.name)?,
                            needs.clone(),
                        ))
                    })
                    .unzip();
                if menu.is_empty() {
                    continue;
                }
                let Ok(menu_index) = u16::try_from(production.menus.len()) else {
                    continue;
                };
                production.menus.push(menu);
                production.menu_needs.push(needs);
                let Ok(kind) = u16::try_from(at) else {
                    continue;
                };
                production.command_set_variants.push((
                    Kind(kind),
                    variant.command_set.clone(),
                    menu_index,
                ));
            }
        }
        production
    }

    pub(crate) fn upgrades(&self, data: &GameData) -> UpgradeMenus {
        let mut out = UpgradeMenus {
            object_menu_of: vec![None; self.entries.len()],
            player_menu_of: vec![None; self.entries.len()],
            ..UpgradeMenus::default()
        };
        let mut upgrade_of: BTreeMap<String, u16> = BTreeMap::new();
        let resolve = |out: &mut UpgradeMenus,
                       upgrade_of: &mut BTreeMap<String, u16>,
                       name: &str,
                       owner: &str,
                       command_button: &str|
         -> Option<u16> {
            let key = name.to_ascii_lowercase();
            if let Some(&index) = upgrade_of.get(&key) {
                return Some(index);
            }
            let Some(bit) = data.upgrade_bits.bit(name) else {
                out.lines.push(format!(
                    "upgrade: {owner}'s button names undeclared upgrade {name}"
                ));
                return None;
            };
            let Some(record) = data
                .upgrades
                .iter()
                .find(|u| u.name.eq_ignore_ascii_case(name))
            else {
                out.lines.push(format!(
                    "upgrade: {owner}'s button for {name} is not offered - no usable \
                     BuildCost/BuildTime"
                ));
                return None;
            };
            let cost = whole(record.cost)?;
            if !(0.0..=MAX_BUILD_SECONDS).contains(&record.build_time_seconds) {
                out.lines.push(format!(
                    "upgrade: {owner}'s button for {name} is not offered - BuildTime out of range"
                ));
                return None;
            }
            let index = u16::try_from(out.upgrades.len()).ok()?;
            out.upgrades.push(UpgradeBuildable {
                name: name.to_owned(),
                cost_discount_template: record.use_object_template_for_cost_discount.clone(),
                bit,
                is_player: record.is_player,
                cost: mint_sage_economy::cost_to_build(
                    cost,
                    &mint_sage_economy::CostModifiers::default(),
                ),
                build_time: mint_sage_economy::time_to_build(
                    record.build_time_seconds,
                    &mint_sage_economy::TimeModifiers::default(),
                ),
                command_button: command_button.to_owned(),
            });
            upgrade_of.insert(key, index);
            Some(index)
        };
        let menus_of = |out: &mut UpgradeMenus,
                        upgrade_of: &mut BTreeMap<String, u16>,
                        set: &str,
                        owner: &str|
         -> (Option<u16>, Option<u16>) {
            let (object_menu, object_needs): (Vec<u16>, Vec<Needs>) =
                mint_sage_modules::object_upgrades(set, &data.command_sets, &data.command_buttons)
                    .iter()
                    .filter_map(|button| {
                        Some((
                            resolve(out, upgrade_of, &button.upgrade, owner, &button.button)?,
                            Needs::of(&data.command_buttons, &button.button),
                        ))
                    })
                    .unzip();
            let object = (!object_menu.is_empty())
                .then(|| u16::try_from(out.object_menus.len()).ok())
                .flatten();
            if object.is_some() {
                out.object_menus.push(object_menu);
                out.object_needs.push(object_needs);
            }
            let (player_menu, player_needs): (Vec<u16>, Vec<Needs>) =
                mint_sage_modules::player_upgrades(set, &data.command_sets, &data.command_buttons)
                    .iter()
                    .filter_map(|button| {
                        Some((
                            resolve(out, upgrade_of, &button.upgrade, owner, &button.button)?,
                            Needs::of(&data.command_buttons, &button.button),
                        ))
                    })
                    .unzip();
            let player = (!player_menu.is_empty())
                .then(|| u16::try_from(out.player_menus.len()).ok())
                .flatten();
            if player.is_some() {
                out.player_menus.push(player_menu);
                out.player_needs.push(player_needs);
            }
            (object, player)
        };
        for (at, entry) in self.entries.iter().enumerate() {
            if let Some(set) = entry.record.command_set.as_deref() {
                let (object, player) = menus_of(&mut out, &mut upgrade_of, set, &entry.record.name);
                if let Some(slot) = out.object_menu_of.get_mut(at) {
                    *slot = object;
                }
                if let Some(slot) = out.player_menu_of.get_mut(at) {
                    *slot = player;
                }
            }
            let Ok(kind) = u16::try_from(at) else {
                continue;
            };
            for variant in &entry.command_set_variants {
                let (object, player) = menus_of(
                    &mut out,
                    &mut upgrade_of,
                    &variant.command_set,
                    &entry.record.name,
                );
                out.variant_menus
                    .push((Kind(kind), variant.command_set.clone(), object, player));
            }
        }
        out
    }

    pub(crate) fn power_names(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .flat_map(|entry| entry.record.powers.iter().map(|slot| slot.power.as_str()))
    }

    pub(crate) fn trains(&self, kind: Kind) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.builds.contains(&kind))
    }

    pub(crate) fn construct_menus(&self) -> ConstructMenus {
        let mut out = ConstructMenus {
            menu_of: vec![None; self.entries.len()],
            ..ConstructMenus::default()
        };
        for (at, entry) in self.entries.iter().enumerate() {
            if !entry.record.dozer || entry.constructs.is_empty() {
                continue;
            }
            let (Ok(menu_index), Some(slot)) =
                (u16::try_from(out.menus.len()), out.menu_of.get_mut(at))
            else {
                continue;
            };
            *slot = Some(menu_index);
            out.menus.push(entry.constructs.clone());
        }
        out
    }

    pub(crate) fn hero_setup(
        &self,
        data: &GameData,
        rosters: &[Vec<String>],
    ) -> (Vec<Vec<Option<Kind>>>, Vec<Buildable>, Vec<String>) {
        let mut buildables: Vec<Buildable> = Vec::new();
        let mut lines = Vec::new();
        let kinds: Vec<Vec<Option<Kind>>> = rosters
            .iter()
            .map(|names| names.iter().map(|name| self.kind(name)).collect())
            .collect();
        for kind in kinds.iter().flatten().flatten() {
            if buildables.iter().any(|b| b.kind == *kind) {
                continue;
            }
            match self.buildable(data, *kind) {
                Ok(built) => buildables.push(built),
                Err(why) => lines.push(format!(
                    "heroes: {} cannot be recruited - {why}",
                    self.get(*kind).map_or("?", |e| e.record.name.as_str())
                )),
            }
        }
        (kinds, buildables, lines)
    }

    fn buildable(&self, data: &GameData, product: Kind) -> Result<Buildable, &'static str> {
        let entry = self.get(product).ok_or("not in the table")?;
        let record = &entry.record;
        let cost = record
            .build_cost
            .and_then(whole)
            .filter(|cost| *cost >= 0)
            .ok_or("no usable BuildCost")?;
        let seconds = record
            .build_time
            .filter(|seconds| (0.0..=MAX_BUILD_SECONDS).contains(seconds))
            .ok_or("no usable BuildTime")?;
        let points = record
            .command_points
            .and_then(whole)
            .and_then(|points| i32::try_from(points).ok())
            .filter(|points| *points >= 0)
            .unwrap_or(0);
        let output = match entry.formation {
            Some(formation) => {
                let member = entry
                    .member
                    .ok_or("a horde whose RankInfo names no template")?;
                let count = data
                    .formations
                    .tables()
                    .get(usize::from(formation))
                    .map_or(0, mint_sage_formation::FormationTable::len);
                Output::Horde {
                    formation,
                    member: self
                        .spawn(
                            &data.combat,
                            member,
                            Vec2::ZERO,
                            Facing::EAST,
                            PlayerId::new(0),
                        )
                        .ok_or("member not in the table")?,
                    count: u32::try_from(count).unwrap_or(0),
                    vision: vision(record).unwrap_or(crate::shroud::HORDE_SHROUD_RANGE),
                }
            }
            None => Output::Unit(
                self.spawn(
                    &data.combat,
                    product,
                    Vec2::ZERO,
                    Facing::EAST,
                    PlayerId::new(0),
                )
                .map(|spawn| Spawn {
                    upkeep: self.upkeep(product),
                    ..spawn
                })
                .ok_or("not in the table")?,
            ),
        };
        Ok(Buildable {
            output,
            kind: product,
            cost: mint_sage_economy::cost_to_build(
                cost,
                &mint_sage_economy::CostModifiers::default(),
            ),
            build_time: mint_sage_economy::time_to_build(
                seconds,
                &mint_sage_economy::TimeModifiers::default(),
            ),
            command_points: points,
            label: record.name.clone(),
            builder: KindOf::parse(&record.kind_of.join(" "))
                .0
                .contains(KindOf::DOZER),
        })
    }

    pub(crate) fn producer(
        &self,
        production: &Production,
        kind: Kind,
        position: Vec2,
        facing: Facing,
    ) -> Option<ProducerSpawn> {
        let menu = (*production.menu_of.get(usize::from(kind.0))?)?;
        let record = &self.get(kind)?.record;
        Some(ProducerSpawn {
            menu,
            rally: exit_rally(record, position, facing),
            depth: queue_depth(record),
        })
    }

    pub(crate) fn describe(&self, kind: Kind) -> String {
        let Some(entry) = self.get(kind) else {
            return format!("kind {}: not in the table", kind.0);
        };
        let record = &entry.record;
        let or_none =
            |value: Option<f32>| value.map_or_else(|| String::from("-"), |v| format!("{v:.0}"));
        format!(
            "template {} {}: {}{}{} MaxHealth={} ShroudClear={} r={} h={} weapon={} armour={} \
             CommandSet={} cost={} time={} points={}{}{}",
            kind.0,
            record.name,
            if record.depth > 0 {
                format!("(inherits through {} parent(s)) ", record.depth)
            } else {
                String::new()
            },
            if record.dozer { "DOZER " } else { "" },
            entry
                .formation
                .map_or_else(String::new, |f| format!("horde (formation {f}) ")),
            or_none(record.max_health),
            or_none(vision(record)),
            or_none(record.geometry.radius),
            or_none(record.geometry.height),
            record.weapon.as_deref().unwrap_or("-"),
            record.armor.as_deref().unwrap_or("-"),
            record.command_set.as_deref().unwrap_or("-"),
            or_none(record.build_cost),
            or_none(record.build_time),
            or_none(record.command_points),
            if entry.builds.is_empty() {
                String::new()
            } else {
                format!("; builds {} kind(s)", entry.builds.len())
            },
            record
                .income
                .map_or_else(String::new, |(ms, amount)| format!(
                    "; earns {amount:.0} every {ms:.0} ms"
                )),
        )
    }
}

const RALLY_CLEARANCE: f32 = 20.0;

fn structure_reach(record: &ObjectRecord) -> f32 {
    record
        .geometry
        .radius
        .filter(|radius| *radius > 0.0 && *radius <= MAX_RADIUS)
        .unwrap_or(FALLBACK_RADIUS)
}

pub(crate) fn clear_radius(record: &ObjectRecord) -> f32 {
    structure_reach(record) + RALLY_CLEARANCE
}

pub(crate) fn exit_rally(record: &ObjectRecord, position: Vec2, facing: Facing) -> Vec2 {
    let local = record
        .natural_rally_point
        .unwrap_or_else(|| [clear_radius(record), 0.0]);
    rotate(position, facing, local)
}

const MAX_EXIT_DELAY_MS: f32 = 60_000.0;

pub(crate) fn production_exit_delay(exit_delay_ms: Option<f32>) -> LogicFrames {
    let ms = exit_delay_ms
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .map_or(0.0, |ms| ms.min(MAX_EXIT_DELAY_MS).ceil());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    mint_sage_modules::retail_duration_ticks(ms as u32)
}

pub(crate) fn rotate(origin: Vec2, facing: Facing, local: [f32; 2]) -> Vec2 {
    let axis = facing.to_vector();
    let [x, y] = local;
    Vec2::new(
        origin.x + axis.x * x - axis.y * y,
        origin.y + axis.y * x + axis.x * y,
    )
}

pub(crate) fn vision(record: &ObjectRecord) -> Option<f32> {
    record
        .shroud_clearing_range
        .or(record.vision_range)
        .filter(|range| range.is_finite() && *range >= 0.0)
}

#[cfg(test)]
mod vision_tests {
    use super::vision;
    use mint_sage_modules::ObjectRecord;

    #[test]
    fn a_huge_declared_range_is_kept_not_discarded() {
        let record = ObjectRecord {
            vision_range: Some(123_456.0),
            ..ObjectRecord::default()
        };
        assert_eq!(vision(&record), Some(123_456.0));
    }

    #[test]
    fn a_negative_or_non_finite_range_is_none() {
        for bad in [-1.0, f32::NAN, f32::INFINITY] {
            let record = ObjectRecord {
                vision_range: Some(bad),
                ..ObjectRecord::default()
            };
            assert_eq!(vision(&record), None, "{bad}");
        }
    }

    #[test]
    fn shroud_clearing_range_still_wins_over_vision_range() {
        let record = ObjectRecord {
            shroud_clearing_range: Some(300.0),
            vision_range: Some(123_456.0),
            ..ObjectRecord::default()
        };
        assert_eq!(vision(&record), Some(300.0));
    }
}

pub(crate) fn income(record: &ObjectRecord) -> Option<(LogicFrames, i64)> {
    let (timing, amount) = record.income?;
    if !(timing > 0.0 && timing <= MAX_DEPOSIT_MS && (0.0..=MAX_WHOLE).contains(&amount)) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let timing = timing as u32;
    Some((LogicFrames::from_milliseconds(timing), whole(amount)?))
}

pub(crate) fn claimant(record: &ObjectRecord) -> Option<crate::claims::Claimant> {
    let radius = record
        .claim_radius
        .filter(|radius| radius.is_finite() && *radius > 0.0)?;
    Some(crate::claims::Claimant {
        radius,
        high_priority: record.claim_high_priority,
        scales_income: record.income_scales_with_claim,
        share: None,
    })
}

pub(crate) fn speed_census_line(kinds: &KindTable) -> String {
    let mut resolved: Vec<(&str, f32)> = kinds
        .iter()
        .filter_map(|entry| Some((entry.record.name.as_str(), entry.limits?.limits.max_speed)))
        .collect();
    if resolved.is_empty() {
        return format!(
            "movement: 0 of {} kind(s) resolved real locomotor data",
            kinds.len()
        );
    }
    resolved.sort_by(|a, b| a.1.total_cmp(&b.1));
    let sample = |list: &[(&str, f32)]| -> String {
        list.iter()
            .map(|(name, speed)| format!("{name}@{speed:.0}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let n = resolved.len();
    let take = n.min(3);
    let own_surfaces = kinds
        .iter()
        .filter(|entry| {
            entry
                .mover
                .is_some_and(|mover| mover != mint_sage_passability::GROUND)
        })
        .count();
    format!(
        "movement: {n} of {} kind(s) resolved real locomotor data (u/s); slowest [{}], \
         fastest [{}]; {own_surfaces} kind(s) cross what a walker cannot",
        kinds.len(),
        sample(resolved.get(..take).unwrap_or_default()),
        sample(resolved.get(n.saturating_sub(take)..).unwrap_or_default()),
    )
}

pub(crate) fn burning_run_census_line(kinds: &KindTable) -> String {
    let cavalry = kinds
        .iter()
        .filter(|entry| entry.burning_run.is_some_and(|run| run.is_cavalry))
        .count();
    let total = kinds
        .iter()
        .filter(|entry| entry.burning_run.is_some())
        .count();
    format!(
        "movement: {total} of {} kind(s) resolved a SET_BURNINGDEATH locomotor ({cavalry} of them \
         BurningDeathIsCavalry)",
        kinds.len()
    )
}

pub(crate) fn whole(value: f32) -> Option<i64> {
    if !(-MAX_WHOLE..=MAX_WHOLE).contains(&value) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = value as i64;
    Some(whole)
}

pub(crate) const NORMAL_CONDITION: &str = "SET_NORMAL";

const RATE_UNIT_MS: f32 = 1000.0;

const DEFAULT_CLOSE_ENOUGH_DIST: f32 = 1.0;

const FLIER_ARRIVAL_RADIUS: f32 = crate::unit::PLACEHOLDER_LIMITS.arrival_radius;

const DEFAULT_TURN_TIME_MS: f32 = 1000.0;

const DEFAULT_FAST_TURN_RADIUS: f32 = 20.0;

#[allow(clippy::cast_precision_loss)]
pub(crate) const RETAIL_FRAME_SECONDS: f32 = 1.0 / crate::game_data::RETAIL_LOGIC_HZ as f32;

fn whole_frames(ms: f32) -> f32 {
    (ms / (RATE_UNIT_MS * RETAIL_FRAME_SECONDS)).ceil().max(1.0) * RETAIL_FRAME_SECONDS
}

fn turn_rate(turn_time: Option<mint_sage_ini::Duration>) -> Option<f32> {
    let ms = turn_time
        .map(mint_sage_ini::Duration::raw)
        .filter(|v| v.is_finite() && *v > 0.0)?;
    Some(2.0 * core::f32::consts::PI / whole_frames(ms))
}

const DEGREES_TO_RADIANS: f32 = core::f32::consts::PI / 180.0;

const DEFAULT_TURN_THRESHOLD_DEGREES: f32 = 15.0;

const DEFAULT_ACC_DEC_TRIGGER: f32 = 0.5;

fn turn_threshold(def: &mint_sage_ini::LocomotorDef) -> f32 {
    def.turn_threshold
        .filter(|v| v.is_finite())
        .unwrap_or(DEFAULT_TURN_THRESHOLD_DEGREES)
        * DEGREES_TO_RADIANS
}

fn turn_circle(def: &mint_sage_ini::LocomotorDef) -> Option<mint_movement::TurnCircle> {
    if !def
        .appearance
        .as_deref()?
        .eq_ignore_ascii_case("GIANT_BIRD")
    {
        return None;
    }
    let radius = |field: Option<f32>| field.filter(|v| v.is_finite()).unwrap_or(0.0).max(0.0);
    Some(mint_movement::TurnCircle {
        slow_radius: radius(def.slow_turn_radius),
        fast_radius: radius(def.fast_turn_radius),
    })
}

const LEGS_APPEARANCES: [&str; 3] = ["TWO_LEGS", "FOUR_LEGS_HUGE", "HUGE_TWO_LEGS"];

fn turn_slowdown(
    def: &mint_sage_ini::LocomotorDef,
    speed: f32,
    turn_rate: f32,
) -> mint_movement::TurnSlowdown {
    use mint_movement::TurnSlowdown;
    let appearance = def.appearance.as_deref().unwrap_or("");
    let is = |name: &str| appearance.eq_ignore_ascii_case(name);
    if is("FOUR_LEGS_HUGE") || is("HUGE_TWO_LEGS") {
        TurnSlowdown {
            cap: 0.9,
            ..TurnSlowdown::PROPORTIONAL
        }
    } else if is("SHIP") {
        TurnSlowdown {
            cap: 0.9,
            full_at: 3.5 * turn_rate * RETAIL_FRAME_SECONDS,
            unhindered_above: None,
            pivot_below: speed / acceleration_frames(def),
        }
    } else if !(is("FOUR_WHEELS") || is("TREADS")) {
        TurnSlowdown {
            cap: 0.0,
            ..TurnSlowdown::PROPORTIONAL
        }
    } else {
        TurnSlowdown {
            full_at: core::f32::consts::FRAC_PI_4,
            cap: 1.0,
            unhindered_above: Some(0.25 * speed),
            pivot_below: speed / acceleration_frames(def),
        }
    }
}

fn acceleration_frames(def: &mint_sage_ini::LocomotorDef) -> f32 {
    def.acceleration
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .map_or(1.0, |ms| {
            (ms / (RATE_UNIT_MS * RETAIL_FRAME_SECONDS)).ceil().max(1.0)
        })
}

pub(crate) fn motion_limits(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
    condition: &str,
) -> Option<Kinematics> {
    limits_for_slot(record.locomotor_for(condition)?, locomotors)
}

fn limits_for_slot(
    slot: &mint_sage_modules::LocomotorSlot,
    locomotors: &mint_sage_ini::LocomotorIndex,
) -> Option<Kinematics> {
    let speed = slot.speed.filter(|v| v.is_finite())?.max(0.0);
    let def = locomotors.get(slot.locomotor.as_deref()?)?;
    let rate = |field: Option<f32>| {
        field
            .filter(|ms| ms.is_finite() && *ms > 0.0)
            .map_or(speed, |ms| speed / whole_frames(ms))
    };
    let turn_circle = turn_circle(def);
    let is_appearance = |name: &str| {
        def.appearance
            .as_deref()
            .is_some_and(|appearance| appearance.eq_ignore_ascii_case(name))
    };
    let is_horde = is_appearance("HORDE");
    let is_wheels = is_appearance("FOUR_WHEELS") || is_appearance("TREADS");
    let stops_dead = LEGS_APPEARANCES.iter().any(|legs| is_appearance(legs));
    let turn = turn_rate(def.turn_time)
        .unwrap_or(2.0 * core::f32::consts::PI / whole_frames(DEFAULT_TURN_TIME_MS));
    let damaged_turn_rate = turn_rate(def.turn_time_damaged).unwrap_or(turn);
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        max_speed: speed,
        desired_speed: f32::INFINITY,
        stops_dead,
        acceleration: rate(def.acceleration),
        braking: rate(def.braking),
        turn_rate: turn,
        arrival_radius: def
            .close_enough_dist
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(if turn_circle.is_some() {
                FLIER_ARRIVAL_RADIUS
            } else {
                DEFAULT_CLOSE_ENOUGH_DIST
            }),
        turn_threshold: turn_threshold(def),
        acc_dec_trigger: def
            .acc_dec_trigger
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(DEFAULT_ACC_DEC_TRIGGER),
        min_speed: def
            .min_speed
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .max(0.0),
        turn_circle,
        turn_slowdown: turn_slowdown(def, speed, turn),
        frame: RETAIL_FRAME_SECONDS,
        turn_while_moving: !is_horde || def.turn_while_moving.unwrap_or(true),
        walk_distance: def
            .walk_distance
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .max(0.0),
        turn_phases_at_speed: def.enable_high_speed_turn_model_conditions.unwrap_or(false),
        corner_radius: if turn_circle.is_some() {
            0.0
        } else {
            def.fast_turn_radius
                .filter(|v| v.is_finite())
                .unwrap_or(DEFAULT_FAST_TURN_RADIUS)
                .max(0.0)
        },
    };
    Some(Kinematics {
        limits,
        damaged_turn_rate,
        damaged_speed: def
            .speed_damaged
            .filter(|share| share.is_finite() && *share >= 0.0)
            .filter(|_| def.charge_ignores_condition != Some(true))
            .unwrap_or(1.0),
        look_ahead_mult: def
            .look_ahead_mult
            .filter(|mult| mult.is_finite() && *mult > 1.0)
            .unwrap_or(1.0),
        max_turn_without_reform: Some(
            def.max_turn_without_reform
                .filter(|degrees| degrees.is_finite() && *degrees >= 0.0)
                .map_or(0.0, f32::to_radians),
        ),
        charge_speed: def
            .charge_speed
            .filter(|share| share.is_finite() && *share >= 0.0)
            .unwrap_or(1.0),
        charge_available: def.charge_available.unwrap_or(false),
        charge_ignores_condition: def.charge_ignores_condition.unwrap_or(false),
        departure: crate::unit::DepartureRadii {
            slow: def
                .slow_turn_radius
                .filter(|v| v.is_finite())
                .unwrap_or(0.0),
            fast: def
                .fast_turn_radius
                .filter(|v| v.is_finite())
                .unwrap_or(DEFAULT_FAST_TURN_RADIUS),
            max_turn: def
                .max_turn_without_reform
                .filter(|v| v.is_finite())
                .map_or(core::f32::consts::TAU, f32::to_radians),
            max_turn_fast: def
                .turn_threshold_hs
                .filter(|v| v.is_finite())
                .map_or(core::f32::consts::PI, f32::to_radians),
        },
        turn_pivot_offset: if is_wheels {
            0.0
        } else {
            def.turn_pivot_offset
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
        },
        backs_in_terror: def.can_move_backwards == Some(true) && (is_horde || stops_dead),
        crew_powered: def.crew_powered.unwrap_or(false),
        scales_walls: def.scales_walls.unwrap_or(false),
        wander_radius: def
            .wander_about_point_radius
            .filter(|radius| radius.is_finite())
            .unwrap_or(0.0)
            .max(0.0),
    })
}

fn surface_mask(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
    condition: &str,
) -> Option<u16> {
    let slot = record.locomotor_for(condition)?;
    let def = locomotors.get(slot.locomotor.as_deref()?)?;
    Some(
        SURFACE_TABLE
            .iter()
            .enumerate()
            .filter(|(_, name)| def.surfaces.iter().any(|s| s.eq_ignore_ascii_case(name)))
            .fold(0_u16, |mask, (row, _)| mask | (1_u16 << row)),
    )
}

fn mover_class(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
    condition: &str,
) -> Option<mint_movement::MoverClass> {
    let slot = record.locomotor_for(condition)?;
    let def = locomotors.get(slot.locomotor.as_deref()?)?;
    let mover =
        mint_sage_passability::mover_class(&def.surfaces, def.scales_walls.unwrap_or(false))?;
    Some(if record.can_path_through_gates == Some(false) {
        mint_movement::MoverClass {
            blocked_by: mover
                .blocked_by
                .union(mint_sage_passability::GATE_RESTRICTED),
            ..mover
        }
    } else {
        mover
    })
}

#[cfg(test)]
mod mover_class_tests {
    use super::{mover_class, ObjectRecord, NORMAL_CONDITION};
    use mint_sage_ini::{BlockTable, Document, LocomotorIndex};
    use mint_sage_modules::TemplateIndex;

    fn record_and_locomotors(text: &str, name: &str) -> (ObjectRecord, LocomotorIndex) {
        let document = Document::parse(name, text, &BlockTable::corpus_path()).unwrap();
        let mut locomotors = LocomotorIndex::new();
        locomotors.insert_document(&document);
        let mut index = TemplateIndex::new();
        index.insert_document(&document);
        (
            ObjectRecord::read(&index, name, &mint_sage_ini::ValueErrors::new()).unwrap(),
            locomotors,
        )
    }

    const LOCOMOTOR: &str = "\
        Locomotor Test\n  \
          Surfaces = GROUND\n\
        End\n";

    #[test]
    fn can_path_through_gates_no_refuses_gate_restricted_cells() {
        let text = format!(
            "{LOCOMOTOR}Object Mumak\n  \
              LocomotorSet\n    Locomotor = Test\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n  \
              CanPathThroughGates = No\n\
            End\n"
        );
        let (record, locomotors) = record_and_locomotors(&text, "Mumak");
        let mover = mover_class(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert!(mover
            .blocked_by
            .intersects(mint_sage_passability::GATE_RESTRICTED));
    }

    #[test]
    fn absent_can_path_through_gates_is_unrestricted() {
        let text = format!(
            "{LOCOMOTOR}Object Soldier\n  \
              LocomotorSet\n    Locomotor = Test\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n\
            End\n"
        );
        let (record, locomotors) = record_and_locomotors(&text, "Soldier");
        let mover = mover_class(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert!(!mover
            .blocked_by
            .intersects(mint_sage_passability::GATE_RESTRICTED));
    }
}

fn other_locomotor_sets(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
) -> Vec<(String, Kinematics)> {
    record
        .locomotor_sets
        .iter()
        .filter_map(|slot| {
            let flag = slot
                .condition
                .strip_prefix("SET_")
                .unwrap_or(&slot.condition);
            if flag.eq_ignore_ascii_case(NORMAL_CONDITION.trim_start_matches("SET_")) {
                return None;
            }
            let limits = limits_for_slot(slot, locomotors)?;
            Some((flag.to_string(), limits))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BurningRun {
    pub(crate) radius: f32,
    pub(crate) is_cavalry: bool,
}

fn burning_run(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
) -> Option<BurningRun> {
    let slot = record
        .locomotor_sets
        .iter()
        .find(|slot| slot.condition.eq_ignore_ascii_case("SET_BURNINGDEATH"))?;
    let def = locomotors.get(slot.locomotor.as_deref()?)?;
    let radius = def
        .burning_death_radius
        .filter(|v| v.is_finite() && *v > 0.0)?;
    Some(BurningRun {
        radius,
        is_cavalry: def.burning_death_is_cavalry.unwrap_or(false),
    })
}

fn weapon_creation_lists(data: &GameData, record: &ObjectRecord) -> Vec<String> {
    let mut queue: Vec<String> = record.weapon_names();
    let mut seen: Vec<usize> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    while let Some(name) = queue.pop() {
        let Some(id) = data.combat.weapons.id(&name) else {
            continue;
        };
        if seen.contains(&id.index()) {
            continue;
        }
        seen.push(id.index());
        let Some(template) = data.combat.weapons.get(id) else {
            continue;
        };
        for (index, nugget) in template.nuggets.iter().enumerate() {
            if let mint_sage_combat::Nugget::Projectile(projectile) = nugget {
                queue.push(projectile.warhead.clone());
            }
            if let Some(mint_sage_combat::EffectNugget::WeaponOcl(ocl)) =
                data.combat.effects.get(id.index(), index)
            {
                out.push(ocl.list.clone());
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AirborneCategory {
    Vehicle,
    Infantry,
    Monster,
}

impl AirborneCategory {
    fn of(kind_of: &[String]) -> Self {
        let has = |token: &str| kind_of.iter().any(|k| k.eq_ignore_ascii_case(token));
        if has("VEHICLE") {
            Self::Vehicle
        } else if has("INFANTRY") {
            Self::Infantry
        } else {
            Self::Monster
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SwoopProfile {
    pub(crate) acceleration_rate: f32,
    pub(crate) standoff_height: f32,
    pub(crate) standoff_radius: f32,
    pub(crate) terminal_velocity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Airborne {
    pub(crate) category: AirborneCategory,
    pub(crate) above: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DiveProfile {
    pub(crate) level: f32,
    pub(crate) reach: f32,
    pub(crate) slope: f32,
}

const DEFAULT_SWOOP_STANDOFF: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlightProfile {
    pub(crate) airborne: Option<Airborne>,
    pub(crate) preferred_height: Option<f32>,
    pub(crate) dive: Option<DiveProfile>,
    pub(crate) charge_speed_fraction: Option<f32>,
    pub(crate) charge_available: bool,
    pub(crate) uncollided: bool,
    pub(crate) look_ahead_mult: f32,
    pub(crate) lift: f32,
    pub(crate) swoop: Option<SwoopProfile>,
    pub(crate) height: mint_movement::HeightRule,
    pub(crate) formation_priority: u8,
    pub(crate) river_modifier: f32,
}

pub(crate) const DEFAULT_FORMATION_PRIORITY: u8 = 7;

fn height_rule(def: &mint_sage_ini::LocomotorDef) -> mint_movement::HeightRule {
    use mint_movement::HeightRule;
    let preferred = def
        .preferred_height
        .filter(|h| h.is_finite())
        .unwrap_or(0.0);
    let behaviour = def.z_axis_behavior.as_deref().unwrap_or("");
    let is = |name: &str| behaviour.eq_ignore_ascii_case(name);
    let lifted = preferred != 0.0;
    let damping = Some(
        def.preferred_height_damping
            .filter(|d| d.is_finite())
            .unwrap_or(1.0),
    );
    if is("FIXED_ABSOLUTE_HEIGHT") {
        HeightRule::Fixed {
            height: preferred,
            damping: None,
        }
    } else if lifted && is("ABSOLUTE_HEIGHT") {
        HeightRule::Fixed {
            height: preferred,
            damping,
        }
    } else if is("SEA_LEVEL") {
        HeightRule::Surface {
            above: preferred,
            damping: None,
        }
    } else if is("FIXED_SURFACE_RELATIVE_HEIGHT") || is("FIXED_RELATIVE_TO_GROUND_AND_BUILDINGS") {
        HeightRule::Surface {
            above: preferred,
            damping: None,
        }
    } else if lifted && (is("SURFACE_RELATIVE_HEIGHT") || is("RELATIVE_TO_HIGHEST_LAYER")) {
        HeightRule::Surface {
            above: preferred,
            damping,
        }
    } else if is("FLOATING_Z") {
        HeightRule::Unchanged
    } else {
        HeightRule::ON_SURFACE
    }
}

pub(crate) fn flight_profile(
    record: &ObjectRecord,
    locomotors: &mint_sage_ini::LocomotorIndex,
    condition: &str,
) -> Option<FlightProfile> {
    let slot = record.locomotor_for(condition)?;
    let def = locomotors.get(slot.locomotor.as_deref()?)?;
    let airborne = def
        .airborne_targeting_height
        .filter(|above| above.is_finite())
        .map(|above| Airborne {
            category: AirborneCategory::of(&record.kind_of),
            above,
        });
    let height = height_rule(def);
    let swoop = match (
        def.swoop_acceleration_rate,
        def.swoop_standoff_height,
        def.swoop_standoff_radius,
        def.swoop_terminal_velocity,
    ) {
        (
            Some(acceleration_rate),
            Some(standoff_height),
            Some(standoff_radius),
            Some(terminal_velocity),
        ) => Some(SwoopProfile {
            acceleration_rate,
            standoff_height,
            standoff_radius,
            terminal_velocity,
        }),
        _ => None,
    };
    Some(FlightProfile {
        airborne,
        preferred_height: def.preferred_height,
        dive: dive_profile(def, height),
        charge_speed_fraction: def.charge_speed,
        charge_available: def.charge_available.unwrap_or(false),
        uncollided: def.non_dirty_transform.unwrap_or(false),
        look_ahead_mult: def
            .look_ahead_mult
            .filter(|mult| mult.is_finite() && *mult > 1.0)
            .unwrap_or(1.0),
        lift: def
            .lift
            .filter(|lift| lift.is_finite() && *lift >= 0.0)
            .unwrap_or(1.0),
        swoop,
        height,
        formation_priority: def.formation_priority.unwrap_or(DEFAULT_FORMATION_PRIORITY),
        river_modifier: def
            .river_modifier
            .filter(|share| share.is_finite() && *share >= 0.0)
            .unwrap_or(1.0),
    })
}

fn dive_profile(
    def: &mint_sage_ini::LocomotorDef,
    height: mint_movement::HeightRule,
) -> Option<DiveProfile> {
    let level = def.preferred_attack_height.filter(|h| h.is_finite())?;
    if height == mint_movement::HeightRule::ON_SURFACE {
        return None;
    }
    let reach = def
        .swoop_standoff_radius
        .filter(|r| r.is_finite())
        .unwrap_or(DEFAULT_SWOOP_STANDOFF);
    let standoff = def
        .swoop_standoff_height
        .filter(|h| h.is_finite())
        .unwrap_or(DEFAULT_SWOOP_STANDOFF);
    Some(DiveProfile {
        level,
        reach,
        slope: (standoff - level).abs() / reach,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod motion_limits_tests {
    use super::{motion_limits, ObjectRecord, NORMAL_CONDITION};
    use mint_sage_ini::{BlockTable, Document, LocomotorIndex};
    use mint_sage_modules::TemplateIndex;

    fn record_and_locomotors(text: &str, name: &str) -> (ObjectRecord, LocomotorIndex) {
        let document = Document::parse(name, text, &BlockTable::corpus_path()).unwrap();
        let mut locomotors = LocomotorIndex::new();
        locomotors.insert_document(&document);
        let mut index = TemplateIndex::new();
        index.insert_document(&document);
        (
            ObjectRecord::read(&index, name, &mint_sage_ini::ValueErrors::new()).unwrap(),
            locomotors,
        )
    }

    #[test]
    fn the_ini_rates_convert_through_the_locomotor_sets_own_speed() {
        let text = "\
            Locomotor Test\n  \
              Surfaces = GROUND\n  \
              Acceleration = 210\n  \
              Braking = 420\n  \
              TurnTime = 2000\n  \
              TurnThreshold = 90\n\
            End\n\
            Object Soldier\n  \
              LocomotorSet\n    Locomotor = Test\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Soldier");
        let limits = motion_limits(&record, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        assert_eq!(limits.max_speed, 45.0);
        assert!((limits.acceleration - 112.5).abs() < 0.01);
        assert!((limits.braking - 75.0).abs() < 0.01);
        assert!((limits.turn_rate - core::f32::consts::PI).abs() < 0.001);
        assert!((limits.turn_threshold - core::f32::consts::FRAC_PI_2).abs() < 0.001);
    }

    #[test]
    fn a_missing_locomotor_set_or_an_unresolved_locomotor_name_is_none() {
        let (keep, locomotors) = record_and_locomotors("Object Keep\nEnd\n", "Keep");
        assert!(motion_limits(&keep, &locomotors, NORMAL_CONDITION).is_none());

        let text = "Object Ghost\n  \
              LocomotorSet\n    Locomotor = NoSuchLocomotor\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n\
            End\n";
        let (ghost, locomotors) = record_and_locomotors(text, "Ghost");
        assert!(motion_limits(&ghost, &locomotors, NORMAL_CONDITION).is_none());
    }

    #[test]
    fn a_zero_speed_set_keeps_its_locomotors_turn_rate() {
        let text = "Locomotor Mount\n  Surfaces = GROUND\n  TurnTime = 10000\nEnd\n\
            Object Catapult\n  \
              LocomotorSet\n    Locomotor = Mount\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Catapult");
        let limits = motion_limits(&record, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        assert_eq!(limits.max_speed, 0.0);
        assert!((limits.turn_rate - core::f32::consts::TAU / 10.0).abs() < 1e-5);
    }

    #[test]
    fn absent_rate_fields_fall_back_rather_than_refusing_the_template() {
        let text = "Locomotor Bare\n  Surfaces = GROUND\nEnd\n\
            Object Soldier\n  \
              LocomotorSet\n    Locomotor = Bare\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Soldier");
        let limits = motion_limits(&record, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        assert_eq!(limits.max_speed, 30.0);
        assert_eq!(limits.acceleration, 30.0, "one second to reach full speed");
        assert_eq!(limits.braking, 30.0);
        assert!((limits.turn_rate - core::f32::consts::TAU).abs() < 1e-5);
        assert!(
            (limits.turn_threshold - 15.0_f32.to_radians()).abs() < 1e-6,
            "an absent threshold is 15 degrees, not zero (B776)"
        );
    }

    #[test]
    fn two_templates_sharing_a_locomotor_at_different_speeds_get_different_limits() {
        let text = "Locomotor Shared\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\nEnd\n\
            Object HordeBody\n  \
              LocomotorSet\n    Locomotor = Shared\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n\
            End\n\
            Object HordeMember\n  \
              LocomotorSet\n    Locomotor = Shared\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n\
            End\n";
        let document = Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap();
        let mut locomotors = LocomotorIndex::new();
        locomotors.insert_document(&document);
        let mut index = TemplateIndex::new();
        index.insert_document(&document);
        let body =
            ObjectRecord::read(&index, "HordeBody", &mint_sage_ini::ValueErrors::new()).unwrap();
        let member =
            ObjectRecord::read(&index, "HordeMember", &mint_sage_ini::ValueErrors::new()).unwrap();
        let body_limits = motion_limits(&body, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        let member_limits = motion_limits(&member, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        assert!(member_limits.max_speed > body_limits.max_speed);
        assert!(member_limits.acceleration > body_limits.acceleration);
    }

    #[test]
    fn a_second_locomotor_set_is_kept_separately_from_set_normal() {
        let text = "Locomotor Foot\n  Surfaces = GROUND\nEnd\n\
            Locomotor Horse\n  Surfaces = GROUND\nEnd\n\
            Object Rider\n  \
              LocomotorSet\n    Locomotor = Foot\n    Condition = SET_NORMAL\n    Speed = 20\n  End\n\
              LocomotorSet\n    Locomotor = Horse\n    Condition = SET_MOUNTED\n    Speed = 60\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Rider");
        let on_foot = motion_limits(&record, &locomotors, NORMAL_CONDITION)
            .unwrap()
            .limits;
        assert_eq!(on_foot.max_speed, 20.0);
        let other = super::other_locomotor_sets(&record, &locomotors);
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].0, "MOUNTED");
        assert_eq!(other[0].1.limits.max_speed, 60.0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod flight_profile_tests {
    use super::{flight_profile, AirborneCategory, ObjectRecord, NORMAL_CONDITION};
    use mint_sage_ini::{BlockTable, Document, LocomotorIndex};
    use mint_sage_modules::TemplateIndex;

    fn record_and_locomotors(text: &str, name: &str) -> (ObjectRecord, LocomotorIndex) {
        let document = Document::parse(name, text, &BlockTable::corpus_path()).unwrap();
        let mut locomotors = LocomotorIndex::new();
        locomotors.insert_document(&document);
        let mut index = TemplateIndex::new();
        index.insert_document(&document);
        (
            ObjectRecord::read(&index, name, &mint_sage_ini::ValueErrors::new()).unwrap(),
            locomotors,
        )
    }

    #[test]
    fn an_air_locomotor_with_a_monster_kind_of_reads_as_airborne_monster() {
        let text = "\
            Locomotor FellBeastLocomotor\n  \
              Surfaces = AIR\n  \
              PreferredHeight = 121\n  \
              PreferredAttackHeight = 5\n  \
              SwoopAccelerationRate = 0.012\n  \
              SwoopStandoffHeight = 119.0\n  \
              SwoopStandoffRadius = 310.0\n  \
              SwoopTerminalVelocity = 0.18\n  \
              AirborneTargetingHeight = 25\n  \
              ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\n\
            End\n\
            Object FellBeast\n  KindOf = HERO MONSTER\n  \
              LocomotorSet\n    Locomotor = FellBeastLocomotor\n    Condition = SET_NORMAL\n    Speed = 90\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "FellBeast");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        let airborne = flight.airborne.unwrap();
        assert_eq!(
            (airborne.category, airborne.above),
            (AirborneCategory::Monster, 25.0)
        );
        assert_eq!(flight.preferred_height, Some(121.0));
        let dive = flight.dive.unwrap();
        assert_eq!((dive.level, dive.reach), (5.0, 310.0));
        let swoop = flight.swoop.unwrap();
        assert_eq!(swoop.standoff_radius, 310.0);
        assert_eq!(swoop.terminal_velocity, 0.18);
    }

    #[test]
    fn a_ground_locomotor_is_never_airborne_and_carries_the_charge_fields() {
        let text = "\
            Locomotor RohirrimLocomotor\n  \
              Surfaces = GROUND\n  \
              ChargeSpeed = 200%\n  \
              ChargeAvailable = Yes\n\
            End\n\
            Object Rohirrim\n  KindOf = CAVALRY\n  \
              LocomotorSet\n    Locomotor = RohirrimLocomotor\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Rohirrim");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(flight.airborne, None);
        assert_eq!(flight.charge_speed_fraction, Some(2.0));
        assert!(flight.charge_available);
        assert!(flight.swoop.is_none());
    }

    #[test]
    fn a_template_with_no_locomotor_set_has_no_flight_profile() {
        let (record, locomotors) = record_and_locomotors("Object Keep\nEnd\n", "Keep");
        assert!(flight_profile(&record, &locomotors, NORMAL_CONDITION).is_none());
    }

    #[test]
    fn a_lifted_locomotor_damps_by_default_and_by_its_own_field() {
        let text = "\
            Locomotor CrebainLocomotor\n  \
              Surfaces = AIR\n  \
              PreferredHeight = 50\n  \
              ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\n\
            End\n\
            Object Crebain\n  KindOf = MONSTER\n  \
              LocomotorSet\n    Locomotor = CrebainLocomotor\n    Condition = SET_NORMAL\n    Speed = 90\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Crebain");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(
            flight.height,
            mint_movement::HeightRule::Surface {
                above: 50.0,
                damping: Some(1.0)
            }
        );

        let text = "\
            Locomotor ComancheLocomotor\n  \
              Surfaces = AIR\n  \
              PreferredHeight = 100\n  \
              PreferredHeightDamping = 0.5\n  \
              ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\n\
            End\n\
            Object Comanche\n  KindOf = VEHICLE\n  \
              LocomotorSet\n    Locomotor = ComancheLocomotor\n    Condition = SET_NORMAL\n    Speed = 90\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Comanche");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(
            flight.height,
            mint_movement::HeightRule::Surface {
                above: 100.0,
                damping: Some(0.5)
            }
        );
    }

    #[test]
    fn fixed_absolute_height_lands_at_once() {
        let text = "\
            Locomotor WaterWaveLocomotor\n  \
              Surfaces = WATER\n  \
              PreferredHeight = 8.32\n  \
              ZAxisBehavior = FIXED_ABSOLUTE_HEIGHT\n\
            End\n\
            Object WaterWave\n  KindOf = VEHICLE\n  \
              LocomotorSet\n    Locomotor = WaterWaveLocomotor\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "WaterWave");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(
            flight.height,
            mint_movement::HeightRule::Fixed {
                height: 8.32,
                damping: None
            }
        );
    }

    #[test]
    fn sea_level_adds_preferred_height_and_is_never_damped() {
        let text = "\
            Locomotor SubLocomotor\n  \
              Surfaces = WATER\n  \
              PreferredHeight = -15\n  \
              ZAxisBehavior = SEA_LEVEL\n\
            End\n\
            Object Sub\n  KindOf = VEHICLE\n  \
              LocomotorSet\n    Locomotor = SubLocomotor\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Sub");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(
            flight.height,
            mint_movement::HeightRule::Surface {
                above: -15.0,
                damping: None
            }
        );
    }

    #[test]
    fn floating_z_leaves_height_unmanaged() {
        let text = "\
            Locomotor BuoyLocomotor\n  \
              Surfaces = WATER\n  \
              PreferredHeight = 3\n  \
              ZAxisBehavior = FLOATING_Z\n\
            End\n\
            Object Buoy\n  KindOf = VEHICLE\n  \
              LocomotorSet\n    Locomotor = BuoyLocomotor\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n\
            End\n";
        let (record, locomotors) = record_and_locomotors(text, "Buoy");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(flight.height, mint_movement::HeightRule::Unchanged);
    }

    #[test]
    fn floating_z_keeps_its_height_even_with_stick_to_ground() {
        let text = "            Locomotor GhostLocomotor
                Surfaces = GROUND
                PreferredHeight = 3
                ZAxisBehavior = FLOATING_Z
                StickToGround = Yes
            End
            Object Ghost
  KindOf = INFANTRY
                LocomotorSet
    Locomotor = GhostLocomotor
    Condition = SET_NORMAL
    Speed = 10
  End
            End
";
        let (record, locomotors) = record_and_locomotors(text, "Ghost");
        let flight = flight_profile(&record, &locomotors, NORMAL_CONDITION).unwrap();
        assert_eq!(flight.height, mint_movement::HeightRule::Unchanged);
    }
}
