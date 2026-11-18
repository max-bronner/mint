use std::path::PathBuf;

const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[must_use]
pub fn os_font_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("WINDIR").map(|windir| PathBuf::from(windir).join("Fonts"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn is_bare_file_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && file_name != ".."
        && file_name != "."
}

#[must_use]
pub fn os_font_file_exists(file_name: &str) -> bool {
    if !is_bare_file_name(file_name) {
        return false;
    }
    os_font_dir()
        .map(|dir| dir.join(file_name))
        .is_some_and(|path| path.is_file())
}

#[must_use]
pub fn read_os_font_file(file_name: &str) -> Option<Vec<u8>> {
    if !is_bare_file_name(file_name) {
        return None;
    }
    let path = os_font_dir()?.join(file_name);
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FONT_FILE_BYTES {
        return None;
    }
    std::fs::read(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_with_a_separator_is_rejected() {
        assert!(!is_bare_file_name("../times.ttf"));
        assert!(!is_bare_file_name("Fonts/times.ttf"));
        assert!(!is_bare_file_name("Fonts\\times.ttf"));
    }

    #[test]
    fn a_bare_file_name_is_accepted() {
        assert!(is_bare_file_name("times.ttf"));
    }

    #[test]
    fn a_path_escaping_name_never_reads_or_reports_present() {
        assert!(!os_font_file_exists("..\\times.ttf"));
        assert!(read_os_font_file("..\\times.ttf").is_none());
    }
}
