use crate::victim::Relation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollideMask(u16);

impl Default for CollideMask {
    fn default() -> Self {
        Self(Self::STRUCTURES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CollideClass {
    pub structure: bool,
    pub controlled: bool,
    pub shrubbery: bool,
    pub projectile: bool,
    pub small_missile: bool,
    pub ballistic_missile: bool,
    pub monster: bool,
    pub wall: bool,
}

impl CollideMask {
    const ALLIES: u16 = 0x0001;
    const ENEMIES: u16 = 0x0002;
    const STRUCTURES: u16 = 0x0004;
    const SHRUBBERY: u16 = 0x0008;
    const PROJECTILES: u16 = 0x0010;
    const WALLS: u16 = 0x0020;
    const SMALL_MISSILES: u16 = 0x0040;
    const BALLISTIC_MISSILES: u16 = 0x0080;
    const CONTROLLED_STRUCTURES: u16 = 0x0100;
    const NEUTRAL: u16 = 0x0200;
    const MONSTERS: u16 = 0x0400;

    pub fn parse<S: AsRef<str>>(tokens: &[S]) -> Result<Self, &str> {
        tokens.iter().try_fold(Self(0), |mask, token| {
            let token = token.as_ref();
            let bit = match token.to_ascii_uppercase().as_str() {
                "ALLIES" => Self::ALLIES,
                "ENEMIES" => Self::ENEMIES,
                "NEUTRAL" => Self::NEUTRAL,
                "STRUCTURES" => Self::STRUCTURES,
                "SHRUBBERY" => Self::SHRUBBERY,
                "PROJECTILES" => Self::PROJECTILES,
                "WALLS" => Self::WALLS,
                "SMALL_MISSILES" => Self::SMALL_MISSILES,
                "BALLISTIC_MISSILES" => Self::BALLISTIC_MISSILES,
                "CONTROLLED_STRUCTURES" => Self::CONTROLLED_STRUCTURES,
                "MONSTERS" => Self::MONSTERS,
                _ => return Err(token),
            };
            Ok(Self(mask.0 | bit))
        })
    }

    #[must_use]
    pub const fn collides(self, relation: Relation, class: CollideClass) -> bool {
        let mut required = match relation {
            Relation::Allies => Self::ALLIES,
            Relation::Enemies => Self::ENEMIES,
            Relation::Neutral => Self::NEUTRAL,
        };
        if class.structure {
            required |= if class.controlled {
                Self::CONTROLLED_STRUCTURES
            } else {
                Self::STRUCTURES
            };
        }
        if class.shrubbery {
            required |= Self::SHRUBBERY;
        }
        if class.projectile {
            required |= Self::PROJECTILES;
        }
        if class.wall {
            required |= Self::WALLS;
        }
        if class.small_missile {
            required |= Self::SMALL_MISSILES;
        }
        if class.ballistic_missile {
            required |= Self::BALLISTIC_MISSILES;
        }
        if class.monster {
            required |= Self::MONSTERS;
        }
        self.0 & required != 0
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn an_object_stops_a_projectile_only_through_a_class_the_mask_names() {
        let rock = CollideMask::parse(&["MONSTERS", "structures"]).unwrap();
        let monster = CollideClass {
            monster: true,
            ..CollideClass::default()
        };
        let building = CollideClass {
            structure: true,
            ..CollideClass::default()
        };
        assert!(!rock.collides(Relation::Enemies, CollideClass::default()));
        assert!(rock.collides(Relation::Enemies, monster));
        assert!(rock.collides(Relation::Enemies, building));
        let own_building = CollideClass {
            controlled: true,
            ..building
        };
        assert!(!rock.collides(Relation::Allies, own_building));
        let arrow = CollideMask::parse(&["ENEMIES"]).unwrap();
        assert!(!arrow.collides(Relation::Allies, CollideClass::default()));
        assert!(!arrow.collides(Relation::Neutral, CollideClass::default()));
        assert!(arrow.collides(Relation::Enemies, CollideClass::default()));
    }
}
