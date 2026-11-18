use std::collections::BTreeMap;

use mint_sage_ini::object::{MAX_INHERITANCE_DEPTH, OBJECT_KEYWORDS};
use mint_sage_ini::{Block, Document, MacroTable, UpgradeIndex};
use mint_sim::module::{Capability, ModuleRegistry};
use mint_sim::{Entity, Frame, World};

use crate::fields::BlockFields;
use crate::upgrade_bits::UpgradeBits;

const MODULE_SLOT_KEYWORDS: [&str; 2] = ["Behavior", "Body"];

const DEFAULT_TEMPLATE_NAME: &str = "DefaultThingTemplate";

const INHERITABLE_MODULE_KEYWORD: &str = "InheritableModule";
const REPLACE_MODULE_KEYWORD: &str = "ReplaceModule";
const ADD_MODULE_KEYWORD: &str = "AddModule";
const REMOVE_MODULE_KEYWORD: &str = "RemoveModule";

#[derive(Debug, Clone, Copy)]
pub struct ModuleDeclaration<'a> {
    pub name: &'a str,
    pub block: &'a Block,
}

#[derive(Debug, Default)]
pub struct TemplateIndex<'a> {
    entries: BTreeMap<String, &'a Block>,
    overrides: BTreeMap<String, Vec<&'a Block>>,
    macros: MacroTable,
    upgrades: UpgradeIndex,
}

