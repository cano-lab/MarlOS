//! "Word anchors" HTML transform — wrap the leading ~half of every
//! word in `<b class="word-anchor">` so the eye fixates faster.
//! Mirrors the codebase's existing math-anchor naming. Generic
//! re-implementation of the fixation-emphasis technique; deliberately
//! avoids any third-party trademark name.
//!
//! Runs **after** all other HTML transforms (math-anchor tagging, figure
//! hoisting, structure analysis, TOC/LoF generation, front/back-matter
//! injection) so the skip-set can match on classes/section types that
//! those passes set up.
//!
//! Scope: prose body only. We only transform text inside `<section>`
//! elements whose `data-section-type` is `chapter` or `interlude`, and
//! we skip text inside math anchors, math, code, headings, the small-
//! caps lead-in span, the drop cap, and figure captions.

/// Apply word-anchor fixation emphasis to `html`. Idempotent enough:
/// nested `<b class="word-anchor">` would be visually no-op anyway
/// because we never emit one inside an already-skipped context.
pub fn apply(html: &str) -> String {
    let mut p = Parser::new(html);
    p.run();
    p.out
}

/// HTML elements we never recurse into.
fn tag_skips_content(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "template"
            | "code"
            | "pre"
            | "kbd"
            | "samp"
            | "var"
            | "tt"
            | "math"
            | "svg"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "figcaption"
            | "nav"
            | "head"
            | "title"
    )
}

/// HTML5 void elements — never have a closing tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Classes that mark generated chrome we never word-anchor.
fn class_skips_content(class: &str) -> bool {
    // Sub-string match against a class attribute value. Cheap and good
    // enough — class values are space-separated and we don't need exact
    // tokenization for these unambiguous markers.
    const NEEDLES: &[&str] = &[
        "math-anchor",
        "lead-in",
        "dropcap",
        "tocmark",
        "marlos-toc",
        "toc-entry",
        "toc-title",
        "lof-",
        "gen-",
        "generated-",
        "running-header",
        "copyright-",
        "dedication-",
        "book-cover",
    ];
    NEEDLES.iter().any(|n| class.contains(n))
}

struct Parser<'a> {
    bytes: &'a [u8],
    src: &'a str,
    pos: usize,
    out: String,
    /// Depth of currently-open elements that suppress the transform.
    skip_depth: u32,
    /// Depth of `<section data-section-type="chapter|interlude">` we are
    /// currently inside. We emit anchors only when this is > 0.
    body_depth: u32,
    /// Stack of (lowercase_tag_name, contributes_to_skip_depth,
    /// contributes_to_body_depth). On close, we pop and decrement.
    stack: Vec<StackFrame>,
}

