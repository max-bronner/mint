#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    id: u32,
    generation: u32,
    template: usize,
}

#[derive(Debug, Default)]
pub(super) struct DrawableKeys {
    keys: Vec<Key>,
    by_id: Vec<(u32, usize)>,
}

impl DrawableKeys {
    pub(super) fn rebuild(&mut self, keys: impl Iterator<Item = (u32, u32, usize)>) {
        self.keys.clear();
        self.keys.extend(keys.map(|(id, generation, template)| Key {
            id,
            generation,
            template,
        }));
        self.by_id.clear();
        self.by_id.extend(
            self.keys
                .iter()
                .enumerate()
                .map(|(position, key)| (key.id, position)),
        );
        if !self.by_id.is_sorted() {
            self.by_id.sort_unstable();
        }
    }

    fn first_of_id(&self, id: u32, wanted: impl Fn(&Key) -> bool) -> Option<usize> {
        let start = self.by_id.partition_point(|(held, _)| *held < id);
        self.by_id
            .get(start..)?
            .iter()
            .take_while(|(held, _)| *held == id)
            .map(|(_, position)| *position)
            .find(|position| self.keys.get(*position).is_some_and(&wanted))
    }

    pub(super) fn by_id(&self, id: u32) -> Option<usize> {
        self.first_of_id(id, |_| true)
    }

    pub(super) fn by_key(&self, id: u32, generation: u32, template: usize) -> Option<usize> {
        self.first_of_id(id, |key| {
            key.generation == generation && key.template == template
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(
        list: &[(u32, u32, usize)],
        id: u32,
        generation: u32,
        template: usize,
    ) -> Option<usize> {
        list.iter().position(|k| *k == (id, generation, template))
    }

    #[test]
    fn lookups_match_a_linear_find_sorted_or_not() {
        let sorted = [
            (1, 0, 4),
            (3, 0, 2),
            (3, 0, 1),
            (3, 1, 1),
            (3, 0, 1),
            (9, 2, 0),
        ];
        let shuffled = [
            (9, 2, 0),
            (3, 0, 1),
            (1, 0, 4),
            (3, 1, 1),
            (3, 0, 2),
            (3, 0, 1),
        ];
        for list in [sorted, shuffled] {
            let mut keys = DrawableKeys::default();
            keys.rebuild(list.iter().copied());
            for id in 0..11 {
                assert_eq!(
                    keys.by_id(id),
                    list.iter().position(|k| k.0 == id),
                    "id {id}"
                );
                for generation in 0..3 {
                    for template in 0..5 {
                        assert_eq!(
                            keys.by_key(id, generation, template),
                            linear(&list, id, generation, template),
                            "key {id} {generation} {template}"
                        );
                    }
                }
            }
        }
    }
}
