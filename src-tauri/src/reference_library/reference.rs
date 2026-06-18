//! Reference data model for the research library.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub isbn: Option<String>,
    pub issn: Option<String>,
    pub url: Option<String>,

    // Publication details
    pub journal: Option<String>,
    pub publisher: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub edition: Option<String>,

    // Content
    pub abstract_text: Option<String>,
    pub keywords: Vec<String>,
    pub notes: Option<String>,

    // File attachment
    pub pdf_path: Option<String>,

    // Organization
    pub collections: Vec<String>,
    pub tags: Vec<String>,

    // Status
    pub reading_status: ReadingStatus,
    pub rating: Option<u8>, // 1-5

    // Citation key for BibTeX
    pub cite_key: String,

    // Reference type
    pub ref_type: ReferenceType,

    // Timestamps
    pub added_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,

    // Link to SemanticObject system
    pub suid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReadingStatus {
    Unread,
    Reading,
    Read,
    Archived,
}

impl Default for ReadingStatus {
    fn default() -> Self {
        ReadingStatus::Unread
    }
}

impl ReadingStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ReadingStatus::Unread => "unread",
            ReadingStatus::Reading => "reading",
            ReadingStatus::Read => "read",
            ReadingStatus::Archived => "archived",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "reading" => ReadingStatus::Reading,
            "read" => ReadingStatus::Read,
            "archived" => ReadingStatus::Archived,
            _ => ReadingStatus::Unread,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceType {
    Article,
    Book,
    InProceedings,
    InCollection,
    PhdThesis,
    MastersThesis,
    TechReport,
    Misc,
    Webpage,
    Patent,
    Dataset,
}

impl Default for ReferenceType {
    fn default() -> Self {
        ReferenceType::Article
    }
}

impl ReferenceType {
    pub fn as_str(&self) -> &str {
        match self {
            ReferenceType::Article => "article",
            ReferenceType::Book => "book",
            ReferenceType::InProceedings => "inproceedings",
            ReferenceType::InCollection => "incollection",
            ReferenceType::PhdThesis => "phdthesis",
            ReferenceType::MastersThesis => "mastersthesis",
            ReferenceType::TechReport => "techreport",
            ReferenceType::Misc => "misc",
            ReferenceType::Webpage => "webpage",
            ReferenceType::Patent => "patent",
            ReferenceType::Dataset => "dataset",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "article" => ReferenceType::Article,
            "book" => ReferenceType::Book,
            "inproceedings" | "conference" => ReferenceType::InProceedings,
            "incollection" => ReferenceType::InCollection,
            "phdthesis" => ReferenceType::PhdThesis,
            "mastersthesis" => ReferenceType::MastersThesis,
            "techreport" => ReferenceType::TechReport,
            "webpage" | "online" => ReferenceType::Webpage,
            "patent" => ReferenceType::Patent,
            "dataset" => ReferenceType::Dataset,
            _ => ReferenceType::Misc,
        }
    }
}

impl Reference {
    pub fn new(title: &str, authors: Vec<String>) -> Self {
        let now = Utc::now();
        let cite_key = generate_cite_key(&authors, None, &title);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            authors,
            year: None,
            doi: None,
            isbn: None,
            issn: None,
            url: None,
            journal: None,
            publisher: None,
            volume: None,
            issue: None,
            pages: None,
            edition: None,
            abstract_text: None,
            keywords: Vec::new(),
            notes: None,
            pdf_path: None,
            collections: Vec::new(),
            tags: Vec::new(),
            reading_status: ReadingStatus::Unread,
            rating: None,
            cite_key,
            ref_type: ReferenceType::Article,
            added_at: now,
            modified_at: now,
            suid: None,
        }
    }

    pub fn authors_display(&self) -> String {
        match self.authors.len() {
            0 => "Unknown".to_string(),
            1 => self.authors[0].clone(),
            2 => format!("{} & {}", self.authors[0], self.authors[1]),
            _ => format!("{} et al.", self.authors[0]),
        }
    }
}

/// Generate a BibTeX cite key from authors and year (e.g., "smith2024deep")
pub fn generate_cite_key(authors: &[String], year: Option<i32>, title: &str) -> String {
    let author_part = authors.first()
        .map(|a| {
            // Extract last name
            a.split_whitespace()
                .last()
                .unwrap_or("unknown")
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let year_part = year.map(|y| y.to_string()).unwrap_or_default();

    let title_word = title
        .split_whitespace()
        .find(|w| w.len() > 3) // Skip short words like "the", "a", "of"
        .unwrap_or("ref")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();

    format!("{}{}{}", author_part, year_part, title_word)
}