struct StackFrame {
    tag: String,
    bumped_skip: bool,
    bumped_body: bool,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            bytes: src.as_bytes(),
            src,
            pos: 0,
            out: String::with_capacity(src.len() + src.len() / 8),
            skip_depth: 0,
            body_depth: 0,
            stack: Vec::with_capacity(32),
        }
    }

    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == b'<' {
                self.consume_tag();
            } else {
                self.consume_text();
            }
        }
    }

    fn consume_text(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'<' {
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        if self.body_depth > 0 && self.skip_depth == 0 {
            transform_text(text, &mut self.out);
        } else {
            self.out.push_str(text);
        }
    }

    fn consume_tag(&mut self) {
        // Comments and CDATA: pass through verbatim.
        if self.starts_with_at("<!--", self.pos) {
            if let Some(end) = find(self.src, "-->", self.pos + 4) {
                self.out.push_str(&self.src[self.pos..end + 3]);
                self.pos = end + 3;
            } else {
                self.out.push_str(&self.src[self.pos..]);
                self.pos = self.bytes.len();
            }
            return;
        }
        if self.starts_with_at("<![CDATA[", self.pos) {
            if let Some(end) = find(self.src, "]]>", self.pos + 9) {
                self.out.push_str(&self.src[self.pos..end + 3]);
                self.pos = end + 3;
            } else {
                self.out.push_str(&self.src[self.pos..]);
                self.pos = self.bytes.len();
            }
            return;
        }
        if self.starts_with_at("<!", self.pos) || self.starts_with_at("<?", self.pos) {
            // doctype / PI
            if let Some(end) = find(self.src, ">", self.pos + 2) {
                self.out.push_str(&self.src[self.pos..end + 1]);
                self.pos = end + 1;
            } else {
                self.out.push_str(&self.src[self.pos..]);
                self.pos = self.bytes.len();
            }
            return;
        }

        // Find tag end. Quoted attribute values may contain '>', so
        // track quote state.
        let tag_start = self.pos;
        let mut i = self.pos + 1;
        let mut quote: u8 = 0;
        while i < self.bytes.len() {
            let c = self.bytes[i];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            i += 1;
        }
        if i >= self.bytes.len() {
            // Malformed — pass through.
            self.out.push_str(&self.src[tag_start..]);
            self.pos = self.bytes.len();
            return;
        }
        let tag_end = i + 1;
        let raw = &self.src[tag_start..tag_end];
        self.out.push_str(raw);
        self.pos = tag_end;

        // Classify.
        let inner = &raw[1..raw.len() - 1];
        if let Some(close_name) = parse_close(inner) {
            self.handle_close(&close_name);
        } else if let Some((name, self_closing, attrs)) = parse_open(inner) {
            self.handle_open(&name, self_closing, attrs);
        }
        // Anything else (PI, weird) already emitted above.
    }

    fn handle_open(&mut self, tag: &str, self_closing: bool, attrs: &str) {
        let lname = tag.to_ascii_lowercase();
        let void = is_void(&lname) || self_closing;

        if void {
            // Nothing to push. raw script/style void don't exist.
            return;
        }

        // Decide if this tag contributes to skip_depth.
        let class_val = extract_class_value(attrs).unwrap_or_default();
        let bumped_skip = tag_skips_content(&lname) || class_skips_content(&class_val);

        // Decide body_depth: enter when this is <section data-section-type="chapter"|"interlude">.
        let mut bumped_body = false;
        if lname == "section" {
            if let Some(st) = extract_attr_value(attrs, "data-section-type") {
                if st == "chapter" || st == "interlude" {
                    bumped_body = true;
                }
            }
        }

        if bumped_skip {
            self.skip_depth += 1;
        }
        if bumped_body {
            self.body_depth += 1;
        }
        self.stack.push(StackFrame {
            tag: lname,
            bumped_skip,
            bumped_body,
        });
    }

    fn handle_close(&mut self, tag: &str) {
        let lname = tag.to_ascii_lowercase();
        // Pop until we find a matching tag — tolerate mismatched HTML.
        let mut idx = None;
        for (k, frame) in self.stack.iter().enumerate().rev() {
            if frame.tag == lname {
                idx = Some(k);
                break;
            }
        }
        if let Some(k) = idx {
            while self.stack.len() > k {
                let frame = self.stack.pop().unwrap();
                if frame.bumped_skip {
                    self.skip_depth = self.skip_depth.saturating_sub(1);
                }
                if frame.bumped_body {
                    self.body_depth = self.body_depth.saturating_sub(1);
                }
            }
        }
        // Unmatched close: ignore — already emitted into output.
    }

    fn starts_with_at(&self, needle: &str, at: usize) -> bool {
        self.bytes.len() >= at + needle.len()
            && &self.src[at..at + needle.len()] == needle
    }
}

fn find(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].find(needle).map(|i| from + i)
}

/// Parse the inside of `<…>` for an open tag. Returns (name, self_closing, attrs).
fn parse_open(inner: &str) -> Option<(String, bool, &str)> {
    let trimmed = inner.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return None;
    }
    // Tag name: leading letters/digits.
    let name_end = trimmed
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(trimmed.len());
    let name = &trimmed[..name_end];
    if name.is_empty() || !name.chars().next().unwrap().is_ascii_alphabetic() {
        return None;
    }
    let rest = trimmed[name_end..].trim();
    let self_closing = rest.ends_with('/');
    let attrs = rest.strip_suffix('/').unwrap_or(rest).trim_end();
    Some((name.to_string(), self_closing, attrs))
}

