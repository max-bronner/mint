use super::*;

#[test]
fn the_level_string_names_the_difficulty_by_its_first_character() {
    for (level, want) in [
        ("Easy", Difficulty::Easy),
        ("Medium", Difficulty::Medium),
        ("Hard", Difficulty::Hard),
        ("x", Difficulty::Medium),
        ("", Difficulty::Medium),
    ] {
        assert_eq!(difficulty_from_level(level), want, "{level:?}");
    }
}

#[test]
fn the_local_player_is_the_first_human_side_else_the_first_named_one() {
    let side = |neutral, human| SideFacts { neutral, human };
    let sides = [
        side(true, false),
        side(false, false),
        side(false, true),
        side(false, true),
    ];
    assert_eq!(pick_local(&sides), Some(2));
    assert_eq!(pick_local(&sides[..2]), Some(1));
    assert_eq!(pick_local(&sides[..1]), None);
}
