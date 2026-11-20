use super::*;

const EVENTS: &str = "<SageLuaScriptSection>\n\
                      <Events><InternalEvent Name=\"OnPoked\" /></Events>\n\
                      <EventList Name=\"Flagpole\">\n\
                      <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                      </EventList>\n\
                      </SageLuaScriptSection>";

const SCRIPT: &str = "function Poked(self, data)\n\
                      \x20 local who = ObjectCapturingObjectPlayerSide(self)\n\
                      \x20 if who == nil then\n\
                      \x20   who = ObjectPlayerSide(self)\n\
                      \x20 end\n\
                      \x20 ObjectHideSubObjectPermanently(self, \"PART_ONE\", true)\n\
                      \x20 ObjectHideSubObjectPermanently(self, \"PART_TWO\", true)\n\
                      \x20 if who == \"One\" then\n\
                      \x20   ObjectHideSubObjectPermanently(self, \"PART_ONE\", false)\n\
                      \x20 elseif who == \"Two\" then\n\
                      \x20   ObjectHideSubObjectPermanently(self, \"PART_TWO\", false)\n\
                      \x20 else\n\
                      \x20   ObjectHideSubObjectPermanently(self, \"PART_NONE\", false)\n\
                      \x20 end\n\
                      end\n";

fn hide(name: &str) -> Effect {
    Effect::SubObjects {
        show: [].into(),
        hide: [Name::from(name)].into(),
        permanent: true,
    }
}

fn show(name: &str) -> Effect {
    Effect::SubObjects {
        show: [Name::from(name)].into(),
        hide: [].into(),
        permanent: true,
    }
}

fn run(facts: &ObjectFacts<'_>) -> HandlerRun {
    ObjectScripts::load(EVENTS, SCRIPT)
        .expect("both files parse")
        .run(
            "Flagpole",
            "OnPoked",
            None,
            facts,
            EventSource::default(),
            None,
        )
        .expect("the list binds the event")
}

#[test]
fn the_capturing_side_is_the_one_part_left_shown() {
    let run = run(&ObjectFacts {
        side: Some("Two"),
        capturing_side: Some("One"),
        conditions: &[],
    });
    assert_eq!(
        run.effects,
        vec![hide("PART_ONE"), hide("PART_TWO"), show("PART_ONE")]
    );
}

#[test]
fn a_side_no_branch_names_falls_through_to_the_last_one() {
    let run = run(&ObjectFacts {
        side: Some("Three"),
        capturing_side: None,
        conditions: &[],
    });
    assert_eq!(
        run.effects,
        vec![hide("PART_ONE"), hide("PART_TWO"), show("PART_NONE")]
    );
}

