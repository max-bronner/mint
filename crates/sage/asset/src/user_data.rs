const INSTALL_INFO_MAGIC: &[u8; 4] = b"GI  ";
const LEAF_KEY: &str = "UserDataLeafName";

pub const SAVE_FOLDER: &str = "Save";

#[must_use]
pub fn install_info_value(bytes: &[u8], key: &str) -> Option<String> {
    let rest = bytes.strip_prefix(INSTALL_INFO_MAGIC)?;
    let (count, rest) = rest.split_first_chunk::<4>()?;
    let count = u32::from_le_bytes(*count);
    let terminated = rest.get(..rest.iter().rposition(|&b| b == 0)?)?;
    let mut strings = terminated.split(|&b| b == 0);
    for _ in 0..count {
        let name = strings.next()?;
        let value = strings.next()?;
        if name.eq_ignore_ascii_case(key.as_bytes()) {
            return Some(value.iter().copied().map(char::from).collect());
        }
    }
    None
}

#[must_use]
pub fn user_data_leaf(install_info: Option<&[u8]>, game_data_leaf: Option<&str>) -> Option<String> {
    install_info
        .and_then(|bytes| install_info_value(bytes, LEAF_KEY))
        .filter(|leaf| !leaf.is_empty())
        .or_else(|| {
            game_data_leaf
                .filter(|leaf| !leaf.is_empty())
                .map(str::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut bytes = INSTALL_INFO_MAGIC.to_vec();
        bytes.extend_from_slice(&u32::try_from(pairs.len()).unwrap().to_le_bytes());
        for (key, value) in pairs {
            for s in [key, value] {
                bytes.extend_from_slice(s.as_bytes());
                bytes.push(0);
            }
        }
        bytes
    }

    #[test]
    fn the_install_leaf_wins_over_game_data() {
        let bytes = info(&[
            ("SkuName", "testsku"),
            ("UserDataLeafName", "My Test Files"),
        ]);
        assert_eq!(
            user_data_leaf(Some(&bytes), Some("Game Data Files")).as_deref(),
            Some("My Test Files")
        );
    }

    #[test]
    fn game_data_answers_when_the_install_has_no_leaf() {
        let bytes = info(&[("SkuName", "testsku"), ("UserDataLeafName", "")]);
        assert_eq!(
            user_data_leaf(Some(&bytes), Some("Game Data Files")).as_deref(),
            Some("Game Data Files")
        );
        assert_eq!(
            user_data_leaf(None, Some("Game Data Files")).as_deref(),
            Some("Game Data Files")
        );
        assert_eq!(user_data_leaf(None, Some("")), None);
    }

    #[test]
    fn a_value_is_read_as_its_pair_never_as_a_key() {
        let bytes = info(&[("Other", "UserDataLeafName"), ("UserDataLeafName", "Leaf")]);
        assert_eq!(
            install_info_value(&bytes, "UserDataLeafName").as_deref(),
            Some("Leaf")
        );
    }

    #[test]
    fn a_truncated_or_foreign_file_has_no_value() {
        let bytes = info(&[("UserDataLeafName", "Leaf")]);
        for cut in 0..bytes.len() {
            assert_eq!(
                install_info_value(&bytes[..cut], "UserDataLeafName"),
                None,
                "cut {cut}"
            );
        }
        let mut foreign = bytes.clone();
        foreign[0] = b'X';
        assert_eq!(install_info_value(&foreign, "UserDataLeafName"), None);
        let mut fewer = bytes;
        fewer[4] = 0;
        assert_eq!(install_info_value(&fewer, "UserDataLeafName"), None);
    }
}
