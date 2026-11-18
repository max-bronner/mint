use std::collections::HashMap;
use std::sync::Arc;

use mint_render::Renderer;
use mint_sage_wnd::{DrawItem, Layout, Window};
use mint_vfs::Vfs;

use crate::host::{IconProvider, SurfaceIcon};

pub mod check;
pub mod combo;
pub mod entry;
pub mod list;
pub mod map_window;
pub mod slider;

#[must_use]
pub fn gadget_key(init: &str, name: &str) -> String {
    format!("{init}/{name}")
}

#[must_use]
pub fn template_path(load: &str) -> String {
    let load = load.trim().replace('\\', "/").to_ascii_lowercase();
    let load = load.trim_start_matches('/');
    if load.starts_with("window/") {
        load.to_owned()
    } else {
        format!("window/{load}")
    }
}

pub struct Templates {
    vfs: Arc<Vfs>,
    parsed: HashMap<String, Option<Arc<Layout>>>,
}

impl Templates {
    #[must_use]
    pub fn new(vfs: Arc<Vfs>) -> Self {
        Self {
            vfs,
            parsed: HashMap::new(),
        }
    }

    pub fn get(&mut self, load: &str) -> Option<Arc<Layout>> {
        let path = template_path(load);
        if let Some(found) = self.parsed.get(&path) {
            return found.clone();
        }
        let parsed = match self.vfs.read(&path) {
            Ok(bytes) => match Layout::parse(&String::from_utf8_lossy(&bytes)) {
                Ok(layout) => Some(Arc::new(layout)),
                Err(error) => {
                    println!("gadget: {path} does not parse ({error})");
                    None
                }
            },
            Err(_) => {
                println!("gadget: {path} is not in the archives");
                None
            }
        };
        self.parsed.insert(path, parsed.clone());
        parsed
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageReport {
    pub named: Vec<String>,
    pub unresolved: Vec<String>,
}

pub fn image_report(
    layout: &Layout,
    mut resolve: impl FnMut(&str) -> Option<SurfaceIcon>,
) -> ImageReport {
    let mut report = ImageReport::default();
    for window in layout.all_windows() {
        for key in window.draw_keys() {
            for item in window.draw_data(key).unwrap_or_default() {
                if let Some(name) = item.image {
                    if !report.named.contains(&name) {
                        if resolve(&name).is_none() {
                            report.unresolved.push(name.clone());
                        }
                        report.named.push(name);
                    }
                }
            }
        }
    }
    report
}

pub fn slot_icon(
    icons: &mut IconProvider,
    renderer: &mut Renderer,
    item: Option<&DrawItem>,
) -> Option<SurfaceIcon> {
    let name = item?.image.as_deref()?;
    icons.icon_by_image(renderer, name)
}

#[must_use]
pub fn gadget_window(layout: &Layout) -> Option<&Window> {
    layout.windows.first()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_names_its_template_under_window() {
        assert_eq!(template_path("Apt/ListBox.wnd"), "window/apt/listbox.wnd");
        assert_eq!(
            template_path("Apt\\ComboBox.wnd"),
            "window/apt/combobox.wnd"
        );
        assert_eq!(template_path("Window/Apt/X.wnd"), "window/apt/x.wnd");
        assert_eq!(template_path(" /apt/y.wnd "), "window/apt/y.wnd");
    }

    #[test]
    fn an_image_report_counts_each_name_once() {
        let layout = Layout::parse(
            "WINDOW\n WINDOWTYPE = SCROLLLISTBOX;\n ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: Hilite, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;\n HILITEDRAWDATA = IMAGE: Hilite, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0, IMAGE: Gone, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;\nEND\n",
        )
        .unwrap_or_default();
        let mut asked = Vec::new();
        let report = image_report(&layout, |name| {
            asked.push(name.to_owned());
            None
        });
        assert_eq!(report.named, vec!["Hilite", "Gone"]);
        assert_eq!(report.unresolved, vec!["Hilite", "Gone"]);
        assert_eq!(asked, vec!["Hilite", "Gone"], "each asked once");
    }
}
