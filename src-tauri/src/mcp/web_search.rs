//! Web Search Implementation
//!
//! This module provides web search and page fetching capabilities.
//! It uses DuckDuckGo for search (no API key needed) and can fetch
//! and extract content from web pages.
//!
//! Academic search uses free APIs:
//! - Semantic Scholar: Academic papers with citations
//! - arXiv: Preprints in CS, physics, math
//! - CrossRef: DOI-indexed publications

use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

// ============================================================================
// Search Results
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source_domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total_found: usize,
}

/// Search mode for different types of sources
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    #[default]
    Web,
    Academic,
    Both,
}

// ============================================================================
// Academic Paper Results
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcademicPaper {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub abstract_text: Option<String>,
    pub url: String,
    pub pdf_url: Option<String>,
    pub citation_count: Option<i32>,
    pub source: String, // "semantic_scholar", "arxiv", "crossref"
    pub doi: Option<String>,
    pub venue: Option<String>, // journal or conference
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcademicSearchResults {
    pub query: String,
    pub papers: Vec<AcademicPaper>,
    pub total_found: usize,
}

/// Search the web using DuckDuckGo
pub async fn search(query: &str, num_results: usize) -> Result<SearchResults, String> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Use DuckDuckGo HTML search
    let encoded_query = urlencoding::encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

    log::info!("Searching DuckDuckGo for: {}", query);

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Search returned status: {}", response.status()));
    }

    let html = response.text().await
        .map_err(|e| format!("Failed to read search response: {}", e))?;

    // Parse HTML results
    let document = Html::parse_document(&html);

    // DuckDuckGo result selectors
    let result_selector = Selector::parse(".result").unwrap();
    let title_selector = Selector::parse(".result__title a").unwrap();
    let snippet_selector = Selector::parse(".result__snippet").unwrap();
    let url_selector = Selector::parse(".result__url").unwrap();

    let mut results = Vec::new();

    for result in document.select(&result_selector).take(num_results) {
        // Get title and URL from the title link
        if let Some(title_elem) = result.select(&title_selector).next() {
            let title = title_elem.text().collect::<String>().trim().to_string();

            // Get the actual URL from href
            let raw_href = title_elem.value().attr("href").unwrap_or("");
            let url = extract_url_from_ddg(raw_href);

            // Get snippet
            let snippet = result.select(&snippet_selector)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // Get displayed URL for domain extraction
            let displayed_url = result.select(&url_selector)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let source_domain = extract_domain(&url).unwrap_or(displayed_url);

            if !url.is_empty() && !title.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                    source_domain,
                });
            }
        }
    }

    log::info!("Found {} search results", results.len());

    Ok(SearchResults {
        query: query.to_string(),
        results: results.clone(),
        total_found: results.len(),
    })
}

// ============================================================================
// Academic Search
// ============================================================================

/// Search for academic papers using multiple sources
pub async fn search_academic(query: &str, num_results: usize) -> Result<AcademicSearchResults, String> {
    log::info!("Academic search for: {}", query);

    let mut all_papers = Vec::new();

    // Search Semantic Scholar (primary source)
    match search_semantic_scholar(query, num_results).await {
        Ok(papers) => {
            log::info!("Found {} papers from Semantic Scholar", papers.len());
            all_papers.extend(papers);
        }
        Err(e) => {
            log::warn!("Semantic Scholar search failed: {}", e);
        }
    }

    // Search arXiv (good for preprints)
    match search_arxiv(query, num_results / 2).await {
        Ok(papers) => {
            log::info!("Found {} papers from arXiv", papers.len());
            all_papers.extend(papers);
        }
        Err(e) => {
            log::warn!("arXiv search failed: {}", e);
        }
    }

    // Deduplicate by title (fuzzy match)
    let mut seen_titles = std::collections::HashSet::new();
    all_papers.retain(|p| {
        let normalized = p.title.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");
        seen_titles.insert(normalized)
    });

    // Sort by citation count (if available)
    all_papers.sort_by(|a, b| {
        b.citation_count.unwrap_or(0).cmp(&a.citation_count.unwrap_or(0))
    });

    // Limit results
    all_papers.truncate(num_results);

    Ok(AcademicSearchResults {
        query: query.to_string(),
        total_found: all_papers.len(),
        papers: all_papers,
    })
}

