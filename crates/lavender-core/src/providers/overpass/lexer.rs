//! Single-pass lexer for Overpass QL queries.
//!
//! Ported directly from `legacy/src/sources/overpass.js`:
//! Blanks out quoted literals (collapses to empty quotes `""` or `''`) and strips
//! line and block comments in one walk, ensuring comment markers inside quoted strings
//! are treated as string content, not comments, and vice-versa.

/// Strips comments (`//...` and `/*...*/`) and blanks quoted string literals.
#[must_use]
pub fn strip_overpass_noise(src: &str) -> String {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;

    while i < n {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            while i < n {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(quote as char);
            out.push(quote as char);
            continue;
        }

        if b == b'/' && i + 1 < n && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < n {
                i += 2;
            } else {
                i = n;
            }
            out.push(' ');
            continue;
        }

        if b == b'/' && i + 1 < n && bytes[i + 1] == b'/' {
            i += 2;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }

        out.push(b as char);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_quoted_literals() {
        assert_eq!(strip_overpass_noise(r#"["name"="Central Park"]"#), r#"[""=""]"#);
        assert_eq!(strip_overpass_noise(r"['highway'='primary']"), r"[''='']");
    }

    #[test]
    fn handles_escaped_quotes_inside_strings() {
        assert_eq!(strip_overpass_noise(r#"["note"="He said \"hello\" yesterday"]"#), r#"[""=""]"#);
        assert_eq!(strip_overpass_noise(r"['desc'='It\'s nice']"), r"[''='']");
    }

    #[test]
    fn strips_line_and_block_comments() {
        let q = "/* header comment */\n[out:json]; // trailing line comment\nnode(123);";
        let cleaned = strip_overpass_noise(q);
        assert!(!cleaned.contains("header comment"));
        assert!(!cleaned.contains("trailing line comment"));
        assert!(cleaned.contains("[out:json]"));
        assert!(cleaned.contains("node(123)"));
    }

    #[test]
    fn does_not_strip_comments_inside_quotes() {
        let q = r#"node["url"="https://example.com//test/*not_comment*/"];"#;
        assert_eq!(strip_overpass_noise(q), r#"node[""=""];"#);
    }

    #[test]
    fn does_not_treat_quotes_inside_comments_as_strings() {
        let q = "// this has \"a quote\nnode(123);";
        let cleaned = strip_overpass_noise(q);
        assert!(cleaned.contains("node(123);"));
    }
}
