use std::collections::BTreeSet;

use mint_sage_ini::{
    AnimationStateDef, FxList, FxListIndex, FxNugget, FxParticleNugget, ModelStateDef, NameField,
    NameFieldIndex, ObjectIndex, ParticleIndex, ParticleSystemDef,
};

const OBJECT_KEYWORDS: [&str; 3] = mint_sage_ini::object::OBJECT_KEYWORDS;
const WEAPON_KEYWORD: &str = "Weapon";

pub const WHICH_SPECIAL_WEAPON: &str = "WhichSpecialWeapon";
pub const UNPACK_TIME: &str = "UnpackTime";
const FX_LIST_KEYWORD: &str = "FXList";
const PARTICLE_SYSTEM_KEYWORD: &str = "FXParticleSystem";
const ENTERING_STATE_FX_KEYWORD: &str = mint_sage_ini::anim_state::ENTERING_STATE_FX_KEYWORD;

fn conditions_label(conditions: &[String]) -> String {
    if conditions.is_empty() {
        "(default)".to_owned()
    } else {
        conditions.join(" ")
    }
}

pub const MAX_TREE_DEPTH: u32 = 12;

pub const MAX_NODES: usize = 500_000;

const BUCKET_SIZE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Category {
    FxLists,
    Systems,
    Objects,
    Weapons,
}

