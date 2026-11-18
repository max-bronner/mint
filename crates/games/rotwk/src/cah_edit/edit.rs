use std::sync::Arc;

use mint_sage_cah::HeroRecord;
use mint_sage_ini::BlingType;

use super::powers::PowerPage;
use super::system::{eq, CahSystem};

const MAX_UI_SLOT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotKind {
    Attribute,
    Appearance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) binder: usize,
    pub(crate) min: i32,
    pub(crate) max: i32,
    pub(crate) default: i32,
    pub(crate) cur: i32,
}

impl Slot {
    #[must_use]
    pub(crate) fn count(&self) -> i32 {
        self.max.saturating_sub(self.min).saturating_add(1)
    }
}

pub(crate) enum AutoChange<'a> {
    Default,
    Random(&'a mut dyn FnMut(i32, i32) -> i32),
    Reset,
    Recommend,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExamineApply {
    pub(crate) skip: bool,
    pub(crate) edited: bool,
    pub(crate) changed: usize,
    pub(crate) weapon: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HeroEdit {
    pub(super) sys: Arc<CahSystem>,
    pub(super) record: HeroRecord,
    attributes: Vec<Option<Slot>>,
    appearances: Vec<Option<Slot>>,
    points_total: u32,
    points_available: u32,
    pub(super) page: Option<PowerPage>,
    template: Option<(u32, u32)>,
    examine: ExamineApply,
}

fn to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl HeroEdit {
    #[must_use]
    pub(crate) fn from_record(sys: Arc<CahSystem>, record: HeroRecord) -> Self {
        let mut edit = Self {
            sys,
            record,
            attributes: Vec::new(),
            appearances: Vec::new(),
            points_total: 0,
            points_available: 0,
            page: None,
            template: None,
            examine: ExamineApply {
                skip: true,
                ..ExamineApply::default()
            },
        };
        edit.build_slots();
        edit
    }

    #[must_use]
    pub(crate) fn new_hero(sys: Arc<CahSystem>, class: u32, subclass: u32) -> Option<Self> {
        sys.subclass(
            usize::try_from(class).ok()?,
            usize::try_from(subclass).ok()?,
        )?;
        let mut edit = Self::from_record(sys, HeroRecord::default());
        edit.set_class_and_type(class, subclass);
        Some(edit)
    }

    #[must_use]
    pub(crate) fn record(&self) -> &HeroRecord {
        &self.record
    }

    pub(crate) fn set_class_and_type(&mut self, class: u32, subclass: u32) -> bool {
        let known = usize::try_from(class)
            .ok()
            .zip(usize::try_from(subclass).ok())
            .is_some_and(|(c, s)| self.sys.subclass(c, s).is_some());
        if !known {
            return false;
        }
        if self.template == Some((class, subclass)) {
            return true;
        }
        self.template = Some((class, subclass));
        self.record = HeroRecord {
            class_index: class,
            subclass_index: subclass,
            ..HeroRecord::default()
        };
        self.apply_default_colours();
        self.page = None;
        self.build_slots();
        self.reset_attributes();
        self.auto_change(AutoChange::Default);
        true
    }

    pub(crate) fn take_examine(&mut self) -> ExamineApply {
        std::mem::take(&mut self.examine)
    }

    fn apply_default_colours(&mut self) {
        self.examine.skip = true;
        let (class, subclass) = self.class_subclass();
        let Some(sub) = self.sys.subclass(class, subclass) else {
            return;
        };
        let defaults = [
            sub.default_primary_color,
            sub.default_secondary_color,
            sub.default_tertiary_color,
        ];
        for (colour, default) in self.record.colors.iter_mut().zip(defaults) {
            if let Some(default) = default {
                *colour = default;
            }
        }
    }

    pub(crate) fn set_colour(&mut self, index: usize, packed: u32) {
        if let Some(colour) = self.record.colors.get_mut(index) {
            self.examine.edited |= *colour != packed;
            *colour = packed;
        }
    }

    pub(crate) fn class_subclass(&self) -> (usize, usize) {
        (
            usize::try_from(self.record.class_index).unwrap_or(usize::MAX),
            usize::try_from(self.record.subclass_index).unwrap_or(usize::MAX),
        )
    }

    fn table(&self, kind: SlotKind) -> &Vec<Option<Slot>> {
        match kind {
            SlotKind::Attribute => &self.attributes,
            SlotKind::Appearance => &self.appearances,
        }
    }

    #[must_use]
    pub(crate) fn slot(&self, kind: SlotKind, index: usize) -> Option<&Slot> {
        self.table(kind).get(index)?.as_ref()
    }

    #[must_use]
    pub(crate) fn slot_len(&self, kind: SlotKind) -> usize {
        self.table(kind).len()
    }

    #[must_use]
    pub(crate) fn slot_group(&self, kind: SlotKind, index: usize) -> Option<&str> {
        let slot = self.slot(kind, index)?;
        self.sys.def.binders.get(slot.binder)?.group.as_deref()
    }

    #[must_use]
    pub(crate) fn slot_choice(&self, kind: SlotKind, index: usize) -> Option<&str> {
        let slot = self.slot(kind, index)?;
        let group = self.slot_group(kind, index)?;
        let (class, sub) = self.class_subclass();
        let options = self.sys.binder_options(class, sub, group)?;
        let at = usize::try_from(slot.cur).ok()?;
        options.upgrades.get(at).map(String::as_str)
    }

    #[must_use]
    pub(crate) fn points(&self) -> (u32, u32) {
        (self.points_available, self.points_total)
    }

    fn build_slots(&mut self) {
        self.attributes.clear();
        self.appearances.clear();
        self.points_total = 0;
        self.points_available = 0;
        let sys = Arc::clone(&self.sys);
        let (class, subclass) = self.class_subclass();
        let Some(sub) = sys.subclass(class, subclass) else {
            return;
        };
        self.points_total = sub.spendable_attribute_points.unwrap_or(0);
        self.points_available = self.points_total;
        for (index, binder) in sys.def.binders.iter().enumerate() {
            let (Some(group), Some(kind), Some(ui)) =
                (binder.group.as_deref(), binder.bling_type, binder.ui_slot)
            else {
                continue;
            };
            let kind = match kind {
                BlingType::Attribute => SlotKind::Attribute,
                BlingType::Appearance => SlotKind::Appearance,
            };
            let ui = usize::try_from(ui).unwrap_or(usize::MAX);
            if ui >= MAX_UI_SLOT || self.slot(kind, ui).is_some() {
                continue;
            }
            let (min, max, default) = match sys.binder_options(class, subclass, group) {
                Some(options) => {
                    let last = to_i32(
                        u32::try_from(options.upgrades.len().saturating_sub(1)).unwrap_or(0),
                    );
                    let position = |name: &Option<String>| {
                        name.as_deref()
                            .and_then(|n| options.upgrades.iter().position(|u| eq(u, n)))
                            .map_or(-1, |p| to_i32(u32::try_from(p).unwrap_or(u32::MAX)))
                    };
                    match kind {
                        SlotKind::Appearance => {
                            (0, last, to_i32(u32::try_from(options.default).unwrap_or(0)))
                        }
                        SlotKind::Attribute => {
                            let Some(attribute) = sub
                                .attributes
                                .iter()
                                .find(|a| a.group.as_deref().is_some_and(|g| eq(g, group)))
                            else {
                                continue;
                            };
                            (
                                position(&attribute.min_upgrade),
                                position(&attribute.max_upgrade),
                                position(&attribute.default_upgrade),
                            )
                        }
                    }
                }
                None if kind == SlotKind::Attribute => {
                    let c = to_i32(self.record.class_index);
                    let s = to_i32(self.record.subclass_index);
                    let b = to_i32(u32::try_from(index).unwrap_or(0));
                    let min = c
                        .saturating_add(1)
                        .saturating_mul(s.saturating_add(1))
                        .saturating_mul(b.saturating_add(1))
                        % 10
                        + 5;
                    let max = min + b.saturating_mul(3) % 10 - c + 3;
                    (min, max, 0)
                }
                None => continue,
            };
            let min = min.min(max);
            let default = default.clamp(min, max);
            let cur = match kind {
                SlotKind::Appearance => default,
                SlotKind::Attribute => min,
            };
            let slot = Slot {
                binder: index,
                min,
                max,
                default,
                cur,
            };
            let table = match kind {
                SlotKind::Attribute => &mut self.attributes,
                SlotKind::Appearance => &mut self.appearances,
            };
            if table.len() <= ui {
                table.resize(ui.saturating_add(1), None);
            }
            if let Some(cell) = table.get_mut(ui) {
                *cell = Some(slot);
            }
            let stored = self
                .record
                .blings
                .iter()
                .find(|(g, _)| g.eq_ignore_ascii_case(group.as_bytes()))
                .map(|&(_, v)| to_i32(v));
            self.set(kind, ui, stored.unwrap_or(cur));
        }
    }

    pub(crate) fn set(&mut self, kind: SlotKind, index: usize, value: i32) -> bool {
        let Some(slot) = self.slot(kind, index).copied() else {
            return false;
        };
        if value < slot.min || value > slot.max {
            return false;
        }
        if kind == SlotKind::Attribute {
            let available =
                i64::from(self.points_available) + i64::from(slot.cur) - i64::from(value);
            match u32::try_from(available) {
                Ok(a) if a <= self.points_total => self.points_available = a,
                _ => return false,
            }
        }
        let table = match kind {
            SlotKind::Attribute => &mut self.attributes,
            SlotKind::Appearance => &mut self.appearances,
        };
        if let Some(Some(s)) = table.get_mut(index) {
            s.cur = value;
        }
        if let Some(group) = self.slot_group(kind, index).map(str::to_owned) {
            let stored = u32::try_from(value).unwrap_or(0);
            let weapon = self
                .sys
                .def()
                .weapon_group
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&group));
            let changed = match self
                .record
                .blings
                .iter_mut()
                .find(|(g, _)| g.eq_ignore_ascii_case(group.as_bytes()))
            {
                Some(entry) => std::mem::replace(&mut entry.1, stored) != stored,
                None => {
                    self.record.blings.push((group.into_bytes(), stored));
                    true
                }
            };
            self.examine.edited = true;
            if changed {
                self.examine.changed = self.examine.changed.saturating_add(1);
                self.examine.weapon = weapon;
            }
        }
        true
    }

    pub(crate) fn step_appearance(&mut self, index: usize, delta: i32) {
        let Some(slot) = self.slot(SlotKind::Appearance, index).copied() else {
            return;
        };
        let count = slot.count();
        if count <= 0 {
            return;
        }
        let offset = slot.cur.saturating_add(delta).saturating_sub(slot.min);
        let value = slot.min.saturating_add(offset.rem_euclid(count));
        self.set(SlotKind::Appearance, index, value);
    }

    pub(crate) fn step_attribute(&mut self, index: usize, delta: i32) {
        let Some(slot) = self.slot(SlotKind::Attribute, index).copied() else {
            return;
        };
        let delta = delta.signum();
        let allowed = match delta {
            -1 => self.points_available < self.points_total,
            1 => self.points_available >= 1,
            _ => false,
        };
        if !allowed {
            return;
        }
        let value = slot.cur.saturating_add(delta).clamp(slot.min, slot.max);
        self.set(SlotKind::Attribute, index, value);
    }

    pub(crate) fn auto_change(&mut self, change: AutoChange<'_>) {
        match change {
            AutoChange::Default => {
                self.apply_default_colours();
                for index in 0..self.appearances.len() {
                    if let Some(slot) = self.slot(SlotKind::Appearance, index).copied() {
                        self.set(SlotKind::Appearance, index, slot.default);
                    }
                }
            }
            AutoChange::Random(rand) => {
                for index in 0..self.appearances.len() {
                    if let Some(slot) = self.slot(SlotKind::Appearance, index).copied() {
                        let value = rand(slot.min, slot.max).clamp(slot.min, slot.max);
                        self.set(SlotKind::Appearance, index, value);
                    }
                }
            }
            AutoChange::Reset => self.reset_attributes(),
            AutoChange::Recommend => {
                self.reset_attributes();
                for index in 0..self.attributes.len() {
                    if let Some(slot) = self.slot(SlotKind::Attribute, index).copied() {
                        self.set(SlotKind::Attribute, index, slot.default);
                    }
                }
            }
        }
    }

    fn reset_attributes(&mut self) {
        for index in 0..self.attributes.len() {
            if let Some(slot) = self.slot(SlotKind::Attribute, index).copied() {
                self.set(SlotKind::Attribute, index, slot.min);
            }
        }
    }

    #[must_use]
    pub(crate) fn dress_upgrades(&self) -> Vec<String> {
        let (class, subclass) = self.class_subclass();
        let mut grants: Vec<String> = Vec::new();
        grants.extend(self.sys.def().map_mode_upgrade.clone());
        for (group, index) in &self.record.blings {
            let group = String::from_utf8_lossy(group);
            let chosen = self
                .sys
                .binder_options(class, subclass, &group)
                .zip(usize::try_from(*index).ok())
                .and_then(|(options, index)| options.upgrades.get(index));
            grants.extend(chosen.cloned());
        }
        if let Some(class_def) = self.sys.def().classes.get(class) {
            grants.extend(class_def.upgrade.clone());
            grants.extend(
                class_def
                    .subclasses
                    .get(subclass)
                    .and_then(|sub| sub.upgrade.clone()),
            );
        }
        grants
    }

    #[must_use]
    pub(crate) fn name(&self) -> String {
        self.record.name_string()
    }

    #[must_use]
    pub(crate) fn id(&self) -> Option<String> {
        self.record.id_str().filter(|id| !id.is_empty())
    }

    pub(crate) fn set_id(&mut self, id: &str) {
        self.record.id = Some(id.as_bytes().to_vec());
    }

    pub(crate) fn set_name(&mut self, text: &str) {
        self.record.name = text.trim().encode_utf16().collect();
    }

    #[must_use]
    pub(crate) fn award_state(
        &self,
        i: usize,
        stats: &std::collections::BTreeMap<String, i32>,
    ) -> Option<bool> {
        let (class, sub) = self.class_subclass();
        self.sys.award_state(class, sub, i, stats)
    }
}
