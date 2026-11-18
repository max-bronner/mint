use std::collections::BTreeMap;

use mint_sim::StateHasher;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ObjectLists(BTreeMap<String, Vec<String>>);

impl ObjectLists {
    pub(crate) fn add(&mut self, list: &str, template: &str) {
        let types = self.0.entry(list.to_owned()).or_default();
        if !types.iter().any(|t| t == template) {
            types.push(template.to_owned());
        }
    }

    pub(crate) fn remove(&mut self, list: &str, template: &str) {
        if let Some(types) = self.0.get_mut(list) {
            types.retain(|t| t != template);
            if types.is_empty() {
                self.0.remove(list);
            }
        }
    }

    pub(crate) fn types_of(&self, name: &str) -> Vec<String> {
        self.0
            .get(name)
            .cloned()
            .unwrap_or_else(|| vec![name.to_owned()])
    }

    pub(crate) fn is_list(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (name, types) in &self.0 {
            hasher.write_bytes(name.as_bytes());
            hasher.write_u8(0);
            hasher.write_usize(types.len());
            for t in types {
                hasher.write_bytes(t.as_bytes());
                hasher.write_u8(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_the_last_member_turns_the_list_back_into_a_plain_template_name() {
        let mut lists = ObjectLists::default();
        lists.add("Group", "A");
        lists.add("Group", "A");
        lists.add("Group", "B");
        assert_eq!(lists.types_of("Group"), ["A", "B"]);
        lists.remove("Group", "A");
        lists.remove("Group", "B");
        assert!(!lists.is_list("Group"));
        assert_eq!(lists.types_of("Group"), ["Group"]);
    }
}
