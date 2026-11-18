use crate::instance::{InstanceId, InstanceKind};
use crate::library::{CharacterRef, Library, MovieId};
use crate::timeline::Stage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadRequest {
    pub(crate) target: String,
    pub(crate) url: Option<String>,
}

pub(crate) fn movie_name(url: &str) -> Option<String> {
    let file = url.rsplit(['/', '\\']).next()?;
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    (!stem.is_empty()).then(|| stem.to_ascii_lowercase())
}

pub(crate) fn names_a_clip(target: &str) -> bool {
    let t = target.trim();
    let lower = t.to_ascii_lowercase();
    !t.is_empty()
        && !matches!(lower.as_str(), "_blank" | "_self" | "_top" | "_parent")
        && (lower.starts_with("_level0.") || lower.starts_with("_root.") || t.starts_with('/'))
}

impl Stage {
    pub(crate) fn request_load(&mut self, target: &str, url: Option<&str>) {
        if !names_a_clip(target) {
            return;
        }
        if self.pending_loads.len() >= self.vm_limits.max_outward {
            self.fail(crate::Error::Limit {
                what: "pending movie loads",
                limit: self.vm_limits.max_outward,
            });
            return;
        }
        self.pending_loads.push(LoadRequest {
            target: target.trim().to_owned(),
            url: url.map(str::to_owned),
        });
    }

    pub(crate) fn run_loads(&mut self, lib: &Library) {
        for request in std::mem::take(&mut self.pending_loads) {
            let Some(target) = self.clip_at_path(&request.target) else {
                self.counters.loads_refused = self.counters.loads_refused.saturating_add(1);
                continue;
            };
            match request.url.as_deref() {
                Some(url) => match movie_name(url).and_then(|name| lib.movie_id(&name)) {
                    Some(movie) => self.load_into(lib, target, movie),
                    None => {
                        self.counters.loads_refused = self.counters.loads_refused.saturating_add(1);
                    }
                },
                None => self.unload(lib, target),
            }
        }
    }

    fn clip_at_path(&self, path: &str) -> Option<InstanceId> {
        let lower = path.to_ascii_lowercase();
        let rest = ["_level0", "_root"]
            .iter()
            .find_map(|prefix| lower.starts_with(prefix).then(|| path.get(prefix.len()..)))
            .flatten()
            .unwrap_or(path);
        let mut at = self.root?;
        let mut any = false;
        for segment in rest.split(['.', '/']).filter(|s| !s.is_empty()) {
            at = self.child_by_name(at, segment)?;
            any = true;
        }
        let live = self
            .arena
            .get(at)
            .is_some_and(|i| !i.removed && i.clip().is_some());
        (any && live).then_some(at)
    }

    fn clear_children(&mut self, lib: &Library, clip: InstanceId) {
        let children: Vec<InstanceId> = self
            .arena
            .get(clip)
            .map(|i| i.children.values().copied().collect())
            .unwrap_or_default();
        for child in children {
            self.remove_child_directly(lib, child);
        }
    }

    fn load_into(&mut self, lib: &Library, clip: InstanceId, movie: MovieId) {
        let Some(timeline) = lib.movie(movie).map(|m| m.root) else {
            return;
        };
        self.clear_children(lib, clip);
        let Some(inst) = self.arena.get_mut(clip) else {
            return;
        };
        inst.character = CharacterRef { movie, id: 0 };
        inst.visible = true;
        if let InstanceKind::Clip(state) = &mut inst.kind {
            state.timeline = timeline;
            state.current_frame = 0;
            state.playing = true;
            state.initialized = false;
        }
        if !self.exec.contains(&clip) {
            self.exec.push_front(clip);
        }
        if self.scripts_enabled {
            self.queue_init_actions(lib, movie, clip);
        }
        self.counters.movies_loaded = self.counters.movies_loaded.saturating_add(1);
        self.loaded.push((clip, movie));
    }

    fn unload(&mut self, lib: &Library, clip: InstanceId) {
        self.clear_children(lib, clip);
        if let Some(state) = self.arena.get_mut(clip).and_then(|i| i.clip_mut()) {
            state.playing = false;
        }
        self.loaded.retain(|(c, _)| *c != clip);
    }
}

#[cfg(test)]
mod tests {
    use super::{movie_name, names_a_clip};

    #[test]
    fn a_url_names_a_movie_by_its_file_stem() {
        assert_eq!(
            movie_name("InGameHeroSelect.swf").as_deref(),
            Some("ingameheroselect")
        );
        assert_eq!(movie_name("art/Skirmish.apt").as_deref(), Some("skirmish"));
        assert_eq!(movie_name("x\\Palantir").as_deref(), Some("palantir"));
        assert_eq!(movie_name(".swf"), None);
    }

    #[test]
    fn a_target_is_a_clip_only_below_the_root() {
        assert!(names_a_clip("_level0.HeroSelectUI"));
        assert!(names_a_clip("_root.a.b"));
        assert!(names_a_clip("/a/b"));
        assert!(!names_a_clip("_blank"));
        assert!(!names_a_clip("_level0"));
        assert!(!names_a_clip(""));
        assert!(!names_a_clip("http://x"));
    }
}
