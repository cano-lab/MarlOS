//! M0 spike: prove the typst pipeline produces a PDF.
//!
//! Goals (per the plan at C:\Users\jerro\.claude\plans\rustling-riding-frost.md):
//! 1. Implement a minimal `World` trait impl.
//! 2. Decode one bundled WOFF2 → TTF at startup so typst's FontBook
//!    has something to render with (decision artifact: confirms the
//!    woff2 → ttf path works, defers OTF-from-source switch to M1).
//! 3. Compile a hardcoded typst source.
//! 4. Write a PDF.
//! 5. Time the whole thing. Target ≤ 500 ms hello-world; ≤ 5 s for
//!    the larger compile in M0.5.
//!
//! Run with:
//!   cargo run --bin typst_spike --release
//!
//! Output: target/typst_spike.pdf

use std::time::Instant;

use chrono::Datelike;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_pdf::PdfOptions;

// Use one bundled WOFF2 (latin EB Garamond 400 normal). We already
// `include_bytes!` this in pdf_export.rs; re-embed here so the spike
// is standalone.
const EB_GARAMOND_400_WOFF2: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/eb-garamond/files/eb-garamond-latin-400-normal.woff2"
);
const EB_GARAMOND_400I_WOFF2: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/eb-garamond/files/eb-garamond-latin-400-italic.woff2"
);
const EB_GARAMOND_600_WOFF2: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/eb-garamond/files/eb-garamond-latin-600-normal.woff2"
);

/// Decode a WOFF2 byte slice into an OpenType (sfnt) byte vector.
/// NB: the `woff` crate's WOFF2 decoder pulls in libz-sys (C zlib).
/// Acceptable for the spike but flagged as non-pure-Rust — M1 should
/// either bundle TTF/OTF directly or swap to a pure-Rust deflate.
fn woff2_to_otf(bytes: &[u8]) -> Result<Vec<u8>, String> {
    woff::version2::decompress(bytes).ok_or_else(|| "woff2 decode failed".to_string())
}

/// Bundled fonts for the spike. M1 will read the same WOFF2 set the
/// chromium path used to embed, decode them once, and hand them to
/// the FontBook.
fn load_fonts() -> Vec<Font> {
    let raw = [
        EB_GARAMOND_400_WOFF2,
        EB_GARAMOND_400I_WOFF2,
        EB_GARAMOND_600_WOFF2,
    ];
    let mut fonts = Vec::new();
    for (i, w) in raw.iter().enumerate() {
        let otf = match woff2_to_otf(w) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("WOFF2 #{} decode failed: {}", i, e);
                continue;
            }
        };
        let bytes = Bytes::new(otf);
        // A TTF can contain a font collection; iterate indices until None.
        for idx in 0..32 {
            match Font::new(bytes.clone(), idx) {
                Some(f) => fonts.push(f),
                None => break,
            }
        }
    }
    fonts
}

/// Minimal `World` impl. Single main source, bundled fonts only, no
/// disk access, no packages, no images.
struct SpikeWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: FileId,
    source: Source,
}

impl SpikeWorld {
    fn new(typst_source: String) -> Self {
        let fonts = load_fonts();
        let book = FontBook::from_fonts(fonts.iter());
        let main = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(main, typst_source);
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main,
            source,
        }
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().to_owned()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        // Spike: no images, no extra files.
        Err(FileError::NotFound(id.vpath().as_rootless_path().to_owned()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        let now = chrono::Local::now();
        Datetime::from_ymd(now.year_ce().1 as i32, now.month() as u8, now.day() as u8)
    }
}

/// 10 real equations grepped from the physics manuscript at
/// X:\ARCH\ETE 26\Book 1- Crumbs\Markdown\nothing is the impossibility v10.5.md.
/// Each is the inside of `$...$` (so feed to `mitex::convert_math`
/// with the dollar signs stripped).
const SAMPLE_EQUATIONS: &[&str] = &[
    r"E = m c^2",
    r"1.055 \times 10^{-34}",
    r"(\dot{a}/a)^2",
    r"1/ \sqrt{(2E_p)}",
    r"6.626 \times 10^{-34}",
    r"2\sqrt{2}",
    r"B(\nu,T) = \frac{2h\nu^3}{c^2}\cdot\frac{1}{e^{h\nu/kT} - 1}",
    r"T = \frac{\hbar c^3}{8\pi G M k_B}",
    r"\langle \psi | \hat{H} | \psi \rangle",
    r"\sum_{i=0}^{n} \frac{1}{i^2}",
];

fn run_mitex_coverage() {
    eprintln!("=== mitex coverage report ===");
    let mut ok = 0usize;
    let mut fail = 0usize;
    for eq in SAMPLE_EQUATIONS {
        match mitex::convert_math(eq, None) {
            Ok(out) => {
                ok += 1;
                eprintln!("OK    | {:60} → {}", trunc(eq, 60), trunc(&out, 80));
            }
            Err(e) => {
                fail += 1;
                eprintln!("FAIL  | {:60} → {}", trunc(eq, 60), trunc(&e, 80));
            }
        }
    }
    eprintln!(
        "{}/{} ({:.0}%) translated cleanly",
        ok,
        ok + fail,
        100.0 * ok as f64 / (ok + fail) as f64
    );
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n - 1).collect();
        out.push('…');
        out
    }
}

