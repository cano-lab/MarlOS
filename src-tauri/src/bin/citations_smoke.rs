//! Smoke test for the citation transformer against the actual manuscript.
//!
//! Usage: cargo run --features tauri-app --bin citations_smoke -- <markdown-path>

use std::path::PathBuf;

use marlos_lib::typesetter::transform_citations;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .expect("usage: citations_smoke <markdown-path>");
    let md = std::fs::read_to_string(&path).expect("read manuscript");

    println!("== Source ==");
    println!("  bytes: {}", md.len());
    println!("  lines: {}", md.lines().count());

    let r = transform_citations(&md);

    println!("\n== Result ==");
    println!("  notes total:        {}", r.note_count);
    println!("  chapters w/ notes:  {}", r.chapters_with_notes);
    println!("  warnings:           {}", r.warnings.len());
    println!("  output bytes:       {}", r.transformed.len());

    if !r.warnings.is_empty() {
        println!("\n== Warnings ==");
        for w in r.warnings.iter().take(20) {
            println!("  {}", w);
        }
        if r.warnings.len() > 20 {
            println!("  ... ({} more)", r.warnings.len() - 20);
        }
    }

    // Spot-check: first three superscript references and the start of
    // the Notes section.
    println!("\n== Sample superscripts ==");
    for line in r.transformed.lines().filter(|l| l.contains("sup class=\"note-ref\"")).take(3) {
        let trimmed = if line.len() > 200 { &line[..200] } else { line };
        println!("  {trimmed}{}", if line.len() > 200 { "..." } else { "" });
    }

    println!("\n== Notes section preview ==");
    if let Some(idx) = r.transformed.find("\n# Notes\n") {
        let snippet = &r.transformed[idx..(idx + 600).min(r.transformed.len())];
        for line in snippet.lines().take(15) {
            println!("  {}", line);
        }
    } else {
        println!("  (no Notes section emitted)");
    }

    // Dump transformed markdown for inspection / pandoc run.
    let dump = std::env::temp_dir().join("marlos-citations-smoke-out.md");
    std::fs::write(&dump, &r.transformed).expect("dump");
    println!("\n== Transformed markdown written to: {} ==", dump.display());
}
