use mint_sage_apt_player::Value;
use mint_sage_livingworld::{Army, ArmyId, Command, LivingWorldState, PlayerId};
use mint_sage_str::{format_placeholders, Placeholder};

use super::retinue::{RetinueView, SLOT_LOADED};
use super::{panels, Hud, PendingCall, Strategic};
use crate::wotr_hud::{describe, ClipEvent, Selection};
use crate::wotr_movies::Kind;

const SIDES: [(&str, &str); 2] = [
    ("Top", "_OnTopPanelLoaded"),
    ("Bottom", "_OnBottomPanelLoaded"),
];
const CP_LABEL: &str = "STRATEGICHUD:ArmyUnitSwapperCP";
const ROOT_KEY: &str = "_level0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Button {
    MoveDown,
    MoveUp,
    Swap,
}

impl Button {
    const ALL: [Button; 3] = [Button::MoveDown, Button::MoveUp, Button::Swap];

    fn name(self) -> &'static str {
        match self {
            Button::MoveDown => "moveDown",
            Button::MoveUp => "moveUp",
            Button::Swap => "swap",
        }
    }
}

struct Panel {
    selection: Selection,
    view: RetinueView,
    texts: [String; 3],
}

impl Panel {
    fn new(army: ArmyId, members: Vec<u32>) -> Self {
        Self {
            selection: Selection {
                army: Some(army),
                members,
                ..Selection::default()
            },
            view: RetinueView::default(),
            texts: Default::default(),
        }
    }
}

pub(super) struct SwapperRun {
    panels: [Panel; 2],
    state: State,
}

impl SwapperRun {
    pub(super) fn new(top: ArmyId, picks: Vec<u32>, bottom: ArmyId) -> Self {
        Self {
            panels: [Panel::new(top, picks), Panel::new(bottom, Vec::new())],
            state: State::Opening,
        }
    }
}

pub(super) fn button_command(
    button: Button,
    state: &LivingWorldState,
    local: PlayerId,
    top: &Selection,
    bottom: &Selection,
) -> Option<Command> {
    let (a, b, a_members, b_members) = match button {
        Button::MoveDown => (top, bottom, top.members.clone(), Vec::new()),
        Button::MoveUp => (bottom, top, bottom.members.clone(), Vec::new()),
        Button::Swap if bottom.members.is_empty() => return None,
        Button::Swap => (top, bottom, top.members.clone(), bottom.members.clone()),
    };
    if a_members.is_empty() {
        return None;
    }
    let (a, b) = (a.army?, b.army?);
    state
        .check_swap(local, a, b, &a_members, &b_members)
        .ok()
        .map(|()| Command::SwapArmyMembers {
            a,
            b,
            a_members,
            b_members,
        })
}

fn panel_texts(army: &Army, state: &LivingWorldState, cp_template: Option<&str>) -> [String; 3] {
    let rules = &state.rules;
    let region = rules
        .region_rules(army.region)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let name = army
        .hero
        .and_then(|h| rules.unit_names.get(usize::from(h.0)).cloned())
        .unwrap_or_else(|| "Garrison".to_owned());
    let cap = state.army_cap(army);
    let cp = match cp_template {
        Some(template) if cap > 0 => format_placeholders(
            template,
            &[
                Placeholder::Int(i64::from(state.army_cp(army))),
                Placeholder::Int(i64::from(cap)),
            ],
        ),
        _ => String::new(),
    };
    [region, name, cp]
}

impl Hud {
    pub(super) fn open_swapper(&mut self, top: ArmyId, bottom: ArmyId) {
        let picks = self.selection.members.clone();
        self.say(format!(
            "hud: StrategicArmyUnitSwapper opens: army {} (members {picks:?}) over army {}",
            top.0, bottom.0
        ));
        self.swapper = Some(SwapperRun::new(top, picks, bottom));
        self.show(Kind::Swapper, true);
    }