impl<'a> TemplateIndex<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &'a Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
        self.macros.insert_defines(document.defines());
        self.upgrades.insert_document(document);
    }

    #[must_use]
    pub fn macros(&self) -> &MacroTable {
        &self.macros
    }

    #[must_use]
    pub fn upgrades(&self) -> &UpgradeIndex {
        &self.upgrades
    }

    pub fn insert_block(&mut self, block: &'a Block) {
        if !OBJECT_KEYWORDS.iter().any(|k| block.keyword == *k) {
            return;
        }
        let Some(name) = block.name() else { return };
        self.entries.insert(name.to_ascii_lowercase(), block);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    #[must_use]
    pub fn lineage(&self, name: &str) -> Vec<&'a Block> {
        let mut path: Vec<&Block> = Vec::new();
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let Some(&block) = self.entries.get(&key) else {
                break;
            };
            path.push(block);
            let parent = if block.keyword == "Object" {
                None
            } else {
                block.args.get(1).map(|p| p.to_ascii_lowercase())
            };
            match parent {
                Some(p) => key = p,
                None => break,
            }
        }
        path.reverse();
        if let Some(layers) = self.overrides.get(&name.to_ascii_lowercase()) {
            if !path.is_empty() {
                path.extend(layers.iter().copied());
            }
        }
        path
    }

    pub fn insert_override_document(&mut self, document: &'a Document) -> OverrideLoad {
        let mut load = OverrideLoad::default();
        for block in document.roots() {
            if !OBJECT_KEYWORDS.iter().any(|k| block.keyword == *k) {
                continue;
            }
            let Some(name) = block.name() else { continue };
            let key = name.to_ascii_lowercase();
            for slot in block
                .blocks()
                .filter(|b| MODULE_SLOT_TAGS.contains(&b.keyword.as_str()))
            {
                load.errors.push(format!(
                    "{}:{}: {} {name}: `{}` outside AddModule/ReplaceModule",
                    slot.file, slot.line, block.keyword, slot.keyword
                ));
            }
            let existing = self.entries.contains_key(&key);
            let known = if existing {
                self.lineage(&key)
            } else {
                block
                    .args
                    .get(1)
                    .map(|parent| self.lineage(parent))
                    .unwrap_or_default()
            };
            let mut tags = self.default_tags();
            for link in &known {
                collect_tags(link, &mut tags);
            }
            for (tag, line) in removed_tags(block) {
                if !tags.iter().any(|(held, _)| *held == tag) {
                    load.errors.push(format!(
                        "{}:{line}: {} {name}: no module tagged `{tag}` to remove or replace",
                        block.file, block.keyword
                    ));
                }
            }
            for wrapper in block
                .blocks()
                .filter(|b| b.keyword == REPLACE_MODULE_KEYWORD)
            {
                let Some(replaced) = wrapper.args.first() else {
                    continue;
                };
                let Some((_, kind)) = tags.iter().find(|(held, _)| held == replaced) else {
                    continue;
                };
                for slot in wrapper
                    .blocks()
                    .filter(|b| MODULE_SLOT_TAGS.contains(&b.keyword.as_str()))
                {
                    let (Some(new_kind), new_tag) = (slot.args.first(), slot.args.get(1)) else {
                        continue;
                    };
                    if new_kind.as_str() != *kind || new_tag == Some(replaced) {
                        load.errors.push(format!(
                            "{}:{}: {} {name}: ReplaceModule {replaced} needs a `{kind}` with a new tag",
                            slot.file, slot.line, block.keyword
                        ));
                    }
                }
            }
            if existing {
                self.overrides.entry(key.clone()).or_default().push(block);
            } else {
                self.entries.insert(key.clone(), block);
            }
            if !load.touched.contains(&key) {
                load.touched.push(key);
            }
        }
        self.macros.insert_defines(document.defines());
        self.upgrades.insert_document(document);
        load
    }

    fn default_tags(&self) -> Vec<(&'a str, &'a str)> {
        let mut tags = Vec::new();
        if let Some(block) = self
            .entries
            .get(&DEFAULT_TEMPLATE_NAME.to_ascii_lowercase())
        {
            collect_tags(block, &mut tags);
        }
        tags
    }

    #[must_use]
    pub fn resolve(&self, name: &str, registry: &ModuleRegistry) -> Vec<ModuleDeclaration<'a>> {
        self.compose(
            name,
            |decl| {
                declared_as_body(decl)
                    || registry
                        .capabilities_of(decl.name)
                        .is_some_and(|mask| mask.contains(Capability::Body))
            },
            |decl| {
                registry
                    .capabilities_of(decl.name)
                    .is_some_and(|mask| mask.contains(Capability::Die))
            },
        )
    }

    #[must_use]
    pub fn declared_modules(&self, name: &str) -> Vec<ModuleDeclaration<'a>> {
        self.compose(name, declared_as_body, is_die_capable)
    }

    fn compose(
        &self,
        name: &str,
        is_body: impl Fn(&ModuleDeclaration<'_>) -> bool,
        is_die_capable_default: impl Fn(&ModuleDeclaration<'_>) -> bool,
    ) -> Vec<ModuleDeclaration<'a>> {
        let lineage = self.lineage(name);
        let Some((root, rest)) = lineage.split_first() else {
            return Vec::new();
        };
        let mut list: Vec<ModuleDeclaration<'a>> = Vec::new();

        let default_parts = (!name.eq_ignore_ascii_case(DEFAULT_TEMPLATE_NAME))
            .then(|| {
                self.entries
                    .get(&DEFAULT_TEMPLATE_NAME.to_ascii_lowercase())
            })
            .flatten()
            .map(|&block| default_declarations(block));
        if let Some((inheritable, _)) = &default_parts {
            list.extend(inheritable.iter().copied());
        }

        let root_effects = own_module_effects(root);
        if let Some((_, plain)) = &default_parts {
            for candidate in plain {
                let evicted = root_effects.declares.iter().any(|new| {
                    (is_body(new) && is_body(candidate))
                        || (is_die_capable_default(new) && is_die_capable_default(candidate))
                });
                if !evicted {
                    list.push(*candidate);
                }
            }
        }
        apply_effects(&mut list, &root_effects, &is_body);

        for block in rest {
            apply_effects(&mut list, &own_module_effects(block), &is_body);
        }
        list
    }
}

fn declared_as_body(decl: &ModuleDeclaration<'_>) -> bool {
    decl.block.keyword == "Body"
}

fn die_capable_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        crate::register_every_family(&mut registry, &mut world);
        registry
            .names_with(Capability::Die)
            .into_iter()
            .map(str::to_owned)
            .collect()
    })
}

fn is_die_capable(decl: &ModuleDeclaration<'_>) -> bool {
    die_capable_names()
        .iter()
        .any(|name| name.eq_ignore_ascii_case(decl.name))
        || decl.name.ends_with("Die")
}

fn own_module_declarations(object: &Block) -> Vec<ModuleDeclaration<'_>> {
    object
        .blocks()
        .filter(|b| MODULE_SLOT_KEYWORDS.iter().any(|k| b.keyword == *k))
        .filter_map(|b| {
            b.selector()
                .map(|name| ModuleDeclaration { name, block: b })
        })
        .collect()
}

fn default_declarations(
    default_block: &Block,
) -> (Vec<ModuleDeclaration<'_>>, Vec<ModuleDeclaration<'_>>) {
    let plain = own_module_declarations(default_block);
    let inheritable = default_block
        .blocks()
        .filter(|b| b.keyword == INHERITABLE_MODULE_KEYWORD)
        .flat_map(own_module_declarations)
        .collect();
    (inheritable, plain)
}

