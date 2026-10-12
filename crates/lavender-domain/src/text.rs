//! Sanitising of free text that arrives from external providers.

/// Removes control characters, collapses runs of whitespace and truncates to `max_chars`
/// characters. Provider strings (place names, callsigns) are untrusted input.
#[must_use]
pub fn clean(input: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_chars * 4));
    let mut count = 0;
    let mut last_space = true;
    for ch in input.chars() {
        if count >= max_chars {
            break;
        }
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                count += 1;
                last_space = true;
            }
        } else if !ch.is_control() {
            out.push(ch);
            count += 1;
            last_space = false;
        }
    }
    let trimmed_len = out.trim_end().len();
    out.truncate(trimmed_len);
    out
}

/// Like [`clean`], but `None` for strings that are empty after cleaning.
#[must_use]
pub fn clean_opt(input: Option<&str>, max_chars: usize) -> Option<String> {
    input.map(|s| clean(s, max_chars)).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_controls_and_collapses_whitespace() {
        assert_eq!(clean("  SAS\u{0}123\n\t  \u{7}x ", 64), "SAS123 x");
    }

    #[test]
    fn truncates_by_characters_not_bytes() {
        assert_eq!(
            clean("Ålesund harbour", 4),
            "Ålesund harbour".chars().take(4).collect::<String>()
        );
    }

    #[test]
    fn empty_becomes_none() {
        assert_eq!(clean_opt(Some(" \u{1} "), 10), None);
        assert_eq!(clean_opt(Some("ok"), 10), Some("ok".to_owned()));
    }
}