    pub(super) fn swapper_event<S: Strategic>(&mut self, session: &mut S, event: &ClipEvent) {
        let Some(mut run) = self.swapper.take() else {
            return;
        };
        let suffix = event.suffix.as_str();
        let open = run.state == State::Open;
        let mut closed = false;
        let button = match suffix {
            "_OnMoveDownButtonClicked" => Some(Button::MoveDown),
            "_OnMoveUpButtonClicked" => Some(Button::MoveUp),
            "_OnSwapButtonClicked" => Some(Button::Swap),
            _ => None,
        };
        let panel_of = |run: &SwapperRun| run.panels.iter().position(|p| p.view.holds(&event.clip));
        match suffix {
            "_OnOpen" if run.state == State::Opening => run.state = State::Open,
            "_OnClosed" if run.state == State::Closing => closed = true,
            "_OnExitButtonClicked" if open => {
                self.call(Kind::Swapper, "", "FadeOut", Vec::new());
                run.state = State::Closing;
            }
            _ if button.is_some() && open => {
                let [top, bottom] = &run.panels;
                let command = button.and_then(|b| {
                    button_command(
                        b,
                        session.state(),
                        self.local,
                        &top.selection,
                        &bottom.selection,
                    )
                });
                match command {
                    Some(command) => {
                        let result = session.submit(self.local, command.clone());
                        let line = describe(self.local, &command, &result);
                        self.say(line);
                    }
                    None => self.say(format!("hud: swapper {button:?} is disabled")),
                }
            }
            s if SIDES.iter().any(|(_, loaded)| *loaded == s) => {
                if let Some(index) = SIDES.iter().position(|(_, loaded)| *loaded == s) {
                    let root = event.args.strip_prefix("_level0.").unwrap_or(&event.args);
                    if let Some(panel) = run.panels.get_mut(index) {
                        if panel.view.root != root {
                            panel.view = RetinueView::at(root);
                            panel.texts = Default::default();
                        }
                    }
                }
            }
            s if s.starts_with(SLOT_LOADED) => {
                if let Some(panel) = panel_of(&run).and_then(|i| run.panels.get_mut(i)) {
                    self.slot_loaded(Kind::Swapper, &mut panel.view, s, &event.args);
                }
            }
            "_OnClicked" | "_OnLeaderIconClicked" | "_OnIconListBackgroundClicked" => {
                if let Some(panel) = panel_of(&run).and_then(|i| run.panels.get_mut(i)) {
                    match suffix {
                        "_OnLeaderIconClicked" => panel.selection.leader_clicked(session.state()),
                        "_OnIconListBackgroundClicked" => panel.selection.background_clicked(),
                        _ => {
                            if let Some(member) = panel.view.member_at(&event.clip) {
                                panel.selection.member_clicked(member, self.ctrl);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if closed {
            self.show(Kind::Swapper, false);
            self.say("hud: StrategicArmyUnitSwapper closed".to_owned());
        } else {
            self.swapper = Some(run);
        }
    }

    pub(super) fn update_swapper(&mut self, state: &LivingWorldState) {
        let Some(mut run) = self.swapper.take() else {
            return;
        };
        let gone = run.panels.iter().any(|p| {
            p.selection
                .army
                .is_none_or(|id| !state.armies.contains_key(&id))
        });
        if gone {
            self.show(Kind::Swapper, false);
            self.say("hud: StrategicArmyUnitSwapper closed: one of its armies is gone".to_owned());
            return;
        }
        let template = self.movies.label(CP_LABEL);
        for (panel, (side, _)) in run.panels.iter_mut().zip(SIDES) {
            let Some(army) = panel.selection.army.and_then(|id| state.armies.get(&id)) else {
                continue;
            };
            if panel.view.root.is_empty()
                || !self
                    .movies
                    .exists(Kind::Swapper, &format!("{}.memberName", panel.view.page()))
            {
                continue;
            }
            let texts = panel_texts(army, state, template.as_deref());
            if texts != panel.texts {
                for (name, text) in ["RegionName", "ArmyName", "CP"].iter().zip(&texts) {
                    let key = format!("{ROOT_KEY}_{side}{name}");
                    self.movies.set_text(Kind::Swapper, &key, text);
                }
                panel.texts = texts;
            }
            let root = panel.view.root.clone();
            self.publish_to(
                Kind::Swapper,
                &root,
                panels::retinue_layout(army.is_hero_army()),
            );
            self.fill_retinue(
                Kind::Swapper,
                &mut panel.view,
                army,
                &panel.selection,
                state,
            );
        }
        let [top, bottom] = &run.panels;
        for button in Button::ALL {
            if !self
                .movies
                .exists(Kind::Swapper, &format!("main.{}", button.name()))
            {
                continue;
            }
            let on = button_command(button, state, self.local, &top.selection, &bottom.selection)
                .is_some();
            let label = if on { "_up" } else { "_disabled" };
            let key = format!("SetButtonState#{}", button.name());
            if self.cache.changed(Kind::Swapper, "", &key, label) {
                self.calls.push_back(PendingCall {
                    kind: Kind::Swapper,
                    path: String::new(),
                    function: "SetButtonState".to_owned(),
                    args: vec![Value::str(button.name()), Value::str(label)],
                    tries: 0,
                    cache_key: Some(key),
                });
            }
        }
        self.swapper = Some(run);
    }
}
