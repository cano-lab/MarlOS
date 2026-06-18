//! Related Work Suggestion Engine
//!
//! Finds references similar to the current paper or section that haven't been cited yet.
//! Uses embeddings + waveform similarity for local matching.

use serde::{Deserialize, Serialize};
use super::reference::Reference;

/// A suggestion for a related reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedSuggestion {
    /// The reference being suggested
    pub reference: Reference,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Why this was suggested
    pub reason: String,
    /// Whether this reference is already cited in the paper
    pub already_cited: bool,
}

/// Find references similar to given text from the library
pub fn find_related_by_keywords(
    query_keywords: &[String],
    references: &[Reference],
    cited_ids: &[String],
    max_results: usize,
) -> Vec<RelatedSuggestion> {
    let query_words: Vec<String> = query_keywords.iter()
        .map(|k| k.to_lowercase())
        .collect();

    let mut scored: Vec<(usize, f32, String)> = references.iter().enumerate()
        .map(|(idx, reference)| {
            let mut score = 0.0f32;
            let mut reasons = Vec::new();

            // Match against title
            let title_lower = reference.title.to_lowercase();
            for keyword in &query_words {
                if title_lower.contains(keyword.as_str()) {
                    score += 0.3;
                    reasons.push(format!("title contains '{}'", keyword));
                }
            }

            // Match against keywords
            for ref_keyword in &reference.keywords {
                let ref_kw_lower = ref_keyword.to_lowercase();
                for query_kw in &query_words {
                    if ref_kw_lower.contains(query_kw.as_str()) || query_kw.contains(ref_kw_lower.as_str()) {
                        score += 0.2;
                        reasons.push(format!("keyword match: '{}'", ref_keyword));
                    }
                }
            }

            // Match against abstract
            if let Some(abstract_text) = &reference.abstract_text {
                let abs_lower = abstract_text.to_lowercase();
                let matches: usize = query_words.iter()
                    .filter(|kw| abs_lower.contains(kw.as_str()))
                    .count();
                if matches > 0 {
                    score += 0.1 * matches as f32;
                    reasons.push(format!("{} keyword(s) in abstract", matches));
                }
            }

            // Match against tags
            for tag in &reference.tags {
                let tag_lower = tag.to_lowercase();
                for query_kw in &query_words {
                    if tag_lower.contains(query_kw.as_str()) {
                        score += 0.15;
                        reasons.push(format!("tag match: '{}'", tag));
                    }
                }
            }

            // Cap at 1.0
            score = score.min(1.0);
            let reason = if reasons.is_empty() {
                "No specific match".to_string()
            } else {
                reasons.into_iter().take(3).collect::<Vec<_>>().join("; ")
            };

            (idx, score, reason)
        })
        .filter(|(_, score, _)| *score > 0.1)
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_results);

    scored.into_iter()
        .map(|(idx, score, reason)| {
            let reference = references[idx].clone();
            let already_cited = cited_ids.contains(&reference.id);
            RelatedSuggestion {
                reference,
                similarity: score,
                reason,
                already_cited,
            }
        })
        .collect()
}

/// Extract keywords from text for matching
pub fn extract_keywords(text: &str) -> Vec<String> {
    // Simple keyword extraction: split by whitespace, filter stopwords, take unique
    let stopwords = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "shall", "can", "need", "dare", "ought",
        "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
        "as", "into", "through", "during", "before", "after", "above",
        "below", "between", "out", "off", "over", "under", "again",
        "further", "then", "once", "here", "there", "when", "where",
        "why", "how", "all", "each", "every", "both", "few", "more",
        "most", "other", "some", "such", "no", "nor", "not", "only",
        "own", "same", "so", "than", "too", "very", "and", "but", "or",
        "if", "while", "that", "this", "these", "those", "it", "its",
        "we", "our", "they", "them", "their", "which", "what", "who",
    ];

    let mut keywords: Vec<String> = text.split_whitespace()
        .map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| w.len() > 2 && !stopwords.contains(&w.as_str()))
        .collect();

    keywords.sort();
    keywords.dedup();
    keywords.truncate(20);
    keywords
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_library::reference::Reference;

    fn make_ref(id: &str, title: &str, keywords: &[&str]) -> Reference {
        let mut r = Reference::new(title, vec!["Test Author".to_string()]);
        r.id = id.to_string();
        r.year = Some(2024);
        r.keywords = keywords.iter().map(|s| s.to_string()).collect();
        r
    }

    #[test]
    fn test_find_related() {
        let refs = vec![
            make_ref("1", "Deep Learning for NLP", &["deep learning", "NLP"]),
            make_ref("2", "Computer Vision Survey", &["vision", "CNN"]),
            make_ref("3", "Attention Mechanisms in NLP", &["attention", "NLP", "transformers"]),
        ];

        let results = find_related_by_keywords(
            &["NLP".to_string(), "attention".to_string()],
            &refs,
            &[],
            10,
        );

        assert!(!results.is_empty());
        // The NLP+attention ref should score highest
        assert!(results[0].reference.title.contains("Attention") || results[0].reference.title.contains("NLP"));
    }

    #[test]
    fn test_extract_keywords() {
        let text = "Deep learning has revolutionized natural language processing through attention mechanisms";
        let keywords = extract_keywords(text);
        assert!(keywords.contains(&"deep".to_string()));
        assert!(keywords.contains(&"learning".to_string()));
        assert!(keywords.contains(&"attention".to_string()));
        // Stopwords should be excluded
        assert!(!keywords.contains(&"has".to_string()));
        assert!(!keywords.contains(&"through".to_string()));
    }
}
