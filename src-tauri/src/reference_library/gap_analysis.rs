//! Gap Analysis for Research Papers
//!
//! Analyzes a collection of references to identify gaps in coverage:
//! - Temporal gaps (years not covered)
//! - Methodological gaps (missing methods)
//! - Conceptual gaps (uncovered subtopics)

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::reference::Reference;

/// Result of a gap analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapAnalysisResult {
    /// Temporal gaps (years with few or no publications)
    pub temporal_gaps: Vec<TemporalGap>,
    /// Keyword clusters and their coverage
    pub keyword_coverage: Vec<KeywordCluster>,
    /// Overall statistics
    pub stats: AnalysisStats,
    /// Suggested search queries to fill gaps
    pub suggested_queries: Vec<String>,
}

/// A gap in temporal coverage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalGap {
    /// Year range
    pub from_year: i32,
    pub to_year: i32,
    /// How many references exist in this range
    pub count: usize,
    /// Severity: "none", "low", "medium", "high"
    pub severity: String,
}

/// A cluster of related keywords and their coverage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordCluster {
    /// Primary keyword/theme
    pub keyword: String,
    /// Number of references mentioning this
    pub reference_count: usize,
    /// Coverage level: "well-covered", "moderate", "sparse", "missing"
    pub coverage: String,
}

/// Overall analysis statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisStats {
    pub total_references: usize,
    pub year_range: Option<(i32, i32)>,
    pub unique_keywords: usize,
    pub unique_authors: usize,
    pub avg_refs_per_year: f32,
}

/// Perform gap analysis on a set of references
pub fn analyze_gaps(references: &[Reference], topic_keywords: &[String]) -> GapAnalysisResult {
    let temporal_gaps = analyze_temporal(references);
    let keyword_coverage = analyze_keywords(references, topic_keywords);
    let stats = compute_stats(references);
    let suggested_queries = generate_suggestions(&temporal_gaps, &keyword_coverage, topic_keywords);

    GapAnalysisResult {
        temporal_gaps,
        keyword_coverage,
        stats,
        suggested_queries,
    }
}

fn analyze_temporal(references: &[Reference]) -> Vec<TemporalGap> {
    let years: Vec<i32> = references.iter()
        .filter_map(|r| r.year)
        .collect();

    if years.is_empty() {
        return Vec::new();
    }

    let min_year = *years.iter().min().unwrap();
    let max_year = *years.iter().max().unwrap();

    // Count references per year
    let mut year_counts: HashMap<i32, usize> = HashMap::new();
    for &y in &years {
        *year_counts.entry(y).or_insert(0) += 1;
    }

    let avg = years.len() as f32 / (max_year - min_year + 1).max(1) as f32;

    let mut gaps = Vec::new();
    let mut gap_start: Option<i32> = None;

    for year in min_year..=max_year {
        let count = *year_counts.get(&year).unwrap_or(&0);
        let is_sparse = (count as f32) < avg * 0.3;

        if is_sparse {
            if gap_start.is_none() {
                gap_start = Some(year);
            }
        } else if let Some(start) = gap_start {
            let gap_len = year - start;
            let severity = match gap_len {
                0..=1 => "low",
                2..=3 => "medium",
                _ => "high",
            };
            // Count total refs in gap
            let gap_count: usize = (start..year).map(|y| year_counts.get(&y).unwrap_or(&0)).sum();
            gaps.push(TemporalGap {
                from_year: start,
                to_year: year - 1,
                count: gap_count,
                severity: severity.to_string(),
            });
            gap_start = None;
        }
    }

    // Close trailing gap
    if let Some(start) = gap_start {
        let gap_count: usize = (start..=max_year).map(|y| year_counts.get(&y).unwrap_or(&0)).sum();
        gaps.push(TemporalGap {
            from_year: start,
            to_year: max_year,
            count: gap_count,
            severity: "medium".to_string(),
        });
    }

    gaps
}

