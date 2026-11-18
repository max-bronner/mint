use std::collections::BTreeMap;

use mint_sage_ini::UpgradeIndex;
use mint_sim::module::{UpgradeBit, UpgradeMask};

#[derive(Debug, Clone, Default)]
pub struct UpgradeBits {
    by_name: BTreeMap<String, UpgradeBit>,
    sub_masks: Vec<UpgradeMask>,
    is_player: Vec<bool>,
    persists: Vec<bool>,
    groups: BTreeMap<String, UpgradeMask>,
}

impl UpgradeBits {
    #[must_use]
    pub fn from_index(index: &UpgradeIndex) -> Self {
        let by_name: BTreeMap<String, UpgradeBit> = index
            .names()
            .enumerate()
            .map(|(position, name)| {
                let bit = u32::try_from(position).unwrap_or(u32::MAX);
                (name.to_owned(), UpgradeBit::new(bit))
            })
            .collect();
        let mut bits = Self {
            by_name,
            sub_masks: Vec::new(),
            is_player: Vec::new(),
            persists: Vec::new(),
            groups: BTreeMap::new(),
        };
        bits.sub_masks = index
            .names()
            .map(|name| {
                index
                    .get(name)
                    .map(|def| bits.mask(&def.sub_upgrade_template_names))
                    .unwrap_or_default()
            })
            .collect();
        bits.is_player = index
            .names()
            .map(|name| {
                index.get(name).is_some_and(|def| {
                    def.kind
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("PLAYER"))
                })
            })
            .collect();
        bits.persists = index
            .names()
            .map(|name| index.get(name).is_some_and(|def| def.persists_in_campaign))
            .collect();
        for name in index.names() {
            let (Some(def), Some(bit)) = (index.get(name), bits.bit(name)) else {
                continue;
            };
            if let Some(group) = def.group_name.as_deref() {
                if !bits.is_player(bit) {
                    bits.groups
                        .entry(group.to_ascii_lowercase())
                        .or_default()
                        .set(bit);
                }
            }
        }
        bits
    }

    #[must_use]
    pub fn group_mask<I, S>(&self, groups: I) -> UpgradeMask
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut mask = UpgradeMask::EMPTY;
        for group in groups {
            if let Some(members) = self.groups.get(&group.as_ref().to_ascii_lowercase()) {
                mask = mask.union(members);
            }
        }
        mask
    }

    #[must_use]
    pub fn persists_in_campaign(&self, bit: UpgradeBit) -> bool {
        usize::try_from(bit.index())
            .ok()
            .and_then(|at| self.persists.get(at))
            .copied()
            .unwrap_or(false)
    }

    #[must_use]
    pub fn bit(&self, name: &str) -> Option<UpgradeBit> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    #[must_use]
    pub fn mask<I, S>(&self, names: I) -> UpgradeMask
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut mask = UpgradeMask::EMPTY;
        for name in names {
            if let Some(bit) = self.bit(name.as_ref()) {
                mask.set(bit);
            }
        }
        mask
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    #[must_use]
    pub fn sub_mask(&self, bit: UpgradeBit) -> UpgradeMask {
        #[allow(clippy::cast_possible_truncation)]
        let index = bit.index() as usize;
        self.sub_masks.get(index).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn is_player(&self, bit: UpgradeBit) -> bool {
        #[allow(clippy::cast_possible_truncation)]
        let index = bit.index() as usize;
        self.is_player.get(index).copied().unwrap_or(false)
    }

    pub fn names(&self) -> impl Iterator<Item = (&str, UpgradeBit)> {
        self.by_name.iter().map(|(name, &bit)| (name.as_str(), bit))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    fn parse(text: &str) -> Document {
        Document::parse("upgrade.ini", text, &BlockTable::priority_blocks_path()).unwrap()
    }

    #[test]
    fn bits_are_assigned_in_alphabetical_order_not_declaration_order() {
        let doc = parse(
            "Upgrade Upgrade_Zeta\nEnd\n\
             Upgrade Upgrade_Alpha\nEnd\n\
             Upgrade Upgrade_Mid\nEnd\n",
        );
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::from_index(&index);

        assert_eq!(bits.bit("Upgrade_Alpha"), Some(UpgradeBit::new(0)));
        assert_eq!(bits.bit("Upgrade_Mid"), Some(UpgradeBit::new(1)));
        assert_eq!(bits.bit("Upgrade_Zeta"), Some(UpgradeBit::new(2)));
    }

    #[test]
    fn assignment_does_not_depend_on_which_document_is_inserted_first() {
        let a = parse("Upgrade Upgrade_Alpha\nEnd\n");
        let b = parse("Upgrade Upgrade_Zeta\nEnd\n");

        let mut forward = UpgradeIndex::new();
        forward.insert_document(&a);
        forward.insert_document(&b);

        let mut backward = UpgradeIndex::new();
        backward.insert_document(&b);
        backward.insert_document(&a);

        let bits_forward = UpgradeBits::from_index(&forward);
        let bits_backward = UpgradeBits::from_index(&backward);

        assert_eq!(
            bits_forward.bit("Upgrade_Alpha"),
            bits_backward.bit("Upgrade_Alpha")
        );
        assert_eq!(
            bits_forward.bit("Upgrade_Zeta"),
            bits_backward.bit("Upgrade_Zeta")
        );
    }

    #[test]
    fn name_lookup_is_case_insensitive() {
        let doc = parse("Upgrade Upgrade_Something\nEnd\n");
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::from_index(&index);
        assert_eq!(bits.bit("upgrade_SOMETHING"), bits.bit("Upgrade_Something"));
        assert!(bits.bit("upgrade_SOMETHING").is_some());
    }

    #[test]
    fn an_unknown_name_resolves_to_no_bit_and_mask_silently_omits_it() {
        let index = UpgradeIndex::new();
        let bits = UpgradeBits::from_index(&index);
        assert_eq!(bits.bit("Upgrade_Nonexistent"), None);
        let mask = bits.mask(["Upgrade_Nonexistent"]);
        assert!(mask.is_empty());
    }

    #[test]
    fn sub_mask_resolves_a_chain_and_defaults_empty() {
        let doc = parse(
            "Upgrade Upgrade_RohanForgedBlades\n  \
                SubUpgradeTemplateNames = Upgrade_ForPeasants Upgrade_ForElves\n\
            End\n\
            Upgrade Upgrade_ForPeasants\nEnd\n\
            Upgrade Upgrade_ForElves\nEnd\n",
        );
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::from_index(&index);

        let parent = bits.bit("Upgrade_RohanForgedBlades").unwrap();
        let chain = bits.sub_mask(parent);
        assert!(chain.contains(bits.bit("Upgrade_ForPeasants").unwrap()));
        assert!(chain.contains(bits.bit("Upgrade_ForElves").unwrap()));
        assert!(
            !chain.contains(parent),
            "the parent is not its own sub-upgrade"
        );

        let leaf = bits.bit("Upgrade_ForPeasants").unwrap();
        assert!(bits.sub_mask(leaf).is_empty(), "a leaf chains to nothing");
    }

    #[test]
    fn is_player_reads_type_and_defaults_object_for_unset_or_unknown() {
        let doc = parse(
            "Upgrade Upgrade_PlayerOne\n  Type = PLAYER\nEnd\n\
             Upgrade Upgrade_ObjectOne\n  Type = OBJECT\nEnd\n\
             Upgrade Upgrade_Unset\nEnd\n",
        );
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::from_index(&index);

        assert!(bits.is_player(bits.bit("Upgrade_PlayerOne").unwrap()));
        assert!(!bits.is_player(bits.bit("Upgrade_ObjectOne").unwrap()));
        assert!(!bits.is_player(bits.bit("Upgrade_Unset").unwrap()));
        assert!(
            !bits.is_player(UpgradeBit::new(999)),
            "past the assigned range"
        );
    }

    #[test]
    fn mask_unions_every_resolvable_name() {
        let doc = parse("Upgrade Upgrade_A\nEnd\nUpgrade Upgrade_B\nEnd\n");
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::from_index(&index);
        let mask = bits.mask(["Upgrade_A", "Upgrade_B", "Upgrade_Unknown"]);
        assert!(mask.contains(bits.bit("Upgrade_A").unwrap()));
        assert!(mask.contains(bits.bit("Upgrade_B").unwrap()));
    }
}
