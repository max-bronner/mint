use std::collections::BTreeMap;

use mint_sage_ini::value::parse_percent;
use mint_sage_ini::Block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadiusAffects(u8);

impl Default for RadiusAffects {
    fn default() -> Self {
        Self(Self::ALLIES | Self::ENEMIES | Self::NEUTRALS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Allies,
    Enemies,
    Neutral,
}

impl RadiusAffects {
    const SELF: u8 = 0x01;
    const ALLIES: u8 = 0x02;
    const ENEMIES: u8 = 0x04;
    const NEUTRALS: u8 = 0x08;
    const SUICIDE: u8 = 0x10;
    const NOT_SIMILAR: u8 = 0x20;
    const NOT_AIRBORNE: u8 = 0x40;

    #[must_use]
    pub fn parse<S: AsRef<str>>(tokens: &[S]) -> Self {
        Self(tokens.iter().fold(0, |mask, token| {
            mask | match token.as_ref().to_ascii_uppercase().as_str() {
                "SELF" => Self::SELF,
                "ALLIES" => Self::ALLIES,
                "ENEMIES" => Self::ENEMIES,
                "NEUTRALS" => Self::NEUTRALS,
                "SUICIDE" => Self::SUICIDE,
                "NOT_SIMILAR" => Self::NOT_SIMILAR,
                "NOT_AIRBORNE" => Self::NOT_AIRBORNE,
                _ => 0,
            }
        }))
    }

    #[must_use]
    pub const fn admits(
        self,
        relation: Relation,
        is_source: bool,
        similar: bool,
        relation_exempt: bool,
    ) -> bool {
        if is_source && self.0 & Self::SELF == 0 {
            return false;
        }
        if self.0 & Self::NOT_SIMILAR != 0 && similar && matches!(relation, Relation::Allies) {
            return false;
        }
        if relation_exempt {
            return true;
        }
        let required = match relation {
            Relation::Allies => Self::ALLIES,
            Relation::Enemies => Self::ENEMIES,
            Relation::Neutral => Self::NEUTRALS,
        };
        self.0 & required != 0
    }

    #[must_use]
    pub const fn affects_allies(self) -> bool {
        self.0 & Self::ALLIES != 0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VictimRules {
    pub special: Vec<String>,
    pub scalars: Vec<(f32, Vec<String>)>,
}

impl VictimRules {
    #[must_use]
    pub fn read(block: &Block, unreadable: &mut usize) -> Option<Self> {
        let special: Vec<String> = block
            .field("SpecialObjectFilter")
            .map(|field| field.args.clone())
            .unwrap_or_default();
        let mut scalars = Vec::new();
        for field in block.fields_named("DamageScalar") {
            let Some((first, filter)) = field.args.split_first() else {
                continue;
            };
            let factor = parse_percent(first)
                .or_else(|| mint_sage_ini::value::parse_float(first))
                .filter(|value| value.is_finite() && *value >= 0.0);
            match factor {
                Some(factor) => scalars.push((factor, filter.to_vec())),
                None => *unreadable = unreadable.saturating_add(1),
            }
        }
        (!special.is_empty() || !scalars.is_empty()).then_some(Self { special, scalars })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VictimRuleTable(BTreeMap<(usize, usize), VictimRules>);

impl VictimRuleTable {
    pub fn insert(&mut self, weapon: usize, nugget: usize, rules: VictimRules) {
        self.0.insert((weapon, nugget), rules);
    }

    pub fn clear_weapon(&mut self, weapon: usize) {
        self.0.retain(|(held, _), _| *held != weapon);
    }

    #[must_use]
    pub fn get(&self, weapon: usize, nugget: usize) -> Option<&VictimRules> {
        self.0.get(&(weapon, nugget))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(usize, usize), &VictimRules)> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    #[test]
    fn a_relation_exempt_victim_is_reached_by_an_enemies_only_mask() {
        let mask = RadiusAffects::parse(&["ENEMIES"]);
        assert!(mask.admits(Relation::Neutral, false, false, true));
        assert!(!mask.admits(Relation::Neutral, false, false, false));
        assert!(!RadiusAffects::parse(&["ENEMIES"]).admits(Relation::Neutral, true, false, true));
    }

    #[test]
    fn the_default_mask_reaches_every_side_but_not_the_source() {
        let mask = RadiusAffects::default();
        for relation in [Relation::Allies, Relation::Enemies, Relation::Neutral] {
            assert!(mask.admits(relation, false, false, false), "{relation:?}");
        }
        assert!(!mask.admits(Relation::Allies, true, false, false));
        assert!(RadiusAffects::parse(&["SELF", "ALLIES"]).admits(
            Relation::Allies,
            true,
            false,
            false
        ));
    }

    #[test]
    fn each_relationship_needs_its_own_bit() {
        let mask = RadiusAffects::parse(&["ENEMIES", "NEUTRALS"]);
        assert!(mask.admits(Relation::Enemies, false, false, false));
        assert!(mask.admits(Relation::Neutral, false, false, false));
        assert!(!mask.admits(Relation::Allies, false, false, false));
        let none = RadiusAffects::parse::<&str>(&[]);
        assert!(!none.admits(Relation::Enemies, false, false, false));
        assert_eq!(RadiusAffects::parse(&["mines"]).bits(), 0);
    }

    #[test]
    fn not_similar_spares_only_allied_objects_of_the_same_template() {
        let mask = RadiusAffects::parse(&["ALLIES", "ENEMIES", "not_similar"]);
        assert!(!mask.admits(Relation::Allies, false, true, false));
        assert!(mask.admits(Relation::Allies, false, false, false));
        assert!(mask.admits(Relation::Enemies, false, true, false));
    }

    fn nugget(text: &str) -> Document {
        Document::parse("weapon.ini", text, &BlockTable::priority_blocks_path()).unwrap()
    }

    #[test]
    fn a_nuggets_scalars_and_special_filter_read_as_tokens() {
        let doc = nugget(
            "Weapon EarthquakePhaseOneWeapon\n\tDamageNugget\n\t\tDamage = 2000\n\t\t\
             DamageScalar = 25% ALL -STRUCTURE\n\t\tDamageScalar = 50% NONE +COMMANDCENTER\n\t\t\
             DamageScalar = LOTS NONE +MINE\n\t\tSpecialObjectFilter = NONE +RohanGenericEnt ENEMIES\n\t\t\
             DamageType = SIEGE\n\tEnd\nEnd\n",
        );
        let block = doc.roots()[0].blocks().next().unwrap();
        let mut bad = 0;
        let rules = VictimRules::read(block, &mut bad).unwrap();
        assert_eq!(bad, 1);
        assert_eq!(rules.scalars.len(), 2);
        assert_eq!(rules.scalars[0].0, 0.25);
        assert_eq!(rules.scalars[1].1, ["NONE", "+COMMANDCENTER"]);
        assert_eq!(rules.special, ["NONE", "+RohanGenericEnt", "ENEMIES"]);
        let plain = nugget("Weapon W\n\tDamageNugget\n\t\tDamage = 1\n\tEnd\nEnd\n");
        assert!(VictimRules::read(plain.roots()[0].blocks().next().unwrap(), &mut bad).is_none());
    }
}
