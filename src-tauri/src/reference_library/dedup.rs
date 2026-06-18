//! Duplicate detection for the reference library.

use super::reference::Reference;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DuplicateMatch {
    pub reference_id: String,
    pub existing_id: String,
    pub confidence: f64,
    pub reason: String,
}

/// Check a new reference against existing ones for duplicates
pub fn find_duplicates(new_ref: &Reference, existing: &[Reference]) -> Vec<DuplicateMatch> {
    let mut matches = Vec::new();

    for existing_ref in existing {
        if existing_ref.id == new_ref.id { continue; }

        // Exact DOI match = definite duplicate
        if let (Some(new_doi), Some(existing_doi)) = (&new_ref.doi, &existing_ref.doi) {
            if !new_doi.is_empty() && normalize_doi(new_doi) == normalize_doi(existing_doi) {
                matches.push(DuplicateMatch {
                    reference_id: new_ref.id.clone(),
                    existing_id: existing_ref.id.clone(),
                    confidence: 1.0,
                    reason: format!("Same DOI: {}", new_doi),
                });
                continue;
            }
        }

        // Exact ISBN match
        if let (Some(new_isbn), Some(existing_isbn)) = (&new_ref.isbn, &existing_ref.isbn) {
            if !new_isbn.is_empty() && normalize_isbn(new_isbn) == normalize_isbn(existing_isbn) {
                matches.push(DuplicateMatch {
                    reference_id: new_ref.id.clone(),
                    existing_id: existing_ref.id.clone(),
                    confidence: 1.0,
                    reason: format!("Same ISBN: {}", new_isbn),
                });
                continue;
            }
        }

        // Title similarity + author overlap = likely duplicate
        let title_sim = title_similarity(&new_ref.title, &existing_ref.title);
        let author_overlap = author_overlap_score(&new_ref.authors, &existing_ref.authors);

        if title_sim > 0.85 && author_overlap > 0.5 {
            let confidence = (title_sim * 0.6 + author_overlap * 0.4).min(0.99);
            matches.push(DuplicateMatch {
                reference_id: new_ref.id.clone(),
                existing_id: existing_ref.id.clone(),
                confidence,
                reason: format!(
                    "Similar title ({:.0}%) and overlapping authors ({:.0}%)",
                    title_sim * 100.0,
                    author_overlap * 100.0
                ),
            });
        }
    }

    matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    matches
}

fn normalize_doi(doi: &str) -> String {
    doi.trim()
        .to_lowercase()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .to_string()
}

fn normalize_isbn(isbn: &str) -> String {
    isbn.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

fn title_similarity(a: &str, b: &str) -> f64 {
    let a_norm = normalize_title(a);
    let b_norm = normalize_title(b);

    if a_norm == b_norm { return 1.0; }
    if a_norm.is_empty() || b_norm.is_empty() { return 0.0; }

    // Word-level Jaccard similarity
    let a_words: std::collections::HashSet<&str> = a_norm.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b_norm.split_whitespace().collect();

    let intersection = a_words.intersection(&b_words).count() as f64;
    let union = a_words.union(&b_words).count() as f64;

    if union == 0.0 { 0.0 } else { intersection / union }
}

fn normalize_title(title: &str) -> String {
    title.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn author_overlap_score(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }

    let a_last: std::collections::HashSet<String> = a.iter()
        .filter_map(|name| name.split_whitespace().last())
        .map(|s| s.to_lowercase())
        .collect();

    let b_last: std::collections::HashSet<String> = b.iter()
        .filter_map(|name| name.split_whitespace().last())
        .map(|s| s.to_lowercase())
        .collect();

    let intersection = a_last.intersection(&b_last).count() as f64;
    let min_len = a_last.len().min(b_last.len()) as f64;

    if min_len == 0.0 { 0.0 } else { intersection / min_len }
}
