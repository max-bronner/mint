use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Document, Field, Node};
use crate::value::{parse_float, parse_int, tagged_value};

pub const NOTIFICATION_BOX_KEYWORD: &str = "InGameNotificationBox";

const BLOCK_FIELDS: [&str; 3] = [
    "DefaultOpenAudio",
    "DefaultMessageFont",
    "DefaultMessageColor",
];
const TYPE_BLOCK: &str = "NotificationType";
const TYPE_FIELDS: [&str; 5] = ["Title", "OpenAudio", "Icon", "MessageFont", "MessageColor"];

const UNKNOWN_FIELD: &str = "a field this block declares";
const UNKNOWN_NODE: &str = "a block this block declares";

#[derive(Debug, Clone, PartialEq)]
pub struct MessageFont {
    pub name: String,
    pub size: f32,
    pub bold: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationType {
    pub name: String,
    pub title: Option<String>,
    pub open_audio: Option<String>,
    pub icon: Option<String>,
    pub font: Option<MessageFont>,
    pub colour: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationBoxDef {
    pub types: Vec<NotificationType>,
}

impl NotificationBoxDef {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&NotificationType> {
        self.types
            .iter()
            .rev()
            .find(|kind| kind.name.eq_ignore_ascii_case(name))
    }
}

fn font(field: &Field, errors: &ValueErrors) -> Option<MessageFont> {
    let args: Vec<&str> = field
        .args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "=")
        .collect();
    let parsed = (|| {
        let name = (*args.first()?).to_owned();
        let size = parse_float(args.get(1)?)?;
        let bold = crate::value::parse_bool(args.get(2)?)?;
        Some(MessageFont { name, size, bold })
    })();
    if parsed.is_none() {
        errors.field(
            NOTIFICATION_BOX_KEYWORD,
            field,
            "a font name, a size and a bold flag",
        );
    }
    parsed
}

fn colour(field: &Field, errors: &ValueErrors) -> Option<[u8; 4]> {
    let channel = |tag: &str| {
        tagged_value(&field.args, tag)
            .and_then(parse_int)
            .and_then(|v| u8::try_from(v).ok())
    };
    let parsed = match (channel("R"), channel("G"), channel("B")) {
        (Some(r), Some(g), Some(b)) => Some([r, g, b, channel("A").unwrap_or(255)]),
        _ => None,
    };
    if parsed.is_none() {
        errors.field(NOTIFICATION_BOX_KEYWORD, field, "R:G:B:A colour bytes");
    }
    parsed
}

fn text(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .map(str::to_owned)
        .filter(|value| !crate::value::is_none(value))
}

fn declares(list: &[&str], keyword: &str) -> bool {
    list.iter().any(|name| name.eq_ignore_ascii_case(keyword))
}

fn refuse_unknown(block: &Block, errors: &ValueErrors) {
    let is_type = block.keyword.eq_ignore_ascii_case(TYPE_BLOCK);
    let (context, fields) = if is_type {
        (TYPE_BLOCK, &TYPE_FIELDS[..])
    } else {
        (NOTIFICATION_BOX_KEYWORD, &BLOCK_FIELDS[..])
    };
    for child in &block.children {
        match child {
            Node::Field(field) if !declares(fields, &field.keyword) => {
                errors.field(context, field, UNKNOWN_FIELD);
            }
            Node::Field(_) => {}
            Node::Block(nested) if !is_type && nested.keyword.eq_ignore_ascii_case(TYPE_BLOCK) => {
                refuse_unknown(nested, errors);
            }
            Node::Block(nested) => {
                errors.at(
                    &nested.file,
                    nested.line,
                    context,
                    &nested.keyword,
                    &nested.args.join(" "),
                    UNKNOWN_NODE,
                );
            }
            Node::Script(script) => {
                errors.at(
                    &script.file,
                    script.line,
                    context,
                    "BeginScript",
                    "",
                    UNKNOWN_NODE,
                );
            }
        }
    }
}

#[must_use]
pub fn read_notification_box(documents: &[Document], errors: &ValueErrors) -> NotificationBoxDef {
    let mut def = NotificationBoxDef::default();
    let (mut audio, mut default_font, mut default_colour) = (None, None, None);
    let blocks = documents
        .iter()
        .flat_map(Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case(NOTIFICATION_BOX_KEYWORD));
    for block in blocks {
        refuse_unknown(block, errors);
        if let Some(value) = text(block, "DefaultOpenAudio") {
            audio = Some(value);
        }
        if let Some(read) = block
            .field("DefaultMessageFont")
            .and_then(|f| font(f, errors))
        {
            default_font = Some(read);
        }
        if let Some(read) = block
            .field("DefaultMessageColor")
            .and_then(|f| colour(f, errors))
        {
            default_colour = Some(read);
        }
        for kind in block.blocks_named("NotificationType") {
            let Some(name) = kind.args.iter().find(|a| a.as_str() != "=") else {
                errors.at(&kind.file, kind.line, TYPE_BLOCK, TYPE_BLOCK, "", "named");
                continue;
            };
            def.types.push(NotificationType {
                name: name.clone(),
                title: text(kind, "Title"),
                open_audio: text(kind, "OpenAudio").or_else(|| audio.clone()),
                icon: text(kind, "Icon"),
                font: kind
                    .field("MessageFont")
                    .and_then(|f| font(f, errors))
                    .or_else(|| default_font.clone()),
                colour: kind
                    .field("MessageColor")
                    .and_then(|f| colour(f, errors))
                    .or(default_colour),
            });
        }
    }
    def
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn read(text: &str) -> (NotificationBoxDef, Vec<crate::diagnostic::ValueError>) {
        let document = Document::parse("box.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        let errors = ValueErrors::new();
        let def = read_notification_box(&[document], &errors);
        (def, errors.take())
    }

    const SHAPE: &str = "InGameNotificationBox\n  DefaultMessageFont = \"Face\" 16 No\n  \
         DefaultMessageColor = R:255 G:204 B:0\n  DefaultOpenAudio = Chime\n  \
         NotificationType Hint\n    Title = HUD:Hint\n  End\n  \
         NotificationType Own\n    OpenAudio = Other\n    Icon = Pic\n    MessageColor = R:1 G:2 B:3\n  End\nEnd\n";

    #[test]
    fn a_type_copies_the_defaults_and_overrides_only_what_it_sets() {
        let (def, errors) = read(SHAPE);
        assert!(errors.is_empty(), "{errors:?}");
        let hint = def.get("hint").unwrap();
        assert_eq!(hint.open_audio.as_deref(), Some("Chime"));
        assert_eq!(hint.colour, Some([255, 204, 0, 255]));
        assert_eq!(hint.font.as_ref().map(|f| f.size), Some(16.0));
        let own = def.get("Own").unwrap();
        assert_eq!(own.open_audio.as_deref(), Some("Other"));
        assert_eq!(own.colour, Some([1, 2, 3, 255]));
        assert_eq!(own.icon.as_deref(), Some("Pic"));
        assert!(def.get("Missing").is_none());
    }

    #[test]
    fn a_field_the_block_does_not_declare_is_refused() {
        let (_, errors) =
            read(&SHAPE.replace("  DefaultOpenAudio = Chime\n", "  DefaultVolume = 3\n"));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].field, "DefaultVolume");
    }

    #[test]
    fn a_field_a_type_does_not_declare_is_refused() {
        let (_, errors) = read(&SHAPE.replace("    Icon = Pic\n", "    Sound = Pic\n"));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].field, "Sound");
        assert_eq!(errors[0].context, "NotificationType");
    }

    #[test]
    fn a_type_without_a_name_is_refused() {
        let (_, errors) = read(
            "InGameNotificationBox
  NotificationType
  End
End
",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
    }
}
