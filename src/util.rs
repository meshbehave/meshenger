/// Format a duration in seconds as a human-readable "X ago" string.
pub fn format_ago(seconds: i64) -> String {
    if seconds < 0 {
        return "in the future".to_string();
    }
    if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}

/// Format a duration in seconds as a human-readable uptime string.
pub fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Parse a node ID from a string. Accepts:
/// - Hex with prefix: "!ebb0a1ce"
/// - Hex without prefix: "ebb0a1ce"
/// - Decimal: "3954221518"
pub fn parse_node_id(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('!') {
        u32::from_str_radix(hex, 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() == 8 {
        // 8 hex chars without prefix
        u32::from_str_radix(s, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_ago_seconds() {
        assert_eq!(format_ago(0), "0s ago");
        assert_eq!(format_ago(30), "30s ago");
        assert_eq!(format_ago(59), "59s ago");
    }

    #[test]
    fn test_format_ago_minutes() {
        assert_eq!(format_ago(60), "1m ago");
        assert_eq!(format_ago(120), "2m ago");
        assert_eq!(format_ago(3599), "59m ago");
    }

    #[test]
    fn test_format_ago_hours() {
        assert_eq!(format_ago(3600), "1h ago");
        assert_eq!(format_ago(7200), "2h ago");
        assert_eq!(format_ago(86399), "23h ago");
    }

    #[test]
    fn test_format_ago_days() {
        assert_eq!(format_ago(86400), "1d ago");
        assert_eq!(format_ago(172800), "2d ago");
        assert_eq!(format_ago(604800), "7d ago");
    }

    #[test]
    fn test_format_ago_negative() {
        assert_eq!(format_ago(-1), "in the future");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3599), "59m 59s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(86399), "23h 59m");
    }

    #[test]
    fn test_format_duration_days() {
        assert_eq!(format_duration(86400), "1d 0h 0m");
        assert_eq!(format_duration(90061), "1d 1h 1m");
    }

    #[test]
    fn test_parse_node_id_hex_with_prefix() {
        assert_eq!(parse_node_id("!ebb0a1ce"), Some(0xebb0a1ce));
        assert_eq!(parse_node_id("!00000001"), Some(1));
        assert_eq!(parse_node_id("!ffffffff"), Some(0xffffffff));
    }

    #[test]
    fn test_parse_node_id_hex_without_prefix() {
        assert_eq!(parse_node_id("ebb0a1ce"), Some(0xebb0a1ce));
        assert_eq!(parse_node_id("00000001"), Some(1));
    }

    #[test]
    fn test_parse_node_id_decimal() {
        assert_eq!(parse_node_id("3954221518"), Some(3954221518));
        assert_eq!(parse_node_id("1"), Some(1));
        assert_eq!(parse_node_id("4294967295"), Some(u32::MAX));
    }

    #[test]
    fn test_parse_node_id_invalid() {
        assert_eq!(parse_node_id(""), None);
        assert_eq!(parse_node_id("not_a_number"), None);
        assert_eq!(parse_node_id("!zzzzzzzz"), None);
        assert_eq!(parse_node_id("99999999999"), None); // overflow
    }

    #[test]
    fn test_parse_node_id_whitespace() {
        assert_eq!(parse_node_id("  !ebb0a1ce  "), Some(0xebb0a1ce));
        assert_eq!(parse_node_id("  123  "), Some(123));
    }
}
