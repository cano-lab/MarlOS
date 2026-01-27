//! Research Provider - Source Collection, Management, and Research Assistance
//!
//! Data structures and utilities for managing research sources.
//! Sources are stored as SemanticObjects with tags for filtering.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use regex::Regex;

// ============================================================================
// Source Types
// ============================================================================

/// Type of source material
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Article,
    Paper,
    Book,
    WebPage,
    Video,
    Podcast,
    Documentation,
    CodeRepository,
    Other(String),
}

impl Default for SourceType {
    fn default() -> Self {
        SourceType::WebPage
    }
}

/// Citation format styles
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CitationStyle {
    APA,
    MLA,
    Chicago,
    Harvard,
    IEEE,
    BibTeX,
}

impl Default for CitationStyle {
    fn default() -> Self {
        CitationStyle::APA
    }
}

// ============================================================================
// Source Struct
// ============================================================================

/// A research source with full metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Unique source ID
    pub id: String,

    /// Source title
    pub title: String,

    /// Original URL or file path
    pub url: Option<String>,

    /// Source type
    pub source_type: SourceType,

    /// Authors
    pub authors: Vec<String>,

    /// Publication date
    pub published_date: Option<String>,

    /// Date accessed/added
    pub accessed_date: DateTime<Utc>,

    /// Full content (if extracted)
    pub content: Option<String>,

    /// AI-generated summary
    pub summary: Option<String>,

    /// Key points extracted by AI
    pub key_points: Vec<String>,

    /// User-defined tags
    pub tags: Vec<String>,

    /// Pre-formatted citations
    pub citations: HashMap<String, String>,

    /// Reliability score (0.0 - 1.0)
    pub reliability_score: Option<f32>,

    /// Notes added by user
    pub notes: Option<String>,

    /// Publisher or website name
    pub publisher: Option<String>,

    /// DOI if academic paper
    pub doi: Option<String>,

    /// ISBN if book
    pub isbn: Option<String>,
}

impl Source {
    pub fn new(title: &str, url: Option<&str>) -> Self {
        Self {
            id: format!("src_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            title: title.to_string(),
            url: url.map(|s| s.to_string()),
            source_type: SourceType::default(),
            authors: Vec::new(),
            published_date: None,
            accessed_date: Utc::now(),
            content: None,
            summary: None,
            key_points: Vec::new(),
            tags: Vec::new(),
            citations: HashMap::new(),
            reliability_score: None,
            notes: None,
            publisher: None,
            doi: None,
            isbn: None,
        }
    }
}

// ============================================================================
// Citation Generator
// ============================================================================

pub struct CitationGenerator;

impl CitationGenerator {
    /// Generate citation in specified style
    pub fn generate(source: &Source, style: CitationStyle) -> String {
        match style {
            CitationStyle::APA => Self::generate_apa(source),
            CitationStyle::MLA => Self::generate_mla(source),
            CitationStyle::Chicago => Self::generate_chicago(source),
            CitationStyle::Harvard => Self::generate_harvard(source),
            CitationStyle::IEEE => Self::generate_ieee(source),
            CitationStyle::BibTeX => Self::generate_bibtex(source),
        }
    }

    fn generate_apa(source: &Source) -> String {
        let authors = if source.authors.is_empty() {
            String::new()
        } else if source.authors.len() == 1 {
            format!("{}", source.authors[0])
        } else if source.authors.len() == 2 {
            format!("{} & {}", source.authors[0], source.authors[1])
        } else {
            format!("{} et al.", source.authors[0])
        };

        let year = source.published_date.as_ref()
            .map(|d| format!("({})", d))
            .unwrap_or_else(|| "(n.d.)".to_string());

        let title = &source.title;

        let url_part = source.url.as_ref()
            .map(|u| format!(" Retrieved from {}", u))
            .unwrap_or_default();

        if authors.is_empty() {
            format!("{}. {}{}", title, year, url_part).trim().to_string()
        } else {
            format!("{} {}. {}.{}", authors, year, title, url_part).trim().to_string()
        }
    }

    fn generate_mla(source: &Source) -> String {
        let authors = if source.authors.is_empty() {
            String::new()
        } else if source.authors.len() == 1 {
            format!("{}.", source.authors[0])
        } else if source.authors.len() == 2 {
            format!("{}, and {}.", source.authors[0], source.authors[1])
        } else {
            format!("{}, et al.", source.authors[0])
        };

        let title = format!("\"{}\"", source.title);

        let publisher = source.publisher.as_ref()
            .map(|p| format!("{}, ", p))
            .unwrap_or_default();

        let date = source.published_date.as_ref()
            .map(|d| format!("{}", d))
            .unwrap_or_default();

        let url_part = source.url.as_ref()
            .map(|u| format!(" {}", u))
            .unwrap_or_default();

        format!("{} {} {}{}.{}", authors, title, publisher, date, url_part).trim().to_string()
    }