impl Category {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FxLists => "fx-lists",
            Self::Systems => "systems",
            Self::Objects => "objects",
            Self::Weapons => "weapons",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| text.eq_ignore_ascii_case(category.label()))
    }

    pub const ALL: [Self; 4] = [Self::FxLists, Self::Systems, Self::Objects, Self::Weapons];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeKind {
    Category(Category),
    Bucket,
    FxList,
    ParticleNugget,
    BoneNugget,
    OtherNugget,
    SysBoneLeaf,
    System,
    Dangling,
    Stopped,
    Object,
    Weapon,
    ModelState,
    AnimState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Role {
    Root,
    Nugget,
    NuggetSystem,
    Slave,
    PerParticle,
    Collision,
    BoneList,
    SysBone,
    ModelState,
    AnimState,
    Entering,
    Event,
    Field,
}

impl Role {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Root => "",
            Self::Nugget => "nugget",
            Self::NuggetSystem => "system",
            Self::Slave => "slave",
            Self::PerParticle => "per-particle",
            Self::Collision => "collision",
            Self::BoneList => "at-bone",
            Self::SysBone => "sysbone",
            Self::ModelState => "model state",
            Self::AnimState => "anim state",
            Self::Entering => "entering",
            Self::Event => "fx event",
            Self::Field => "field",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub role: Role,
    pub name: String,
    pub detail: String,
    pub system: Option<String>,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub depth: u32,
    pub conditions: Vec<String>,
    pub fire_after: Option<f32>,
}

impl Node {
    #[must_use]
    pub fn line(&self) -> String {
        let role = self.role.label();
        let mut line = String::new();
        if !role.is_empty() {
            line.push_str(role);
            line.push_str(": ");
        }
        line.push_str(&self.name);
        if !self.detail.is_empty() {
            line.push_str("  ");
            line.push_str(&self.detail);
        }
        line
    }
}

#[derive(Debug, Clone, Default)]
pub struct FxTree {
    nodes: Vec<Node>,
    categories: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub index: &'a ParticleIndex,
    pub lists: &'a FxListIndex,
    pub objects: &'a ObjectIndex,
    pub names: &'a NameFieldIndex,
}

impl FxTree {
    #[must_use]
    pub fn build(sources: Sources<'_>) -> Self {
        let mut builder = Builder {
            tree: Self::default(),
            index: sources.index,
            lists: sources.lists,
            objects: sources.objects,
            names: sources.names,
            referenced: BTreeSet::new(),
            path: Vec::new(),
        };
        builder.build();
        builder.tree
    }

    #[must_use]
    pub fn object_of(&self, index: usize) -> Option<&str> {
        let mut current = Some(index);
        for _ in 0..MAX_NODES {
            let node = self.node(current?)?;
            if node.kind == NodeKind::Object {
                return Some(node.name.as_str());
            }
            current = node.parent;
        }
        None
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn node(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    #[must_use]
    pub fn category(&self, category: Category) -> Option<usize> {
        self.categories.iter().copied().find(|index| {
            self.node(*index).map(|node| node.kind) == Some(NodeKind::Category(category))
        })
    }

    #[must_use]
    pub fn roots(&self, category: Category, filter: &str) -> Vec<usize> {
        let Some(header) = self.category(category) else {
            return Vec::new();
        };
        let needle = filter.to_ascii_lowercase();
        let mut found = Vec::new();
        for bucket in self.children(header) {
            for root in self.children(bucket) {
                if self.matches(root, &needle) {
                    found.push(root);
                }
            }
        }
        found
    }

    #[must_use]
    pub fn rows(&self, filter: &str, expanded: &BTreeSet<usize>) -> Vec<usize> {
        let needle = filter.to_ascii_lowercase();
        let mut rows = Vec::new();
        for header in &self.categories {
            rows.push(*header);
            if !expanded.contains(header) {
                continue;
            }
            for bucket in self.children(*header) {
                let matching: Vec<usize> = self
                    .children(bucket)
                    .into_iter()
                    .filter(|root| self.matches(*root, &needle))
                    .collect();
                if matching.is_empty() {
                    continue;
                }
                rows.push(bucket);
                if !expanded.contains(&bucket) {
                    continue;
                }
                for root in matching {
                    self.push_subtree(root, expanded, &mut rows);
                }
            }
        }
        rows
    }

    #[must_use]
    pub fn subtree_systems(&self, index: usize) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut stack = vec![index];
        let mut seen = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            let Some(node) = self.node(current) else {
                continue;
            };
            if let Some(system) = node.system.as_ref() {
                found.insert(system.clone());
            }
            stack.extend(node.children.iter().copied());
        }
        found
    }

    #[must_use]
    pub fn path(&self, index: usize) -> String {
        let mut parts = Vec::new();
        let mut current = Some(index);
        let mut steps = 0usize;
        while let Some(at) = current {
            let Some(node) = self.node(at) else {
                break;
            };
            if node.kind != NodeKind::Bucket {
                parts.push(node.name.clone());
            }
            current = node.parent;
            steps = steps.saturating_add(1);
            if steps > MAX_NODES {
                break;
            }
        }
        parts.reverse();
        parts.join(" > ")
    }

    #[must_use]
    pub fn render(&self, index: usize) -> String {
        let base = self.node(index).map_or(0, |node| node.depth);
        let mut out = String::new();
        let mut stack = vec![index];
        let mut guard = 0usize;
        while let Some(current) = stack.pop() {
            guard = guard.saturating_add(1);
            if guard > MAX_NODES {
                break;
            }
            let Some(node) = self.node(current) else {
                continue;
            };
            for _ in 0..node.depth.saturating_sub(base) {
                out.push_str("  ");
            }
            out.push_str(&node.line());
            out.push('\n');
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
        out
    }

    #[must_use]
    pub fn find_root(&self, name: &str) -> Option<usize> {
        for category in Category::ALL {
            for root in self.roots(category, "") {
                if self
                    .node(root)
                    .is_some_and(|node| node.name.eq_ignore_ascii_case(name))
                {
                    return Some(root);
                }
            }
        }
        None
    }

    fn children(&self, index: usize) -> Vec<usize> {
        self.node(index)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }

    fn matches(&self, index: usize, needle: &str) -> bool {
        needle.is_empty()
            || self
                .node(index)
                .is_some_and(|node| node.name.to_ascii_lowercase().contains(needle))
    }

    fn push_subtree(&self, index: usize, expanded: &BTreeSet<usize>, rows: &mut Vec<usize>) {
        rows.push(index);
        if !expanded.contains(&index) {
            return;
        }
        for child in self.children(index) {
            self.push_subtree(child, expanded, rows);
        }
    }
}

struct Builder<'a> {
    tree: FxTree,
    index: &'a ParticleIndex,
    lists: &'a FxListIndex,
    objects: &'a ObjectIndex,
    names: &'a NameFieldIndex,
    referenced: BTreeSet<String>,
    path: Vec<(NodeKind, String)>,
}

impl Builder<'_> {
    fn build(&mut self) {
        self.collect_referenced();
        let list_names: Vec<String> = self
            .lists
            .iter()
            .map(|(_, list)| list.name.clone())
            .collect();
        self.category(Category::FxLists, &list_names);

        let systems: Vec<String> = self
            .index
            .iter()
            .map(|(_, definition)| definition.name.clone())
            .collect();
        self.category(Category::Systems, &systems);

        let objects: Vec<String> = self
            .objects
            .names()
            .map(str::to_owned)
            .filter(|name| self.object_has_fx(name))
            .collect();
        self.category(Category::Objects, &objects);

        let weapons: Vec<String> = self
            .names
            .names_of(WEAPON_KEYWORD)
            .map(str::to_owned)
            .filter(|name| {
                self.composed_fields(&[WEAPON_KEYWORD], name)
                    .iter()
                    .any(|field| self.resolves(&field.value))
            })
            .collect();
        self.category(Category::Weapons, &weapons);
    }

    fn collect_referenced(&mut self) {
        let mut found = BTreeSet::new();
        for keyword in self.names.keywords() {
            if keyword.eq_ignore_ascii_case(FX_LIST_KEYWORD)
                || keyword.eq_ignore_ascii_case(PARTICLE_SYSTEM_KEYWORD)
            {
                continue;
            }
            for name in self.names.names_of(&keyword) {
                for field in self.names.get(&keyword, name) {
                    found.insert(field.value.to_ascii_lowercase());
                }
            }
        }
        for name in self.objects.names() {
            if let Some(bones) = self.objects.particle_bones(name) {
                for bone in bones.all() {
                    found.insert(bone.system.to_ascii_lowercase());
                }
            }
            for state in self.objects.animation_states(name) {
                for event in &state.fx_events {
                    found.insert(event.fx_list.to_ascii_lowercase());
                }
            }
        }
        self.referenced = found;
    }

    fn root_detail(&self, role: Role, name: &str, detail: String) -> String {
        if role != Role::Root || self.referenced.contains(&name.to_ascii_lowercase()) {
            return detail;
        }
        if detail.is_empty() {
            "unreferenced".to_owned()
        } else {
            format!("{detail}, unreferenced")
        }
    }

    fn resolves(&self, name: &str) -> bool {
        self.lists.get(name).is_some() || self.index.get(name).is_some()
    }

    fn object_has_fx(&self, name: &str) -> bool {
        if self
            .objects
            .model_states(name)
            .iter()
            .any(|state| !state.particles.is_empty())
        {
            return true;
        }
        if self.objects.animation_states(name).iter().any(|state| {
            !state.fx_events.is_empty()
                || state
                    .entering_state_fx
                    .as_deref()
                    .is_some_and(|list| self.lists.get(list).is_some())
        }) {
            return true;
        }
        self.composed_fields(&OBJECT_KEYWORDS, name)
            .iter()
            .any(|field| self.resolves(&field.value) || self.weapon_has_fx(&field.value))
    }

    fn composed_fields(&self, keywords: &[&str], name: &str) -> Vec<NameField> {
        let lineage = if keywords.len() > 1 {
            self.objects.lineage(name)
        } else {
            vec![name.to_ascii_lowercase()]
        };
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        let mut out = Vec::new();
        for step in &lineage {
            let mut here = BTreeSet::new();
            let rows: Vec<&NameField> = keywords
                .iter()
                .flat_map(|keyword| self.names.get(keyword, step))
                .collect();
            for field in rows {
                let key = (
                    field.owner.to_ascii_lowercase(),
                    field.keyword.to_ascii_lowercase(),
                );
                if seen.contains(&key) {
                    continue;
                }
                here.insert(key);
                if field
                    .keyword
                    .eq_ignore_ascii_case(ENTERING_STATE_FX_KEYWORD)
                {
                    continue;
                }
                out.push(field.clone());
            }
            seen.extend(here);
        }
        out
    }

    fn walk_object(&mut self, name: &str, role: Role, parent: usize, depth: u32) -> Option<usize> {
        let template = self.objects.lookup(name);
        let node = self.push(Node {
            kind: NodeKind::Object,
            role,
            name: name.to_owned(),
            detail: template
                .model()
                .map_or_else(|| "no model".to_owned(), str::to_owned),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        });
        let below = depth.saturating_add(1);
        let model_states: Vec<ModelStateDef> = self.objects.model_states(name).to_vec();
        for state in &model_states {
            if state.particles.is_empty() {
                continue;
            }
            let row = self.push(Node {
                kind: NodeKind::ModelState,
                role: Role::ModelState,
                name: conditions_label(&state.conditions),
                detail: state.model.clone().unwrap_or_default(),
                system: None,
                children: Vec::new(),
                parent: Some(node),
                depth: below,
                conditions: state.conditions.clone(),
                fire_after: None,
            });
            self.attach(node, row);
            for bone in &state.particles {
                let Some(child) =
                    self.walk_system(&bone.system, Role::SysBone, row, below.saturating_add(1))
                else {
                    continue;
                };
                let detail = format!(
                    "bone {}{}",
                    bone.bone.as_deref().unwrap_or("(none)"),
                    if bone.follow_bone { ", FollowBone" } else { "" }
                );
                self.prefix_detail(child, &detail);
                self.attach(row, child);
            }
        }
        let animation_states: Vec<AnimationStateDef> = self.objects.animation_states(name).to_vec();
        for state in &animation_states {
            let entering = state
                .entering_state_fx
                .as_deref()
                .filter(|list| self.lists.get(list).is_some());
            if entering.is_none() && state.fx_events.is_empty() {
                continue;
            }
            let clips: Vec<&str> = state.clips.iter().map(|clip| clip.asset()).collect();
            let row = self.push(Node {
                kind: NodeKind::AnimState,
                role: Role::AnimState,
                name: conditions_label(&state.conditions),
                detail: clips.join(", "),
                system: None,
                children: Vec::new(),
                parent: Some(node),
                depth: below,
                conditions: state.conditions.clone(),
                fire_after: None,
            });
            self.attach(node, row);
            if let Some(list) = entering {
                if let Some(child) =
                    self.walk_list(list, Role::Entering, row, below.saturating_add(1))
                {
                    self.attach(row, child);
                }
            }
            for event in &state.fx_events {
                let Some(child) =
                    self.walk_list(&event.fx_list, Role::Event, row, below.saturating_add(1))
                else {
                    continue;
                };
                let detail = match event.frame_stop {
                    Some(stop) => format!("Frame:{} FrameStop:{stop}", event.frame),
                    None => format!("Frame:{}", event.frame),
                };
                self.prefix_detail(child, &detail);
                self.attach(row, child);
            }
        }
        self.walk_fields(&OBJECT_KEYWORDS, name, node, below);
        Some(node)
    }

    fn walk_weapon(&mut self, name: &str, role: Role, parent: usize, depth: u32) -> Option<usize> {
        let node = self.push(Node {
            kind: NodeKind::Weapon,
            role,
            name: name.to_owned(),
            detail: String::new(),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        });
        self.walk_fields(&[WEAPON_KEYWORD], name, node, depth.saturating_add(1));
        Some(node)
    }

    fn weapon_has_fx(&self, name: &str) -> bool {
        self.composed_fields(&[WEAPON_KEYWORD], name)
            .iter()
            .any(|field| self.resolves(&field.value))
    }

    fn walk_fields(&mut self, keywords: &[&str], name: &str, parent: usize, depth: u32) {
        let under_object = keywords != [WEAPON_KEYWORD];
        for field in self.composed_fields(keywords, name) {
            let child = if self.lists.get(&field.value).is_some() {
                self.walk_list(&field.value, Role::Field, parent, depth)
            } else if self.index.get(&field.value).is_some() {
                self.walk_system(&field.value, Role::Field, parent, depth)
            } else if under_object && self.weapon_has_fx(&field.value) {
                let weapon = self.walk_weapon(&field.value, Role::Field, parent, depth);
                if let Some(node) = weapon.and_then(|index| self.tree.nodes.get_mut(index)) {
                    let which = field
                        .sibling(WHICH_SPECIAL_WEAPON)
                        .and_then(|text| text.parse::<f32>().ok());
                    if let Some(pose) = mint_sage_modules::special_weapon_condition(which) {
                        node.conditions = vec![pose.to_string()];
                    }
                    node.fire_after = field
                        .sibling(UNPACK_TIME)
                        .and_then(|text| text.parse::<f32>().ok())
                        .map(|milliseconds| milliseconds / 1000.0);
                }
                weapon
            } else {
                continue;
            };
            if let Some(child) = child {
                let label = field.label();
                self.prefix_detail(child, &label);
                self.attach(parent, child);
            }
        }
    }

    fn prefix_detail(&mut self, index: usize, text: &str) {
        if let Some(node) = self.tree.nodes.get_mut(index) {
            if node.detail.is_empty() {
                node.detail = text.to_owned();
            } else {
                node.detail = format!("{text}  {}", node.detail);
            }
        }
    }

    fn category(&mut self, category: Category, names: &[String]) {
        let header = self.push(Node {
            kind: NodeKind::Category(category),
            role: Role::Root,
            name: format!("{} ({})", category.label(), names.len()),
            detail: String::new(),
            system: None,
            children: Vec::new(),
            parent: None,
            depth: 0,
            conditions: Vec::new(),
            fire_after: None,
        });
        self.tree.categories.push(header);
        for chunk in names.chunks(BUCKET_SIZE) {
            let label = bucket_label(chunk);
            let bucket = self.push(Node {
                kind: NodeKind::Bucket,
                role: Role::Root,
                name: label,
                detail: format!("{} entries", chunk.len()),
                system: None,
                children: Vec::new(),
                parent: Some(header),
                depth: 1,
                conditions: Vec::new(),
                fire_after: None,
            });
            self.attach(header, bucket);
            for name in chunk {
                let root = match category {
                    Category::FxLists => self.walk_list(name, Role::Root, bucket, 2),
                    Category::Systems => self.walk_system(name, Role::Root, bucket, 2),
                    Category::Objects => self.walk_object(name, Role::Root, bucket, 2),
                    Category::Weapons => self.walk_weapon(name, Role::Root, bucket, 2),
                };
                if let Some(root) = root {
                    self.attach(bucket, root);
                }
            }
        }
    }

    fn walk_list(&mut self, name: &str, role: Role, parent: usize, depth: u32) -> Option<usize> {
        let Some(list) = self.lists.get(name).cloned() else {
            return self.dangling(name, role, parent, depth, "FXList");
        };
        if let Some(stop) = self.enter(NodeKind::FxList, &list.name, role, parent, depth) {
            return Some(stop);
        }
        let node = self.push(Node {
            kind: NodeKind::FxList,
            role,
            name: list.name.clone(),
            detail: self.root_detail(role, &list.name, list_detail(&list)),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        });
        let nuggets = list.nuggets;
        let bones = list.particle_sys_bones;
        for nugget in &nuggets {
            if let Some(child) = self.walk_nugget(nugget, node, depth.saturating_add(1)) {
                self.attach(node, child);
            }
        }
        for bone in &bones {
            let leaf = self.push(Node {
                kind: NodeKind::SysBoneLeaf,
                role: Role::SysBone,
                name: bone.join(" "),
                detail: "not fired by the list".to_owned(),
                system: None,
                children: Vec::new(),
                parent: Some(node),
                depth: depth.saturating_add(1),
                conditions: Vec::new(),
                fire_after: None,
            });
            self.attach(node, leaf);
        }
        self.path.pop();
        Some(node)
    }

    fn walk_nugget(&mut self, nugget: &FxNugget, parent: usize, depth: u32) -> Option<usize> {
        if let Some(particle) = &nugget.particle {
            let node = self.push(Node {
                kind: NodeKind::ParticleNugget,
                role: Role::Nugget,
                name: nugget.keyword.clone(),
                detail: particle_nugget_detail(particle, nugget.name.as_deref()),
                system: None,
                children: Vec::new(),
                parent: Some(parent),
                depth,
                conditions: Vec::new(),
                fire_after: None,
            });
            if let Some(named) = nugget.name.as_deref() {
                if let Some(child) =
                    self.walk_system(named, Role::NuggetSystem, node, depth.saturating_add(1))
                {
                    self.attach(node, child);
                }
            }
            return Some(node);
        }
        if let Some(bone) = nugget.at_bone_pos.as_ref() {
            let node = self.push(Node {
                kind: NodeKind::BoneNugget,
                role: Role::Nugget,
                name: nugget.keyword.clone(),
                detail: format!(
                    "bone {}",
                    bone.bone_name.as_deref().unwrap_or("(none declared)")
                ),
                system: None,
                children: Vec::new(),
                parent: Some(parent),
                depth,
                conditions: Vec::new(),
                fire_after: None,
            });
            if let Some(nested) = bone.fx.as_deref() {
                if let Some(child) =
                    self.walk_list(nested, Role::BoneList, node, depth.saturating_add(1))
                {
                    self.attach(node, child);
                }
            }
            return Some(node);
        }
        Some(self.push(Node {
            kind: NodeKind::OtherNugget,
            role: Role::Nugget,
            name: nugget.keyword.clone(),
            detail: nugget.name.clone().unwrap_or_default(),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        }))
    }

    fn walk_system(&mut self, name: &str, role: Role, parent: usize, depth: u32) -> Option<usize> {
        let Some(definition) = self.index.get(name).cloned() else {
            return self.dangling(name, role, parent, depth, "FXParticleSystem");
        };
        if let Some(stop) = self.enter(NodeKind::System, &definition.name, role, parent, depth) {
            return Some(stop);
        }
        let node = self.push(Node {
            kind: NodeKind::System,
            role,
            name: definition.name.clone(),
            detail: self.root_detail(role, &definition.name, system_detail(&definition)),
            system: Some(definition.name.to_ascii_lowercase()),
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        });
        let below = depth.saturating_add(1);
        if let Some(slave) = definition.slave_system.as_deref() {
            if let Some(child) = self.walk_system(slave, Role::Slave, node, below) {
                self.attach(node, child);
            }
        }
        if let Some(attached) = definition.per_particle_system.as_deref() {
            if let Some(child) = self.walk_system(attached, Role::PerParticle, node, below) {
                self.attach(node, child);
            }
        }
        if let Some(event) = definition.event.as_ref() {
            if let Some(child) = self.walk_list(&event.fx, Role::Collision, node, below) {
                self.attach(node, child);
            }
        }
        self.path.pop();
        Some(node)
    }

    fn enter(
        &mut self,
        kind: NodeKind,
        name: &str,
        role: Role,
        parent: usize,
        depth: u32,
    ) -> Option<usize> {
        let key = (kind, name.to_ascii_lowercase());
        let repeated = self.path.contains(&key);
        if !repeated && depth <= MAX_TREE_DEPTH {
            self.path.push(key);
            return None;
        }
        let why = if repeated {
            "already above - cycle"
        } else {
            "depth cap"
        };
        Some(self.push(Node {
            kind: NodeKind::Stopped,
            role,
            name: name.to_owned(),
            detail: why.to_owned(),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        }))
    }

    fn dangling(
        &mut self,
        name: &str,
        role: Role,
        parent: usize,
        depth: u32,
        what: &str,
    ) -> Option<usize> {
        Some(self.push(Node {
            kind: NodeKind::Dangling,
            role,
            name: name.to_owned(),
            detail: format!("no {what} of this name"),
            system: None,
            children: Vec::new(),
            parent: Some(parent),
            depth,
            conditions: Vec::new(),
            fire_after: None,
        }))
    }

    fn push(&mut self, node: Node) -> usize {
        if self.tree.nodes.len() >= MAX_NODES {
            return self.tree.nodes.len().saturating_sub(1);
        }
        self.tree.nodes.push(node);
        self.tree.nodes.len().saturating_sub(1)
    }

    fn attach(&mut self, parent: usize, child: usize) {
        if parent == child {
            return;
        }
        if let Some(node) = self.tree.nodes.get_mut(parent) {
            node.children.push(child);
        }
    }
}

