use std::collections::BTreeMap;

use mint_sim::{Component, Frame, LogicFrames, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PowerId(u16);

impl PowerId {
    pub(crate) const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PowerNames {
    by_name: BTreeMap<String, PowerId>,
}

impl PowerNames {
    pub(crate) fn build(names: impl Iterator<Item = String>) -> Self {
        let unique: std::collections::BTreeSet<String> =
            names.map(|name| name.to_ascii_lowercase()).collect();
        let by_name = unique
            .into_iter()
            .enumerate()
            .map(|(position, name)| {
                let id = u16::try_from(position).unwrap_or(u16::MAX);
                (name, PowerId(id))
            })
            .collect();
        Self { by_name }
    }

    pub(crate) fn id(&self, name: &str) -> Option<PowerId> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.by_name.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, PowerId)> {
        self.by_name.iter().map(|(name, &id)| (name.as_str(), id))
    }

    pub(crate) fn reload_table(
        &self,
        special_powers: &mint_sage_ini::SpecialPowerIndex,
    ) -> Vec<(Option<LogicFrames>, bool)> {
        let mut table = vec![(None, false); self.len()];
        for (name, id) in self.iter() {
            if let Some(slot) = table.get_mut(usize::from(id.get())) {
                if let Some(def) = special_powers.get(name) {
                    *slot = (
                        def.reload_time_ms
                            .map(mint_sage_modules::retail_duration_ticks),
                        def.shared_synced_timer,
                    );
                }
            }
        }
        table
    }

    pub(crate) fn required_sciences(
        &self,
        special_powers: &mint_sage_ini::SpecialPowerIndex,
        sciences: &crate::sciences::ScienceTable,
    ) -> Vec<Vec<crate::sciences::ScienceId>> {
        let mut table = vec![Vec::new(); self.len()];
        for (name, id) in self.iter() {
            let Some(def) = special_powers.get(name) else {
                continue;
            };
            if def.required_sciences.is_empty() {
                continue;
            }
            let mut needs: Vec<crate::sciences::ScienceId> = def
                .required_sciences
                .iter()
                .filter_map(|science| sciences.id(science))
                .collect();
            if needs.is_empty() {
                needs.push(crate::sciences::ScienceId::new(u16::MAX));
            }
            if let Some(slot) = table.get_mut(usize::from(id.get())) {
                *slot = needs;
            }
        }
        table
    }

    pub(crate) fn cast_rules(
        &self,
        special_powers: &mint_sage_ini::SpecialPowerIndex,
    ) -> Vec<PowerCastRule> {
        let mut table = vec![PowerCastRule::default(); self.len()];
        for (name, id) in self.iter() {
            let Some(def) = special_powers.get(name) else {
                continue;
            };
            let Some(slot) = table.get_mut(usize::from(id.get())) else {
                continue;
            };
            slot.max_cast_range = def.max_cast_range;
            slot.captures = def
                .power_enum
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(CAPTURE_BUILDING_ENUM));
            slot.grab = def.power_enum.as_deref().and_then(GrabEnum::of);
            slot.charge_attack = def
                .power_enum
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(CHARGE_ATTACK_ENUM));
            slot.effect = PowerEnumEffect::of(def.power_enum.as_deref());
            slot.cancels_disguise = def.power_enum.as_deref().is_some_and(|value| {
                value.eq_ignore_ascii_case(SMITE_CANCEL_DISGUISE_ENUM)
                    || value.eq_ignore_ascii_case(ATTRIBUTEMOD_CANCEL_DISGUISE_ENUM)
            });
            slot.initiate_sound = def.initiate_sound.clone();
            slot.initiate_at_location_sound = def.initiate_at_location_sound.clone();
            slot.radius_cursor = def.radius_cursor_radius;
            slot.recharge_discount = def.flags.respect_recharge_time_discount;
            slot.water_ok = def.flags.water_ok;
            slot.needs_object_filter = def.flags.needs_object_filter;
            slot.shroud = ShroudNeed::of(def.power_enum.as_deref());
            slot.object_filter = (!def.object_filter.is_empty())
                .then(|| crate::defeat::ObjectFilter::parse(&def.object_filter));
            slot.forbidden = match def.forbidden_object_range {
                Some(range) if !def.forbidden_object_filter.is_empty() => Some((
                    crate::defeat::ObjectFilter::parse(&def.forbidden_object_filter),
                    range,
                )),
                _ => None,
            };
        }
        table
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PowerCastRule {
    pub(crate) max_cast_range: Option<f32>,
    pub(crate) object_filter: Option<crate::defeat::ObjectFilter>,
    pub(crate) forbidden: Option<(crate::defeat::ObjectFilter, f32)>,
    pub(crate) captures: bool,
    pub(crate) grab: Option<GrabEnum>,
    pub(crate) charge_attack: bool,
    pub(crate) effect: PowerEnumEffect,
    pub(crate) radius_cursor: Option<f32>,
    pub(crate) recharge_discount: bool,
    pub(crate) water_ok: bool,
    pub(crate) needs_object_filter: bool,
    pub(crate) shroud: ShroudNeed,
    pub(crate) cancels_disguise: bool,
    pub(crate) initiate_sound: Option<String>,
    pub(crate) initiate_at_location_sound: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShroudNeed {
    #[default]
    Any,
    NotShrouded,
    Visible,
}

impl ShroudNeed {
    fn of(power_enum: Option<&str>) -> Self {
        let Some(value) = power_enum else {
            return Self::Any;
        };
        let named = |list: &[&str]| list.iter().any(|name| value.eq_ignore_ascii_case(name));
        if named(&SHROUD_VISIBLE_ENUMS) {
            Self::Visible
        } else if named(&SHROUD_NOT_SHROUDED_ENUMS) {
            Self::NotShrouded
        } else {
            Self::Any
        }
    }
}

const SHROUD_VISIBLE_ENUMS: [&str; 43] = [
    "SPECIAL_PRINCE_OF_DOL_ARMOTH",
    "SPECIAL_OSGILIATH_VETERANS",
    "SPECIAL_RANGER_AMBUSH",
    "SPECIAL_CHARGE_ATTACK",
    "SPECIAL_ARROW_STORM",
    "SPECIAL_LEVEL_POSITION",
    "SPECIAL_BALROG_BREATH",
    "SPECIAL_BALROG_WINGS",
    "SPECIAL_TRAINING",
    "SPECIAL_SPELL_BOOK_HEAL",
    "SPECIAL_SPELL_BOOK_RALLYING_CALL",
    "SPECIAL_SPELL_BOOK_WAR_CHANT",
    "SPECIAL_SPELL_BOOK_DEVASTATION",
    "SPECIAL_SPELL_BOOK_UNTAMED_ALLEGIANCE",
    "SPECIAL_GIMLI_LEAP",
    "SPECIAL_PERSONAL_FLOOD",
    "SPECIAL_CURSE_ENEMY",
    "SPECIAL_SPELL_BOOK_BLIGHT",
    "SPECIAL_SPELL_BOOK_SNOWBIND",
    "SPECIAL_SPELL_BOOK_CHILL_WIND",
    "SPECIAL_AOE_ATTACK_HEAL",
    "SPECIAL_SPELL_BOOK_SPAWN_LONE_TOWER",
    "SPECIAL_SPELL_BOOK_UNDERMINE",
    "SPECIAL_SPELL_BOOK_CITADEL",
    "SPECIAL_SPELL_BOOK_BARRICADE",
    "SPECIAL_SPELL_BOOK_TOM_BOMBADIL",
    "SPECIAL_SPELL_BOOK_HOBBIT_ALLIES",
    "SPECIAL_SPELL_BOOK_MEN_OF_DALE_ALLIES",
    "SPECIAL_SPELL_BOOK_ROHAN_ALLIES",
    "SPECIAL_SPELL_BOOK_DUNEDAIN_ALLIES",
    "SPECIAL_SPELL_BOOK_ENT_ALLIES",
    "SPECIAL_SPELL_BOOK_WILD_MEN_ALLIES",
    "SPECIAL_SPELL_BOOK_SPIDERLING_ALLIES",
    "SPECIAL_SPELL_BOOK_AWAKEN_WYRM",
    "SPECIAL_SPELL_BOOK_WATCHER_ALLY",
    "SPECIAL_SPELL_BOOK_BALROG_ALLY",
    "SPECIAL_SPELL_BOOK_DRAGON_ALLY",
    "SPECIAL_TELEPORT_LIST_TO_POSITION",
    "SPECIAL_SPELL_BOOK_GENERAL_SUMMON",
    "SPECIAL_SPELL_BOOK_ARMY_OF_THE_DEAD",
    "SPECIAL_SPAWN_OATHBREAKERS",
    "SPECIAL_CALL_OF_THE_DEEP",
    "SPECIAL_SKULL_TOTEM",
];

const SHROUD_NOT_SHROUDED_ENUMS: [&str; 10] = [
    "SPECIAL_TOGGLE_MOUNTED",
    "SPECIAL_DOMINATE_ENEMY",
    "SPECIAL_SPELL_BOOK_EAGLE_ALLIES",
    "SPECIAL_KINGS_FAVOR",
    "SPECIAL_TELEPORT_TEAM_TO_CASTER",
    "SPECIAL_STORE_LIST_1",
    "SPECIAL_STORE_LIST_2",
    "SPECIAL_SUMMON_ALLIES",
    "SPECIAL_SPELL_BOOK_ELVEN_WOOD",
    "SPECIAL_SPELL_BOOK_TAINT",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PowerEnumEffect {
    #[default]
    InTheModule,
    HealBurst,
    Fear,
}

impl PowerEnumEffect {
    fn of(power_enum: Option<&str>) -> Self {
        let Some(value) = power_enum else {
            return Self::InTheModule;
        };
        if value.eq_ignore_ascii_case(ATHELAS_ENUM) || value.eq_ignore_ascii_case(ELVEN_GRACE_ENUM)
        {
            Self::HealBurst
        } else if value.eq_ignore_ascii_case(SCREECH_ENUM) {
            Self::Fear
        } else {
            Self::InTheModule
        }
    }
}

const ATHELAS_ENUM: &str = "SPECIAL_ATHELAS";
const ELVEN_GRACE_ENUM: &str = "SPECIAL_ELVEN_GRACE";
const SCREECH_ENUM: &str = "SPECIAL_SCREECH";

pub(crate) const CAPTURE_BUILDING_ENUM: &str = "SPECIAL_INFANTRY_CAPTURE_BUILDING";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrabEnum {
    Passenger,
    Chunk,
}

impl GrabEnum {
    fn of(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("SPECIAL_GRAB_PASSENGER") {
            Some(Self::Passenger)
        } else if value.eq_ignore_ascii_case("SPECIAL_GRAB_CHUNK") {
            Some(Self::Chunk)
        } else {
            None
        }
    }

    pub(crate) const fn vendor_kind(self) -> &'static str {
        match self {
            Self::Passenger => "ROCK_VENDOR",
            Self::Chunk => "CHUNK_VENDOR",
        }
    }
}

const CHARGE_ATTACK_ENUM: &str = "SPECIAL_CHARGE_ATTACK";

const SMITE_CANCEL_DISGUISE_ENUM: &str = "SPECIAL_SMITE_CANCELDISGUISE";
const ATTRIBUTEMOD_CANCEL_DISGUISE_ENUM: &str = "SPECIAL_ATTRIBUTEMOD_CANCELDISGUISE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerInstance {
    pub(crate) power: PowerId,
    pub(crate) available_on: Frame,
    pub(crate) reload: LogicFrames,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PowerBook(pub(crate) Vec<PowerInstance>);

impl Component for PowerBook {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for instance in &self.0 {
            hasher.write_u16(instance.power.get());
            hasher.write_u64(instance.available_on.get());
            hasher.write_u64(u64::from(instance.reload.get()));
        }
    }
}

impl PowerBook {
    pub(crate) fn instance(&self, power: PowerId) -> Option<&PowerInstance> {
        self.0.iter().find(|instance| instance.power == power)
    }

    pub(crate) fn set_available_on(&mut self, power: PowerId, on: Frame, reload: LogicFrames) {
        if let Some(instance) = self.0.iter_mut().find(|instance| instance.power == power) {
            instance.available_on = on;
            instance.reload = reload;
        } else {
            self.0.push(PowerInstance {
                power,
                available_on: on,
                reload,
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlayerPowerCooldowns(pub(crate) Vec<(PowerId, Frame, LogicFrames)>);

impl Component for PlayerPowerCooldowns {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (power, on, reload) in &self.0 {
            hasher.write_u16(power.get());
            hasher.write_u64(on.get());
            hasher.write_u64(u64::from(reload.get()));
        }
    }
}

impl PlayerPowerCooldowns {
    pub(crate) fn available_on(&self, power: PowerId) -> Frame {
        self.0
            .iter()
            .find(|(held, _, _)| *held == power)
            .map_or(Frame::ZERO, |(_, on, _)| *on)
    }

    pub(crate) fn cooldown(&self, power: PowerId) -> Option<(Frame, LogicFrames)> {
        self.0
            .iter()
            .find(|(held, _, _)| *held == power)
            .map(|(_, on, reload)| (*on, *reload))
    }

    pub(crate) fn set_available_on(&mut self, power: PowerId, on: Frame, reload: LogicFrames) {
        if let Some(entry) = self.0.iter_mut().find(|(held, _, _)| *held == power) {
            entry.1 = on;
            entry.2 = reload;
        } else {
            self.0.push((power, on, reload));
        }
    }
}

#[must_use]
pub(crate) const fn is_ready(paused: bool, available_on: Frame, now: Frame) -> bool {
    !paused && now.get() >= available_on.get()
}

#[must_use]
pub(crate) fn start_recharge(now: Frame, reload: LogicFrames) -> Frame {
    now.checked_add(reload).unwrap_or(Frame::FOREVER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_names_are_assigned_alphabetically_and_case_insensitively() {
        let names = PowerNames::build(
            [
                "SpecialAbilityZeta",
                "SpecialAbilityAlpha",
                "specialabilityalpha",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        assert_eq!(names.len(), 2, "the duplicate collapses");
        assert_eq!(
            names.id("SpecialAbilityAlpha"),
            names.id("SPECIALABILITYALPHA")
        );
        assert!(names.id("SpecialAbilityAlpha").unwrap() < names.id("SpecialAbilityZeta").unwrap());
        assert!(names.id("Missing").is_none());
    }

    #[test]
    fn a_paused_power_is_never_ready_regardless_of_its_timer() {
        assert!(!is_ready(true, Frame::ZERO, Frame::new(1_000_000)));
    }

    #[test]
    fn readiness_flips_exactly_on_the_available_frame() {
        let available = Frame::new(100);
        assert!(!is_ready(false, available, Frame::new(99)));
        assert!(is_ready(false, available, Frame::new(100)));
        assert!(is_ready(false, available, Frame::new(101)));
    }

    #[test]
    fn starting_recharge_sets_available_on_to_now_plus_reload() {
        let now = Frame::new(30);
        let reload = LogicFrames::from_frames(60);
        assert_eq!(start_recharge(now, reload), Frame::new(90));
    }

    #[test]
    fn a_zero_length_reload_is_ready_the_very_next_frame_not_the_same_one() {
        let now = Frame::new(30);
        let available = start_recharge(now, LogicFrames::ZERO);
        assert_eq!(available, now);
        assert!(
            is_ready(false, available, now),
            "same frame is already ready"
        );
    }

    #[test]
    fn a_power_book_looks_up_and_updates_its_own_instance() {
        let mut book = PowerBook::default();
        let power = PowerId(3);
        assert!(book.instance(power).is_none());
        book.set_available_on(power, Frame::new(50), LogicFrames::from_frames(50));
        assert_eq!(
            book.instance(power).map(|i| i.available_on),
            Some(Frame::new(50))
        );
        book.set_available_on(power, Frame::new(80), LogicFrames::from_frames(80));
        assert_eq!(book.0.len(), 1, "updates in place rather than duplicating");
        assert_eq!(
            book.instance(power).map(|i| i.available_on),
            Some(Frame::new(80))
        );
    }

    #[test]
    fn player_cooldowns_default_to_ready_for_an_unfired_power() {
        let cooldowns = PlayerPowerCooldowns::default();
        assert_eq!(cooldowns.available_on(PowerId(0)), Frame::ZERO);
    }

    #[test]
    fn cast_rules_resolve_range_object_filter_and_forbidden() {
        let doc = mint_sage_ini::Document::parse(
            "specialpower.ini",
            "SpecialPower Grab\n  Flags = LIMIT_DISTANCE\n  MaxCastRange = 200\n  \
                ObjectFilter = NONE +INFANTRY -HERO\nEnd\n\
             SpecialPower Summon\n  Flags = NO_FORBIDDEN_OBJECTS\n  \
                ForbiddenObjectFilter = ANY +STRUCTURE\n  ForbiddenObjectRange = 50\nEnd\n\
             SpecialPower Plain\nEnd\n",
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        let mut index = mint_sage_ini::SpecialPowerIndex::new();
        index.insert_document(&doc, &mint_sage_ini::MacroTable::new());
        let names = PowerNames::build(["Grab", "Summon", "Plain"].into_iter().map(str::to_owned));
        let rules = names.cast_rules(&index);

        let grab = &rules[usize::from(names.id("Grab").unwrap().get())];
        assert_eq!(grab.max_cast_range, Some(200.0));
        assert!(grab.object_filter.is_some());
        assert!(grab.forbidden.is_none());

        let summon = &rules[usize::from(names.id("Summon").unwrap().get())];
        assert!(summon.max_cast_range.is_none());
        assert!(summon.object_filter.is_none());
        let (_, range) = summon.forbidden.as_ref().unwrap();
        assert_eq!(*range, 50.0);

        let plain = &rules[usize::from(names.id("Plain").unwrap().get())];
        assert!(plain.max_cast_range.is_none());
        assert!(plain.object_filter.is_none());
        assert!(plain.forbidden.is_none());
    }
}
