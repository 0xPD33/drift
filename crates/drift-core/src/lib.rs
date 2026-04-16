pub mod agent;
pub mod claude_trust;
#[cfg(feature = "drivers")]
pub mod driver;
pub mod config;
#[cfg(feature = "dispatch")]
pub mod dispatch;
pub mod env;
pub mod events;
#[cfg(feature = "handoff")]
pub mod handoff;
pub mod kdl;
pub mod lifecycle;
pub mod niri;
pub mod paths;
#[cfg(feature = "post-dispatch")]
pub mod post_dispatch;
pub mod project_state;
pub mod registry;
pub mod session;
pub mod supervisor;
pub mod sync;
#[cfg(feature = "tasks")]
pub mod tasks;
pub mod workspace;
pub mod workspace_names;
#[cfg(feature = "worktree")]
pub mod worktree;

pub use config::FeaturesConfig as FeatureFlags;

/// Parse YAML frontmatter from content with `---` delimiters.
/// Returns `(yaml_between_delimiters, rest_after_second_delimiter)`.
///
/// Handles CRLF line endings and whitespace-only lines around delimiters.
pub fn parse_yaml_frontmatter(content: &str) -> Option<(&str, &str)> {
    let lines: Vec<&str> = content.lines().collect();

    // Find first line that is exactly "---" (after trimming whitespace)
    let first = lines.iter().position(|l| l.trim() == "---")?;

    // Find second "---" delimiter after the first
    let second = lines[first + 1..].iter().position(|l| l.trim() == "---")? + first + 1;

    // Compute byte offset of the YAML content between delimiters
    let mut byte_offset = 0;
    for (i, line) in content.lines().enumerate() {
        if i < first {
            // +1 for the newline character (or +2 for CRLF, but .lines() strips that)
            byte_offset += line.len() + if content[byte_offset..].starts_with(&format!("{line}\r\n")) { 2 } else { 1 };
        } else if i == first {
            byte_offset += line.len() + if content[byte_offset..].starts_with(&format!("{line}\r\n")) { 2 } else { 1 };
            break;
        }
    }
    let yaml_start = byte_offset;

    // Find byte offset of the closing delimiter line
    let mut yaml_end = yaml_start;
    for (i, line) in content[yaml_start..].lines().enumerate() {
        if i + first + 1 == second {
            break;
        }
        yaml_end += line.len() + if content[yaml_end..].starts_with(&format!("{line}\r\n")) { 2 } else { 1 };
    }

    let yaml = &content[yaml_start..yaml_end];

    // Compute byte offset past the closing delimiter line
    let closing_line = lines[second];
    let mut rest_offset = yaml_end + closing_line.len();
    // Skip the line ending after the closing ---
    if content[rest_offset..].starts_with("\r\n") {
        rest_offset += 2;
    } else if content[rest_offset..].starts_with('\n') {
        rest_offset += 1;
    }

    let rest = &content[rest_offset..];

    Some((yaml, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_basic() {
        let content = "---\nkey: value\n---\nbody text\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value\n");
        assert_eq!(rest, "body text\n");
    }

    #[test]
    fn frontmatter_crlf() {
        let content = "---\r\nkey: value\r\n---\r\nbody\r\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value\r\n");
        assert_eq!(rest, "body\r\n");
    }

    #[test]
    fn frontmatter_extra_dashes_in_body() {
        let content = "---\nkey: value\n---\nsome text\n---\nmore text\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value\n");
        assert_eq!(rest, "some text\n---\nmore text\n");
    }

    #[test]
    fn frontmatter_whitespace_delimiters() {
        let content = "  ---  \nkey: value\n  ---  \nbody\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value\n");
        assert_eq!(rest, "body\n");
    }

    #[test]
    fn frontmatter_no_delimiters() {
        assert!(parse_yaml_frontmatter("just text").is_none());
    }

    #[test]
    fn frontmatter_only_opening() {
        assert!(parse_yaml_frontmatter("---\nkey: value\n").is_none());
    }

    #[test]
    fn frontmatter_empty_body() {
        let content = "---\nkey: value\n---\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value\n");
        assert_eq!(rest, "");
    }

    #[test]
    fn frontmatter_empty_yaml() {
        let content = "---\n---\nbody\n";
        let (yaml, rest) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(yaml, "");
        assert_eq!(rest, "body\n");
    }
}
