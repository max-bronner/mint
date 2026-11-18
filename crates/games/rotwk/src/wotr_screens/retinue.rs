use std::collections::BTreeMap;

use mint_sage_apt_player::Value;
use mint_sage_livingworld::{Army, ArmyId, LivingWorldState};

use super::{panels, Hud};
use crate::hud::printed;
use crate::wotr_hud::Selection;
use crate::wotr_movies::Kind;

const ICON_CONTENT: &str = "ArmyUnitIcon";
const ICON_INSTANCE: &str = "icon";
pub(super) const SLOT_LOADED: &str = "_OnUnitIconSlotLoaded";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placement {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) visible: bool,
}

const NEW_SLOT: Placement = Placement {
    x: 0.0,
    y: 0.0,
    visible: true,
};

#[derive(Debug, Default)]
pub(super) struct RetinueView {
    pub(super) root: String,
    pub(super) army: Option<ArmyId>,
    slots: Vec<u32>,
    icons: BTreeMap<u32, String>,
    placed: BTreeMap<u32, Placement>,
    stage_asked: bool,
}

impl RetinueView {
    pub(super) fn at(root: &str) -> Self {
        Self {
            root: root.to_owned(),
            ..Self::default()
        }
    }

    pub(super) fn page(&self) -> String {
        format!("{}.main", self.root)
    }

    pub(super) fn member_at(&self, clip: &str) -> Option<u32> {
        let clip = clip.strip_prefix("_level0.").unwrap_or(clip);
        self.icons
            .iter()
            .find(|(_, path)| clip == path.as_str() || clip.starts_with(&format!("{path}.")))
            .map(|(id, _)| *id)
    }

    pub(super) fn holds(&self, clip: &str) -> bool {
        let clip = clip.strip_prefix("_level0.").unwrap_or(clip);
        !self.root.is_empty() && (clip == self.root || clip.starts_with(&format!("{}.", self.root)))
    }
}

pub(super) fn grid(
    count: usize,
    stage: (f32, f32),
    slot: (f32, f32),
    scroll: f32,
) -> Vec<Placement> {
    let (width, height) = slot;
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let columns = (stage.0 / width).floor().max(0.0) as usize;
    let shown = |y: f32| y + height > 1.0 - scroll && stage.1 - scroll - 1.0 > y;
    let (mut x, mut y, mut in_row) = (0.0f32, 0.0f32, 0usize);
    let mut visible = shown(y);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(Placement { x, y, visible });
        in_row = in_row.saturating_add(1);
        if in_row >= columns {
            in_row = 0;
            x = 0.0;
            y += height;
            visible = shown(y);
        } else {
            x += width;
        }
    }
    out
}

impl Hud {
    pub(super) fn slot_loaded(
        &mut self,
        kind: Kind,
        view: &mut RetinueView,
        suffix: &str,
        args: &str,
    ) {
        let Some(id) = suffix
            .strip_prefix(SLOT_LOADED)
            .and_then(|n| n.parse::<u32>().ok())
        else {
            return;
        };
        if !view.slots.contains(&id) {
            return;
        }
        let frame = args.strip_prefix("_level0.").unwrap_or(args).to_owned();
        self.call(
            kind,
            &frame,
            "CreateContent",
            vec![Value::str(ICON_CONTENT), Value::str(ICON_INSTANCE)],
        );
        view.icons.insert(id, format!("{frame}.{ICON_INSTANCE}"));
    }

    pub(super) fn fill_retinue(
        &mut self,
        kind: Kind,
        view: &mut RetinueView,
        army: &Army,
        picks: &Selection,
        state: &LivingWorldState,
    ) {
        if view.army != Some(army.id) {
            for id in std::mem::take(&mut view.slots) {
                self.call(
                    kind,
                    &view.root,
                    "DeleteUnitIconSlot",
                    vec![Value::Number(f64::from(id))],
                );
            }
            view.icons.clear();
            view.placed.clear();
            view.stage_asked = false;
            self.cache.clear_under(kind, &view.page());
            view.army = Some(army.id);
        }
        let members = members(army, state);
        let root = view.root.clone();
        self.publish_to(kind, &root, panels::retinue_page(&members, &picks.members));
        let present: Vec<u32> = members.iter().filter(|m| !m.leader).map(|m| m.id).collect();
        let gone: Vec<u32> = view
            .slots
            .iter()
            .copied()
            .filter(|id| !present.contains(id))
            .collect();
        for id in gone {
            self.call(
                kind,
                &view.root,
                "DeleteUnitIconSlot",
                vec![Value::Number(f64::from(id))],
            );
            view.slots.retain(|s| *s != id);
            view.icons.remove(&id);
            view.placed.remove(&id);
        }
        for id in &present {
            if !view.slots.contains(id) {
                self.call(
                    kind,
                    &view.root,
                    "CreateUnitIconSlot",
                    vec![Value::Number(f64::from(*id))],
                );
                view.slots.push(*id);
            }
        }
        if !view.stage_asked && !view.slots.is_empty() {
            self.call(kind, &view.root, "GetUnitIconStageDimensions", Vec::new());
            view.stage_asked = true;
        }
        self.place_slots(kind, view);
        let icons: Vec<(u32, String)> = view.icons.iter().map(|(id, p)| (*id, p.clone())).collect();
        for (id, path) in icons {
            if let Some(member) = members.iter().find(|m| m.id == id) {
                let picked = picks.members.contains(&id);
                self.publish_to(kind, &path, panels::retinue_icon(member, picked));
            }
        }
    }

    fn place_slots(&mut self, kind: Kind, view: &mut RetinueView) {
        if view.slots.is_empty() {
            return;
        }
        let root = printed(&view.root);
        let mut read = |name: &str| self.movies.extern_number(kind, &format!("{root}{name}"));
        let (Some(stage_w), Some(stage_h), Some(slot_w), Some(slot_h)) = (
            read("_UnitIconStageWidth"),
            read("_UnitIconStageHeight"),
            read("_NewSlotWidth"),
            read("_NewSlotHeight"),
        ) else {
            return;
        };
        let places = grid(view.slots.len(), (stage_w, stage_h), (slot_w, slot_h), 0.0);
        for (id, place) in view.slots.clone().into_iter().zip(places) {
            let was = view.placed.get(&id).copied().unwrap_or(NEW_SLOT);
            if (was.x, was.y) != (place.x, place.y) {
                self.call(
                    kind,
                    &view.root,
                    "MoveUnitIconSlot",
                    vec![
                        Value::Number(f64::from(id)),
                        Value::Number(f64::from(place.x)),
                        Value::Number(f64::from(place.y)),
                    ],
                );
            }
            if was.visible != place.visible {
                self.call(
                    kind,
                    &view.root,
                    "SetUnitIconSlotVisibility",
                    vec![Value::Number(f64::from(id)), Value::Bool(place.visible)],
                );
            }
            view.placed.insert(id, place);
        }
    }
}

pub(super) fn members(army: &Army, state: &LivingWorldState) -> Vec<panels::MemberSlot> {
    let rules = &state.rules;
    army.roster
        .iter()
        .map(|m| panels::MemberSlot {
            id: m.id,
            name: rules
                .unit_names
                .get(usize::from(m.unit.0))
                .cloned()
                .unwrap_or_default(),
            quantity: m.count,
            level: m.level,
            upgraded: m.is_upgraded(),
            leader: army.hero == Some(m.unit),
            command_points: rules
                .units
                .get(usize::from(m.unit.0))
                .map_or(0, |u| u.command_points)
                .saturating_mul(i32::try_from(m.count).unwrap_or(0)),
        })
        .collect()
}
