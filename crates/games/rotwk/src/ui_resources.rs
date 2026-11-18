use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use mint_sage_apt_player::Library;

#[derive(Clone)]
pub(crate) struct UiResources {
    pub(crate) library: Arc<Library>,
    pub(crate) movies: usize,
    pub(crate) fonts: Arc<HashMap<String, mint_ui::Font>>,
    pub(crate) strings: Arc<mint_sage_str::StringTable>,
    pub(crate) string_files: usize,
    pub(crate) string_failures: usize,
}

impl UiResources {
    pub(crate) fn load(install_roots: &[PathBuf]) -> Self {
        let (library, names) = mint_sage_apt_viewer::load_player_library(install_roots);
        let fonts = Arc::new(mint_sage_apt_viewer::load_bundled_fonts(install_roots));
        let (strings, string_files, string_failures) =
            mint_sage_apt_viewer::load_string_table_from_roots(install_roots);
        Self {
            library,
            movies: names.len(),
            fonts,
            strings: Arc::new(strings),
            string_files,
            string_failures,
        }
    }
}
