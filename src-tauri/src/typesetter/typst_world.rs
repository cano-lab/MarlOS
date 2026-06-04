//! Production `typst::World` implementation for the PDF backend.
//!
//! Wraps a single in-memory main source plus the book's `root_dir`
//! for resolving manuscript-relative image paths. Sandboxes the
//! engine: any `FileId` carrying a typst package spec is rejected,
//! so a user-authored `custom.typ` cannot pull `@preview/...` packages
//! from the network or filesystem.
//!
//! Fonts come from [`crate::typesetter::typst_fonts`].

use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::Datelike;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use super::typst_fonts;

/// The typst World handed to the compiler. Holds one main source +
/// one image-resolution root. Images are looked up via `file(id)`
/// against the configured root directory.
pub struct BookWorld {
    library: &'static LazyHash<Library>,
    book: &'static LazyHash<FontBook>,
    main: FileId,
    source: Source,
    /// Filesystem root for resolving `image("foo.png")` calls. Any
    /// FileId path is joined to this; absolute paths inside the
    /// virtual FS are rejected as out-of-sandbox.
    root_dir: PathBuf,
}

/// Cached default library. typst's `Library` is hash-keyed by the
/// compiler so we only need one instance for the whole process.
fn library() -> &'static LazyHash<Library> {
    static L: OnceLock<LazyHash<Library>> = OnceLock::new();
    L.get_or_init(|| LazyHash::new(Library::default()))
}

impl BookWorld {
    /// Build a World around the given typst source. `root_dir` is the
    /// book's directory — `image("cover.png")` resolves against it.
    pub fn new(source_text: String, root_dir: impl Into<PathBuf>) -> Self {
        let main = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(main, source_text);
        Self {
            library: library(),
            book: typst_fonts::font_book(),
            main,
            source,
            root_dir: root_dir.into(),
        }
    }

    /// Resolve a typst FileId to an absolute filesystem path under
    /// `root_dir`. Returns Err if the FileId carries a package spec
    /// (we sandbox those) or escapes the root via `..`.
    fn resolve_file_path(&self, id: FileId) -> FileResult<PathBuf> {
        // Sandbox: reject package imports outright. typst's
        // `@preview/...` syntax produces a FileId with package =
        // Some(PackageSpec); we never allow those.
        if id.package().is_some() {
            return Err(FileError::Other(Some(typst::foundations::eco_format!(
                "package imports are disabled in this sandbox"
            ))));
        }
        let vpath = id.vpath();
        let raw = vpath.as_rootless_path();
        // Join under root_dir; canonicalize and verify the result
        // still lives inside root_dir to block `..`-escape.
        let candidate = self.root_dir.join(raw);
        let canon = match std::fs::canonicalize(&candidate) {
            Ok(p) => p,
            Err(_) => return Err(FileError::NotFound(raw.to_path_buf())),
        };
        let root_canon = std::fs::canonicalize(&self.root_dir)
            .unwrap_or_else(|_| self.root_dir.clone());
        if !canon.starts_with(&root_canon) {
            return Err(FileError::AccessDenied);
        }
        Ok(canon)
    }
}

impl World for BookWorld {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            // No multi-file support in V1 — we synthesize one big main.
            Err(FileError::NotFound(
                id.vpath().as_rootless_path().to_path_buf(),
            ))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.resolve_file_path(id)?;
        let data = std::fs::read(&path)
            .map_err(|e| FileError::from_io(e, &path))?;
        Ok(Bytes::new(data))
    }
    fn font(&self, index: usize) -> Option<Font> {
        typst_fonts::bundled_faces().get(index).cloned()
    }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        let now = chrono::Local::now();
        Datetime::from_ymd(now.year_ce().1 as i32, now.month() as u8, now.day() as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::layout::PagedDocument;
    use typst_pdf::PdfOptions;

    /// Compile a tiny typst source that uses italic EB Garamond on
    /// Greek letters. Confirms (1) the World accepts our source,
    /// (2) the bundled fonts include the Greek subset, (3) typst can
    /// resolve italic Greek without falling back to a missing face,
    /// and (4) the produced PDF embeds "EB Garamond".
    #[test]
    fn compiles_italic_greek() {
        // `αβγ` is U+03B1..U+03B3 — covered only by EBG_GREEK_*.
        let src = r#"
#set page(width: 3in, height: 1in, margin: 0.2in)
#set text(font: "EB Garamond", size: 16pt)
#text(style: "italic")[αβγ]
"#;
        let tmp = std::env::temp_dir().join("marlos-typst-world-test");
        let _ = std::fs::create_dir_all(&tmp);
        let world = BookWorld::new(src.to_string(), &tmp);

        let warned = typst::compile::<PagedDocument>(&world);
        let doc = warned.output.expect("compile succeeds");
        assert_eq!(doc.pages.len(), 1, "expected exactly one page");

        let pdf_bytes =
            typst_pdf::pdf(&doc, &PdfOptions::default()).expect("pdf encode");

        // Sanity: PDF marker present, EB Garamond appears in the
        // font dictionary (typst inserts a 6-char subset prefix like
        // `AAAAAA+EBGaramond-Italic`).
        assert!(
            pdf_bytes.starts_with(b"%PDF-"),
            "output is not a PDF"
        );
        let hay = String::from_utf8_lossy(&pdf_bytes);
        assert!(
            hay.contains("EBGaramond") || hay.contains("EB Garamond"),
            "EB Garamond not found in PDF font dictionary"
        );
    }

    /// Sanity: package imports are refused.
    #[test]
    fn rejects_package_imports() {
        let src = "#import \"@preview/cetz:0.2\"\n";
        let world = BookWorld::new(src.to_string(), ".");
        let warned = typst::compile::<PagedDocument>(&world);
        assert!(
            warned.output.is_err(),
            "compile should fail when source pulls a @preview package"
        );
    }
}
