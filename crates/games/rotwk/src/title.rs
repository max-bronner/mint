use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleRegistration {
    pub binary: &'static str,
    pub base_game: Option<BaseGame>,
    pub main_menu: &'static [MenuCallback],
    pub unlocks_bonus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseGame {
    pub app_paths_key: &'static str,
    pub marker_files: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuCallback {
    pub callback: &'static str,
    pub action: MenuAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Campaign {
        key: &'static str,
        demo_key: Option<&'static str>,
    },
    Tutorial,
}

pub const ROTWK: TitleRegistration = TitleRegistration {
    binary: "mint-rotwk",
    base_game: Some(BaseGame {
        app_paths_key: r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\lotrbfme2.exe",
        marker_files: &[
            "lotrbfme2.exe",
            "game.dat",
            "lotrbfme2.lcf",
            "eauninstall.exe",
            "filelist.txt",
        ],
    }),
    main_menu: &[
        MenuCallback {
            callback: "Expansion1Campaign",
            action: MenuAction::Campaign {
                key: "ANGMAR_CAMPAIGN",
                demo_key: Some("ANGMAR_CAMPAIGN_DEMO"),
            },
        },
        MenuCallback {
            callback: "BonusCampaign",
            action: MenuAction::Campaign {
                key: "ANGMAR_BONUS_CAMPAIGN",
                demo_key: None,
            },
        },
        MenuCallback {
            callback: "OnTutorial",
            action: MenuAction::Tutorial,
        },
    ],
    unlocks_bonus: true,
};

static CURRENT: OnceLock<&'static TitleRegistration> = OnceLock::new();

pub(crate) fn register(title: &'static TitleRegistration) {
    let _ = CURRENT.set(title);
}

pub(crate) fn current() -> &'static TitleRegistration {
    CURRENT.get().copied().unwrap_or(&ROTWK)
}

impl TitleRegistration {
    #[must_use]
    pub fn menu_action(&self, callback: &str) -> Option<MenuAction> {
        self.main_menu
            .iter()
            .find(|entry| entry.callback == callback)
            .map(|entry| entry.action)
    }

    #[must_use]
    pub fn campaign_key(&self, callback: &str, demo: bool) -> Option<&'static str> {
        match self.menu_action(callback)? {
            MenuAction::Campaign { key, demo_key } => {
                Some(demo_key.filter(|_| demo).unwrap_or(key))
            }
            MenuAction::Tutorial => None,
        }
    }
}

#[cfg(test)]
mod tests;