    fn generate_chicago(source: &Source) -> String {
        let authors = if source.authors.is_empty() {
            String::new()
        } else {
            source.authors.join(", ")
        };

        let title = format!("\"{}\"", source.title);

        let date = source.published_date.as_ref()
            .map(|d| format!(" {}", d))
            .unwrap_or_default();

        let url_part = source.url.as_ref()
            .map(|u| format!(" {}", u))
            .unwrap_or_default();

        if authors.is_empty() {
            format!("{}.{}.{}", title, date, url_part).trim().to_string()
        } else {
            format!("{}. {}.{}.{}", authors, title, date, url_part).trim().to_string()
        }
    }

    fn generate_harvard(source: &Source) -> String {
        // Similar to APA
        Self::generate_apa(source)
    }

    fn generate_ieee(source: &Source) -> String {
        let authors = if source.authors.is_empty() {
            String::new()
        } else {
            source.authors.iter()
                .map(|a| {
                    let parts: Vec<&str> = a.split_whitespace().collect();
                    if parts.len() >= 2 {
                        format!("{}. {}", parts[0].chars().next().unwrap_or('?'), parts.last().unwrap_or(&""))
                    } else {
                        a.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let title = format!("\"{}\"", source.title);

        let date = source.published_date.as_ref()
            .map(|d| format!(", {}", d))
            .unwrap_or_default();

        let url_part = source.url.as_ref()
            .map(|u| format!(". [Online]. Available: {}", u))
            .unwrap_or_default();

        format!("{}, {}{}{}", authors, title, date, url_part).trim().to_string()
    }

    fn generate_bibtex(source: &Source) -> String {
        let entry_type = match source.source_type {
            SourceType::Article => "article",
            SourceType::Paper => "article",
            SourceType::Book => "book",
            SourceType::WebPage => "misc",
            SourceType::Documentation => "manual",
            _ => "misc",
        };

        let key = source.title.chars()
            .filter(|c| c.is_alphanumeric())
            .take(20)
            .collect::<String>()
            .to_lowercase();

        let authors = source.authors.join(" and ");
        let year = source.published_date.as_ref()
            .and_then(|d| d.split('-').next())
            .unwrap_or("n.d.");

        let mut entries = vec![
            format!("  title = {{{}}}", source.title),
        ];

        if !authors.is_empty() {
            entries.push(format!("  author = {{{}}}", authors));
        }
        entries.push(format!("  year = {{{}}}", year));

        if let Some(url) = &source.url {
            entries.push(format!("  url = {{{}}}", url));
        }
        if let Some(doi) = &source.doi {
            entries.push(format!("  doi = {{{}}}", doi));
        }
        if let Some(publisher) = &source.publisher {
            entries.push(format!("  publisher = {{{}}}", publisher));
        }

        format!("@{}{{{},\n{}\n}}", entry_type, key, entries.join(",\n"))
    }

    /// Generate all citation formats for a source
    pub fn generate_all(source: &Source) -> HashMap<String, String> {
        let mut citations = HashMap::new();
        citations.insert("apa".to_string(), Self::generate(source, CitationStyle::APA));
        citations.insert("mla".to_string(), Self::generate(source, CitationStyle::MLA));
        citations.insert("chicago".to_string(), Self::generate(source, CitationStyle::Chicago));
        citations.insert("harvard".to_string(), Self::generate(source, CitationStyle::Harvard));
        citations.insert("ieee".to_string(), Self::generate(source, CitationStyle::IEEE));
        citations.insert("bibtex".to_string(), Self::generate(source, CitationStyle::BibTeX));
        citations
    }
}

// ============================================================================
// Web Content Fetcher
// ============================================================================

pub struct WebFetcher;

impl WebFetcher {
    /// Fetch and extract content from a URL
    pub async fn fetch(url: &str) -> Result<FetchedContent, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("MarlOS Research Agent/1.0")
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let response = client.get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch URL: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("HTTP error: {}", status));
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let html = response.text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Extract metadata and content
        let metadata = Self::extract_metadata(&html, url);
        let text_content = Self::extract_text(&html);

        Ok(FetchedContent {
            url: url.to_string(),
            html,
            text: text_content,
            metadata,
            content_type,
        })
    }

    fn extract_metadata(html: &str, url: &str) -> ContentMetadata {
        let mut metadata = ContentMetadata::default();

        // Extract title from <title> tag
        let title_re = Regex::new(r"<title[^>]*>([^<]+)</title>").ok();
        if let Some(re) = title_re {
            if let Some(caps) = re.captures(html) {
                metadata.title = Some(Self::decode_html_entities(&caps[1]).trim().to_string());
            }
        }

        // Extract meta tags
        let meta_re = Regex::new(r#"<meta\s+(?:name|property)=["']([^"']+)["']\s+content=["']([^"']*)["']"#).ok();
        if let Some(re) = meta_re {
            for caps in re.captures_iter(html) {
                let name = caps[1].to_lowercase();
                let content = Self::decode_html_entities(&caps[2]);

                match name.as_str() {
                    "description" | "og:description" => {
                        if metadata.description.is_none() {
                            metadata.description = Some(content);
                        }
                    }
                    "author" | "og:author" => {
                        metadata.authors.push(content);
                    }
                    "og:title" => {
                        if metadata.title.is_none() {
                            metadata.title = Some(content);
                        }
                    }
                    "og:site_name" => {
                        metadata.site_name = Some(content);
                    }
                    "article:published_time" | "datePublished" => {
                        metadata.published_date = Some(content);
                    }
                    _ => {}
                }
            }
        }

        // Extract from JSON-LD if present
        let jsonld_re = Regex::new(r#"<script[^>]*type=["']application/ld\+json["'][^>]*>([^<]+)</script>"#).ok();
        if let Some(re) = jsonld_re {
            if let Some(caps) = re.captures(html) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&caps[1]) {
                    if let Some(headline) = json.get("headline").and_then(|v| v.as_str()) {
                        if metadata.title.is_none() {
                            metadata.title = Some(headline.to_string());
                        }
                    }
                    if let Some(author) = json.get("author") {
                        if let Some(name) = author.get("name").and_then(|v| v.as_str()) {
                            if !metadata.authors.contains(&name.to_string()) {
                                metadata.authors.push(name.to_string());
                            }
                        }
                    }
                    if let Some(date) = json.get("datePublished").and_then(|v| v.as_str()) {
                        metadata.published_date = Some(date.to_string());
                    }
                    if let Some(publisher) = json.get("publisher").and_then(|p| p.get("name")).and_then(|v| v.as_str()) {
                        metadata.site_name = Some(publisher.to_string());
                    }
                }
            }
        }

        // Extract domain as fallback site name
        if metadata.site_name.is_none() {
            if let Ok(parsed_url) = reqwest::Url::parse(url) {
                metadata.site_name = parsed_url.host_str().map(|h| h.to_string());
            }
        }

        metadata
    }

    fn extract_text(html: &str) -> String {
        // Remove script and style tags
        let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
        let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
        let nav_re = Regex::new(r"(?is)<(nav|header|footer)[^>]*>.*?</\1>").unwrap();

        let mut text = html.to_string();
        text = script_re.replace_all(&text, "").to_string();
        text = style_re.replace_all(&text, "").to_string();
        text = nav_re.replace_all(&text, "").to_string();

        // Convert common block elements to newlines
        let block_re = Regex::new(r"</(p|div|h[1-6]|li|tr|br)[^>]*>").unwrap();
        text = block_re.replace_all(&text, "\n").to_string();

        // Remove remaining HTML tags
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        text = tag_re.replace_all(&text, "").to_string();

        // Decode HTML entities
        text = Self::decode_html_entities(&text);

        // Normalize whitespace
        let whitespace_re = Regex::new(r"\n\s*\n+").unwrap();
        text = whitespace_re.replace_all(&text, "\n\n").to_string();

        text.trim().to_string()
    }

    fn decode_html_entities(text: &str) -> String {
        text.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&nbsp;", " ")
            .replace("&#x27;", "'")
            .replace("&#x2F;", "/")
    }
}

#[derive(Debug, Clone, Default)]
pub struct FetchedContent {
    pub url: String,
    pub html: String,
    pub text: String,
    pub metadata: ContentMetadata,
    pub content_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContentMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub published_date: Option<String>,
    pub site_name: Option<String>,
}

// ============================================================================
// Supporting Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConnection {
    pub source_a: String,
    pub source_b: String,
    pub relationship: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCheckResult {
    pub claim: String,
    pub verdict: String,
    pub confidence: f32,
    pub supporting_sources: Vec<String>,
    pub contradicting_sources: Vec<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citation_apa() {
        let mut source = Source::new("The Art of Programming", Some("https://example.com"));
        source.authors = vec!["John Smith".to_string()];
        source.published_date = Some("2024".to_string());

        let citation = CitationGenerator::generate(&source, CitationStyle::APA);
        assert!(citation.contains("Smith"));
        assert!(citation.contains("2024"));
        assert!(citation.contains("The Art of Programming"));
    }

    #[test]
    fn test_citation_bibtex() {
        let mut source = Source::new("Machine Learning Basics", Some("https://ml.org"));
        source.authors = vec!["Jane Doe".to_string(), "Bob Wilson".to_string()];
        source.published_date = Some("2023".to_string());
        source.source_type = SourceType::Paper;

        let citation = CitationGenerator::generate(&source, CitationStyle::BibTeX);
        assert!(citation.contains("@article"));
        assert!(citation.contains("Jane Doe and Bob Wilson"));
        assert!(citation.contains("2023"));
    }

    #[test]
    fn test_source_creation() {
        let source = Source::new("Test Source", Some("https://test.com"));
        assert!(source.id.starts_with("src_"));
        assert_eq!(source.title, "Test Source");
        assert_eq!(source.url, Some("https://test.com".to_string()));
    }
}
