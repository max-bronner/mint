use std::fmt;

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_ELEMENTS: usize = 16 * 1024;
const MAX_INHERIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEventsError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for ScriptEventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ScriptEventsError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventList {
    pub name: String,
    pub inherit: Option<String>,
    pub handlers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptEvents {
    events: Vec<String>,
    lists: Vec<EventList>,
}

impl ScriptEvents {
    pub fn parse(source: &str) -> Result<Self, ScriptEventsError> {
        let mut events = Self::default();
        Reader::new(source)?.read(&mut events)?;
        events.check_lists()?;
        Ok(events)
    }

    #[must_use]
    pub fn handler(&self, list: &str, event: &str) -> Option<&str> {
        let mut at = self.list(list);
        for _ in 0..MAX_INHERIT {
            let current = at?;
            if let Some((_, function)) = current.handlers.iter().find(|(name, _)| name == event) {
                return Some(function);
            }
            at = current.inherit.as_deref().and_then(|name| self.list(name));
        }
        None
    }

    pub fn lists(&self) -> impl Iterator<Item = &EventList> {
        self.lists.iter()
    }

    #[must_use]
    pub fn unknown_events(&self) -> Vec<(&str, &str)> {
        self.lists
            .iter()
            .flat_map(|list| list.handlers.iter().map(move |(event, _)| (list, event)))
            .filter(|(_, event)| !self.events.iter().any(|known| known == *event))
            .map(|(list, event)| (list.name.as_str(), event.as_str()))
            .collect()
    }

    fn list(&self, name: &str) -> Option<&EventList> {
        self.lists.iter().find(|list| list.name == name)
    }

    fn check_lists(&self) -> Result<(), ScriptEventsError> {
        for list in &self.lists {
            let mut at = list.inherit.as_deref();
            for _ in 0..MAX_INHERIT {
                let Some(name) = at else { break };
                let Some(parent) = self.list(name) else {
                    return Err(ScriptEventsError {
                        line: 0,
                        message: format!(
                            "EventList \"{}\" inherits unknown list \"{name}\"",
                            list.name
                        ),
                    });
                };
                at = parent.inherit.as_deref();
            }
            if at.is_some() {
                return Err(ScriptEventsError {
                    line: 0,
                    message: format!("EventList \"{}\" inherits in a cycle", list.name),
                });
            }
        }
        Ok(())
    }
}

struct Reader<'a> {
    rest: &'a str,
    line: u32,
    open: Vec<&'a str>,
    elements: usize,
    root: bool,
}

