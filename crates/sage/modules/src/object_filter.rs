#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterBase {
    Any,
    All,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    SamePlayer,
    Allies,
    Enemies,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub same_player: bool,
    pub allied: bool,
    pub hostile: bool,
}

impl Standing {
    pub const ENEMY: Self = Self {
        same_player: false,
        allied: false,
        hostile: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    StrictName(String),
    Plain(String),
}

impl Token {
    fn parse(text: &str) -> Self {
        text.strip_prefix("S:")
            .or_else(|| text.strip_prefix("s:"))
            .map_or_else(
                || Self::Plain(text.to_ascii_uppercase()),
                |name| Self::StrictName(name.to_ascii_uppercase()),
            )
    }

    fn names(&self, name: &str) -> bool {
        self.text().eq_ignore_ascii_case(name)
    }

    fn text(&self) -> &str {
        match self {
            Self::StrictName(text) | Self::Plain(text) => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFilter {
    base: FilterBase,
    add: Vec<Token>,
    remove: Vec<Token>,
    relations: Vec<Relation>,
}

impl Default for ObjectFilter {
    fn default() -> Self {
        Self {
            base: FilterBase::Any,
            add: Vec::new(),
            remove: Vec::new(),
            relations: Vec::new(),
        }
    }
}

impl ObjectFilter {
    #[must_use]
    pub fn parse(tokens: &[String]) -> Self {
        let mut filter = Self::default();
        let mut base_seen = false;
        for token in tokens {
            if let Some(name) = token.strip_prefix('+') {
                filter.add.push(Token::parse(name));
            } else if let Some(name) = token.strip_prefix('-') {
                filter.remove.push(Token::parse(name));
            } else {
                let relation = match token.to_ascii_uppercase().as_str() {
                    "SAME_PLAYER" => Some(Relation::SamePlayer),
                    "ALLIES" => Some(Relation::Allies),
                    "ENEMIES" => Some(Relation::Enemies),
                    "NEUTRAL" => Some(Relation::Neutral),
                    _ => None,
                };
                if let Some(relation) = relation {
                    filter.relations.push(relation);
                } else if !base_seen {
                    filter.base = if token.eq_ignore_ascii_case("ALL") {
                        FilterBase::All
                    } else if token.eq_ignore_ascii_case("NONE") {
                        FilterBase::None
                    } else {
                        FilterBase::Any
                    };
                    base_seen = true;
                }
            }
        }
        filter
    }

    fn holds_kind(kind_of: &[String], token: &Token) -> bool {
        match token {
            Token::StrictName(_) => false,
            Token::Plain(text) => kind_of.iter().any(|kind| kind.eq_ignore_ascii_case(text)),
        }
    }

    #[must_use]
    pub fn passes(&self, kind_of: &[String], name: &str) -> bool {
        self.passes_inner(kind_of, name, None)
    }

    #[must_use]
    pub fn passes_equiv(
        &self,
        kind_of: &[String],
        name: &str,
        equivalent: &dyn Fn(&str, &str) -> bool,
    ) -> bool {
        self.passes_inner(kind_of, name, Some(equivalent))
    }

    fn passes_inner(
        &self,
        kind_of: &[String],
        name: &str,
        equivalent: Option<&dyn Fn(&str, &str) -> bool>,
    ) -> bool {
        let names = |token: &Token| {
            token.names(name)
                || matches!(token, Token::Plain(text) if equivalent.is_some_and(|f| f(text, name)))
        };
        if self.remove.iter().any(names) {
            return false;
        }
        if self.add.iter().any(names) {
            return true;
        }
        if self
            .remove
            .iter()
            .any(|token| Self::holds_kind(kind_of, token))
        {
            return false;
        }
        if self.add.is_empty() {
            return self.base != FilterBase::None;
        }
        let holds = |token: &Token| Self::holds_kind(kind_of, token);
        match self.base {
            FilterBase::All | FilterBase::None => self.add.iter().all(holds),
            FilterBase::Any => self.add.iter().any(holds),
        }
    }

    #[must_use]
    pub fn excludes(&self, token: &str) -> bool {
        self.remove
            .iter()
            .any(|t| t.text().eq_ignore_ascii_case(token))
    }

    #[must_use]
    pub fn passes_relation(&self, same_player: bool, allied: bool, hostile: bool) -> bool {
        self.relations.is_empty() || self.names_relation_of(same_player, allied, hostile)
    }

    #[must_use]
    pub fn passes_threat(&self, standing: Standing) -> bool {
        if self.relations.is_empty() {
            standing.hostile
        } else {
            self.names_relation_of(standing.same_player, standing.allied, standing.hostile)
        }
    }

    fn names_relation_of(&self, same_player: bool, allied: bool, hostile: bool) -> bool {
        self.relations.iter().any(|relation| match relation {
            Relation::SamePlayer => same_player,
            Relation::Allies => same_player || allied,
            Relation::Enemies => hostile,
            Relation::Neutral => !same_player && !allied && !hostile,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_shipped_structure_filter_keeps_a_fortress_and_drops_economy_and_ignored() {
        let filter = ObjectFilter::parse(&tokens(
            "NONE +STRUCTURE -IGNORE_FOR_VICTORY -UNATTACKABLE -ECONOMY_STRUCTURE \
             -WALL_UPGRADE -WALL_HUB -WALL_SEGMENT -DEFENSIVE_WALL -Inn -ShipWright \
             -Outpost -SignalFire -CaptureFlag -ShireGreenDragon",
        ));
        let fortress = vec![String::from("STRUCTURE"), String::from("SELECTABLE")];
        assert!(filter.passes(&fortress, "MenFortress"));
        let farm = vec![String::from("STRUCTURE"), String::from("ECONOMY_STRUCTURE")];
        assert!(!filter.passes(&farm, "GondorFarm"));
        let tower = vec![
            String::from("STRUCTURE"),
            String::from("IGNORE_FOR_VICTORY"),
        ];
        assert!(!filter.passes(&tower, "GuardTower"));
        let porter = vec![String::from("DOZER"), String::from("SELECTABLE")];
        assert!(!filter.passes(&porter, "MenPorter"));
        let inn = vec![String::from("STRUCTURE")];
        assert!(!filter.passes(&inn, "Inn"));
    }

    #[test]
    fn an_any_base_passes_everything_but_a_removed_token() {
        let filter = ObjectFilter::parse(&tokens("ANY -DOZER -NOT_AUTOACQUIRABLE"));
        assert!(filter.passes(&[String::from("INFANTRY")], "Soldier"));
        assert!(!filter.passes(&[String::from("DOZER")], "Porter"));
    }

    #[test]
    fn the_default_filter_passes_everything() {
        let filter = ObjectFilter::default();
        assert!(filter.passes(&[], "Anything"));
    }

    #[test]
    fn additions_under_any_restrict_to_what_they_name() {
        let filter = ObjectFilter::parse(&tokens("ANY +INFANTRY +CAVALRY -HERO +MordorWorker"));
        assert!(filter.passes(&[String::from("INFANTRY")], "Soldier"));
        assert!(!filter.passes(&[String::from("STRUCTURE")], "Farm"));
        assert!(!filter.passes(&[String::from("INFANTRY"), String::from("HERO")], "Boromir"));
        assert!(filter.passes(&[String::from("DOZER")], "MordorWorker"));
    }

    #[test]
    fn all_needs_every_addition_and_none_without_additions_passes_nothing() {
        let all = ObjectFilter::parse(&tokens("ALL +INFANTRY +HORDE"));
        assert!(all.passes(&[String::from("INFANTRY"), String::from("HORDE")], "Horde"));
        assert!(!all.passes(&[String::from("INFANTRY")], "Soldier"));
        let none = ObjectFilter::parse(&tokens("NONE"));
        assert!(!none.passes(&[String::from("INFANTRY")], "Soldier"));
        let bare_all = ObjectFilter::parse(&tokens("ALL -HERO"));
        assert!(bare_all.passes(&[String::from("INFANTRY")], "Soldier"));
    }

    #[test]
    fn a_loose_name_token_widens_through_the_caller_equivalence_lookup() {
        let loose = ObjectFilter::parse(&tokens("NONE +MordorGoblinSwordsman"));
        let equivalent = |a: &str, b: &str| {
            a.eq_ignore_ascii_case("MordorGoblinSwordsman")
                && b.eq_ignore_ascii_case("GoblinSwordVariant")
        };
        assert!(loose.passes_equiv(&[], "GoblinSwordVariant", &equivalent));
        assert!(
            !loose.passes(&[], "GoblinSwordVariant"),
            "passes stays exact"
        );
        let strict = ObjectFilter::parse(&tokens("NONE +S:MordorGoblinSwordsman"));
        assert!(
            !strict.passes_equiv(&[], "GoblinSwordVariant", &equivalent),
            "S: stays exact even with an equivalence lookup"
        );
    }

    #[test]
    fn relationship_tokens_name_whose_objects_pass() {
        let farms = ObjectFilter::parse(&tokens("ANY +GondorFarm ALLIES"));
        assert!(farms.passes(&[], "GondorFarm"), "ALLIES is not the base");
        assert!(farms.passes_relation(true, false, false));
        assert!(farms.passes_relation(false, true, false));
        assert!(!farms.passes_relation(false, false, true));
        let enemies = ObjectFilter::parse(&tokens("ANY +GondorFarm ENEMIES"));
        assert!(enemies.passes_relation(false, false, true));
        assert!(!enemies.passes_relation(true, false, false));
        let anyone = ObjectFilter::parse(&tokens("ANY +INFANTRY"));
        assert!(
            anyone.passes_relation(false, false, true),
            "none named: all pass"
        );
    }
}
