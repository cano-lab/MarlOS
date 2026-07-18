//! Markdown → typst emitter for the PDF backend.
//!
//! Walks a comrak AST of the prepared book markdown (post-citation
//! transform, post-file concatenation) and emits a complete typst
//! document, including the preamble derived from [`BookConfig`].
//!
//! This module is the M2 centerpiece of the plan at
//! `C:\Users\jerro\.claude\plans\rustling-riding-frost.md`. It
//! replaces the pandoc + HTML transforms + Chromium pipeline on the
//! PDF export path. The preview path still uses pandoc — see plan.

mod escape;
mod markdown;
mod math;
mod preamble;
mod structure;

pub use structure::analyze_markdown_ast;

use crate::typesetter::book_config::BookConfig;

/// Convert a markdown source string into typst source.
///
/// Pipeline:
/// 1. Run the citation transform's typst-flavored variant — replaces
///    `[CITE: text]` body markers with `<typst>` passthrough tags
///    and appends a `# Notes` section before any `# Appendix`.
/// 2. Build the typst preamble from [`BookConfig`].
/// 3. Walk the (transformed) markdown via comrak and emit typst.
///
/// The output is a self-contained typst document: preamble +
/// document body, ready to hand to
/// [`crate::typesetter::typst_world::BookWorld`].
pub fn markdown_to_typst(md: &str, config: &BookConfig) -> String {
    let transformed = crate::typesetter::citations::transform_citations_to_typst(md);
    let preamble = preamble::build(config);
    let body = markdown::emit_body_with_config(&transformed.transformed, config);
    format!("{preamble}\n{body}")
}

#[cfg(test)]
pub(crate) fn body_only(md: &str) -> String {
    markdown::emit_body(md)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> BookConfig {
        BookConfig {
            doc_type: Default::default(),
            template_id: None,
            book: Default::default(),
            trim: Default::default(),
            typography: Default::default(),
            export: Default::default(),
            files: vec!["x.md".into()],
            root_dir: std::path::PathBuf::from("."),
            config_path: std::path::PathBuf::from("./book.toml"),
        }
    }

    #[test]
    fn emits_a_preamble_and_body() {
        let out = markdown_to_typst("# Hi\n\nHello world.", &cfg());
        assert!(out.contains("#set page"));
        assert!(out.contains("Hello world"));
    }
}
