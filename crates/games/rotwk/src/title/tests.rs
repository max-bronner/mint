use super::*;

#[test]
fn rotwk_registers_what_the_runtime_hard_coded_before_the_split() {
    let before = TitleRegistration {
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
    assert_eq!(ROTWK, before);
    assert_eq!(
        ROTWK.campaign_key("Expansion1Campaign", true),
        Some("ANGMAR_CAMPAIGN_DEMO")
    );
    assert_eq!(ROTWK.campaign_key("GoodCampaign", false), None);
}