/// Trivial markdown→typst converter for the M0.5 timing run. Handles
/// only paragraphs, `#`/`##` headings, and `$...$` math (delegated to
/// mitex). NOT a real translator — M2 owns that. This is just enough
/// to measure typst compile time on a real-size chapter so we can
/// answer "does compile scale" before investing in M1+M2.
fn trivial_md_to_typst(md: &str) -> String {
    let mut out = String::with_capacity(md.len() * 11 / 10);
    for line in md.lines() {
        let trimmed = line.trim_end();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str("= ");
            out.push_str(&escape_inline(rest));
            out.push('\n');
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str("== ");
            out.push_str(&escape_inline(rest));
            out.push('\n');
        } else if trimmed.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&escape_inline(trimmed));
            out.push('\n');
        }
    }
    out
}

/// Escape typst special chars in a prose run, then translate inline
/// `$...$` runs through mitex. Only `$` is handled here — we strip
/// math blocks and convert what's inside.
fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            // grab everything up to the next bare `$`
            let mut math = String::new();
            let mut closed = false;
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == '$' {
                    closed = true;
                    break;
                }
                math.push(next);
            }
            // M0.5: strip math runs entirely so the compile can
            // complete without the @preview/mitex helper package
            // (which our sandboxed World refuses to load). M2 will
            // ship the mitex helpers as part of our preamble so the
            // real translation path works.
            let _ = closed;
            let _ = math;
            out.push_str("[math]");
        } else if matches!(c, '#' | '*' | '_' | '<' | '[' | ']' | '@' | '`' | '\\') {
            out.push('\\');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

fn run_chapter_timing(path: &str) {
    use marlos_lib::typesetter::book_config::BookConfig;
    use marlos_lib::typesetter::typst_emit::markdown_to_typst;
    use marlos_lib::typesetter::typst_world::BookWorld;
    eprintln!("=== chapter timing: {} ===", path);
    let t_read = Instant::now();
    let md = std::fs::read_to_string(path).expect("read md");
    let read_elapsed = t_read.elapsed();
    eprintln!("Read:        {:?} ({} bytes)", read_elapsed, md.len());

    let root = std::path::PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir());

    // Build a default BookConfig anchored at the manuscript dir.
    // M2 doesn't read book.toml; M5 will plug this into the
    // production command.
    let mut config = BookConfig {
        doc_type: Default::default(),
        book: Default::default(),
        trim: Default::default(),
        typography: Default::default(),
        export: Default::default(),
        files: vec![std::path::PathBuf::from(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main.md".to_string())],
        root_dir: root.clone(),
        config_path: root.join("book.toml"),
    };
    let _ = &mut config;

    let t_translate = Instant::now();
    let typst_src = markdown_to_typst(&md, &config);
    let translate_elapsed = t_translate.elapsed();
    eprintln!("md → typst:  {:?} ({} bytes)", translate_elapsed, typst_src.len());

    let world = BookWorld::new(typst_src, root);

    let t_compile = Instant::now();
    let warned = typst::compile::<PagedDocument>(&world);
    let compile_elapsed = t_compile.elapsed();
    let doc = match warned.output {
        Ok(d) => d,
        Err(errs) => {
            for e in errs.iter().take(5) {
                eprintln!("COMPILE ERROR: {}", e.message);
            }
            return;
        }
    };
    eprintln!(
        "{} warning(s)",
        warned.warnings.len()
    );
    // Print the first few unique warnings so we know what to fix.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for w in warned.warnings.iter() {
        let key = w.message.to_string();
        if seen.insert(key.clone()) {
            eprintln!("  WARN: {}", key);
            if seen.len() >= 8 {
                break;
            }
        }
    }

    let t_pdf = Instant::now();
    let pdf_bytes = match typst_pdf::pdf(&doc, &PdfOptions::default()) {
        Ok(b) => b,
        Err(errs) => {
            for e in errs.iter().take(3) {
                eprintln!("PDF ERROR: {}", e.message);
            }
            return;
        }
    };
    let pdf_elapsed = t_pdf.elapsed();

    let out_path = std::env::current_dir()
        .unwrap_or_default()
        .join("target")
        .join("typst_spike_chapter.pdf");
    std::fs::write(&out_path, &pdf_bytes).expect("write pdf");

    eprintln!();
    eprintln!("Pages:       {}", doc.pages.len());
    eprintln!("PDF size:    {} bytes ({:.1} KB)", pdf_bytes.len(), pdf_bytes.len() as f64 / 1024.0);
    eprintln!("Compile:     {:?}", compile_elapsed);
    eprintln!("PDF encode:  {:?}", pdf_elapsed);
    eprintln!("Output:      {}", out_path.display());
}

/// M1 verification: hand the production `BookWorld` an italic Greek
/// source and confirm the resulting PDF embeds "EB Garamond". This
/// exercises the same code path that `lib`-side unit tests would,
/// but bypasses the unrelated pre-existing breakage in the lib's
/// test target.
fn run_m1_world_check() {
    use marlos_lib::typesetter::typst_world::BookWorld;

    let src = r#"
#set page(width: 3in, height: 1in, margin: 0.2in)
#set text(font: "EB Garamond", size: 16pt)
#text(style: "italic")[αβγ — Greek italic glyph subset]
"#;
    let tmp = std::env::temp_dir().join("marlos-typst-m1-check");
    let _ = std::fs::create_dir_all(&tmp);
    let world = BookWorld::new(src.to_string(), &tmp);

    let t_compile = Instant::now();
    let warned = typst::compile::<PagedDocument>(&world);
    let compile_elapsed = t_compile.elapsed();
    let doc = match warned.output {
        Ok(d) => d,
        Err(errs) => {
            for e in errs.iter().take(5) {
                eprintln!("M1 COMPILE ERROR: {}", e.message);
            }
            std::process::exit(2);
        }
    };

    let pdf_bytes = typst_pdf::pdf(&doc, &PdfOptions::default())
        .expect("pdf encode");
    let hay = String::from_utf8_lossy(&pdf_bytes);
    let has_ebg = hay.contains("EBGaramond") || hay.contains("EB Garamond");

    eprintln!("=== M1 world check ===");
    eprintln!("Pages:       {}", doc.pages.len());
    eprintln!("PDF size:    {} bytes", pdf_bytes.len());
    eprintln!("Compile:     {:?}", compile_elapsed);
    eprintln!("EB Garamond in font dict: {}", if has_ebg { "YES" } else { "NO" });

    // Sandbox check: a source that imports @preview/cetz should fail.
    let pkg_src = "#import \"@preview/cetz:0.2\"\n";
    let pkg_world = BookWorld::new(pkg_src.to_string(), ".");
    let pkg_result = typst::compile::<PagedDocument>(&pkg_world);
    let rejected = pkg_result.output.is_err();
    eprintln!("Package-import rejection (sandbox): {}", if rejected { "PASS" } else { "FAIL" });

    if !has_ebg || !rejected {
        std::process::exit(1);
    }
}

/// M2.10 fixture: end-to-end exercise of the markdown → typst → PDF
/// pipeline against a small synthetic book that triggers every
/// feature class (chapters, math anchor, inline math, citations,
/// word anchors).
fn run_m2_fixture() {
    use marlos_lib::typesetter::book_config::BookConfig;
    use marlos_lib::typesetter::typst_emit::markdown_to_typst;
    use marlos_lib::typesetter::typst_world::BookWorld;

    const MD: &str = r##"# Author's Note

This book covers several themes. The opening note runs across
multiple paragraphs to test prose flow.

A second paragraph follows for variety.

# Chapter 1: Beginnings

In the beginning there was confusion. Some sources [CITE: Wallace, *The Beginning* (2024), p. 12] disagreed about the start.

> **Math Anchor — Energy-mass equivalence**: Einstein's famous relation.
>
> $E = m c^2$
>
> The constant `c` is the speed of light in vacuum.

A second paragraph in chapter one.

# Chapter 2: Middles

The middle of the book covers more ground [CITE: A second reference].

# Appendix

Auxiliary material.
"##;

    let tmp = std::env::temp_dir().join("marlos-typst-m2-fixture");
    let _ = std::fs::create_dir_all(&tmp);
    let mut config = BookConfig {
        doc_type: Default::default(),
        book: Default::default(),
        trim: Default::default(),
        typography: Default::default(),
        export: Default::default(),
        files: vec!["main.md".into()],
        root_dir: tmp.clone(),
        config_path: tmp.join("book.toml"),
    };
    config.export.word_anchors = true;

    let typst_src = markdown_to_typst(MD, &config);
    eprintln!("typst src ({} bytes):\n{}\n---", typst_src.len(), &typst_src[..typst_src.len().min(800)]);

    let world = BookWorld::new(typst_src, tmp);
    let t_compile = Instant::now();
    let warned = typst::compile::<PagedDocument>(&world);
    let compile_elapsed = t_compile.elapsed();
    let doc = match warned.output {
        Ok(d) => d,
        Err(errs) => {
            for e in errs.iter().take(5) {
                eprintln!("M2 COMPILE ERROR: {}", e.message);
            }
            std::process::exit(1);
        }
    };
    let pdf_bytes = typst_pdf::pdf(&doc, &PdfOptions::default())
        .expect("pdf encode");
    let hay = String::from_utf8_lossy(&pdf_bytes);

    eprintln!("=== M2 fixture result ===");
    eprintln!("Pages:           {}", doc.pages.len());
    eprintln!("PDF size:        {} bytes", pdf_bytes.len());
    eprintln!("Compile:         {:?}", compile_elapsed);
    eprintln!("Warnings:        {}", warned.warnings.len());
    eprintln!("EBGaramond in PDF: {}", hay.contains("EBGaramond") || hay.contains("EB Garamond"));

    let out_path = std::env::current_dir()
        .unwrap_or_default()
        .join("target")
        .join("typst_spike_m2_fixture.pdf");
    std::fs::write(&out_path, &pdf_bytes).expect("write pdf");
    eprintln!("Output:          {}", out_path.display());
}

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--mitex") => {
            run_mitex_coverage();
            return;
        }
        Some("--chapter") => {
            let path = args.next().expect("--chapter requires a path");
            run_chapter_timing(&path);
            return;
        }
        Some("--m1") => {
            run_m1_world_check();
            return;
        }
        Some("--m2") => {
            run_m2_fixture();
            return;
        }
        _ => {}
    }
    let t_total = Instant::now();

    // A small typst document exercising chapters, headings, italic,
    // and inline math — enough to validate the basic emitter path
    // before M2 builds the real markdown-to-typst translator.
    let typst_src = r#"
