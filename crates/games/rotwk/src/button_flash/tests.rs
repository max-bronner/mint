#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use super::{ButtonFlashes, FlashView};
use crate::rules::map_scripts::UiFlashTarget;

fn cameo(name: &str) -> UiFlashTarget {
    UiFlashTarget::Cameo(name.to_owned())
}

#[test]
fn a_cameo_counts_down_only_while_shown_and_a_press_stops_it() {
    let mut flashes = ButtonFlashes::default();
    flashes.advance(0, []);
    flashes.raise(&cameo("Command_Build"), 2, 0, &[]);
    flashes.advance(40, []);
    assert_eq!(flashes.view(40).cameo("command_build"), Some(true));
    flashes.advance(49, ["Command_Build"]);
    assert_eq!(flashes.view(49).cameo("Command_Build"), Some(true));
    flashes.advance(50, ["Command_Build", "Command_Build"]);
    assert_eq!(flashes.view(50).cameo("Command_Build"), Some(true));
    flashes.advance(60, ["Command_Build"]);
    assert_eq!(flashes.view(60).cameo("Command_Build"), Some(false));
    assert_eq!(flashes.view(60).cameo("Command_Other"), Some(false));

    flashes.raise(&cameo("Command_Build"), 6, 60, &[]);
    flashes.pressed("COMMAND_BUILD");
    assert_eq!(flashes.view(60).cameo("Command_Build"), Some(false));
}

#[test]
fn timed_palantir_flashes_never_shorten_but_the_builder_flash_is_replaced() {
    let mut flashes = ButtonFlashes::default();
    assert_eq!(flashes.view(0), FlashView::default());
    flashes.raise(&UiFlashTarget::SpellStore, 10, 0, &[]);
    flashes.raise(&UiFlashTarget::SpellStore, 1, 100, &[]);
    flashes.raise(&UiFlashTarget::Objectives, 2, 0, &[]);
    flashes.raise(&UiFlashTarget::SelectBuilder, 300, 0, &[]);
    flashes.raise(&UiFlashTarget::SelectBuilder, 30, 100, &[]);
    let at = flashes.view(200);
    assert_eq!(at.player_magic, Some(true));
    assert!(!at.objectives);
    assert_eq!(at.builder, Some(false));
    assert!(flashes.objectives_on(59));
    assert_eq!(flashes.view(300).player_magic, Some(false));
}