/// Search Semantic Scholar API
pub async fn search_semantic_scholar(query: &str, num_results: usize) -> Result<Vec<AcademicPaper>, String> {
    let client = Client::builder()
        .user_agent("MarlOS-Research/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields=title,authors,year,abstract,url,openAccessPdf,citationCount,venue,externalIds",
        encoded_query, num_results
    );

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Semantic Scholar request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Semantic Scholar returned status: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse Semantic Scholar response: {}", e))?;

    let mut papers = Vec::new();

    if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let title = item.get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Untitled")
                .to_string();

            let authors: Vec<String> = item.get("authors")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default();

            let year = item.get("year").and_then(|y| y.as_i64()).map(|y| y as i32);

            let abstract_text = item.get("abstract")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string());

            let url = item.get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();

            let pdf_url = item.get("openAccessPdf")
                .and_then(|p| p.get("url"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());

            let citation_count = item.get("citationCount")
                .and_then(|c| c.as_i64())
                .map(|c| c as i32);

            let doi = item.get("externalIds")
                .and_then(|e| e.get("DOI"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            let venue = item.get("venue")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if !url.is_empty() {
                papers.push(AcademicPaper {
                    title,
                    authors,
                    year,
                    abstract_text,
                    url,
                    pdf_url,
                    citation_count,
                    source: "semantic_scholar".to_string(),
                    doi,
                    venue,
                });
            }
        }
    }

    Ok(papers)
}

/// Search arXiv API
pub async fn search_arxiv(query: &str, num_results: usize) -> Result<Vec<AcademicPaper>, String> {
    let client = Client::builder()
        .user_agent("MarlOS-Research/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "https://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results={}",
        encoded_query, num_results
    );

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("arXiv request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("arXiv returned status: {}", response.status()));
    }

    let xml = response.text().await
        .map_err(|e| format!("Failed to read arXiv response: {}", e))?;

    // Parse arXiv Atom feed
    parse_arxiv_response(&xml)
}

/// Parse arXiv Atom XML response
fn parse_arxiv_response(xml: &str) -> Result<Vec<AcademicPaper>, String> {
    let mut papers = Vec::new();

    // Simple XML parsing for arXiv's Atom feed
    // Each paper is in an <entry> element
    let entries: Vec<&str> = xml.split("<entry>").skip(1).collect();

    for entry in entries {
        let end_idx = entry.find("</entry>").unwrap_or(entry.len());
        let entry = &entry[..end_idx];

        let title = extract_xml_value(entry, "title")
            .map(|s| s.replace('\n', " ").trim().to_string())
            .unwrap_or_default();

        // Extract authors
        let mut authors = Vec::new();
        for author_chunk in entry.split("<author>").skip(1) {
            if let Some(name) = extract_xml_value(author_chunk, "name") {
                authors.push(name);
            }
        }

        let abstract_text = extract_xml_value(entry, "summary")
            .map(|s| s.replace('\n', " ").trim().to_string());

        // Get the abstract link (entry URL)
        let url = extract_link(entry, "alternate")
            .or_else(|| extract_xml_value(entry, "id"))
            .unwrap_or_default();

        // Get PDF link
        let pdf_url = extract_link(entry, "related")
            .or_else(|| {
                // Convert abstract URL to PDF URL
                if url.contains("arxiv.org/abs/") {
                    Some(url.replace("/abs/", "/pdf/") + ".pdf")
                } else {
                    None
                }
            });

        // Extract year from published date
        let year = extract_xml_value(entry, "published")
            .and_then(|s| if s.len() >= 4 { Some(s[0..4].to_string()) } else { None })
            .and_then(|y| y.parse().ok());

        // Extract arXiv ID as a pseudo-DOI
        let arxiv_id = extract_xml_value(entry, "id")
            .and_then(|s| s.rsplit('/').next().map(|s| s.to_string()));

        if !title.is_empty() && !url.is_empty() {
            papers.push(AcademicPaper {
                title,
                authors,
                year,
                abstract_text,
                url,
                pdf_url,
                citation_count: None, // arXiv doesn't provide this
                source: "arxiv".to_string(),
                doi: arxiv_id.map(|id| format!("arXiv:{}", id)),
                venue: Some("arXiv".to_string()),
            });
        }
    }

    Ok(papers)
}