#[test]
fn a_call_the_binding_does_not_answer_is_refused_by_name_and_reported() {
    let script = "function Poked(self, data)\n\
                  \x20 ObjectHideSubObjectPermanently(self, \"PART_ONE\", true)\n\
                  \x20 ObjectDoSpecialPower(self, \"Anything\")\n\
                  end\n";
    let scripts = ObjectScripts::load(EVENTS, script).expect("both files parse");
    let run = scripts
        .run(
            "Flagpole",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .expect("the list binds the event");
    assert_eq!(run.refused, vec![String::from("ObjectDoSpecialPower")]);
    assert!(run.error.is_some(), "the refusal stops the handler");
    assert_eq!(run.effects, vec![hide("PART_ONE")]);
    assert_eq!(
        scripts.unbound_calls(),
        vec![(String::from("ObjectDoSpecialPower"), 1)]
    );
}

#[test]
fn an_event_the_list_does_not_bind_runs_nothing() {
    let scripts = ObjectScripts::load(EVENTS, SCRIPT).expect("both files parse");
    assert!(scripts
        .run(
            "Flagpole",
            "OnMade",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .is_none());
}

#[test]
fn a_handler_declaring_only_self_runs_for_an_event_that_supplies_data() {
    let script = "function Poked(self)\n\
                  \x20 ObjectHideSubObjectPermanently(self, \"PART_ONE\", true)\n\
                  end\n";
    let scripts = ObjectScripts::load(EVENTS, script).expect("both files parse");
    let run = scripts
        .run(
            "Flagpole",
            "OnPoked",
            Some("anything"),
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .expect("the list binds the event");
    assert_eq!(run.error, None);
    assert_eq!(run.effects, vec![hide("PART_ONE")]);
}

#[test]
fn test_model_condition_reads_the_objects_own_held_set() {
    let events = "<SageLuaScriptSection>\n\
                  <Events><InternalEvent Name=\"OnPoked\" /></Events>\n\
                  <EventList Name=\"L\">\n\
                  <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                  </EventList>\n\
                  </SageLuaScriptSection>";
    let script = "function Poked(self)\n\
                  \x20 if ObjectTestModelCondition(self, \"AFLAME\") then\n\
                  \x20   ObjectEnterRampageState(self)\n\
                  \x20 end\n\
                  \x20 if ObjectTestModelCondition(self, \"UNKNOWN_NAME\") then\n\
                  \x20   ObjectPlaySound(self, \"never\")\n\
                  \x20 end\n\
                  end\n";
    let scripts = ObjectScripts::load(events, script).expect("both files parse");
    let held = [String::from("aflame")];
    let facts = ObjectFacts {
        conditions: &held,
        ..ObjectFacts::default()
    };
    let run = scripts
        .run("L", "OnPoked", None, &facts, EventSource::default(), None)
        .expect("the list binds the event");
    assert_eq!(run.effects, vec![Effect::EnterRampage]);
}

#[test]
fn play_sound_queues_the_named_event() {
    let events = "<SageLuaScriptSection>\n\
                  <Events><InternalEvent Name=\"OnPoked\" /></Events>\n\
                  <EventList Name=\"L\">\n\
                  <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                  </EventList>\n\
                  </SageLuaScriptSection>";
    let script = "function Poked(self)\n ObjectPlaySound(self, \"RecoverFromTerror\")\n end\n";
    let scripts = ObjectScripts::load(events, script).expect("both files parse");
    let run = scripts
        .run(
            "L",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .expect("the list binds the event");
    assert_eq!(
        run.effects,
        vec![Effect::PlayObjectSound(Name::from("RecoverFromTerror"))]
    );
}

#[test]
fn the_plain_hide_is_a_plain_write() {
    let scripts = ObjectScripts::load(
        EVENTS,
        "function Poked(self)\n ObjectHideSubObject(self, \"PART\", true)\nend\n",
    )
    .unwrap();
    let run = scripts
        .run(
            "Flagpole",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .unwrap();
    assert_eq!(run.error, None);
    assert_eq!(
        run.effects,
        [Effect::SubObjects {
            show: [].into(),
            hide: [Name::from("PART")].into(),
            permanent: false,
        }]
    );
}

#[test]
fn show_sub_object_permanently_is_refused_not_answered() {
    let events = "<SageLuaScriptSection>\n\
                  <Events><InternalEvent Name=\"OnPoked\" /></Events>\n\
                  <EventList Name=\"L\">\n\
                  <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                  </EventList>\n\
                  </SageLuaScriptSection>";
    let script = "function Poked(self)\n ObjectShowSubObjectPermanently(self, \"X\", true)\n end\n";
    let scripts = ObjectScripts::load(events, script).expect("both files parse");
    let run = scripts
        .run(
            "L",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .expect("the list binds the event");
    assert_eq!(
        run.refused,
        vec![String::from("ObjectShowSubObjectPermanently")]
    );
}

#[test]
fn a_broadcast_call_reaches_the_broadcaster_with_its_arguments() {
    struct Recording(Vec<(BroadcastKind, String, f32)>);
    impl Broadcaster for Recording {
        fn broadcast(&mut self, kind: BroadcastKind, event: &str, radius: f32) {
            self.0.push((kind, event.to_owned(), radius));
        }
    }
    let events = "<SageLuaScriptSection>\n\
                  <Events><InternalEvent Name=\"OnPoked\" /></Events>\n\
                  <EventList Name=\"L\">\n\
                  <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                  </EventList>\n\
                  </SageLuaScriptSection>";
    let script =
        "function Poked(self)\n ObjectBroadcastEventToEnemies(self, \"BeScary\", 150)\n end\n";
    let scripts = ObjectScripts::load(events, script).expect("both files parse");
    let mut recording = Recording(Vec::new());
    let run = scripts
        .run(
            "L",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            Some(&mut recording),
        )
        .expect("the list binds the event");
    assert_eq!(run.error, None);
    assert_eq!(run.refused, Vec::<String>::new());
    assert_eq!(
        recording.0,
        vec![(BroadcastKind::Enemies, String::from("BeScary"), 150.0)]
    );

    let run = scripts
        .run(
            "L",
            "OnPoked",
            None,
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )
        .expect("the list binds the event");
    assert_eq!(
        run.refused,
        vec![String::from("ObjectBroadcastEventToEnemies")]
    );
}
