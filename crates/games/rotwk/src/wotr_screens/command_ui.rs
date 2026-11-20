use std::collections::BTreeMap;

use mint_sage_livingworld::{BuildingId, LivingWorldState, Phase, PlayerId};

use crate::wotr_hud::Selection;

pub(super) const CONTENT: &str = "StrategicCommandButton";
pub(super) const INSTANCE: &str = "button";
pub(super) const FRAMES: usize = 6;
const STRUCTURE_FRAMES: usize = 5;

pub(super) fn arg_field<'a>(args: &'a str, key: &str) -> Option<&'a str> {
    args.split('&')
        .find_map(|part| part.strip_prefix(key)?.strip_prefix('='))
}

#[derive(Debug, Default)]
pub(super) struct Frames {
    printed: BTreeMap<usize, String>,
}

impl Frames {
    pub(super) fn loaded(&mut self, args: &str) -> Option<usize> {
        let index: usize = arg_field(args, "index")?.parse().ok()?;
        let name = arg_field(args, "name")?;
        self.printed.insert(index, name.to_owned());
        Some(index)
    }

    pub(super) fn printed(&self, index: usize) -> Option<&str> {
        self.printed.get(&index).map(String::as_str)
    }

    pub(super) fn path(&self, index: usize) -> Option<String> {
        let printed = self.printed(index)?;
        Some(
            printed
                .strip_prefix("_level0.")
                .unwrap_or(printed)
                .to_owned(),
        )
    }

    pub(super) fn index_of(&self, clip: &str) -> Option<usize> {
        let clip = clip.strip_prefix("_level0.").unwrap_or(clip);
        let frame = clip.strip_suffix(&format!(".{INSTANCE}"))?;
        self.printed
            .keys()
            .copied()
            .find(|i| self.path(*i).as_deref() == Some(frame))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Button {
    Element(&'static str),
    Build(BuildingId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Want {
    pub(super) frame: usize,
    pub(super) button: Button,
    pub(super) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Plot {
    Built { under_construction: bool },
    Empty { offers: Vec<(BuildingId, bool)> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Army {
    pub(super) has_order: bool,
    pub(super) in_details: bool,
    pub(super) members_picked: bool,
}

const TOGGLE: &str = "ToggleSelectionDetailsButton";

pub(super) fn frames_for(army: Option<Army>, plot: Option<Plot>, planning: bool) -> Vec<Want> {
    let mut out = vec![Want {
        frame: 0,
        button: Button::Element(TOGGLE),
        enabled: true,
    }];
    if !planning {
        return out;
    }
    fn element(out: &mut Vec<Want>, frame: usize, name: &'static str, enabled: bool) {
        out.push(Want {
            frame,
            button: Button::Element(name),
            enabled,
        });
    }
    match (plot, army) {
        (Some(Plot::Built { under_construction }), _) => element(
            &mut out,
            5,
            if under_construction {
                "CancelBuildingConstructionButton"
            } else {
                "DestroyBuildingButton"
            },
            true,
        ),
        (Some(Plot::Empty { offers }), _) => {
            for (n, (building, affordable)) in offers.into_iter().take(STRUCTURE_FRAMES).enumerate()
            {
                out.push(Want {
                    frame: n.saturating_add(1),
                    button: Button::Build(building),
                    enabled: affordable,
                });
            }
        }
        (None, Some(army)) if army.in_details => {
            element(&mut out, 1, "CancelArmyMemberMoveButton", army.has_order);
            element(&mut out, 2, "UpgradeUnitButton", army.members_picked);
            element(&mut out, 5, "DisbandArmyMemberButton", army.members_picked);
        }
        (None, Some(army)) => element(&mut out, 1, "CancelArmyMoveButton", army.has_order),
        (None, None) => {}
    }
    out
}

pub(super) fn wanted(state: &LivingWorldState, sel: &Selection, local: PlayerId) -> Vec<Want> {
    let army = sel
        .army
        .and_then(|id| state.armies.get(&id))
        .map(|army| Army {
            has_order: army.order.is_some_and(|dest| dest != army.region),
            in_details: sel.details,
            members_picked: !sel.members.is_empty(),
        });
    let plot = sel.region.zip(sel.plot).and_then(|(region, plot)| {
        let slot = state.region(region)?.plots.get(usize::from(plot))?;
        Some(match slot {
            Some(building) => Plot::Built {
                under_construction: building.under_construction,
            },
            None => {
                let money = state.player(local).map_or(0, |p| p.resources);
                Plot::Empty {
                    offers: state
                        .buildable_here(local)
                        .into_iter()
                        .map(|id| {
                            let cost = state.rules.building(id).map_or(0, |b| b.cost);
                            (id, money >= cost)
                        })
                        .collect(),
                }
            }
        })
    });
    frames_for(army, plot, state.phase == Phase::Planning)
}