#set page(width: 6in, height: 9in, margin: (inside: 1in, outside: 0.875in, top: 0.75in, bottom: 0.75in))
#set text(font: "EB Garamond", size: 11pt)
#set par(leading: 0.5em, justify: true)

= Chapter 1

#lorem(40)

== A subsection

Here is some _italic_ prose. (Math omitted; needs the Math font.)

#lorem(30)

#pagebreak()

= Chapter 2

#lorem(60)

#pagebreak()

#outline()
"#;

    let world = SpikeWorld::new(typst_src.to_string());
    eprintln!("World built: {} font(s) loaded", world.fonts.len());
    for f in &world.fonts {
        let info = f.info();
        eprintln!(
            "  font: family={:?} variant={:?} flags={:?}",
            info.family, info.variant, info.flags
        );
    }

    let t_compile = Instant::now();
    let warned = typst::compile::<PagedDocument>(&world);
    let compile_elapsed = t_compile.elapsed();

    let doc = match warned.output {
        Ok(d) => d,
        Err(errs) => {
            for e in errs.iter() {
                eprintln!("COMPILE ERROR: {}", e.message);
            }
            std::process::exit(1);
        }
    };
    for w in warned.warnings.iter() {
        eprintln!("WARN: {}", w.message);
    }

    let t_pdf = Instant::now();
    let pdf_bytes = match typst_pdf::pdf(&doc, &PdfOptions::default()) {
        Ok(b) => b,
        Err(errs) => {
            for e in errs.iter() {
                eprintln!("PDF ERROR: {}", e.message);
            }
            std::process::exit(1);
        }
    };
    let pdf_elapsed = t_pdf.elapsed();

    let out_path = std::env::current_dir()
        .unwrap_or_default()
        .join("target")
        .join("typst_spike.pdf");
    std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    std::fs::write(&out_path, &pdf_bytes).expect("write pdf");

    let total = t_total.elapsed();
    eprintln!();
    eprintln!("=== typst spike result ===");
    eprintln!("Pages:       {}", doc.pages.len());
    eprintln!("PDF size:    {} bytes ({:.1} KB)", pdf_bytes.len(), pdf_bytes.len() as f64 / 1024.0);
    eprintln!("Compile:     {:?}", compile_elapsed);
    eprintln!("PDF encode:  {:?}", pdf_elapsed);
    eprintln!("Total:       {:?}", total);
    eprintln!("Output:      {}", out_path.display());
}
