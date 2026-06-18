//! DOI and ISBN resolution for auto-populating reference metadata.

use reqwest;
use serde::Deserialize;

use super::reference::{Reference, ReferenceType, generate_cite_key};

/// Resolve a DOI to a Reference using CrossRef API
pub async fn resolve_doi(doi: &str) -> Result<Reference, String> {
    let clean_doi = doi.trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:");

    let url = format!("https://api.crossref.org/works/{}", urlencoding::encode(clean_doi));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client.get(&url)
        .header("User-Agent", "MarlOS/0.1 (mailto:marlos@research.local)")
        .send()
        .await
        .map_err(|e| format!("CrossRef request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("DOI not found (status {})", resp.status()));
    }

    let data: CrossRefResponse = resp.json().await
        .map_err(|e| format!("Failed to parse CrossRef response: {}", e))?;

    let work = data.message;

    let authors: Vec<String> = work.author.unwrap_or_default()
        .iter()
        .map(|a| {
            match (&a.given, &a.family) {
                (Some(given), Some(family)) => format!("{} {}", given, family),
                (None, Some(family)) => family.clone(),
                (Some(given), None) => given.clone(),
                (None, None) => "Unknown".to_string(),
            }
        })
        .collect();

    let title = work.title.unwrap_or_default()
        .into_iter().next()
        .unwrap_or_else(|| "Untitled".to_string());

    let year = work.published_print
        .or(work.published_online)
        .and_then(|dp| dp.date_parts.into_iter().next())
        .and_then(|parts| parts.into_iter().next())
        .and_then(|y| y);

    let ref_type = match work.r#type.as_deref() {
        Some("journal-article") => ReferenceType::Article,
        Some("book") | Some("monograph") => ReferenceType::Book,
        Some("proceedings-article") | Some("book-chapter") => ReferenceType::InProceedings,
        Some("dataset") => ReferenceType::Dataset,
        Some("report") | Some("report-component") => ReferenceType::TechReport,
        _ => ReferenceType::Article,
    };

    let cite_key = generate_cite_key(&authors, year, &title);

    let mut reference = Reference::new(&title, authors);
    reference.doi = Some(clean_doi.to_string());
    reference.year = year;
    reference.journal = work.container_title.and_then(|ct| ct.into_iter().next());
    reference.publisher = work.publisher;
    reference.volume = work.volume;
    reference.issue = work.issue;
    reference.pages = work.page;
    reference.issn = work.issn.and_then(|i| i.into_iter().next());
    reference.url = work.url;
    reference.abstract_text = work.r#abstract;
    reference.cite_key = cite_key;
    reference.ref_type = ref_type;

    if let Some(subjects) = work.subject {
        reference.keywords = subjects;
    }

    Ok(reference)
}

/// Resolve an ISBN to a Reference using Open Library API
pub async fn resolve_isbn(isbn: &str) -> Result<Reference, String> {
    let clean_isbn = isbn.trim()
        .replace("-", "")
        .replace(" ", "");

    let url = format!(
        "https://openlibrary.org/api/books?bibkeys=ISBN:{}&format=json&jscmd=data",
        clean_isbn
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Open Library request failed: {}", e))?;

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Open Library response: {}", e))?;

    let key = format!("ISBN:{}", clean_isbn);
    let book = data.get(&key)
        .ok_or_else(|| "ISBN not found".to_string())?;

    let title = book.get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let authors: Vec<String> = book.get("authors")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let year = book.get("publish_date")
        .and_then(|d| d.as_str())
        .and_then(|d| {
            // Try to extract year from various date formats
            d.chars()
                .collect::<String>()
                .split(|c: char| !c.is_numeric())
                .filter(|s| s.len() == 4)
                .next()
                .and_then(|y| y.parse::<i32>().ok())
        });

    let publisher = book.get("publishers")
        .and_then(|p| p.as_array())
        .and_then(|arr| arr.first())
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let mut reference = Reference::new(&title, authors);
    reference.isbn = Some(clean_isbn);
    reference.year = year;
    reference.publisher = publisher;
    reference.ref_type = ReferenceType::Book;
    reference.cite_key = generate_cite_key(&reference.authors, year, &title);

    if let Some(subjects) = book.get("subjects").and_then(|s| s.as_array()) {
        reference.keywords = subjects.iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .map(|s| s.to_string())
            .take(10)
            .collect();
    }

    Ok(reference)
}

// --- CrossRef API types ---

#[derive(Deserialize)]
struct CrossRefResponse {
    message: CrossRefWork,
}

#[derive(Deserialize)]
struct CrossRefWork {
    title: Option<Vec<String>>,
    author: Option<Vec<CrossRefAuthor>>,
    #[serde(rename = "container-title")]
    container_title: Option<Vec<String>>,
    publisher: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    page: Option<String>,
    #[serde(rename = "published-print")]
    published_print: Option<DateParts>,
    #[serde(rename = "published-online")]
    published_online: Option<DateParts>,
    #[serde(rename = "type")]
    r#type: Option<String>,
    #[serde(rename = "URL")]
    url: Option<String>,
    #[serde(rename = "ISSN")]
    issn: Option<Vec<String>>,
    subject: Option<Vec<String>>,
    #[serde(rename = "abstract")]
    r#abstract: Option<String>,
}

#[derive(Deserialize)]
struct CrossRefAuthor {
    given: Option<String>,
    family: Option<String>,
}

#[derive(Deserialize)]
struct DateParts {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<Option<i32>>>,
}
