/// Post-process transcription text: remove dash artifacts, normalize whitespace.
pub fn post_process(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Remove leading dashes
    let s = trimmed.trim_start_matches('-').trim_start();
    // Remove trailing dashes
    let s = s.trim_end_matches('-').trim_end();

    // Normalize whitespace: collapse runs of whitespace into single space
    let mut result = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws && !result.is_empty() {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(c);
            prev_ws = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert_eq!(post_process(""), "");
        assert_eq!(post_process("  "), "");
    }

    #[test]
    fn strips_leading_dashes() {
        assert_eq!(post_process("- hello world"), "hello world");
        assert_eq!(post_process("-- hello"), "hello");
    }

    #[test]
    fn strips_trailing_dashes() {
        assert_eq!(post_process("hello -"), "hello");
        assert_eq!(post_process("hello --"), "hello");
    }

    #[test]
    fn normalizes_whitespace() {
        assert_eq!(post_process("hello   world"), "hello world");
        assert_eq!(post_process("hello\t\nworld"), "hello world");
    }

    #[test]
    fn passthrough_clean_text() {
        assert_eq!(post_process("open project foo"), "open project foo");
    }
}