#[derive(Debug, Default)]
struct BlockEffects<'a> {
    declares: Vec<ModuleDeclaration<'a>>,
    removes: Vec<&'a str>,
}

fn own_module_effects(object: &Block) -> BlockEffects<'_> {
    let mut effects = BlockEffects {
        declares: own_module_declarations(object),
        removes: Vec::new(),
    };
    for wrapper in object.blocks() {
        if wrapper.keyword == ADD_MODULE_KEYWORD {
            effects.declares.extend(own_module_declarations(wrapper));
        } else if wrapper.keyword == REPLACE_MODULE_KEYWORD {
            if let Some(tag) = wrapper.args.first() {
                effects.removes.push(tag.as_str());
            }
            effects.declares.extend(own_module_declarations(wrapper));
        }
    }
    for field in object.fields() {
        if field.keyword == REMOVE_MODULE_KEYWORD {
            if let Some(tag) = field.value() {
                effects.removes.push(tag);
            }
        }
    }
    effects
}

fn apply_effects<'a>(
    list: &mut Vec<ModuleDeclaration<'a>>,
    effects: &BlockEffects<'a>,
    is_body: &impl Fn(&ModuleDeclaration<'_>) -> bool,
) {
    for &tag in &effects.removes {
        evict_by_tag(list, tag);
    }
    for decl in &effects.declares {
        evict_overridden(list, decl, is_body);
        list.push(*decl);
    }
}

#[derive(Debug, Default)]
pub struct OverrideLoad {
    pub touched: Vec<String>,
    pub errors: Vec<String>,
}

const MODULE_SLOT_TAGS: [&str; 4] = ["Behavior", "Body", "Draw", "ClientUpdate"];

fn collect_tags<'a>(block: &'a Block, tags: &mut Vec<(&'a str, &'a str)>) {
    for child in block.blocks() {
        if MODULE_SLOT_TAGS.contains(&child.keyword.as_str()) {
            if let (Some(kind), Some(tag)) = (child.args.first(), child.args.get(1)) {
                tags.push((tag.as_str(), kind.as_str()));
            }
        } else if [
            ADD_MODULE_KEYWORD,
            REPLACE_MODULE_KEYWORD,
            INHERITABLE_MODULE_KEYWORD,
        ]
        .contains(&child.keyword.as_str())
        {
            collect_tags(child, tags);
        }
    }
}

fn removed_tags(block: &Block) -> Vec<(&str, u32)> {
    let mut out: Vec<(&str, u32)> = block
        .fields()
        .filter(|field| field.keyword == REMOVE_MODULE_KEYWORD)
        .filter_map(|field| Some((field.value()?, field.line)))
        .collect();
    out.extend(
        block
            .blocks()
            .filter(|wrapper| wrapper.keyword == REPLACE_MODULE_KEYWORD)
            .filter_map(|wrapper| Some((wrapper.args.first()?.as_str(), wrapper.line))),
    );
    out
}

fn tag_of<'a>(decl: &ModuleDeclaration<'a>) -> Option<&'a str> {
    decl.block.args.get(1).map(String::as_str)
}

fn evict_by_tag(list: &mut Vec<ModuleDeclaration<'_>>, tag: &str) {
    list.retain(|existing| tag_of(existing).is_none_or(|t| t != tag));
}

fn evict_overridden(
    list: &mut Vec<ModuleDeclaration<'_>>,
    new: &ModuleDeclaration<'_>,
    is_body: &impl Fn(&ModuleDeclaration<'_>) -> bool,
) {
    let new_is_body = is_body(new);
    let new_tag = tag_of(new);
    list.retain(|existing| {
        let same = match (new_tag, tag_of(existing)) {
            (Some(a), Some(b)) => a == b,
            _ => existing.name == new.name,
        };
        if same {
            return false;
        }
        !(new_is_body && is_body(existing))
    });
}

pub fn spawn_template(
    world: &mut World,
    registry: &ModuleRegistry,
    now: Frame,
    index: &TemplateIndex<'_>,
    upgrade_bits: &UpgradeBits,
    name: &str,
) -> mint_sim::Result<Entity> {
    let entity = world.spawn()?;
    for decl in index.resolve(name, registry) {
        let fields = BlockFields(decl.block, index.macros(), upgrade_bits);
        registry.attach_named(world, entity, now, decl.name, &fields)?;
    }
    Ok(entity)
}

#[cfg(test)]
mod tests;
