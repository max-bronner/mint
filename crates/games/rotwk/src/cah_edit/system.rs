use std::collections::BTreeMap;

use mint_sage_ini::{
    AwardDef, BlingBinderDef, BlingDef, CommandButtonDef, CommandSetDef, CreateAHeroSystemDef,
    SubClassDef, ThingStatDef, UpgradeIndex,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinderOptions {
    pub(crate) group: String,
    pub(crate) upgrades: Vec<String>,
    pub(crate) default: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PowerButton {
    pub(crate) name: String,
    pub(crate) min_level: i64,
    pub(crate) prerequisite: Option<String>,
    pub(crate) allowable_upgrades: Vec<String>,
    pub(crate) cost_if_selected: u32,
    pub(crate) icon: Option<String>,
    pub(crate) in_palantir: bool,
    pub(crate) descript_label: Option<String>,
}

impl PowerButton {
    #[must_use]
    pub(crate) fn new(name: &str, def: &CommandButtonDef) -> Self {
        Self {
            name: name.to_owned(),
            min_level: def.create_a_hero_ui_minimum_level.unwrap_or(0),
            prerequisite: def.create_a_hero_ui_prerequisite_button.clone(),
            allowable_upgrades: def.create_a_hero_ui_allowable_upgrades.clone(),
            cost_if_selected: float_to_u32(def.create_a_hero_ui_cost_if_selected.unwrap_or(0.0)),
            icon: def.button_image.clone(),
            in_palantir: def.in_palantir.unwrap_or(false),
            descript_label: def.descript_label.clone(),
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn float_to_u32(value: f32) -> u32 {
    if value.is_nan() || value <= 0.0 {
        0
    } else if value >= 4_294_967_040.0 {
        u32::MAX
    } else {
        value as u32
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CahSystem {
    pub(super) def: CreateAHeroSystemDef,
    pub(super) options: Vec<Vec<Vec<BinderOptions>>>,
    pub(super) buttons: Vec<PowerButton>,
    pub(super) awards: Vec<AwardDef>,
    thing_stats: Vec<ThingStatDef>,
    pub(super) leading_template_slots: usize,
    template_icons: Vec<Option<String>>,
    pub(super) discount_per_level: u32,
    base_cost: u32,
}

pub(super) fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn group_order(upgrades: &UpgradeIndex, name: &str) -> u32 {
    upgrades
        .get(name)
        .and_then(|u| u.group_order)
        .map_or(0, |n| u32::try_from(n.rem_euclid(1 << 32)).unwrap_or(0))
}

#[must_use]
pub(crate) fn leading_template_slots(template: &CommandSetDef) -> usize {
    (1u32..)
        .take_while(|&slot| template.slot(slot).is_some())
        .count()
}

fn find_bling<'a>(def: &'a CreateAHeroSystemDef, upgrade: &str) -> Option<&'a BlingDef> {
    def.blings
        .iter()
        .find(|b| b.upgrade.as_deref().is_some_and(|u| eq(u, upgrade)))
}

fn find_binder<'a>(def: &'a CreateAHeroSystemDef, bling: &BlingDef) -> Option<&'a BlingBinderDef> {
    let group = bling.group.as_deref()?;
    def.binders
        .iter()
        .find(|b| b.group.as_deref().is_some_and(|g| eq(g, group)))
}

struct Pending {
    group: String,
    upgrades: Vec<String>,
    default: Option<String>,
}

fn resolve_options(
    def: &CreateAHeroSystemDef,
    sub: &SubClassDef,
    upgrades: &UpgradeIndex,
    upgrade_groups: &[(String, String)],
) -> Vec<BinderOptions> {
    let mut binders: Vec<Pending> = Vec::new();
    for line in &sub.bling_upgrades {
        for (name, &at) in line.upgrades.iter().zip(&line.at) {
            let Some(binder) = find_bling(def, name).and_then(|b| find_binder(def, b)) else {
                continue;
            };
            let Some(group) = binder.group.as_deref() else {
                continue;
            };
            let index = match binders.iter().position(|p| eq(&p.group, group)) {
                Some(i) => i,
                None => {
                    binders.push(Pending {
                        group: group.to_owned(),
                        upgrades: Vec::new(),
                        default: None,
                    });
                    binders.len().saturating_sub(1)
                }
            };
            let first = binders.get(index).is_some_and(|p| p.upgrades.is_empty());
            let Some(p) = binders.get_mut(index) else {
                continue;
            };
            if at && !first {
                p.default = Some(name.clone());
            }
            if !p.upgrades.iter().any(|u| eq(u, name)) {
                p.upgrades.push(name.clone());
            }
        }
    }
    for (group, name) in crate::create_a_hero::attribute_blings(def, sub, upgrade_groups) {
        match binders.iter_mut().find(|p| eq(&p.group, &group)) {
            Some(p) if p.upgrades.iter().any(|u| eq(u, &name)) => {}
            Some(p) => p.upgrades.push(name),
            None => binders.push(Pending {
                group,
                upgrades: vec![name],
                default: None,
            }),
        }
    }
    binders
        .into_iter()
        .map(|mut p| {
            p.upgrades.sort_by_key(|u| group_order(upgrades, u));
            let default = p
                .default
                .as_deref()
                .and_then(|d| p.upgrades.iter().position(|u| eq(u, d)))
                .unwrap_or(0);
            BinderOptions {
                group: p.group,
                upgrades: p.upgrades,
                default,
            }
        })
        .collect()
}

impl CahSystem {
    #[must_use]
    pub(crate) fn new(
        def: &CreateAHeroSystemDef,
        upgrades: &UpgradeIndex,
        buttons: Vec<PowerButton>,
        awards: Vec<AwardDef>,
        template: Option<&CommandSetDef>,
    ) -> Self {
        let upgrade_groups = crate::create_a_hero::upgrade_groups(upgrades);
        let options = def
            .classes
            .iter()
            .map(|class| {
                class
                    .subclasses
                    .iter()
                    .map(|sub| resolve_options(def, sub, upgrades, &upgrade_groups))
                    .collect()
            })
            .collect();
        Self {
            def: def.clone(),
            options,
            buttons,
            awards,
            thing_stats: Vec::new(),
            leading_template_slots: template.map_or(0, leading_template_slots),
            template_icons: Vec::new(),
            discount_per_level: def.special_power_discount_per_level.map_or(0, float_to_u32),
            base_cost: 0,
        }
    }

    #[must_use]
    pub(crate) fn with_thing_stats(mut self, stats: Vec<ThingStatDef>) -> Self {
        self.thing_stats = stats;
        self
    }

    #[must_use]
    pub(crate) fn thing_stat(&self, name: &str) -> Option<&ThingStatDef> {
        self.thing_stats
            .iter()
            .find(|s| s.stat_name.as_deref().is_some_and(|n| eq(n, name)))
    }

    #[must_use]
    pub(crate) fn with_base_cost(mut self, cost: u32) -> Self {
        self.base_cost = cost;
        self
    }

    #[must_use]
    pub(crate) fn base_cost(&self) -> u32 {
        self.base_cost
    }

    #[must_use]
    pub(crate) fn def(&self) -> &CreateAHeroSystemDef {
        &self.def
    }

    #[must_use]
    pub(crate) fn awards(&self) -> &[AwardDef] {
        &self.awards
    }

    #[must_use]
    pub(crate) fn button(&self, name: &str) -> Option<&PowerButton> {
        self.buttons.iter().find(|b| b.name == name)
    }

    #[must_use]
    pub(crate) fn binder_options(
        &self,
        class: usize,
        subclass: usize,
        group: &str,
    ) -> Option<&BinderOptions> {
        self.options
            .get(class)?
            .get(subclass)?
            .iter()
            .find(|o| eq(&o.group, group))
    }

    #[must_use]
    pub(crate) fn subclass(&self, class: usize, subclass: usize) -> Option<&SubClassDef> {
        self.def.classes.get(class)?.subclasses.get(subclass)
    }

    #[must_use]
    pub(crate) fn with_template_icons(mut self, icons: Vec<Option<String>>) -> Self {
        self.template_icons = icons;
        self
    }

    #[must_use]
    pub(crate) fn template_icons(&self) -> &[Option<String>] {
        &self.template_icons
    }

    #[must_use]
    pub(crate) fn leading_slots(&self) -> usize {
        self.leading_template_slots
    }

    #[must_use]
    pub(crate) fn palantir_cap(&self) -> usize {
        6usize.saturating_sub(self.leading_template_slots)
    }

    #[must_use]
    pub(crate) fn award_state(
        &self,
        class: usize,
        subclass: usize,
        i: usize,
        stats: &BTreeMap<String, i32>,
    ) -> Option<bool> {
        let name = self.subclass(class, subclass)?.awards.get(i)?;
        let award = self
            .awards
            .iter()
            .find(|a| a.name.as_deref().is_some_and(|n| eq(n, name)))?;
        Some(
            !award.triggers.is_empty()
                && award.triggers.iter().all(|t| {
                    let sum: i64 = t
                        .stats
                        .iter()
                        .map(|s| i64::from(stats.get(s).copied().unwrap_or(0)))
                        .sum();
                    sum >= t.threshold.unwrap_or(0)
                }),
        )
    }
}
