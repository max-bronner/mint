pub const TAB_STOP: u16 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line<'a> {
    pub number: u32,
    pub indent: u16,
    pub keyword: &'a str,
    pub args: Vec<&'a str>,
}

impl Line<'_> {
    #[must_use]
    pub fn is_end(&self) -> bool {
        self.args.is_empty() && self.keyword.eq_ignore_ascii_case("End")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive<'a> {
    Include(&'a str),
    Define { name: &'a str, body: &'a str },
    Other(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lexed<'a> {
    Line(Line<'a>),
    Directive {
        number: u32,
        directive: Directive<'a>,
    },
    Script {
        number: u32,
        indent: u16,
        body: String,
    },
}

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

#[must_use]
pub fn decode_latin1(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(UTF8_BOM.as_slice()).unwrap_or(bytes);
    bytes.iter().map(|&b| char::from(b)).collect()
}

#[must_use]
pub fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        match b {
            b';' => return line.get(..i).unwrap_or(line),
            b'/' if bytes.get(i.saturating_add(1)) == Some(&b'/') => {
                return line.get(..i).unwrap_or(line)
            }
            _ => i = i.saturating_add(1),
        }
    }
    line
}

#[must_use]
pub fn indent_width(line: &str) -> u16 {
    let mut n: u16 = 0;
    for ch in line.chars() {
        match ch {
            '\t' => {
                n = n
                    .checked_div(TAB_STOP)
                    .and_then(|q| q.checked_add(1))
                    .and_then(|q| q.checked_mul(TAB_STOP))
                    .unwrap_or(u16::MAX);
            }
            ' ' => n = n.saturating_add(1),
            _ => break,
        }
    }
    n
}

fn tokens(code: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut chars = code.char_indices().peekable();
    let is_sep = |c: char| c.is_ascii_whitespace() || c == '=';

    while let Some(&(_, c)) = chars.peek() {
        if is_sep(c) {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let start = chars.peek().map_or(code.len(), |&(i, _)| i);
            let mut end = start;
            while let Some(&(i, next)) = chars.peek() {
                if next == '"' {
                    end = i;
                    chars.next();
                    break;
                }
                if next == '=' {
                    end = i;
                    break;
                }
                end = i.saturating_add(next.len_utf8());
                chars.next();
            }
            out.push(code.get(start..end).unwrap_or(""));
        } else {
            let start = chars.peek().map_or(code.len(), |&(i, _)| i);
            let mut end = start;
            while let Some(&(i, next)) = chars.peek() {
                if is_sep(next) {
                    break;
                }
                end = i.saturating_add(next.len_utf8());
                chars.next();
            }
            out.push(code.get(start..end).unwrap_or(""));
        }
    }
    out
}

#[must_use]
pub fn lex(source: &str) -> Vec<Lexed<'_>> {
    let mut out = Vec::new();
    let mut script: Option<(u32, u16, String)> = None;

    for (index, raw) in source.split('\n').enumerate() {
        let number = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);

        if let Some((_, _, body)) = script.as_mut() {
            if strip_comment(raw).trim().eq_ignore_ascii_case("EndScript") {
                if let Some((number, indent, body)) = script.take() {
                    out.push(Lexed::Script {
                        number,
                        indent,
                        body,
                    });
                }
            } else {
                body.push_str(raw);
                body.push('\n');
            }
            continue;
        }

        let code = strip_comment(raw).trim();
        if code.is_empty() {
            continue;
        }

        if let Some(rest) = code.strip_prefix('#') {
            out.push(Lexed::Directive {
                number,
                directive: lex_directive(rest),
            });
            continue;
        }

        let mut parts = tokens(code).into_iter();
        let Some(keyword) = parts.next() else {
            continue;
        };

        if keyword.eq("BeginScript") {
            script = Some((number, indent_width(raw), String::new()));
            continue;
        }

        out.push(Lexed::Line(Line {
            number,
            indent: indent_width(raw),
            keyword,
            args: parts.collect(),
        }));
    }

    if let Some((number, indent, body)) = script {
        out.push(Lexed::Script {
            number,
            indent,
            body,
        });
    }

    out
}