/// Parse the inside of `</…>` for a close tag.
fn parse_close(inner: &str) -> Option<String> {
    let trimmed = inner.trim_start();
    let trimmed = trimmed.strip_prefix('/')?;
    let trimmed = trimmed.trim_start();
    let name_end = trimmed
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(trimmed.len());
    let name = &trimmed[..name_end];
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn extract_attr_value(attrs: &str, name: &str) -> Option<String> {
    // Naive but adequate for pandoc output: case-insensitive name match.
    let lower = attrs.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut from = 0;
    while let Some(idx) = lower[from..].find(&needle) {
        let pos = from + idx;
        // must be at attr boundary (preceded by space or start) and
        // followed by '=' optionally whitespace.
        let before_ok = pos == 0
            || lower.as_bytes()[pos - 1].is_ascii_whitespace();
        let after = pos + needle.len();
        let after_ok = after < lower.len() && {
            // skip whitespace
            let rest = &lower[after..];
            let stripped = rest.trim_start();
            stripped.starts_with('=')
        };
        if before_ok && after_ok {
            let rest = &attrs[after..].trim_start();
            let after_eq = rest.strip_prefix('=')?.trim_start();
            return Some(parse_attr_value(after_eq));
        }
        from = pos + needle.len();
    }
    None
}

fn extract_class_value(attrs: &str) -> Option<String> {
    extract_attr_value(attrs, "class")
}

fn parse_attr_value(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }
    let quote = bytes[0];
    if quote == b'"' || quote == b'\'' {
        if let Some(end) = s[1..].find(quote as char) {
            return s[1..1 + end].to_string();
        }
        return s[1..].to_string();
    }
    // unquoted
    let end = s
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(s.len());
    s[..end].to_string()
}

/// Walk a text run and wrap the leading half of each "word" in
/// `<b class="word-anchor">`. Preserves whitespace, punctuation, HTML
/// entities, and non-letter characters.
fn transform_text(text: &str, out: &mut String) {
    let mut chars: Vec<(usize, char)> = text.char_indices().collect();
    chars.push((text.len(), '\0')); // sentinel for end-of-string boundaries

    let mut i = 0;
    while i < chars.len() - 1 {
        let (byte_idx, ch) = chars[i];
        // HTML entities: pass through as-is.
        if ch == '&' {
            if let Some(semi) = text[byte_idx..].find(';') {
                let end = byte_idx + semi + 1;
                // accept only short alphanumeric/# bodies
                let body = &text[byte_idx + 1..end - 1];
                if !body.is_empty() && body.len() < 16 && body.chars().all(is_entity_char) {
                    out.push_str(&text[byte_idx..end]);
                    while i < chars.len() && chars[i].0 < end {
                        i += 1;
                    }
                    continue;
                }
            }
        }
        if is_word_char(ch) {
            // collect contiguous word characters
            let start_byte = byte_idx;
            let mut j = i;
            let mut letter_count = 0usize;
            while j < chars.len() - 1 && is_word_char(chars[j].1) {
                if chars[j].1.is_alphabetic() {
                    letter_count += 1;
                }
                j += 1;
            }
            let end_byte = chars[j].0;
            let word = &text[start_byte..end_byte];
            if letter_count >= 2 {
                emit_anchored_word(word, out);
            } else {
                out.push_str(word);
            }
            i = j;
        } else {
            out.push(ch);
            i += 1;
        }
    }
}

fn is_word_char(c: char) -> bool {
    // Letters and intra-word punctuation (apostrophes). Digits aren't
    // emphasized — purely-numeric tokens stay alone.
    c.is_alphabetic() || c == '\'' || c == '\u{2019}'
}

fn is_entity_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '#'
}