/// Extract value from XML tag
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start_idx) = xml.find(&start_tag) {
        // Find the end of the opening tag
        let after_start = &xml[start_idx..];
        if let Some(content_start) = after_start.find('>') {
            let content_area = &after_start[content_start + 1..];
            if let Some(end_idx) = content_area.find(&end_tag) {
                return Some(content_area[..end_idx].trim().to_string());
            }
        }
    }
    None
}

/// Extract link href by rel attribute
fn extract_link(xml: &str, rel: &str) -> Option<String> {
    let rel_pattern = format!(r#"rel="{}""#, rel);

    for link_chunk in xml.split("<link").skip(1) {
        let end_idx = link_chunk.find('>').or_else(|| link_chunk.find("/>"))?;
        let link = &link_chunk[..end_idx];

        if link.contains(&rel_pattern) {
            // Extract href
            if let Some(href_start) = link.find("href=\"") {
                let after_href = &link[href_start + 6..];
                if let Some(href_end) = after_href.find('"') {
                    return Some(after_href[..href_end].to_string());
                }
            }
        }
    }
    None
}

/// Extract the actual URL from DuckDuckGo's redirect URL
fn extract_url_from_ddg(href: &str) -> String {
    // DuckDuckGo wraps URLs in a redirect: //duckduckgo.com/l/?uddg=<encoded_url>&...
    if href.contains("uddg=") {
        if let Some(start) = href.find("uddg=") {
            let encoded = &href[start + 5..];
            // Find the end (& or end of string)
            let end = encoded.find('&').unwrap_or(encoded.len());
            return urlencoding::decode(&encoded[..end])
                .map(|s| s.to_string())
                .unwrap_or_else(|_| href.to_string());
        }
    }

    // Some results have direct URLs
    if href.starts_with("http") {
        return href.to_string();
    }

    href.to_string()
}

/// Extract domain from URL
fn extract_domain(url: &str) -> Option<String> {
    url.split("://")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .map(|s| s.to_string())
}

// ============================================================================
// Page Fetching
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub word_count: usize,
    pub metadata: PageMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<PageLink>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    pub description: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub site_name: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    pub text: String,
    pub url: String,
}

/// Fetch and extract content from a web page
pub async fn fetch_page(url: &str, extract_links: bool) -> Result<FetchedPage, String> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    log::info!("Fetching page: {}", url);

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("Fetch request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Fetch returned status: {}", response.status()));
    }

    let html = response.text().await
        .map_err(|e| format!("Failed to read page: {}", e))?;

    let document = Html::parse_document(&html);

    // Extract title
    let title = document.select(&Selector::parse("title").unwrap())
        .next()
        .map(|t| t.text().collect::<String>().trim().to_string())
        .or_else(|| {
            document.select(&Selector::parse("h1").unwrap())
                .next()
                .map(|h| h.text().collect::<String>().trim().to_string())
        })
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract metadata
    let metadata = extract_metadata(&document);

    // Extract main content
    let content = extract_main_content(&document);
    let word_count = content.split_whitespace().count();

    // Optionally extract links
    let links = if extract_links {
        Some(extract_links_from_page(&document, url))
    } else {
        None
    };

    Ok(FetchedPage {
        url: url.to_string(),
        title,
        content,
        word_count,
        metadata,
        links,
    })
}