impl<'a> Reader<'a> {
    fn new(source: &'a str) -> Result<Self, ScriptEventsError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(ScriptEventsError {
                line: 1,
                message: format!("file is larger than {MAX_SOURCE_BYTES} bytes"),
            });
        }
        Ok(Self {
            rest: source,
            line: 1,
            open: Vec::new(),
            elements: 0,
            root: false,
        })
    }

    fn error(&self, message: impl ToString) -> ScriptEventsError {
        ScriptEventsError {
            line: self.line,
            message: message.to_string(),
        }
    }

    fn advance(&mut self, bytes: usize) -> Result<&'a str, ScriptEventsError> {
        let (taken, rest) = match (self.rest.get(..bytes), self.rest.get(bytes..)) {
            (Some(taken), Some(rest)) => (taken, rest),
            _ => return Err(self.error("input ends inside a token")),
        };
        self.line = self
            .line
            .saturating_add(u32::try_from(taken.matches('\n').count()).unwrap_or(u32::MAX));
        self.rest = rest;
        Ok(taken)
    }

    fn until(&mut self, end: &str) -> Result<&'a str, ScriptEventsError> {
        let at = self
            .rest
            .find(end)
            .ok_or_else(|| self.error(format!("no closing \"{end}\"")))?;
        let body = self.advance(at)?;
        self.advance(end.len())?;
        Ok(body)
    }

    fn read(&mut self, into: &mut ScriptEvents) -> Result<(), ScriptEventsError> {
        while !self.rest.is_empty() {
            if self.rest.starts_with("<!--") {
                self.advance(4)?;
                self.until("-->")?;
            } else if self.rest.starts_with("<?") {
                if !self.open.is_empty() {
                    return Err(self.error("processing instruction inside an element"));
                }
                self.advance(2)?;
                self.until("?>")?;
            } else if self.rest.starts_with("<!") {
                return Err(self.error("a DOCTYPE or CDATA section"));
            } else if self.rest.starts_with("</") {
                self.advance(2)?;
                let name = self.until(">")?.trim();
                match self.open.pop() {
                    Some(open) if open == name => {}
                    Some(open) => {
                        return Err(self.error(format!("</{name}> closes <{open}>")));
                    }
                    None => return Err(self.error(format!("</{name}> closes nothing"))),
                }
            } else if self.rest.starts_with('<') {
                self.element(into)?;
            } else {
                self.text()?;
            }
        }
        if let Some(open) = self.open.last() {
            return Err(self.error(format!("<{open}> is never closed")));
        }
        if !self.root {
            return Err(self.error("no <SageLuaScriptSection> element"));
        }
        Ok(())
    }

    fn text(&mut self) -> Result<(), ScriptEventsError> {
        let at = self.rest.find('<').unwrap_or(self.rest.len());
        let text = self.advance(at)?;
        if !text.trim().is_empty() && self.open.last() != Some(&"Conditions") {
            return Err(self.error("text content outside <Conditions>"));
        }
        Ok(())
    }

    fn element(&mut self, into: &mut ScriptEvents) -> Result<(), ScriptEventsError> {
        self.advance(1)?;
        let header = self.until(">")?;
        self.elements = self.elements.saturating_add(1);
        if self.elements > MAX_ELEMENTS {
            return Err(self.error(format!("more than {MAX_ELEMENTS} elements")));
        }
        let (header, empty) = match header.strip_suffix('/') {
            Some(header) => (header, true),
            None => (header, false),
        };
        let mut parts = header.trim_start().splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default();
        if name.is_empty() {
            return Err(self.error("an element with no name"));
        }
        let attributes = self.attributes(parts.next().unwrap_or_default())?;
        self.open_element(name, &attributes, into)?;
        if !empty {
            self.open.push(name);
        }
        Ok(())
    }

    fn attributes(&self, mut rest: &'a str) -> Result<Vec<(&'a str, &'a str)>, ScriptEventsError> {
        let mut out = Vec::new();
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                return Ok(out);
            }
            let at = rest
                .find('=')
                .ok_or_else(|| self.error("an attribute with no value"))?;
            let (name, after) = match (rest.get(..at), rest.get(at.saturating_add(1)..)) {
                (Some(name), Some(after)) => (name.trim(), after.trim_start()),
                _ => return Err(self.error("an attribute with no value")),
            };
            let value = match after.strip_prefix('"') {
                Some(after) => after,
                None if after.starts_with('\'') => {
                    return Err(self.error("a single-quoted attribute value"));
                }
                None => return Err(self.error("an unquoted attribute value")),
            };
            let end = value
                .find('"')
                .ok_or_else(|| self.error("an unterminated attribute value"))?;
            let (value, after) = match (value.get(..end), value.get(end.saturating_add(1)..)) {
                (Some(value), Some(after)) => (value, after),
                _ => return Err(self.error("an unterminated attribute value")),
            };
            if value.contains('&') || name.contains('&') {
                return Err(self.error("an entity reference"));
            }
            if out.iter().any(|(held, _)| *held == name) {
                return Err(self.error(format!("attribute \"{name}\" twice")));
            }
            out.push((name, value));
            rest = after;
        }
    }

    fn open_element(
        &mut self,
        name: &str,
        attributes: &[(&str, &str)],
        into: &mut ScriptEvents,
    ) -> Result<(), ScriptEventsError> {
        let parent = self.open.last().copied();
        let attribute = |wanted: &str| {
            attributes
                .iter()
                .find(|(held, _)| *held == wanted)
                .map(|(_, value)| *value)
        };
        let required = |wanted: &str| {
            attribute(wanted).ok_or_else(|| self.error(format!("<{name}> with no {wanted}")))
        };
        let allow = |allowed: &[&str]| {
            for (held, _) in attributes {
                if !allowed.contains(held) {
                    return Err(self.error(format!("<{name}> with attribute \"{held}\"")));
                }
            }
            Ok(())
        };
        match (parent, name) {
            (None, "SageLuaScriptSection") if !self.root => {
                allow(&["xmlns"])?;
                self.root = true;
                Ok(())
            }
            (Some("SageLuaScriptSection"), "Events") => allow(&[]),
            (Some("Events"), "InternalEvent" | "ScriptedEvent" | "ModelConditionEvent") => {
                allow(&["Name"])?;
                let event = required("Name")?;
                if into.events.iter().any(|held| held == event) {
                    return Err(self.error(format!("event \"{event}\" declared twice")));
                }
                into.events.push(event.to_owned());
                Ok(())
            }
            (Some("ModelConditionEvent"), "Conditions") => allow(&[]),
            (Some("SageLuaScriptSection"), "EventList") => {
                allow(&["Name", "Inherit"])?;
                let list = required("Name")?;
                if into.lists.iter().any(|held| held.name == list) {
                    return Err(self.error(format!("EventList \"{list}\" declared twice")));
                }
                into.lists.push(EventList {
                    name: list.to_owned(),
                    inherit: attribute("Inherit").map(ToOwned::to_owned),
                    handlers: Vec::new(),
                });
                Ok(())
            }
            (Some("EventList"), "EventHandler") => {
                allow(&["EventName", "ScriptFunctionName", "DebugSingleStep"])?;
                let event = required("EventName")?.to_owned();
                let function = required("ScriptFunctionName")?.to_owned();
                let Some(list) = into.lists.last_mut() else {
                    return Err(self.error("<EventHandler> outside an EventList"));
                };
                if list.handlers.iter().any(|(held, _)| *held == event) {
                    return Err(self.error(format!(
                        "EventList \"{}\" handles \"{event}\" twice",
                        list.name
                    )));
                }
                list.handlers.push((event, function));
                Ok(())
            }
            (Some(parent), name) => Err(self.error(format!("<{name}> inside <{parent}>"))),
            (None, name) => Err(self.error(format!("<{name}> at the top level"))),
        }
    }
}

#[cfg(test)]
#[path = "script_events/tests.rs"]
mod tests;