/// Emit one word with the leading ~half wrapped in
/// `<b class="word-anchor">`. We split by *letter* count, then walk
/// char-by-char to find the byte boundary at that letter index.
fn emit_anchored_word(word: &str, out: &mut String) {
    let total_letters: usize = word.chars().filter(|c| c.is_alphabetic()).count();
    if total_letters < 2 {
        out.push_str(word);
        return;
    }
    let cut = bold_letter_count(total_letters);
    let mut seen = 0usize;
    let mut split_at = word.len();
    for (idx, ch) in word.char_indices() {
        if ch.is_alphabetic() {
            seen += 1;
            if seen == cut {
                split_at = idx + ch.len_utf8();
                break;
            }
        }
    }
    out.push_str("<b class=\"word-anchor\">");
    out.push_str(&word[..split_at]);
    out.push_str("</b>");
    out.push_str(&word[split_at..]);
}

/// Bionic-reading fixation length: ceil(n / 2). Matches the public
/// reference implementations — "The" → "Th"e, "quick" → "qui"ck.
fn bold_letter_count(n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (n + 1) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(html: &str) -> String {
        // Wrap so the transform actually triggers (needs a chapter section).
        let wrapped = format!(
            r#"<section data-section-type="chapter"><p>{}</p></section>"#,
            html
        );
        let out = apply(&wrapped);
        out
    }

    #[test]
    fn anchors_simple_words() {
        let out = body("The quick brown fox.");
        assert!(out.contains("<b class=\"word-anchor\">Th</b>e"));
        assert!(out.contains("<b class=\"word-anchor\">qui</b>ck"));
        assert!(out.contains("<b class=\"word-anchor\">bro</b>wn"));
        assert!(out.contains("<b class=\"word-anchor\">fo</b>x"));
    }

    #[test]
    fn preserves_punctuation_and_numbers() {
        let out = body("Hello, world! 42 is 1.0e2.");
        assert!(out.contains(", "));
        assert!(out.contains("42"));
        assert!(out.contains("1.0e2"));
    }

    #[test]
    fn skips_outside_chapter() {
        let html = r#"<section data-section-type="front-matter"><p>copyright text here</p></section>"#;
        let out = apply(html);
        assert!(!out.contains("word-anchor"));
    }

    #[test]
    fn skips_math_anchor() {
        let html = r#"<section data-section-type="chapter"><blockquote class="math-anchor"><p>Math Anchor — Title: lead in.</p></blockquote></section>"#;
        let out = apply(html);
        assert!(!out.contains("word-anchor"));
    }

    #[test]
    fn skips_code_and_pre() {
        let html = r#"<section data-section-type="chapter"><pre><code>let x = 1;</code></pre><p>text</p></section>"#;
        let out = apply(html);
        assert!(out.contains("<b class=\"word-anchor\">te</b>xt"));
        assert!(!out.contains("<b class=\"word-anchor\">le</b>t"));
    }

    #[test]
    fn skips_math_elements() {
        let html = r#"<section data-section-type="chapter"><p>before <math><mi>x</mi></math> after</p></section>"#;
        let out = apply(html);
        assert!(out.contains("<b class=\"word-anchor\">bef</b>ore"));
        assert!(out.contains("<b class=\"word-anchor\">aft</b>er"));
        assert!(out.contains("<math>"));
        assert!(!out.contains("<b class=\"word-anchor\">x"));
    }

    #[test]
    fn skips_headings() {
        let html = r#"<section data-section-type="chapter"><h1>Title</h1><p>body</p></section>"#;
        let out = apply(html);
        assert!(!out.contains("<b class=\"word-anchor\">Ti</b>tle"));
        assert!(out.contains("<b class=\"word-anchor\">bo</b>dy"));
    }

    #[test]
    fn idempotent_on_existing_bold() {
        let html = r#"<section data-section-type="chapter"><p><strong>bold</strong> text</p></section>"#;
        let out = apply(html);
        // <strong> is not in the skip list so we still anchor words
        // inside it; the visual is bold-on-bold which is fine.
        assert!(out.contains("<b class=\"word-anchor\">bo</b>ld"));
        assert!(out.contains("<b class=\"word-anchor\">te</b>xt"));
    }

    #[test]
    fn handles_entities() {
        let out = body("Smith &amp; Jones");
        assert!(out.contains("&amp;"));
        assert!(out.contains("<b class=\"word-anchor\">Sm</b>ith"));
        assert!(out.contains("<b class=\"word-anchor\">Jo</b>nes"));
    }
}
