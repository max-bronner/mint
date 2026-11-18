use crate::StringRecord;

#[derive(Debug, Clone, Default)]
pub struct StringTable {
    entries: Vec<Entry>,
    duplicates: usize,
}

#[derive(Debug, Clone)]
struct Entry {
    folded: String,
    label: String,
    text: String,
}

impl StringTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, records: impl IntoIterator<Item = StringRecord>) {
        for record in records {
            let folded = record.label.to_ascii_lowercase();
            if self.contains_folded(&folded) {
                self.duplicates = self.duplicates.saturating_add(1);
                continue;
            }
            let entry = Entry {
                folded,
                label: record.label,
                text: record.text,
            };
            match self
                .entries
                .binary_search_by(|e| e.folded.cmp(&entry.folded))
            {
                Ok(_) => self.duplicates = self.duplicates.saturating_add(1),
                Err(at) => self.entries.insert(at, entry),
            }
        }
    }

    fn contains_folded(&self, folded: &str) -> bool {
        self.entries
            .binary_search_by(|e| e.folded.as_str().cmp(folded))
            .is_ok()
    }

    #[must_use]
    pub fn get(&self, label: &str) -> Option<&str> {
        let folded = label.to_ascii_lowercase();
        let at = self
            .entries
            .binary_search_by(|e| e.folded.as_str().cmp(folded.as_str()))
            .ok()?;
        self.entries.get(at).map(|e| e.text.as_str())
    }

    #[must_use]
    pub fn fetch(&self, label: &str) -> String {
        match self.get(label) {
            Some(text) => text.to_owned(),
            None => format!("MISSING: '{label}'"),
        }
    }

    #[must_use]
    pub fn duplicates(&self) -> usize {
        self.duplicates
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|e| (e.label.as_str(), e.text.as_str()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn record(label: &str, text: &str) -> StringRecord {
        StringRecord {
            label: label.to_owned(),
            text: text.to_owned(),
            speech: None,
        }
    }

    #[test]
    fn lookup_ignores_case() {
        let mut table = StringTable::new();
        table.insert([record("APT:SoloPlay", "SOLO PLAY")]);
        assert_eq!(table.get("apt:soloplay"), Some("SOLO PLAY"));
        assert_eq!(table.get("APT:SOLOPLAY"), Some("SOLO PLAY"));
    }

    #[test]
    fn the_first_definition_wins_and_the_rest_are_counted() {
        let mut table = StringTable::new();
        table.insert([record("K", "first"), record("k", "second")]);
        assert_eq!(table.get("K"), Some("first"));
        assert_eq!(table.duplicates(), 1);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn a_later_file_does_not_override_an_earlier_one() {
        let mut table = StringTable::new();
        table.insert([record("Map:Name", "main")]);
        table.insert([record("Map:Name", "map file")]);
        assert_eq!(table.get("Map:Name"), Some("main"));
    }

    #[test]
    fn a_later_file_still_contributes_its_own_labels() {
        let mut table = StringTable::new();
        table.insert([record("A", "1")]);
        table.insert([record("B", "2")]);
        assert_eq!(table.get("B"), Some("2"));
    }

    #[test]
    fn a_missing_label_fetches_the_engines_placeholder() {
        let table = StringTable::new();
        assert_eq!(table.fetch("APT:Nope"), "MISSING: 'APT:Nope'");
        assert_eq!(table.get("APT:Nope"), None);
    }

    #[test]
    fn iteration_is_sorted_and_stable() {
        let mut table = StringTable::new();
        table.insert([record("b", "2"), record("A", "1"), record("c", "3")]);
        let labels: Vec<&str> = table.iter().map(|(l, _)| l).collect();
        assert_eq!(labels, ["A", "b", "c"]);
    }
}
