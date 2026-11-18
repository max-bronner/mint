use super::*;
use mint_sage_ini::BlockTable;
use mint_sim::module::CapabilityMask;
use mint_sim::{Component, StateHasher};

fn parse(text: &str) -> Document {
    Document::parse("t.ini", text, &BlockTable::object_model_path()).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Health(f32);
impl Component for Health {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}
impl mint_sim::ModuleKind for Health {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}
fn health_from_fields(
    fields: &dyn mint_sim::ModuleFieldSource,
    _now: Frame,
) -> Result<Health, mint_sim::FieldError> {
    fields
        .field_numeric("MaxHealth")
        .map(Health)
        .ok_or_else(|| mint_sim::FieldError::new("missing or unparseable"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Marker;
impl Component for Marker {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl mint_sim::ModuleKind for Marker {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);
}
fn marker_from_fields(
    _fields: &dyn mint_sim::ModuleFieldSource,
    _now: Frame,
) -> Result<Marker, mint_sim::FieldError> {
    Ok(Marker)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct OtherMarker;
impl Component for OtherMarker {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}
impl mint_sim::ModuleKind for OtherMarker {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);
}
fn other_marker_from_fields(
    _fields: &dyn mint_sim::ModuleFieldSource,
    _now: Frame,
) -> Result<OtherMarker, mint_sim::FieldError> {
    Ok(OtherMarker)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ImmortalToy(f32);
impl Component for ImmortalToy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}
impl mint_sim::ModuleKind for ImmortalToy {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}
fn immortal_toy_from_fields(
    fields: &dyn mint_sim::ModuleFieldSource,
    _now: Frame,
) -> Result<ImmortalToy, mint_sim::FieldError> {
    let raw = fields
        .field("MaxHealth")
        .ok_or_else(|| mint_sim::FieldError::new("missing"))?;
    raw.parse()
        .map(ImmortalToy)
        .map_err(|_| mint_sim::FieldError::new("bad"))
}

#[test]
fn resolves_a_flat_objects_own_declarations_in_order() {
    let doc = parse(
        "Object A\n  \
                Body = Health Tag_01\n    MaxHealth = 10\n  End\n  \
                Behavior = Unregistered Tag_02\n  End\n\
            End\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let decls = index.resolve("a", &registry);
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "Health");
    assert_eq!(decls[1].name, "Unregistered");
}

#[test]
fn a_child_inherits_the_parents_modules_root_first() {
    let doc = parse(
        "Object Parent\n  Body = Health Tag_01\n    MaxHealth = 10\n  End\nEnd\n\
             ChildObject Child Parent\n  Behavior = Marker Tag_02\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let decls = index.resolve("child", &registry);
    assert_eq!(
        decls.len(),
        2,
        "parent's module first, then the child's own"
    );
    assert_eq!(decls[0].name, "Health");
    assert_eq!(decls[1].name, "Marker");
}

#[test]
fn a_child_redeclaring_the_same_name_overrides_it_rather_than_duplicating() {
    let doc = parse(
        "Object Parent\n  Body = Health Tag_01\n    MaxHealth = 10\n  End\nEnd\n\
             ChildObject Child Parent\n  Body = Health Tag_01\n    MaxHealth = 99\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Health", health_from_fields);
    let decls = index.resolve("child", &registry);
    assert_eq!(
        decls.len(),
        1,
        "the parent's Health must not survive alongside the child's"
    );
    assert_eq!(
        decls[0].block.field("MaxHealth").and_then(|f| f.value()),
        Some("99"),
        "the surviving declaration is the child's own"
    );
}

#[test]
fn a_different_body_module_evicts_the_inherited_one_by_capability() {
    let doc = parse(
            "Object Parent\n  Body = Health Tag_01\n    MaxHealth = 10\n  End\nEnd\n\
             ChildObject Child Parent\n  Body = Immortal Tag_01\n    MaxHealth = 99999\n  End\nEnd\n",
        );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Health", health_from_fields);
    registry.register_loadable(&mut world, "Immortal", immortal_toy_from_fields);
    let decls = index.resolve("child", &registry);
    assert_eq!(
        decls.len(),
        1,
        "declaring a second Body-capable module must evict the first, regardless of name"
    );
    assert_eq!(decls[0].name, "Immortal");
}

#[test]
fn co_existing_update_modules_are_not_evicted_by_a_different_named_sibling() {
    let doc = parse(
        "Object Parent\n  Behavior = Marker Tag_01\n  End\nEnd\n\
             ChildObject Child Parent\n  Behavior = OtherMarker Tag_02\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Marker", marker_from_fields);
    registry.register_loadable(&mut world, "OtherMarker", other_marker_from_fields);
    let decls = index.resolve("child", &registry);
    assert_eq!(
        decls.len(),
        2,
        "two distinct Update-capable modules must both survive"
    );
}

#[test]
fn inheritance_depth_is_capped_rather_than_looping_forever_on_a_cycle() {
    let doc = parse("ChildObject A A\n  Behavior = Marker Tag_01\nEnd\n");
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let decls = index.resolve("a", &registry);
    assert_eq!(
        decls.len(),
        1,
        "the cycle collapses to one surviving declaration"
    );
}

#[test]
fn spawn_template_attaches_every_resolved_module_and_hashes_identically_twice() {
    let doc = parse(
        "Object A\n  \
                Body = Health Tag_01\n    MaxHealth = 42\n  End\n  \
                Behavior = Marker Tag_02\n  End\n\
            End\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);

    let mut world_a = World::new();
    let mut registry_a = ModuleRegistry::new(&mut world_a);
    registry_a.register_loadable(&mut world_a, "Health", health_from_fields);
    registry_a.register_loadable(&mut world_a, "Marker", marker_from_fields);
    let entity_a = spawn_template(
        &mut world_a,
        &registry_a,
        Frame::ZERO,
        &index,
        &UpgradeBits::default(),
        "a",
    )
    .unwrap();

    let mut world_b = World::new();
    let mut registry_b = ModuleRegistry::new(&mut world_b);
    registry_b.register_loadable(&mut world_b, "Health", health_from_fields);
    registry_b.register_loadable(&mut world_b, "Marker", marker_from_fields);
    let entity_b = spawn_template(
        &mut world_b,
        &registry_b,
        Frame::ZERO,
        &index,
        &UpgradeBits::default(),
        "a",
    )
    .unwrap();

    assert!(world_a.is_live(entity_a));
    assert!(world_b.is_live(entity_b));
    assert!(
        registry_a.tally().is_empty(),
        "both modules are registered, neither should stub"
    );
    assert_eq!(
        world_a.state_hash(),
        world_b.state_hash(),
        "the same template instantiated twice must produce an identical state hash"
    );
}

#[test]
fn spawn_template_resolves_a_max_health_naming_a_macro_from_another_document() {
    let gamedata = parse("#define ELVEN_MALLORN_TREE_HEALTH 2000\n");
    let object = parse(
            "Object MallornTree\n  Body = Health Tag_01\n    MaxHealth = ELVEN_MALLORN_TREE_HEALTH\n  End\nEnd\n",
        );
    let mut index = TemplateIndex::new();
    index.insert_document(&gamedata);
    index.insert_document(&object);
    assert_eq!(
        index.macros().get("ELVEN_MALLORN_TREE_HEALTH"),
        Some(2000.0)
    );

    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Health", health_from_fields);
    let entity = spawn_template(
        &mut world,
        &registry,
        Frame::ZERO,
        &index,
        &UpgradeBits::default(),
        "mallorntree",
    )
    .unwrap();

    assert!(
        registry.tally().is_empty(),
        "the macro must resolve rather than falling back to a stub"
    );
    let id = world.register::<mint_sim::module::ModuleColumn<Health>>();
    let column = world.store(id).unwrap();
    assert_eq!(column.get(entity).unwrap().0[0].state, Health(2000.0));
}

#[test]
fn the_lineage_is_root_first_and_empty_for_an_unknown_name() {
    let doc = parse(
        "Object Root\nEnd\n\
             ChildObject Middle Root\nEnd\n\
             ObjectReskin Leaf Middle\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let names: Vec<&str> = index
        .lineage("LEAF")
        .iter()
        .filter_map(|block| block.name())
        .collect();
    assert_eq!(names, ["Root", "Middle", "Leaf"]);
    assert!(index.lineage("nobody").is_empty());
}

#[test]
fn declared_modules_evicts_an_inherited_body_by_keyword_and_keeps_siblings() {
    let doc = parse(
        "Object Parent\n  \
                Body = ActiveBody Tag_01\n    MaxHealth = 10\n  End\n  \
                Behavior = Marker Tag_02\n  End\n\
             End\n\
             ChildObject Child Parent\n  \
                Body = StructureBody Tag_03\n    MaxHealth = 99\n  End\n  \
                Behavior = OtherMarker Tag_04\n  End\n\
             End\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let names: Vec<&str> = index
        .declared_modules("child")
        .iter()
        .map(|decl| decl.name)
        .collect();
    assert_eq!(names, ["Marker", "StructureBody", "OtherMarker"]);
}

#[test]
fn an_unregistered_declared_module_still_spawns_as_a_loud_stub() {
    let doc = parse("Object A\n  Behavior = SomeFutureModule Tag_01\n  End\nEnd\n");
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let registry = ModuleRegistry::new(&mut world);
    let entity = spawn_template(
        &mut world,
        &registry,
        Frame::ZERO,
        &index,
        &UpgradeBits::default(),
        "a",
    )
    .unwrap();
    assert!(world.is_live(entity));
    assert_eq!(registry.tally().len(), 1);
    assert_eq!(registry.tally()[0].name, "SomeFutureModule");
}

#[test]
fn a_reused_tag_replaces_even_when_the_registered_type_differs() {
    let doc = parse(
        "Object Parent\n  Behavior = Marker Tag_01\n  End\nEnd\n\
             ChildObject Child Parent\n  Behavior = OtherMarker Tag_01\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let names: Vec<&str> = index
        .declared_modules("child")
        .iter()
        .map(|decl| decl.name)
        .collect();
    assert_eq!(
        names,
        ["OtherMarker"],
        "the reused tag replaces Parent's Marker"
    );
}

#[test]
fn a_new_tag_adds_alongside_the_parents_even_when_the_registered_type_matches() {
    let doc = parse(
        "Object Parent\n  Behavior = Marker Tag_01\n  End\nEnd\n\
             ChildObject Child Parent\n  Behavior = Marker Tag_02\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let decls = index.declared_modules("child");
    assert_eq!(
        decls.len(),
        2,
        "a new tag adds rather than replacing, even under the same type name"
    );
}

#[test]
fn default_thing_template_is_copied_onto_every_object_but_itself() {
    let doc = parse(
        "Object DefaultThingTemplate\n  \
                Body = Health Tag_DefaultBody\n    MaxHealth = 1\n  End\n  \
                InheritableModule\n    Behavior = Marker Tag_Inherit\n    End\n  End\n\
             End\n\
             Object A\n  Body = Health Tag_02\n    MaxHealth = 50\n  End\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Health", health_from_fields);
    registry.register_loadable(&mut world, "Marker", marker_from_fields);

    let decls = index.resolve("a", &registry);
    assert_eq!(
        decls.len(),
        2,
        "the inheritable module plus A's own body; the default's own plain Body is evicted"
    );
    assert_eq!(
        decls[0].name, "Marker",
        "inherited module first, root's own declaration last"
    );
    assert_eq!(decls[1].name, "Health");
    assert_eq!(
        decls[1].block.field("MaxHealth").and_then(|f| f.value()),
        Some("50"),
        "A's own Body survives, not the default's"
    );
}

#[test]
fn default_thing_templates_plain_entry_survives_a_template_that_declares_no_matching_module() {
    let doc = parse(
        "Object DefaultThingTemplate\n  \
                Body = Health Tag_DefaultBody\n    MaxHealth = 1\n  End\n\
             End\n\
             Object Prop\nEnd\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register_loadable(&mut world, "Health", health_from_fields);

    let decls = index.resolve("prop", &registry);
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name, "Health");
    assert_eq!(
        decls[0].block.field("MaxHealth").and_then(|f| f.value()),
        Some("1")
    );
}

#[test]
fn remove_module_and_replace_module_evict_by_tag_and_add_module_declares_normally() {
    let doc = parse(
        "Object Parent\n  \
                Behavior = Marker Tag_01\n  End\n  \
                Behavior = OtherMarker Tag_02\n  End\n\
             End\n\
             ChildObject Child Parent\n  \
                RemoveModule = Tag_02\n  \
                ReplaceModule Tag_01\n    Behavior = OtherMarker Tag_03\n    End\n  End\n  \
                AddModule\n    Behavior = Marker Tag_04\n    End\n  End\n\
             End\n",
    );
    let mut index = TemplateIndex::new();
    index.insert_document(&doc);
    let names: Vec<&str> = index
        .declared_modules("child")
        .iter()
        .map(|decl| decl.name)
        .collect();
    assert_eq!(
        names,
        ["OtherMarker", "Marker"],
        "Tag_02 removed, Tag_01 replaced in place by ReplaceModule's nested declaration, \
             AddModule's declaration appended normally"
    );
}

#[test]
fn a_template_derived_from_an_overridden_one_does_not_inherit_the_override() {
    let global = parse("Object Base\n  BuildTime = 20\nEnd\nChildObject Derived Base\nEnd\n");
    let map = parse("Object Base\n  BuildTime = 5\nEnd\n");
    let mut index = TemplateIndex::new();
    index.insert_document(&global);
    let load = index.insert_override_document(&map);
    assert_eq!((load.errors.len(), index.lineage("Derived").len()), (0, 2));
}
