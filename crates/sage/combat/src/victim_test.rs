use crate::armor::ArmorTemplate;
use crate::load::CombatData;
use crate::nugget::{DamageContainedNugget, EffectNugget, GrabNugget, MetaImpactNugget, Nugget};
use crate::weapon::{WeaponId, WeaponTemplate};

const NESTING: u8 = 2;

pub trait VictimFacts {
    fn passenger_share(&self, template: &WeaponTemplate) -> f32;

    fn grab_accepts(&self, nugget: &GrabNugget) -> bool;

    fn paralyze_accepts(&self) -> bool;

    fn meta_impact_accepts(&self, nugget: &MetaImpactNugget) -> bool;

    fn damage_contained_accepts(&self, nugget: &DamageContainedNugget) -> bool;

    fn slave_weapon(&self) -> Option<(WeaponId, Box<dyn VictimFacts + '_>)>;
}

impl CombatData {
    #[must_use]
    pub fn valid_against(
        &self,
        id: WeaponId,
        armour: &ArmorTemplate,
        facts: &dyn VictimFacts,
    ) -> bool {
        self.valid(id, armour, facts, NESTING)
    }

    fn valid(
        &self,
        id: WeaponId,
        armour: &ArmorTemplate,
        facts: &dyn VictimFacts,
        depth: u8,
    ) -> bool {
        let Some(template) = self.weapons.get(id) else {
            return false;
        };
        let share = facts.passenger_share(template);
        template
            .nuggets
            .iter()
            .enumerate()
            .any(|(index, nugget)| match nugget {
                Nugget::Damage(damage) => {
                    !damage.requires_upgrade
                        && armour.adjust_damage(damage.damage_type, damage.damage)
                            * damage.damage_scalar
                            * share
                            > 0.0
                }
                Nugget::Dot(dot) => {
                    !dot.requires_upgrade
                        && armour.adjust_damage(dot.damage_type, dot.damage) * share > 0.0
                }
                Nugget::Projectile(projectile) => {
                    !projectile.requires_upgrade
                        && depth > 0
                        && self.weapons.id(&projectile.warhead).is_some_and(|warhead| {
                            self.valid(warhead, armour, facts, depth.saturating_sub(1))
                        })
                }
                Nugget::MetaImpact(impact) => facts.meta_impact_accepts(impact),
                Nugget::Modifier(_) => true,
                Nugget::Unmodelled(keyword) => match self.effects.get(id.index(), index) {
                    Some(EffectNugget::Grab(grab)) => facts.grab_accepts(grab),
                    Some(EffectNugget::Paralyze(_)) => facts.paralyze_accepts(),
                    Some(EffectNugget::DamageContained(contained)) => {
                        facts.damage_contained_accepts(contained)
                    }
                    Some(EffectNugget::SlaveAttack) => {
                        depth > 0
                            && facts.slave_weapon().is_some_and(|(weapon, slave)| {
                                self.valid(weapon, armour, &*slave, depth.saturating_sub(1))
                            })
                    }
                    Some(EffectNugget::FireLogic(_)) => false,
                    _ => !keyword.eq_ignore_ascii_case("FireLogicNugget"),
                },
            })
    }
}
