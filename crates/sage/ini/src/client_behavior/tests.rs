use crate::object::ObjectIndex;
use crate::table::BlockTable;
use crate::tree::Document;

fn index(text: &str) -> ObjectIndex {
    let document = Document::parse("t.ini", text, &BlockTable::object_model_path()).unwrap();
    let mut index = ObjectIndex::new();
    index.insert_document(&document);
    index
}

#[test]
fn a_shipped_shape_declaration_reads_its_fields() {
    let index = index(
        "Object GUArcher\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 MaxUpdateRangeCap = 300\n\
         \x20\x20 AnimationSound = Sound:BodyFallGeneric1 Animation:GUArcher_SKL.GUArcher_DIEB Frames:19\n\
         \x20 End\n\
         End\n",
    );
    let behavior = index.animation_sound_behavior("GUArcher").unwrap();
    assert_eq!(behavior.max_update_range_cap, Some(300));
    assert_eq!(behavior.sounds.len(), 1);
    let entry = &behavior.sounds[0];
    assert_eq!(entry.sound, "BodyFallGeneric1");
    assert_eq!(entry.animations, ["GUArcher_SKL.GUArcher_DIEB"]);
    assert_eq!(entry.frames, [19]);
}

#[test]
fn a_space_after_the_colon_reads_the_same_as_none() {
    let index = index(
        "Object T\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Sound: Footstep Animation: T_SKL.T_WALK Frames: 3 8\n\
         \x20 End\n\
         End\n",
    );
    let entry = &index.animation_sound_behavior("T").unwrap().sounds[0];
    assert_eq!(entry.sound, "Footstep");
    assert_eq!(entry.frames, [3, 8]);
}

#[test]
fn several_frames_occurrences_flatten_into_one_list() {
    let index = index(
        "Object T\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Sound:S Animation:T.A Frames:1 Frames:2\n\
         \x20 End\n\
         End\n",
    );
    let entry = &index.animation_sound_behavior("T").unwrap().sounds[0];
    assert_eq!(entry.frames, [1, 2]);
}

#[test]
fn a_line_with_no_sound_is_reported_not_dropped() {
    let index = index(
        "Object T\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Animation:T.A Frames:1\n\
         \x20 End\n\
         End\n",
    );
    assert!(index
        .animation_sound_behavior("T")
        .unwrap()
        .sounds
        .is_empty());
    assert_eq!(index.animation_sound_failures().len(), 1);
}

#[test]
fn a_child_object_with_no_behavior_of_its_own_inherits_the_parents() {
    let index = index(
        "Object Parent\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Sound:S Animation:P.A Frames:1\n\
         \x20 End\n\
         End\n\
         ChildObject Child Parent\n\
         End\n",
    );
    let behavior = index.animation_sound_behavior("Child").unwrap();
    assert_eq!(behavior.sounds[0].sound, "S");
}

#[test]
fn a_child_objects_own_block_replaces_the_parents_whole() {
    let index = index(
        "Object Parent\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Sound:ParentSound Animation:P.A Frames:1\n\
         \x20 End\n\
         End\n\
         ChildObject Child Parent\n\
         \x20 ClientBehavior = AnimationSoundClientBehavior\n\
         \x20\x20 AnimationSound = Sound:ChildSound Animation:C.A Frames:1\n\
         \x20 End\n\
         End\n",
    );
    let behavior = index.animation_sound_behavior("Child").unwrap();
    assert_eq!(behavior.sounds.len(), 1);
    assert_eq!(behavior.sounds[0].sound, "ChildSound");
}
