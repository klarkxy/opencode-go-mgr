use chrono::Duration;

/// 解析 opencode-go 429 消息中的重置时长。
///
/// 已知格式：
/// - "Monthly usage limit reached. Resets in 13 days."
/// - "Weekly usage limit reached. Resets in 4 days."
/// - "5-hour usage limit reached. Resets in 13min."
///
/// 返回冷却时长；无法识别返回 `None`（调用方退默认值）。
pub fn parse_reset(text: &str) -> Option<Duration> {
    let idx = text.find("Resets in")?;
    let rest = text[idx + "Resets in".len()..].trim_start();
    // 提取开头的数字
    let digit_end = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if digit_end == 0 {
        return None;
    }
    let n: i64 = rest[..digit_end].parse().ok()?;
    // 单位部分：跳过数字与单位之间的空白，截到第一个非字母为止
    let unit = rest[digit_end..].trim_start();
    let end = unit
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphabetic())
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let unit = unit[..end].to_lowercase();
    match unit.as_str() {
        u if u.starts_with("min") => Some(Duration::minutes(n)),
        u if u.starts_with("hour") || u == "h" => Some(Duration::hours(n)),
        u if u.starts_with("day") => Some(Duration::days(n)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_messages() {
        assert_eq!(
            parse_reset("Monthly usage limit reached. Resets in 13 days."),
            Some(Duration::days(13))
        );
        assert_eq!(
            parse_reset("Weekly usage limit reached. Resets in 4 days."),
            Some(Duration::days(4))
        );
        assert_eq!(
            parse_reset("5-hour usage limit reached. Resets in 13min."),
            Some(Duration::minutes(13))
        );
    }

    #[test]
    fn parses_with_extra_whitespace() {
        assert_eq!(
            parse_reset("Resets in  1  day."),
            Some(Duration::days(1))
        );
    }

    #[test]
    fn returns_none_for_unknown() {
        assert_eq!(parse_reset(""), None);
        assert_eq!(parse_reset("rate limit exceeded"), None);
        assert_eq!(parse_reset("Resets in sometime"), None);
    }
}
