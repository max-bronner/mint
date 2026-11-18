use std::process::ExitCode;

use mint_game_rotwk::title::{MenuAction, MenuCallback, TitleRegistration};

const BFME2: TitleRegistration = TitleRegistration {
    binary: "mint-bfme2",
    base_game: None,
    main_menu: &[
        MenuCallback {
            callback: "GoodCampaign",
            action: MenuAction::Campaign {
                key: "GOOD_CAMPAIGN",
                demo_key: None,
            },
        },
        MenuCallback {
            callback: "EvilCampaign",
            action: MenuAction::Campaign {
                key: "EVIL_CAMPAIGN",
                demo_key: None,
            },
        },
        MenuCallback {
            callback: "OnTutorial",
            action: MenuAction::Tutorial,
        },
    ],
    unlocks_bonus: false,
};

fn main() -> ExitCode {
    mint_game_rotwk::run(&BFME2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_main_menu_callback_starts_its_own_campaign_or_the_tutorial() {
        for (callback, want) in [
            ("GoodCampaign", Some("GOOD_CAMPAIGN")),
            ("EvilCampaign", Some("EVIL_CAMPAIGN")),
            ("Expansion1Campaign", None),
            ("BonusCampaign", None),
            ("OnTutorial", None),
        ] {
            assert_eq!(BFME2.campaign_key(callback, false), want, "{callback}");
        }
        assert_eq!(BFME2.menu_action("OnTutorial"), Some(MenuAction::Tutorial));
    }
}
