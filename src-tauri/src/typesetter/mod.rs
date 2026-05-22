//! Book typesetter — markdown → pandoc HTML → CSS Paged Media → PDF/EPUB.
//!
//! V1 forcing function: publish *Nothing Is the Impossibility — Notes from the
//! edge of physics* by Jérémie Roy. Architecture and phasing in
//! `docs/marlos-typesetter-handoff.md`.
//!
//! This module is the *separate Book mode* path; the existing
//! markdown→`marked`→`window.print()` flow remains for non-book documents.

pub mod book_config;
pub mod citations;
pub mod epub_export;
pub mod pandoc;
pub mod pdf_export;
pub mod structure;

pub use book_config::{BookConfig, BookConfigError, BookMeta};
pub use citations::{
    cleanup_temp_markdown, normalize_unicode_scripts, prepare_book_markdown,
    transform_citations, CitationError, CitationTransformResult,
};
pub use epub_export::{export_epub, EpubExportError};
pub use pandoc::{PandocConverter, PandocConvertResult, PandocError};
pub use pdf_export::{build_export_html, html_to_pdf, paper_size_from_trim, PdfExportError};
pub use structure::{
    analyze as analyze_structure, analyze_with_options as analyze_structure_with_options,
    build_generated_back_matter, build_generated_front_matter, BookSection, BookStructure,
    SectionKind, StructuredHtml,
};
