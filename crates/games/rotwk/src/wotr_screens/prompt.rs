use mint_sage_livingworld::screen_queries::{AUTO_RESOLVE, REAL_TIME, RETREAT};
use mint_sage_livingworld::{battle_sides, Battle, LivingWorldState, PlayerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Ally,
    Enemy,
}

impl Side {
    pub(super) fn add(self) -> &'static str {
        match self {
            Side::Ally => "AddAllyPage",
            Side::Enemy => "AddEnemyPage",
        }
    }

    pub(super) fn fade_in(self) -> &'static str {
        match self {
            Side::Ally => "FadeInAllyPage",
            Side::Enemy => "FadeInEnemyPage",
        }
    }

    pub(super) fn fade_out(self) -> &'static str {
        match self {
            Side::Ally => "FadeOutAllyPage",
            Side::Enemy => "FadeOutEnemyPage",
        }
    }

    pub(super) fn pages(self) -> &'static str {
        match self {
            Side::Ally => "main.allies",
            Side::Enemy => "main.enemies",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Page {
    pub(super) side: Side,
    pub(super) id: u32,
    pub(super) player: PlayerId,
    pub(super) local: bool,
}

impl Page {
    pub(super) fn state(&self) -> &'static str {
        match (self.local, self.side) {
            (true, _) => "_player",
            (false, Side::Ally) => "_ally",
            (false, Side::Enemy) => "_enemy",
        }
    }

    pub(super) fn path(&self) -> String {
        format!("{}.{}.page", self.side.pages(), self.id)
    }
}

pub(super) fn pages(state: &LivingWorldState, battle: &Battle, local: PlayerId) -> Vec<Page> {
    pages_of(&battle_sides(state, battle), local, |a, b| {
        state.allied(a, b)
    })
}

pub(super) fn pages_of(
    teams: &[Vec<PlayerId>],
    local: PlayerId,
    allied: impl Fn(PlayerId, PlayerId) -> bool,
) -> Vec<Page> {
    let mut list = vec![Page {
        side: Side::Ally,
        id: 0,
        player: local,
        local: true,
    }];
    let (mut allies, mut enemies) = (1u32, 0u32);
    for player in teams.iter().flatten().copied() {
        if player == local {
            continue;
        }
        let side = if allied(local, player) {
            Side::Ally
        } else {
            Side::Enemy
        };
        let counter = match side {
            Side::Ally => &mut allies,
            Side::Enemy => &mut enemies,
        };
        list.push(Page {
            side,
            id: *counter,
            player,
            local: false,
        });
        *counter = counter.saturating_add(1);
    }
    list
}

pub(super) fn swap_button(n: u32, heroes: usize) -> (bool, &'static str) {
    let usable = usize::try_from(n.saturating_add(2)).is_ok_and(|need| heroes >= need);
    (usable, if usable { "_up" } else { "_disabled" })
}

pub(super) const LOCAL_GAME: bool = true;

pub(super) fn button_states(mask: u8) -> [(&'static str, &'static str); 3] {
    let label = |bit| if mask & bit != 0 { "_up" } else { "_disabled" };
    [
        ("AutoResolve", label(AUTO_RESOLVE)),
        ("RealTime", label(REAL_TIME)),
        ("Retreat", label(RETREAT)),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TabCall {
    State(usize, &'static str),
    PlayerName(String),
    LocalPlayerName(String),
    NameVisibility(bool),
}

#[derive(Debug, Clone)]
pub(super) struct Tabs {
    side: Side,
    names: Vec<String>,
    selected: Option<usize>,
}

impl Tabs {
    pub(super) fn new(side: Side, names: Vec<String>) -> Self {
        Self {
            side,
            names,
            selected: None,
        }
    }

    pub(super) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(super) fn select(&mut self, tab: usize) -> Vec<TabCall> {
        let Some(name) = self.names.get(tab).cloned() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(old) = self.selected.filter(|old| *old != tab) {
            out.push(TabCall::State(old, "_deselect"));
        }
        out.push(TabCall::State(tab, "_selected"));
        self.selected = Some(tab);
        let local_tab = self.side == Side::Ally && tab == 0;
        out.push(TabCall::PlayerName(name.clone()));
        out.push(TabCall::NameVisibility(!local_tab));
        if local_tab {
            out.push(TabCall::LocalPlayerName(name));
        }
        out
    }
}
