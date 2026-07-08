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
pub mod pdf_import;
pub mod resume_export;
pub mod structure;
pub mod typst_emit;
pub mod typst_fonts;
pub mod typst_world;
pub mod word_anchors;

pub use book_config::{BookConfig, BookConfigError, BookMeta, DocType};
pub use citations::{
    cleanup_temp_markdown, hoist_figure_classes, normalize_unicode_scripts,
    prepare_book_markdown, tag_math_anchors, transform_citations, CitationError,
    CitationTransformResult,
};
pub use epub_export::{export_epub, EpubExportError};
pub use pandoc::{PandocConverter, PandocConvertResult, PandocError};
pub use pdf_export::{build_export_html, html_to_pdf, paper_size_from_trim, PdfExportError};
pub use resume_export::{build_resume_css, build_resume_html};
pub use structure::{
    analyze as analyze_structure, analyze_with_options as analyze_structure_with_options,
    build_generated_back_matter, build_generated_front_matter, build_list_of_figures,
    build_toc, BookSection, BookStructure, SectionKind, StructuredHtml,
};
