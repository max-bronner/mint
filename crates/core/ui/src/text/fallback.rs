#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontFallback {
    pub logical_name: &'static str,
    pub exact: &'static str,
    pub host_equivalents: &'static [&'static str],
    pub open_substitutes: &'static [&'static str],
    pub game_font: Option<&'static str>,
}

impl FontFallback {
    fn rungs(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.exact)
            .chain(self.host_equivalents.iter().copied())
            .chain(self.open_substitutes.iter().copied())
            .chain(self.game_font)
    }
}

#[must_use]
pub fn resolve_font_name(
    chain: &FontFallback,
    mut available: impl FnMut(&str) -> bool,
) -> Option<&'static str> {
    chain.rungs().find(|name| available(name))
}

pub const ARIAL_UNICODE_MS: FontFallback = FontFallback {
    logical_name: "Arial Unicode MS",
    exact: "Arial Unicode MS",
    host_equivalents: &["Segoe UI", "Helvetica", "San Francisco"],
    open_substitutes: &["Noto Sans"],
    game_font: None,
};

pub const ALBERTUS_MT: FontFallback = FontFallback {
    logical_name: "Albertus MT",
    exact: "Albertus MT",
    host_equivalents: &[],
    open_substitutes: &[],
    game_font: None,
};

pub const OMNIA_LT_STD: FontFallback = FontFallback {
    logical_name: "Omnia LT Std",
    exact: "Omnia LT Std",
    host_equivalents: &[],
    open_substitutes: &[],
    game_font: None,
};

pub const SACHA_WYNTER: FontFallback = FontFallback {
    logical_name: "SachaWynter",
    exact: "SachaWynter",
    host_equivalents: &[],
    open_substitutes: &[],
    game_font: None,
};

pub const TIMES_NEW_ROMAN: FontFallback = FontFallback {
    logical_name: "Times New Roman",
    exact: "Times New Roman",
    host_equivalents: &[],
    open_substitutes: &["Liberation Serif", "Tinos"],
    game_font: None,
};

pub const ARIAL: FontFallback = FontFallback {
    logical_name: "Arial",
    exact: "Arial",
    host_equivalents: &[],
    open_substitutes: &["Liberation Sans"],
    game_font: None,
};

pub const COURIER_NEW: FontFallback = FontFallback {
    logical_name: "Courier New",
    exact: "Courier New",
    host_equivalents: &[],
    open_substitutes: &["Liberation Mono"],
    game_font: None,
};

pub const COURIER: FontFallback = FontFallback {
    logical_name: "Courier",
    ..COURIER_NEW
};

pub const ALL: &[FontFallback] = &[
    ARIAL_UNICODE_MS,
    ALBERTUS_MT,
    OMNIA_LT_STD,
    SACHA_WYNTER,
    TIMES_NEW_ROMAN,
    ARIAL,
    COURIER_NEW,
    COURIER,
];

#[must_use]
pub fn chain_for(logical_name: &str) -> Option<&'static FontFallback> {
    ALL.iter().find(|chain| chain.logical_name == logical_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_name_wins_when_it_is_available() {
        let resolved = resolve_font_name(&ARIAL_UNICODE_MS, |name| name == "Arial Unicode MS");
        assert_eq!(resolved, Some("Arial Unicode MS"));
    }

    #[test]
    fn a_later_rung_wins_when_every_earlier_one_is_unavailable() {
        let resolved = resolve_font_name(&ARIAL_UNICODE_MS, |name| name == "Noto Sans");
        assert_eq!(resolved, Some("Noto Sans"));
    }

    #[test]
    fn rungs_are_tried_in_order_not_availability_order() {
        let resolved = resolve_font_name(&ARIAL_UNICODE_MS, |name| {
            name == "Segoe UI" || name == "Noto Sans"
        });
        assert_eq!(resolved, Some("Segoe UI"));
    }

    #[test]
    fn nothing_available_resolves_to_none_not_a_panic_or_a_guess() {
        let resolved = resolve_font_name(&ARIAL_UNICODE_MS, |_| false);
        assert_eq!(
            resolved, None,
            "None is the caller's signal to fall to R8's tofu box"
        );
    }

    #[test]
    fn a_bundled_fonts_own_rung_one_is_all_it_ever_needs() {
        let resolved = resolve_font_name(&ALBERTUS_MT, |name| name == "Albertus MT");
        assert_eq!(resolved, Some("Albertus MT"));
    }

    #[test]
    fn every_rung_four_is_awaiting_owner_or_not_applicable() {
        for chain in ALL {
            assert_eq!(
                chain.game_font, None,
                "{}'s rung 4 must stay None until the owner chooses it, or be \
                 documented as not applicable - it must never be guessed",
                chain.logical_name
            );
        }
    }

    #[test]
    fn courier_aliases_courier_news_stack_in_full_including_rung_one() {
        assert_eq!(COURIER.exact, "Courier New");
        assert_eq!(COURIER.host_equivalents, COURIER_NEW.host_equivalents);
        assert_eq!(COURIER.open_substitutes, COURIER_NEW.open_substitutes);
        assert_eq!(COURIER.game_font, COURIER_NEW.game_font);
        assert_eq!(
            COURIER.logical_name, "Courier",
            "only the lookup key differs"
        );
    }

    #[test]
    fn chain_for_finds_every_logical_name_the_table_lists() {
        for chain in ALL {
            assert_eq!(chain_for(chain.logical_name), Some(chain));
        }
    }

    #[test]
    fn chain_for_an_unknown_name_is_none_not_a_panic() {
        assert_eq!(chain_for("Wingdings"), None);
    }

    #[test]
    fn every_named_font_is_present_exactly_once() {
        assert_eq!(ALL.len(), 8);
        let mut names: Vec<&str> = ALL.iter().map(|chain| chain.logical_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 8, "a logical name repeats in the table");
    }
}