fn analyze_keywords(references: &[Reference], topic_keywords: &[String]) -> Vec<KeywordCluster> {
    // Count occurrences of each keyword across all references
    let mut keyword_freq: HashMap<String, usize> = HashMap::new();

    for reference in references {
        for kw in &reference.keywords {
            let normalized = kw.to_lowercase();
            *keyword_freq.entry(normalized).or_insert(0) += 1;
        }

        // Also check title words for topic keywords
        let title_lower = reference.title.to_lowercase();
        for topic_kw in topic_keywords {
            let topic_lower = topic_kw.to_lowercase();
            if title_lower.contains(&topic_lower) {
                *keyword_freq.entry(topic_lower).or_insert(0) += 1;
            }
        }
    }

    // Build clusters from topic keywords and discovered keywords
    let mut clusters: Vec<KeywordCluster> = Vec::new();

    // Topic keywords first (check coverage)
    for kw in topic_keywords {
        let normalized = kw.to_lowercase();
        let count = *keyword_freq.get(&normalized).unwrap_or(&0);
        let coverage = match count {
            0 => "missing",
            1..=2 => "sparse",
            3..=5 => "moderate",
            _ => "well-covered",
        };
        clusters.push(KeywordCluster {
            keyword: kw.clone(),
            reference_count: count,
            coverage: coverage.to_string(),
        });
    }

    // Add top discovered keywords not already in topic list
    let topic_lower: Vec<String> = topic_keywords.iter().map(|k| k.to_lowercase()).collect();
    let mut extra: Vec<_> = keyword_freq.iter()
        .filter(|(k, _)| !topic_lower.contains(k))
        .collect();
    extra.sort_by(|a, b| b.1.cmp(a.1));

    for (kw, &count) in extra.iter().take(10) {
        let coverage = match count {
            1..=2 => "sparse",
            3..=5 => "moderate",
            _ => "well-covered",
        };
        clusters.push(KeywordCluster {
            keyword: kw.to_string(),
            reference_count: count,
            coverage: coverage.to_string(),
        });
    }

    clusters
}

fn compute_stats(references: &[Reference]) -> AnalysisStats {
    let years: Vec<i32> = references.iter().filter_map(|r| r.year).collect();
    let year_range = if years.is_empty() {
        None
    } else {
        Some((*years.iter().min().unwrap(), *years.iter().max().unwrap()))
    };

    let mut all_keywords = std::collections::HashSet::new();
    let mut all_authors = std::collections::HashSet::new();
    for r in references {
        for kw in &r.keywords { all_keywords.insert(kw.to_lowercase()); }
        for a in &r.authors { all_authors.insert(a.to_lowercase()); }
    }

    let span = year_range.map(|(min, max)| (max - min + 1).max(1) as f32).unwrap_or(1.0);

    AnalysisStats {
        total_references: references.len(),
        year_range,
        unique_keywords: all_keywords.len(),
        unique_authors: all_authors.len(),
        avg_refs_per_year: references.len() as f32 / span,
    }
}

fn generate_suggestions(
    temporal_gaps: &[TemporalGap],
    keyword_coverage: &[KeywordCluster],
    topic_keywords: &[String],
) -> Vec<String> {
    let mut suggestions = Vec::new();

    // Suggest searches for temporal gaps
    for gap in temporal_gaps {
        if gap.severity == "high" || gap.severity == "medium" {
            let topic = topic_keywords.first().map(|s| s.as_str()).unwrap_or("topic");
            suggestions.push(format!(
                "{} research {}-{}", topic, gap.from_year, gap.to_year
            ));
        }
    }

    // Suggest searches for sparse/missing keywords
    for cluster in keyword_coverage {
        if cluster.coverage == "missing" || cluster.coverage == "sparse" {
            suggestions.push(format!("{} survey", cluster.keyword));
        }
    }

    suggestions.truncate(10);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ref(year: i32, keywords: &[&str]) -> Reference {
        let mut r = Reference::new(&format!("Paper from {}", year), vec!["Author".to_string()]);
        r.year = Some(year);
        r.keywords = keywords.iter().map(|s| s.to_string()).collect();
        r
    }

    #[test]
    fn test_temporal_gap_detection() {
        let refs = vec![
            make_ref(2018, &["ML"]),
            make_ref(2019, &["ML"]),
            make_ref(2019, &["DL"]),
            // gap: 2020-2022
            make_ref(2023, &["ML"]),
            make_ref(2024, &["ML"]),
            make_ref(2024, &["DL"]),
        ];

        let result = analyze_gaps(&refs, &["ML".to_string()]);
        assert!(!result.temporal_gaps.is_empty());
    }

    #[test]
    fn test_keyword_coverage() {
        let refs = vec![
            make_ref(2024, &["deep learning", "transformers"]),
            make_ref(2024, &["deep learning", "attention"]),
            make_ref(2024, &["reinforcement learning"]),
        ];

        let result = analyze_gaps(
            &refs,
            &["deep learning".to_string(), "GANs".to_string()],
        );

        let dl = result.keyword_coverage.iter().find(|k| k.keyword == "deep learning").unwrap();
        assert!(dl.reference_count >= 2);

        let gans = result.keyword_coverage.iter().find(|k| k.keyword == "GANs").unwrap();
        assert_eq!(gans.coverage, "missing");
    }

    #[test]
    fn test_suggestions() {
        let refs = vec![
            make_ref(2024, &["ML"]),
        ];

        let result = analyze_gaps(
            &refs,
            &["ML".to_string(), "quantum computing".to_string()],
        );

        // Should suggest searching for the missing topic
        assert!(result.suggested_queries.iter().any(|q| q.contains("quantum computing")));
    }
}
