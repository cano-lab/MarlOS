//! Phase B smoke test against the actual manuscript.
//!
//! Runs the same pipeline the Tauri command path runs, but as a standalone
//! binary so we can validate before wiring up the UI. Pre-existing test
//! compile errors elsewhere in the lib block `cargo test`, so this serves
//! as the integration test for the typesetter module specifically.
//!
//! Usage:
//!   cargo run --bin typesetter_smoke --features tauri-app -- <markdown-path>

use std::path::PathBuf;

use marlos_lib::typesetter::{
    analyze_structure, pandoc::PandocConvertOptions, BookConfig, PandocConverter, SectionKind,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let markdown = args
        .get(1)
        .cloned()
        .expect("usage: typesetter_smoke <markdown-path>");
    let path = PathBuf::from(&markdown);

    println!("== Pandoc probe ==");
    let probe = PandocConverter::probe().await;
    println!("  available: {}", probe.available);
    println!("  path:      {:?}", probe.path);
    println!("  version:   {:?}", probe.version);
    if !probe.available {
        eprintln!("pandoc unavailable: {:?}", probe.error);
        std::process::exit(1);
    }

    println!("\n== Init book.toml ==");
    let toml_path =
        BookConfig::init_from_markdown(&path).expect("init_from_markdown");
    println!("  written: {}", toml_path.display());

    println!("\n== Load book.toml ==");
    let config = BookConfig::load(&toml_path).expect("load");
    println!("  files: {:?}", config.files);
    println!("  trim:  {}", config.trim.size);

    println!("\n== Pandoc convert ==");
    let mut combined = String::new();
    for f in config.resolved_files() {
        let r = PandocConverter::convert_file(&f, &PandocConvertOptions::default())
            .await
            .expect("convert_file");
        combined.push_str(&r.html);
    }
    println!("  bytes: {}", combined.len());

    println!("\n== Structure analyze ==");
    let s = analyze_structure(&combined).expect("analyze");
    println!(
        "  front: {}  chapters: {}  interludes: {}  back: {}",
        s.structure.front_matter_count,
        s.structure.chapter_count,
        s.structure.interlude_count,
        s.structure.back_matter_count,
    );

    println!("\n== Sections ==");
    for sec in &s.structure.sections {
        let label = match sec.kind {
            SectionKind::FrontMatter => "FRONT",
            SectionKind::Chapter => "CHAP ",
            SectionKind::Interlude => "INTER",
            SectionKind::BackMatter => "BACK ",
        };
        let num = sec
            .number
            .map(|n| format!("{n:>3}"))
            .unwrap_or_else(|| "   ".to_string());
        println!("  [{label}] {num}  {}", sec.h1_raw.trim());
    }

    println!("\n== Sample enriched section tags ==");
    for line in s.enriched_html.lines().filter(|l| l.contains("<section ")).take(5) {
        // truncate for readability
        let trimmed = if line.len() > 200 { &line[..200] } else { line };
        println!("  {trimmed}{}", if line.len() > 200 { "..." } else { "" });
    }

    println!("\nOK");
}
