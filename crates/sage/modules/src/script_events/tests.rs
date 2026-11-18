use super::*;

fn file(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n\
         <SageLuaScriptSection xmlns=\"http://example.invalid/x.xsd\">\n\
         \x20   <Events>\n\
         \x20       <InternalEvent Name=\"OnMade\" /> <!-- a comment -->\n\
         \x20       <InternalEvent Name=\"OnPoked\" />\n\
         \x20       <ModelConditionEvent Name=\"Hurt\">\n\
         \x20           <Conditions>+HURT -GONE</Conditions>\n\
         \x20       </ModelConditionEvent>\n\
         \x20   </Events>\n\
         {body}\n\
         </SageLuaScriptSection>\n"
    )
}

const LISTS: &str = "    <EventList Name=\"Base\">\n\
                     \x20       <EventHandler EventName=\"OnMade\" ScriptFunctionName=\"Made\" \
                     DebugSingleStep=\"false\"/>\n\
                     \x20   </EventList>\n\
                     \x20   <EventList Name=\"Leaf\" Inherit=\"Base\">\n\
                     \x20       <EventHandler EventName=\"OnPoked\" ScriptFunctionName=\"Poked\"/>\n\
                     \x20   </EventList>";

#[test]
fn a_list_answers_with_its_parents_handler_for_an_event_of_its_own_it_lacks() {
    let events = ScriptEvents::parse(&file(LISTS)).expect("the subset parses");
    assert_eq!(events.handler("Leaf", "OnPoked"), Some("Poked"));
    assert_eq!(events.handler("Leaf", "OnMade"), Some("Made"));
    assert_eq!(events.handler("Base", "OnPoked"), None);
}

#[test]
fn an_element_outside_the_subset_is_refused_by_name() {
    let source = file("    <ObjectStatusEvent Name=\"Held\" />");
    let error = ScriptEvents::parse(&source).expect_err("the subset has no such element");
    assert!(
        error.message.contains("ObjectStatusEvent"),
        "{error} should name the element"
    );
}

#[test]
fn a_handler_bound_to_an_event_the_file_never_declares_is_listed_and_binds_nothing() {
    let source = file(
        "    <EventList Name=\"Base\">\n\
         \x20       <EventHandler EventName=\"onMade\" ScriptFunctionName=\"Made\"/>\n\
         \x20   </EventList>",
    );
    let events = ScriptEvents::parse(&source).expect("retail ships one of these and loads");
    assert_eq!(events.unknown_events(), vec![("Base", "onMade")]);
    assert_eq!(events.handler("Base", "OnMade"), None);
}

#[test]
fn every_truncation_errors_rather_than_panicking() {
    let source = file(LISTS);
    for end in 1..source.trim_end().len() {
        let Some(prefix) = source.get(..end) else {
            continue;
        };
        assert!(
            ScriptEvents::parse(prefix).is_err(),
            "a prefix of {end} bytes should not parse"
        );
    }
}

#[test]
fn every_single_byte_corruption_errors_or_parses_but_never_panics() {
    let source = file(LISTS);
    for at in 0..source.len() {
        let mut bytes = source.clone().into_bytes();
        if let Some(byte) = bytes.get_mut(at) {
            *byte ^= 0x20;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let _ = ScriptEvents::parse(&text);
    }
}

#[test]
fn nesting_elements_inside_themselves_is_refused_rather_than_stacking_up() {
    let source = format!(
        "{}{}",
        "<EventList Name=\"A\">".repeat(4096),
        "</EventList>"
    );
    assert!(ScriptEvents::parse(&source).is_err());
}

#[test]
fn a_file_larger_than_the_cap_is_refused_before_it_is_read() {
    let source = "x".repeat(MAX_SOURCE_BYTES.saturating_add(1));
    let error = ScriptEvents::parse(&source).expect_err("past the cap");
    assert!(error.message.contains("larger than"), "{error}");
}