fn extract_metadata(document: &Html) -> PageMetadata {
    let meta_selector = Selector::parse("meta").unwrap();

    let mut description = None;
    let mut author = None;
    let mut published_date = None;
    let mut site_name = None;
    let mut keywords = Vec::new();

    for meta in document.select(&meta_selector) {
        let name = meta.value().attr("name").or_else(|| meta.value().attr("property")).unwrap_or("");
        let content = meta.value().attr("content").unwrap_or("");

        match name.to_lowercase().as_str() {
            "description" | "og:description" | "twitter:description" => {
                if description.is_none() {
                    description = Some(content.to_string());
                }
            }
            "author" | "article:author" => {
                if author.is_none() {
                    author = Some(content.to_string());
                }
            }
            "article:published_time" | "date" | "pubdate" => {
                if published_date.is_none() {
                    published_date = Some(content.to_string());
                }
            }
            "og:site_name" => {
                site_name = Some(content.to_string());
            }
            "keywords" => {
                keywords = content.split(',').map(|s| s.trim().to_string()).collect();
            }
            _ => {}
        }
    }

    PageMetadata {
        description,
        author,
        published_date,
        site_name,
        keywords,
    }
}

fn extract_main_content(document: &Html) -> String {
    // Try to find main content area
    let content_selectors = [
        "article",
        "main",
        "[role='main']",
        ".post-content",
        ".article-content",
        ".entry-content",
        ".content",
        "#content",
        ".post",
        ".article",
    ];

    for selector_str in content_selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let text = extract_text_from_element(&element);
                if text.len() > 200 {
                    return clean_text(&text);
                }
            }
        }
    }

    // Fallback: get body content, excluding navigation, scripts, etc.
    if let Ok(body_selector) = Selector::parse("body") {
        if let Some(body) = document.select(&body_selector).next() {
            let text = extract_text_from_element(&body);
            return clean_text(&text);
        }
    }

    String::new()
}

fn extract_text_from_element(element: &scraper::ElementRef) -> String {
    // Skip script, style, nav, header, footer elements
    let skip_tags = ["script", "style", "nav", "header", "footer", "aside", "noscript"];

    let mut text = String::new();

    for node in element.children() {
        if let Some(elem) = scraper::ElementRef::wrap(node) {
            let tag_name = elem.value().name();
            if !skip_tags.contains(&tag_name) {
                text.push_str(&extract_text_from_element(&elem));
                text.push(' ');
            }
        } else if let Some(text_node) = node.value().as_text() {
            text.push_str(text_node.trim());
            text.push(' ');
        }
    }

    text
}

fn clean_text(text: &str) -> String {
    // Remove excessive whitespace
    let mut result = String::new();
    let mut last_was_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    result.trim().to_string()
}

fn extract_links_from_page(document: &Html, base_url: &str) -> Vec<PageLink> {
    let link_selector = Selector::parse("a[href]").unwrap();
    let mut links = Vec::new();

    for link in document.select(&link_selector).take(50) {
        let text = link.text().collect::<String>().trim().to_string();
        if let Some(href) = link.value().attr("href") {
            // Skip javascript, mailto, tel links
            if href.starts_with("javascript:") || href.starts_with("mailto:") || href.starts_with("tel:") {
                continue;
            }

            // Convert relative URLs to absolute
            let url = if href.starts_with("http") {
                href.to_string()
            } else if href.starts_with("//") {
                format!("https:{}", href)
            } else if href.starts_with('/') {
                // Get base domain
                if let Some(domain) = base_url.split("://").nth(1).and_then(|s| s.split('/').next()) {
                    format!("https://{}{}", domain, href)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if !text.is_empty() && text.len() < 200 {
                links.push(PageLink { text, url });
            }
        }
    }

    links
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/path"), Some("example.com".to_string()));
        assert_eq!(extract_domain("http://sub.example.com/path"), Some("sub.example.com".to_string()));
    }

    #[test]
    fn test_clean_text() {
        let input = "  Hello   world\n\n  this   is  a   test  ";
        assert_eq!(clean_text(input), "Hello world this is a test");
    }
}
