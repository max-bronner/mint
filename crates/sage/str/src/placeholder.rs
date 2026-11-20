use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, Copy)]
pub enum Placeholder<'a> {
    Int(i64),
    Float(f64),
    Str(&'a str),
}

const MAX_WIDTH: usize = 64;

enum Spec {
    Percent,
    Value(ValueSpec),
}

enum ValueSpec {
    Int,
    IntZeroPadded(usize),
    Str,
    Float { precision: usize },
}

#[must_use]
pub fn format_placeholder(template: &str, value: Placeholder<'_>) -> String {
    format_placeholders(template, &[value])
}

#[must_use]
pub fn format_placeholders(template: &str, values: &[Placeholder<'_>]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut next = values.iter();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match parse_spec(&mut chars) {
            Some(Spec::Percent) => out.push('%'),
            Some(Spec::Value(spec)) => {
                if let Some(value) = next.next() {
                    render(&mut out, &spec, value);
                }
            }
            None => out.push('%'),
        }
    }
    out
}

fn render(out: &mut String, spec: &ValueSpec, value: &Placeholder<'_>) {
    match spec {
        ValueSpec::Int => out.push_str(&render_int(value, 0)),
        ValueSpec::IntZeroPadded(width) => out.push_str(&render_int(value, *width)),
        ValueSpec::Str => out.push_str(&render_str(value)),
        ValueSpec::Float { precision } => out.push_str(&render_float(value, *precision)),
    }
}

fn render_int(value: &Placeholder<'_>, zero_width: usize) -> String {
    let zero_width = zero_width.min(MAX_WIDTH);
    match value {
        Placeholder::Int(n) => {
            if zero_width == 0 {
                n.to_string()
            } else {
                format!("{n:0zero_width$}")
            }
        }
        Placeholder::Float(f) => render_int(&Placeholder::Int(round_to_i64(*f)), zero_width),
        Placeholder::Str(s) => (*s).to_owned(),
    }
}

fn render_str(value: &Placeholder<'_>) -> String {
    match value {
        Placeholder::Int(n) => n.to_string(),
        Placeholder::Float(f) => render_float(&Placeholder::Float(*f), 0),
        Placeholder::Str(s) => (*s).to_owned(),
    }
}

fn render_float(value: &Placeholder<'_>, precision: usize) -> String {
    let precision = precision.min(MAX_WIDTH);
    let f = match value {
        Placeholder::Float(f) => *f,
        Placeholder::Int(n) => {
            #[allow(clippy::cast_precision_loss)]
            let f = *n as f64;
            f
        }
        Placeholder::Str(s) => return (*s).to_owned(),
    };
    let mut scale = 1.0_f64;
    for _ in 0..precision {
        scale *= 10.0;
    }
    let rounded = (f * scale).round() / scale;
    format!("{rounded:.precision$}")
}

fn round_to_i64(f: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let n = f.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64;
    n
}

fn take_digits(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(&d) = chars.peek() {
        if d.is_ascii_digit() && digits.len() < MAX_WIDTH {
            digits.push(d);
            chars.next();
        } else {
            break;
        }
    }
    digits
}

fn parse_spec(chars: &mut Peekable<Chars<'_>>) -> Option<Spec> {
    let mut probe = chars.clone();
    let width_digits = take_digits(&mut probe);
    let mut precision_digits = None;
    if probe.peek() == Some(&'.') {
        probe.next();
        precision_digits = Some(take_digits(&mut probe));
    }
    let mut conv = probe.next()?;
    if matches!(conv, 'l' | 'h') {
        conv = probe.next()?;
        if !matches!(conv, 's' | 'd') {
            return None;
        }
    }
    let spec = match conv {
        'd' if precision_digits.is_none() && width_digits.is_empty() => Spec::Value(ValueSpec::Int),
        'd' if precision_digits.is_none() && width_digits.starts_with('0') => {
            Spec::Value(ValueSpec::IntZeroPadded(width_digits.parse().ok()?))
        }
        'f' => {
            let digits = precision_digits.as_deref().unwrap_or("");
            if digits.is_empty() {
                return None;
            }
            Spec::Value(ValueSpec::Float {
                precision: digits.parse().ok()?,
            })
        }
        's' | 'S' if width_digits.is_empty() && precision_digits.is_none() => {
            Spec::Value(ValueSpec::Str)
        }
        '%' if width_digits.is_empty() && precision_digits.is_none() => Spec::Percent,
        _ => return None,
    };
    *chars = probe;
    Some(spec)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn percent_d_substitutes_a_decimal_integer() {
        assert_eq!(
            format_placeholder("Cost: %d", Placeholder::Int(42)),
            "Cost: 42"
        );
    }

    #[test]
    fn several_values_fill_the_specifiers_in_order() {
        assert_eq!(
            format_placeholders("CP: %d/%d", &[Placeholder::Int(7), Placeholder::Int(9)]),
            "CP: 7/9"
        );
        assert_eq!(format_placeholder("%d/%d", Placeholder::Int(7)), "7/");
    }

    #[test]
    fn percent_s_substitutes_a_string() {
        assert_eq!(
            format_placeholder("Shortcut: %s", Placeholder::Str("B")),
            "Shortcut: B"
        );
    }

    #[test]
    fn length_modified_string_and_integer_specifiers_fill() {
        for template in ["+%ls", "+%hs", "+%ld", "+%hd"] {
            assert_eq!(format_placeholder(template, Placeholder::Int(5)), "+5");
        }
    }

    #[test]
    fn percent_percent_beside_a_length_modified_specifier_gives_one_percent() {
        assert_eq!(
            format_placeholder("+%ls%% income", Placeholder::Int(10)),
            "+10% income"
        );
    }

    #[test]
    fn zero_padded_width_pads_an_integer() {
        assert_eq!(format_placeholder("%02d", Placeholder::Int(7)), "07");
    }

    #[test]
    fn percent_percent_is_a_literal_percent() {
        assert_eq!(
            format_placeholder("Building: %.0f%%", Placeholder::Float(37.4)),
            "Building: 37%"
        );
    }

    #[test]
    fn width_and_precision_float_rounds_to_the_stated_digits() {
        assert_eq!(format_placeholder("%1.1f", Placeholder::Float(2.34)), "2.3");
    }

    #[test]
    fn an_unrecognised_specifier_is_left_verbatim() {
        assert_eq!(format_placeholder("%g", Placeholder::Int(1)), "%g");
    }

    #[test]
    fn a_trailing_percent_with_no_specifier_does_not_panic() {
        assert_eq!(format_placeholder("100%", Placeholder::Int(1)), "100%");
    }
}