fn lex_directive(rest: &str) -> Directive<'_> {
    let trimmed = rest.trim();
    let (word, tail) = match trimmed.find(char::is_whitespace) {
        Some(i) => (
            trimmed.get(..i).unwrap_or(trimmed),
            trimmed.get(i..).unwrap_or("").trim(),
        ),
        None => (trimmed, ""),
    };

    if word.eq_ignore_ascii_case("include") {
        let path = tail
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .unwrap_or(tail);
        return Directive::Include(path);
    }

    if word.eq_ignore_ascii_case("define") {
        return match tail.find(char::is_whitespace) {
            Some(i) => Directive::Define {
                name: tail.get(..i).unwrap_or(tail),
                body: tail.get(i..).unwrap_or("").trim(),
            },
            None => Directive::Define {
                name: tail,
                body: "",
            },
        };
    }

    Directive::Other(trimmed)
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_first_comment_marker_wins() {
        assert_eq!(
            strip_comment("Armor = PIERCE 25% //archer").trim(),
            "Armor = PIERCE 25%"
        );
        assert_eq!(
            strip_comment("SinkDistance = 9;").trim(),
            "SinkDistance = 9"
        );
        assert_eq!(strip_comment("; FILE: GameData.ini ////").trim(), "");
    }

    #[test]
    fn a_trailing_comment_is_not_an_argument() {
        let items = lex("Object CINE_SauronEye ;A great eye");
        let Lexed::Line(line) = &items[0] else {
            panic!("expected a content line")
        };
        assert_eq!(line.keyword, "Object");
        assert_eq!(line.args, vec!["CINE_SauronEye"]);
    }

    #[test]
    fn equals_is_whitespace() {
        let a = lex("Draw = W3DTreeDraw Tag_01");
        let b = lex("Draw   W3DTreeDraw Tag_01");
        let (Lexed::Line(a), Lexed::Line(b)) = (&a[0], &b[0]) else {
            panic!("expected content lines")
        };
        assert_eq!(a.keyword, "Draw");
        assert_eq!(a.args, b.args);
        assert_eq!(a.args, vec!["W3DTreeDraw", "Tag_01"]);
    }

    #[test]
    fn a_quoted_value_with_internal_spaces_is_one_token() {
        let items = lex(r#"Map = "AI BASE - MORDOR - Base_01""#);
        let Lexed::Line(line) = &items[0] else {
            panic!("expected a content line")
        };
        assert_eq!(line.keyword, "Map");
        assert_eq!(line.args, vec!["AI BASE - MORDOR - Base_01"]);
    }

    #[test]
    fn a_quoted_token_does_not_swallow_what_follows_it() {
        let items = lex(r#"ParticleSysBone = "Bip L Finger2" SoWolf_Ambient_fog FollowBone:YES"#);
        let Lexed::Line(line) = &items[0] else {
            panic!("expected a content line")
        };
        assert_eq!(
            line.args,
            vec!["Bip L Finger2", "SoWolf_Ambient_fog", "FollowBone:YES"]
        );
    }

    #[test]
    fn an_unterminated_quote_runs_to_end_of_line() {
        let items = lex("Comment = \"unterminated banner");
        let Lexed::Line(line) = &items[0] else {
            panic!("expected a content line")
        };
        assert_eq!(line.args, vec!["unterminated banner"]);
    }

    #[test]
    fn an_empty_quoted_string_is_a_present_empty_token() {
        let items = lex(r#"Filename = """#);
        let Lexed::Line(line) = &items[0] else {
            panic!("expected a content line")
        };
        assert_eq!(line.args, vec![""]);
    }

    #[test]
    fn a_tab_advances_four_columns_not_one() {
        assert_eq!(indent_width("\tModel = x"), 4);
        assert_eq!(indent_width("  \tModel = x"), 4);
        assert_eq!(indent_width("    \tModel = x"), 8);
        assert_eq!(indent_width("      Model = x"), 6);
    }

    #[test]
    fn a_script_region_is_one_opaque_item() {
        let items = lex(
            "AnimationState = MOVING\n  BeginScript\n    CurDrawableHideSubObject(\"x\")\n    End\n  EndScript\nEnd\n",
        );
        assert_eq!(items.len(), 3, "state header, script, End");
        let Lexed::Script { number, body, .. } = &items[1] else {
            panic!("expected a script region")
        };
        assert_eq!(*number, 2);
        assert!(body.contains("CurDrawableHideSubObject"));
        let Lexed::Line(last) = &items[2] else {
            panic!("expected a line")
        };
        assert_eq!(last.number, 6);
    }

    #[test]
    fn an_unterminated_script_runs_to_end_of_file() {
        let items = lex("BeginScript\n  anything at all\nEnd\n");
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], Lexed::Script { .. }));
    }

    #[test]
    fn directives_are_recognised_and_quotes_stripped() {
        let items = lex("#include \"..\\x.inc\"\n#define COST 500\n#pragma once\n");
        assert_eq!(items.len(), 3);
        assert!(matches!(
            &items[0],
            Lexed::Directive { directive: Directive::Include(p), .. } if *p == r"..\x.inc"
        ));
        assert!(matches!(
            &items[1],
            Lexed::Directive { directive: Directive::Define { name, body }, .. }
                if *name == "COST" && *body == "500"
        ));
        assert!(matches!(
            &items[2],
            Lexed::Directive {
                directive: Directive::Other(_),
                ..
            }
        ));
    }

    #[test]
    fn arbitrary_input_never_panics() {
        for text in [
            "",
            "\n\n\n",
            "=",
            "=====",
            ";;;;",
            "//",
            "#",
            "#include",
            "#include \"",
            "#define",
            "BeginScript",
            "EndScript",
            "\t\t\t\t",
            "Object \u{e9}\n  Model = \u{e9}\nEnd",
            "\u{0}\u{1}\u{2}",
        ] {
            let _ = lex(text);
        }
    }

    #[test]
    fn a_leading_bom_is_stripped_before_the_first_comment() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(b";////\nObject Foo\n  KindOf = INFANTRY\nEnd\n");
        let text = decode_latin1(&bytes);
        let items = lex(&text);
        let Lexed::Line(first) = &items[0] else {
            panic!("expected the Object header, not a mangled comment line")
        };
        assert_eq!(first.keyword, "Object");
        assert_eq!(first.args, vec!["Foo"]);
    }

    #[test]
    fn a_bom_byte_sequence_mid_file_is_left_alone() {
        let mut bytes = b"Object A ".to_vec();
        bytes.extend_from_slice(&UTF8_BOM);
        bytes.extend_from_slice(b"\nEnd\n");
        let text = decode_latin1(&bytes);
        assert!(text.starts_with("Object A"));
        assert_eq!(text.chars().count(), bytes.len());
    }

    #[test]
    fn latin1_decoding_is_lossless_over_every_byte() {
        let all: Vec<u8> = (0u8..=255).collect();
        let text = decode_latin1(&all);
        assert_eq!(text.chars().count(), 256);
        let back: Vec<u8> = text.chars().map(|c| c as u8).collect();
        assert_eq!(back, all);
    }
}
