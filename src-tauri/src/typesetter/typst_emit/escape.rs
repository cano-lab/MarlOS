//! Escape typst special characters when emitting prose-mode markup.
//!
//! Typst's markup special characters (different set from Markdown
//! and HTML): `#`, `*`, `_`, `<`, `[`, `]`, `@`, `` ` ``, `$`, `\`.
//! Each one is escaped by prefixing with a backslash. This applies
//! to *text* nodes only — we don't run this on typst code we
//! ourselves emit.

/// Escape a string for safe inclusion in typst markup mode.
pub fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        push_escaped(&mut out, c);
    }
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        // Markup specials per the typst reference.
        '\\' | '#' | '*' | '_' | '<' | '[' | ']' | '@' | '`' | '$' => {
            out.push('\\');
            out.push(c);
        }
        _ => out.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_each_special() {
        assert_eq!(escape_markup("a#b"), "a\\#b");
        assert_eq!(escape_markup("[link]"), "\\[link\\]");
        assert_eq!(escape_markup("price: $5"), "price: \\$5");
        assert_eq!(escape_markup("path: a\\b"), "path: a\\\\b");
        assert_eq!(escape_markup("@user"), "\\@user");
        assert_eq!(escape_markup("`code`"), "\\`code\\`");
    }

    #[test]
    fn leaves_normal_text_alone() {
        assert_eq!(
            escape_markup("Hello, world — and welcome."),
            "Hello, world — and welcome."
        );
    }
}
