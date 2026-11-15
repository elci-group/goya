pub fn parse_canonical_i64(text: &str) -> Option<i64> {
    if text.is_empty() || text.len() > 20 {
        return None;
    }
    if text
        .bytes()
        .any(|byte| !matches!(byte, b'-' | b'0'..=b'9'))
    {
        return None;
    }
    let value = text.parse::<i64>().ok()?;
    if format_i64(value) != text {
        return None;
    }
    Some(value)
}

pub fn format_i64(value: i64) -> String {
    value.to_string()
}
