//! Bundled font assets for the typst PDF backend.
//!
//! Embeds the same four families the Chromium pipeline used to ship —
//! EB Garamond (latin/latin-ext/greek 400/400i/600/600i), plus the
//! three SIL-OFL accessibility families (OpenDyslexic 400/400i/700/700i,
//! Atkinson Hyperlegible 400/400i/700/700i, Lexend 400/600/700). All
//! come from `@fontsource/*/files/*.woff2`; we decode WOFF2 → OTF
//! once on first use and hand the resulting `Font` set to typst's
//! `FontBook`.
//!
//! **OS-port caveat:** the `woff` crate's WOFF2 decoder pulls
//! `libz-sys` (C zlib). Acceptable on Windows/macOS/Linux but flagged
//! as non-pure-Rust per the [[feedback-marlos-rust-only]] rule. M-late:
//! either switch to a pure-Rust deflate (`miniz_oxide`) or commit the
//! decoded OTFs directly into the repo so no runtime decode is needed.

use std::sync::OnceLock;

use typst::foundations::Bytes;
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;

// EB Garamond — three Unicode subsets × four weight/style variants
// = 12 files. typst doesn't use unicode-range; we feed it every
// subset and the FontBook orders them so the engine picks whichever
// has a given glyph.
macro_rules! ebg {
    ($subset:literal, $weight:literal, $style:literal) => {
        include_bytes!(concat!(
            "../../../node_modules/@fontsource/eb-garamond/files/eb-garamond-",
            $subset,
            "-",
            $weight,
            "-",
            $style,
            ".woff2"
        ))
    };
}
const EBG_LATIN_400_NORMAL: &[u8] = ebg!("latin", "400", "normal");
const EBG_LATIN_400_ITALIC: &[u8] = ebg!("latin", "400", "italic");
const EBG_LATIN_600_NORMAL: &[u8] = ebg!("latin", "600", "normal");
const EBG_LATIN_600_ITALIC: &[u8] = ebg!("latin", "600", "italic");
const EBG_LATEXT_400_NORMAL: &[u8] = ebg!("latin-ext", "400", "normal");
const EBG_LATEXT_400_ITALIC: &[u8] = ebg!("latin-ext", "400", "italic");
const EBG_LATEXT_600_NORMAL: &[u8] = ebg!("latin-ext", "600", "normal");
const EBG_LATEXT_600_ITALIC: &[u8] = ebg!("latin-ext", "600", "italic");
const EBG_GREEK_400_NORMAL: &[u8] = ebg!("greek", "400", "normal");
const EBG_GREEK_400_ITALIC: &[u8] = ebg!("greek", "400", "italic");
const EBG_GREEK_600_NORMAL: &[u8] = ebg!("greek", "600", "normal");
const EBG_GREEK_600_ITALIC: &[u8] = ebg!("greek", "600", "italic");

// Accessibility families. Latin subset only to keep the binary small;
// the manuscript prose is English. Add latin-ext if a translation
// needs it.
const OPENDYS_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-400-normal.woff2"
);
const OPENDYS_400_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-400-italic.woff2"
);
const OPENDYS_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-700-normal.woff2"
);
const OPENDYS_700_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-700-italic.woff2"
);
const ATKINSON_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-400-normal.woff2"
);
const ATKINSON_400_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-400-italic.woff2"
);
const ATKINSON_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-700-normal.woff2"
);
const ATKINSON_700_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-700-italic.woff2"
);
const LEXEND_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-400-normal.woff2"
);
const LEXEND_600_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-600-normal.woff2"
);
const LEXEND_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-700-normal.woff2"
);

const ALL_WOFF2: &[&[u8]] = &[
    EBG_LATIN_400_NORMAL,
    EBG_LATIN_400_ITALIC,
    EBG_LATIN_600_NORMAL,
    EBG_LATIN_600_ITALIC,
    EBG_LATEXT_400_NORMAL,
    EBG_LATEXT_400_ITALIC,
    EBG_LATEXT_600_NORMAL,
    EBG_LATEXT_600_ITALIC,
    EBG_GREEK_400_NORMAL,
    EBG_GREEK_400_ITALIC,
    EBG_GREEK_600_NORMAL,
    EBG_GREEK_600_ITALIC,
    OPENDYS_400_NORMAL,
    OPENDYS_400_ITALIC,
    OPENDYS_700_NORMAL,
    OPENDYS_700_ITALIC,
    ATKINSON_400_NORMAL,
    ATKINSON_400_ITALIC,
    ATKINSON_700_NORMAL,
    ATKINSON_700_ITALIC,
    LEXEND_400_NORMAL,
    LEXEND_600_NORMAL,
    LEXEND_700_NORMAL,
];

static FACES: OnceLock<Vec<Font>> = OnceLock::new();
static BOOK: OnceLock<LazyHash<FontBook>> = OnceLock::new();

/// All decoded font faces, in the order they were declared above.
/// Computed once on first call; subsequent calls return the cached
/// `Vec`. Decoding 23 WOFF2 files cost ~100 ms in M0 timings.
pub fn bundled_faces() -> &'static [Font] {
    FACES.get_or_init(decode_all).as_slice()
}

/// FontBook built from `bundled_faces()`. Wrapped in `LazyHash` so
/// typst can memoize against it.
pub fn font_book() -> &'static LazyHash<FontBook> {
    BOOK.get_or_init(|| LazyHash::new(FontBook::from_fonts(bundled_faces().iter())))
}

fn decode_all() -> Vec<Font> {
    let mut out = Vec::with_capacity(ALL_WOFF2.len() * 2);
    for (i, woff2_bytes) in ALL_WOFF2.iter().enumerate() {
        let otf = match woff::version2::decompress(woff2_bytes) {
            Some(o) => o,
            None => {
                log::error!("font #{i}: WOFF2 decode failed (file empty or malformed)");
                continue;
            }
        };
        // A single OTF file can technically be a TrueType Collection
        // (.ttc) with multiple faces. @fontsource ships single-face
        // files, but iterate defensively.
        let bytes = Bytes::new(otf);
        for idx in 0..32 {
            match Font::new(bytes.clone(), idx) {
                Some(f) => out.push(f),
                None => break,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_faces() {
        let faces = bundled_faces();
        // 23 WOFF2 files, each yielding one face → 23 total.
        assert!(faces.len() >= 23, "expected ≥ 23 faces, got {}", faces.len());

        // Confirm every expected family is present.
        let families: std::collections::HashSet<&str> =
            faces.iter().map(|f| f.info().family.as_str()).collect();
        for expected in &[
            "EB Garamond",
            "OpenDyslexic",
            "Atkinson Hyperlegible",
            "Lexend",
        ] {
            assert!(
                families.contains(expected),
                "family {expected:?} not found; got {families:?}"
            );
        }
    }

    #[test]
    fn font_book_is_populated() {
        let book = font_book();
        // The book is the LazyHash<FontBook>; deref to inspect.
        assert!(
            (**book).families().count() > 0,
            "font book should have at least one family"
        );
    }
}
