const TINOS_REGULAR: &[u8] = include_bytes!("../../fonts/tinos/Tinos-Regular.ttf");

#[must_use]
pub fn bundled_substitute_font(name: &str) -> Option<&'static [u8]> {
    match name {
        "Tinos" => Some(TINOS_REGULAR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Font;

    #[test]
    fn tinos_regular_parses_and_has_a_and_0() {
        let bytes = bundled_substitute_font("Tinos").expect("Tinos is bundled");
        let font = Font::from_bytes(bytes.to_vec()).expect("bundled font must parse");
        assert!(font.glyph_for_char('A').is_some());
        assert!(font.glyph_for_char('0').is_some());
    }

    #[test]
    fn an_unknown_name_is_none_not_a_panic() {
        assert!(bundled_substitute_font("Liberation Serif").is_none());
    }
}
