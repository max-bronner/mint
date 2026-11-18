use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageSource {
    Territory,
    Tutorial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) source: MessageSource,
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) args: Vec<String>,
}

pub(crate) fn fill(text: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = text;
    while let Some(at) = rest.find("%ls") {
        out.push_str(rest.get(..at).unwrap_or_default());
        out.push_str(args.next().map_or("", String::as_str));
        rest = rest.get(at.saturating_add(3)..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Default)]
pub(crate) struct MessageQueue {
    waiting: VecDeque<Message>,
    current: Option<Message>,
    closed: Vec<MessageSource>,
}

impl MessageQueue {
    pub(crate) fn push(&mut self, message: Message) {
        self.waiting.push_back(message);
    }

    pub(crate) fn start_next(&mut self) -> Option<&Message> {
        if self.current.is_none() {
            self.current = self.waiting.pop_front();
            return self.current.as_ref();
        }
        None
    }

    pub(crate) fn current(&self) -> Option<&Message> {
        self.current.as_ref()
    }

    pub(crate) fn close(&mut self) -> bool {
        match self.current.take() {
            Some(message) => {
                self.closed.push(message.source);
                true
            }
            None => false,
        }
    }

    pub(crate) fn take_closed(&mut self) -> Vec<MessageSource> {
        std::mem::take(&mut self.closed)
    }
}

pub(crate) fn is_button_command(clip: &str, suffix: &str) -> bool {
    clip.ends_with("MessageBox") && suffix.starts_with("_OnButton")
}