fn bucket_label(names: &[String]) -> String {
    let cut = |name: &String| -> String {
        let mut short: String = name.chars().take(18).collect();
        if name.chars().count() > 18 {
            short.push('~');
        }
        short
    };
    match (names.first(), names.last()) {
        (Some(first), Some(last)) if names.len() > 1 => format!("{} .. {}", cut(first), cut(last)),
        (Some(only), _) => cut(only),
        _ => "(empty)".to_owned(),
    }
}

fn list_detail(list: &FxList) -> String {
    let mut parts = vec![format!("{} nuggets", list.nuggets.len())];
    if !list.particle_sys_bones.is_empty() {
        parts.push(format!("{} ParticleSysBone", list.particle_sys_bones.len()));
    }
    if list.play_even_if_shrouded == Some(true) {
        parts.push("PlayEvenIfShrouded".to_owned());
    }
    parts.join(", ")
}

fn particle_nugget_detail(particle: &FxParticleNugget, name: Option<&str>) -> String {
    let mut parts = vec![format!("Name = {}", name.unwrap_or("(none declared)"))];
    let [x, y, z] = particle.offset;
    if x != 0.0 || y != 0.0 || z != 0.0 {
        parts.push(format!("Offset {x} {y} {z}"));
    }
    for (flag, word) in [
        (particle.orient_to_object, "OrientToObject"),
        (particle.attach_to_object, "AttachToObject"),
        (particle.create_at_ground_height, "CreateAtGroundHeight"),
        (particle.set_target_matrix, "SetTargetMatrix"),
    ] {
        if flag {
            parts.push(word.to_owned());
        }
    }
    if particle.initial_delay.high > 0.0 {
        parts.push(format!(
            "InitialDelay {}..{} frames",
            particle.initial_delay.low, particle.initial_delay.high
        ));
    }
    parts.join(", ")
}

fn system_detail(definition: &ParticleSystemDef) -> String {
    let mut parts = vec![definition.kind.as_str().to_owned()];
    if let Some(texture) = definition.particle_name.as_deref() {
        parts.push(texture.to_owned());
    }
    if definition.one_shot {
        parts.push("IsOneShot".to_owned());
    }
    if let Some(lifetime) = definition.system_lifetime {
        parts.push(format!("SystemLifetime {lifetime}"));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests;
