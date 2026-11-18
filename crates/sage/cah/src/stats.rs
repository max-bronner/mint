use std::collections::BTreeMap;

const LINE_CHUNK: usize = 1023;

#[must_use]
pub fn parse(text: &str) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    let mut rest = text.as_bytes();
    while !rest.is_empty() {
        let limit = rest.len().min(LINE_CHUNK);
        let window = rest.get(..limit).unwrap_or(rest);
        let take = window
            .iter()
            .position(|&b| b == b'\n')
            .map_or(limit, |i| i.saturating_add(1));
        let (line, tail) = rest.split_at(take.min(rest.len()));
        rest = tail;
        if let Some((key, value)) = split_line(line) {
            out.insert(key, value);
        }
    }
    out
}

fn split_line(line: &[u8]) -> Option<(String, i32)> {
    let line = line.trim_ascii();
    let start = line.iter().position(|&b| b != b'=')?;
    let body = line.get(start..)?;
    let (key, value) = match body.iter().position(|&b| b == b'=') {
        Some(i) => (body.get(..i)?, body.get(i.saturating_add(1)..)?),
        None => (body, &[][..]),
    };
    let key = key.trim_ascii();
    if key.is_empty() {
        return None;
    }
    Some((
        String::from_utf8_lossy(key).into_owned(),
        atoi(value.trim_ascii()),
    ))
}

fn atoi(text: &[u8]) -> i32 {
    let (negative, digits) = match text.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text),
    };
    let mut value: i64 = 0;
    for &b in digits.iter().take_while(|b| b.is_ascii_digit()) {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(b.saturating_sub(b'0')))
            .min(i64::from(u32::MAX));
    }
    let signed = if negative {
        value.saturating_neg()
    } else {
        value
    };
    i32::try_from(signed).unwrap_or(if negative { i32::MIN } else { i32::MAX })
}

#[must_use]
pub fn write(map: &BTreeMap<String, i32>) -> String {
    let mut out = String::new();
    for (key, value) in map {
        out.push_str(&format!("{key} = {value}\r\n"));
    }
    out
}
