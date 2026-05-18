//! `book.toml` — declarative book configuration.
//!
//! Phase B's data model. A book is a directory containing one or more
//! markdown files plus a `book.toml`. Per the handoff doc, V1 ships with
//! one trim size (6×9 in) but the data flows through config so V2 can add
//! more without refactoring the renderer.
//!
//! ```toml
//! [book]
//! title = "Nothing Is the Impossibility"
//! subtitle = "Notes from the edge of physics"
//! author = "Jérémie Roy"
//! isbn = ""
//!
//! [trim]
//! size = "6x9"        # only "6x9" supported in V1
//! margins_in = { inside = 1.0, outside = 0.875, top = 0.75, bottom = 0.75 }
//!
//! [typography]
//! body_font = "EB Garamond"
//! body_size_pt = 11
//! body_leading_pt = 14
//!
//! # Files in reading order. Paths relative to book.toml dir.
//! files = ["nothing-is-the-impossibility-v7.md"]
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum BookConfigError {
    #[error("book.toml not found in {0}")]
    NotFound(PathBuf),
    #[error("IO error reading book config: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed book.toml: {0}")]
    Parse(String),
    #[error("invalid book.toml: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BookMeta {
    pub title: String,
    pub subtitle: String,
    pub author: String,
    pub isbn: String,
    /// Front cover image path, relative to book dir or absolute.
    pub cover_image: Option<String>,
    /// Back cover image path, relative to book dir or absolute.
    pub back_cover_image: Option<String>,
    /// Locale / language tag for the EPUB and pandoc.
    pub language: Option<String>,
    /// Publisher name, prints on the copyright page. Empty = omit.
    #[serde(default)]
    pub publisher: String,
    /// Year string for the copyright line. Empty = use current year.
    /// String (not int) so "2025-2026" or "© 2026" variants work.
    #[serde(default)]
    pub copyright_year: String,
    /// Name on the © line. Empty = fall back to `author`.
    #[serde(default)]
    pub copyright_holder: String,
    /// Dedication text, printed on its own page after copyright.
    /// Empty = omit dedication page entirely.
    #[serde(default)]
    pub dedication: String,
    /// Acknowledgements text. Multi-paragraph (split on blank lines).
    /// Renders as italicized, centered text on its own back-matter
    /// page, under an "Acknowledgements" heading. Empty = omit.
    #[serde(default)]
    pub acknowledgements: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrimMargins {
    pub inside: f32,
    pub outside: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for TrimMargins {
    fn default() -> Self {
        // KDP-friendly 6x9 defaults from the handoff doc
        Self {
            inside: 1.0,
            outside: 0.875,
            top: 0.75,
            bottom: 0.75,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrimConfig {
    /// Trim size identifier. V1 supports "6x9" only; the field exists so
    /// V2 can add presets without a CSS refactor.
    pub size: String,
    pub margins_in: TrimMargins,
}

impl Default for TrimConfig {
    fn default() -> Self {
        Self {
            size: "6x9".to_string(),
            margins_in: TrimMargins::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TypographyConfig {
    pub body_font: String,
    pub body_size_pt: f32,
    pub body_leading_pt: f32,
    /// Multiplier applied to the top margin of H1/H2/H3 headings.
    /// 1.0 = current "tight" defaults. 1.5–2.0 gives chapter and
    /// subsection headings more breathing room above them.
    pub heading_space_em: f32,
    /// Format of the running header (the strip at the top of each page
    /// within a chapter). Helps the reader locate themselves without
    /// having to remember the chapter title. One of:
    ///   "title"                — just the chapter title (current default)
    ///   "chapter-number"       — "Chapter 3"
    ///   "chapter-number-title" — "Chapter 3 · The Title"
    ///   "compact-arabic"       — "3 · The Title"
    ///   "compact-roman"        — "III · The Title"
    pub running_header_style: String,
    /// Number of words after the drop cap that get the small-caps
    /// lead-in treatment in the `traditional` chapter-opener preset.
    /// The structure parser wraps the first N words of every chapter's
    /// first paragraph in a <span class="lead-in"> regardless of preset,
    /// so the CSS for `modern` can ignore it and `traditional` can
    /// style it. Default 5.
    pub lead_in_word_count: u32,
    /// Chapter opener style preset. One of:
    ///   "modern" (default)    — centered small-caps title at 2em, no
    ///                            special lead-in. V1 behavior.
    ///   "traditional"          — smaller 1.6em title with 6em top
    ///                            whitespace, large 4em drop cap, and
    ///                            small-caps lead-in (the first
    ///                            lead_in_word_count words).
    /// The drop-cap and lead-in <span> tags are always written by the
    /// structure parser; this preset only changes which CSS targets
    /// them.
    pub chapter_opener_style: String,
}

impl Default for TypographyConfig {
    fn default() -> Self {
        Self {
            body_font: "EB Garamond".to_string(),
            body_size_pt: 11.0,
            body_leading_pt: 14.0,
            heading_space_em: 1.0,
            running_header_style: "title".to_string(),
            lead_in_word_count: 5,
            chapter_opener_style: "modern".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExportConfig {
    /// Color mode flag. V1 supports "bw" only; field exists so V2 can flip
    /// without a pipeline change.
    pub color_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BookConfig {
    pub book: BookMeta,
    pub trim: TrimConfig,
    pub typography: TypographyConfig,
    pub export: ExportConfig,
    /// Markdown file paths in reading order, relative to the book dir.
    pub files: Vec<String>,
    /// Resolved at load time. Not serialized.
    #[serde(skip)]
    pub root_dir: PathBuf,
    #[serde(skip)]
    pub config_path: PathBuf,
}

impl Default for BookConfig {
    fn default() -> Self {
        Self {
            book: BookMeta::default(),
            trim: TrimConfig::default(),
            typography: TypographyConfig::default(),
            export: ExportConfig::default(),
            files: Vec::new(),
            root_dir: PathBuf::new(),
            config_path: PathBuf::new(),
        }
    }
}

impl BookConfig {
    /// Find `book.toml` starting from a path that may be:
    ///  - a `book.toml` file directly,
    ///  - a directory (we'll look for `book.toml` inside),
    ///  - any other file (we'll look for `book.toml` next to it — useful
    ///    when the user pastes the manuscript markdown path).
    pub fn locate(path: &Path) -> Result<PathBuf, BookConfigError> {
        let candidate = if path.is_file() {
            let is_toml = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("toml"))
                .unwrap_or(false);
            if is_toml {
                path.to_path_buf()
            } else {
                // Treat the path's parent dir as the book directory.
                path.parent()
                    .map(|p| p.join("book.toml"))
                    .unwrap_or_else(|| PathBuf::from("book.toml"))
            }
        } else if path.is_dir() {
            path.join("book.toml")
        } else {
            return Err(BookConfigError::NotFound(path.to_path_buf()));
        };
        if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(BookConfigError::NotFound(candidate))
        }
    }

    pub fn load(path: &Path) -> Result<Self, BookConfigError> {
        let config_path = Self::locate(path)?;
        let contents = std::fs::read_to_string(&config_path)?;
        let mut config: BookConfig = toml::from_str(&contents)
            .map_err(|e| BookConfigError::Parse(e.to_string()))?;
        config.root_dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        config.config_path = config_path;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), BookConfigError> {
        if self.files.is_empty() {
            return Err(BookConfigError::Invalid(
                "files list is empty — at least one markdown file is required".into(),
            ));
        }
        for f in &self.files {
            let abs = self.root_dir.join(f);
            if !abs.is_file() {
                return Err(BookConfigError::Invalid(format!(
                    "file '{}' not found (expected at {})",
                    f,
                    abs.display()
                )));
            }
        }
        Ok(())
    }

    /// Absolute paths to the markdown files in reading order.
    pub fn resolved_files(&self) -> Vec<PathBuf> {
        self.files.iter().map(|f| self.root_dir.join(f)).collect()
    }

    /// Serialize this config to TOML text. We hand-format the output
    /// rather than use `toml::to_string` because the table-key ordering
    /// matters: `files` must appear at the top level *before* any
    /// `[section]` header, otherwise TOML will parse it as belonging to
    /// the most recent table.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();
        out.push_str("# Marlos book.toml — edit and reload to apply.\n\n");
        out.push_str("# Markdown files in reading order, relative to this file.\n");
        out.push_str("files = [");
        for (i, f) in self.files.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&toml_string_literal(f));
        }
        out.push_str("]\n\n");

        out.push_str("[book]\n");
        out.push_str(&format!("title = {}\n", toml_string_literal(&self.book.title)));
        out.push_str(&format!("subtitle = {}\n", toml_string_literal(&self.book.subtitle)));
        out.push_str(&format!("author = {}\n", toml_string_literal(&self.book.author)));
        out.push_str(&format!("isbn = {}\n", toml_string_literal(&self.book.isbn)));
        out.push_str(&format!(
            "language = {}\n",
            toml_string_literal(self.book.language.as_deref().unwrap_or("en"))
        ));
        if let Some(cover) = &self.book.cover_image {
            out.push_str(&format!("cover_image = {}\n", toml_string_literal(cover)));
        }
        if let Some(back) = &self.book.back_cover_image {
            out.push_str(&format!(
                "back_cover_image = {}\n",
                toml_string_literal(back)
            ));
        }
        out.push_str(&format!(
            "publisher = {}\n",
            toml_string_literal(&self.book.publisher)
        ));
        out.push_str(&format!(
            "copyright_year = {}\n",
            toml_string_literal(&self.book.copyright_year)
        ));
        out.push_str(&format!(
            "copyright_holder = {}\n",
            toml_string_literal(&self.book.copyright_holder)
        ));
        out.push_str(&format!(
            "dedication = {}\n",
            toml_string_literal(&self.book.dedication)
        ));
        out.push_str(&format!(
            "acknowledgements = {}\n",
            toml_string_literal(&self.book.acknowledgements)
        ));
        out.push('\n');

        out.push_str("[trim]\n");
        out.push_str(&format!("size = {}\n", toml_string_literal(&self.trim.size)));
        out.push_str(&format!(
            "margins_in = {{ inside = {}, outside = {}, top = {}, bottom = {} }}\n",
            fmt_float(self.trim.margins_in.inside),
            fmt_float(self.trim.margins_in.outside),
            fmt_float(self.trim.margins_in.top),
            fmt_float(self.trim.margins_in.bottom),
        ));
        out.push('\n');

        out.push_str("[typography]\n");
        out.push_str(&format!(
            "body_font = {}\n",
            toml_string_literal(&self.typography.body_font)
        ));
        out.push_str(&format!(
            "body_size_pt = {}\n",
            fmt_float(self.typography.body_size_pt)
        ));
        out.push_str(&format!(
            "body_leading_pt = {}\n",
            fmt_float(self.typography.body_leading_pt)
        ));
        out.push_str(&format!(
            "heading_space_em = {}\n",
            fmt_float(self.typography.heading_space_em)
        ));
        out.push_str(&format!(
            "running_header_style = {}\n",
            toml_string_literal(&self.typography.running_header_style)
        ));
        out.push_str(&format!(
            "lead_in_word_count = {}\n",
            self.typography.lead_in_word_count
        ));
        out.push_str(&format!(
            "chapter_opener_style = {}\n",
            toml_string_literal(&self.typography.chapter_opener_style)
        ));
        out.push('\n');

        out.push_str("[export]\n");
        out.push_str(&format!(
            "color_mode = {}\n",
            toml_string_literal(&self.export.color_mode)
        ));

        out
    }

    /// Write this config to its `config_path`. Caller is responsible for
    /// having a valid `config_path` set (load or locate first).
    pub fn save(&self) -> Result<(), BookConfigError> {
        if self.config_path.as_os_str().is_empty() {
            return Err(BookConfigError::Invalid(
                "BookConfig has no config_path — cannot save".into(),
            ));
        }
        std::fs::write(&self.config_path, self.to_toml_string())?;
        Ok(())
    }

    /// Generate a starter `book.toml` next to the given markdown file.
    /// Used by the UI when the user picks a markdown file in a directory
    /// that doesn't have a book.toml yet.
    pub fn init_from_markdown(markdown_path: &Path) -> Result<PathBuf, BookConfigError> {
        let dir = markdown_path
            .parent()
            .ok_or_else(|| BookConfigError::Invalid("markdown path has no parent".into()))?;
        let target = dir.join("book.toml");
        if target.exists() {
            return Ok(target);
        }
        let file_name = markdown_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("manuscript.md");
        let stem = markdown_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");

        let toml_text = format!(
            r#"# Marlos book.toml — generated starter. Edit freely.

# Markdown files in reading order, relative to this file.
files = ["{file}"]

[book]
title = "{title}"
subtitle = ""
author = ""
isbn = ""
language = "en"

[trim]
size = "6x9"
margins_in = {{ inside = 1.0, outside = 0.875, top = 0.75, bottom = 0.75 }}

[typography]
body_font = "EB Garamond"
body_size_pt = 11.0
body_leading_pt = 14.0

[export]
color_mode = "bw"
"#,
            title = stem.replace(['-', '_'], " "),
            file = file_name,
        );
        std::fs::write(&target, toml_text)?;
        Ok(target)
    }
}

fn toml_string_literal(s: &str) -> String {
    // Use a basic-string with escapes for safety. TOML basic strings allow
    // escaped \" \\ \n \t etc. We intentionally avoid literal strings so
    // values containing quotes round-trip cleanly.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn fmt_float(v: f32) -> String {
    // Always emit at least one decimal point so toml-rs parses the value
    // as a float, not an integer (avoids round-trip loss when the user
    // sets margins_in.inside = 1).
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}
