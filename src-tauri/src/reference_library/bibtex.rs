//! BibTeX import and export for the reference library.

use super::reference::{Reference, ReferenceType, generate_cite_key};
use regex::Regex;

/// Parse a BibTeX string into a list of References
pub fn parse_bibtex(input: &str) -> Result<Vec<Reference>, String> {
    let mut references = Vec::new();
    let entry_re = Regex::new(r"@(\w+)\s*\{([^,]*),").unwrap();

    // Split on entry boundaries
    let entries = split_entries(input);

    for entry_str in entries {
        let trimmed = entry_str.trim();
        if trimmed.is_empty() { continue; }

        if let Some(caps) = entry_re.captures(trimmed) {
            let entry_type = caps.get(1).map(|m| m.as_str()).unwrap_or("misc");
            let cite_key = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("unknown");

            let fields = parse_fields(trimmed);

            let title = fields.get("title")
                .map(|s| clean_bibtex_value(s))
                .unwrap_or_else(|| "Untitled".to_string());

            let authors = fields.get("author")
                .map(|s| parse_authors(&clean_bibtex_value(s)))
                .unwrap_or_default();

            let year = fields.get("year")
                .and_then(|s| clean_bibtex_value(s).parse::<i32>().ok());

            let mut reference = Reference::new(&title, authors);
            reference.cite_key = cite_key.to_string();
            reference.year = year;
            reference.ref_type = ReferenceType::from_str(entry_type);
            reference.journal = fields.get("journal").map(|s| clean_bibtex_value(s));
            reference.publisher = fields.get("publisher").map(|s| clean_bibtex_value(s));
            reference.volume = fields.get("volume").map(|s| clean_bibtex_value(s));
            reference.issue = fields.get("number").map(|s| clean_bibtex_value(s));
            reference.pages = fields.get("pages").map(|s| clean_bibtex_value(s));
            reference.doi = fields.get("doi").map(|s| clean_bibtex_value(s));
            reference.isbn = fields.get("isbn").map(|s| clean_bibtex_value(s));
            reference.issn = fields.get("issn").map(|s| clean_bibtex_value(s));
            reference.url = fields.get("url").map(|s| clean_bibtex_value(s));
            reference.abstract_text = fields.get("abstract").map(|s| clean_bibtex_value(s));
            reference.edition = fields.get("edition").map(|s| clean_bibtex_value(s));

            if let Some(kw) = fields.get("keywords") {
                reference.keywords = clean_bibtex_value(kw)
                    .split(|c| c == ',' || c == ';')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect();
            }

            references.push(reference);
        }
    }

    Ok(references)
}

/// Export a list of References to BibTeX format
pub fn export_bibtex(references: &[Reference]) -> String {
    let mut output = String::new();

    for r in references {
        let entry_type = r.ref_type.as_str();
        output.push_str(&format!("@{}{{{},\n", entry_type, r.cite_key));

        add_field(&mut output, "title", &Some(r.title.clone()));
        add_field(&mut output, "author", &Some(r.authors.join(" and ")));
        if let Some(year) = r.year {
            add_field(&mut output, "year", &Some(year.to_string()));
        }
        add_field(&mut output, "journal", &r.journal);
        add_field(&mut output, "publisher", &r.publisher);
        add_field(&mut output, "volume", &r.volume);
        add_field(&mut output, "number", &r.issue);
        add_field(&mut output, "pages", &r.pages);
        add_field(&mut output, "doi", &r.doi);
        add_field(&mut output, "isbn", &r.isbn);
        add_field(&mut output, "issn", &r.issn);
        add_field(&mut output, "url", &r.url);
        add_field(&mut output, "abstract", &r.abstract_text);
        add_field(&mut output, "edition", &r.edition);

        if !r.keywords.is_empty() {
            add_field(&mut output, "keywords", &Some(r.keywords.join(", ")));
        }

        output.push_str("}\n\n");
    }

    output
}

/// Export a single reference to BibTeX
pub fn reference_to_bibtex(reference: &Reference) -> String {
    export_bibtex(&[reference.clone()])
}

// --- Internal helpers ---

fn split_entries(input: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    let mut in_entry = false;

    for ch in input.chars() {
        if ch == '@' && depth == 0 {
            if !current.trim().is_empty() && in_entry {
                entries.push(current.clone());
            }
            current.clear();
            in_entry = true;
        }
        if in_entry {
            current.push(ch);
            if ch == '{' { depth += 1; }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    entries.push(current.clone());
                    current.clear();
                    in_entry = false;
                }
            }
        }
    }

    if !current.trim().is_empty() && in_entry {
        entries.push(current);
    }

    entries
}

fn parse_fields(entry: &str) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();

    // Find the content between first { and last }
    let start = entry.find('{').map(|i| i + 1).unwrap_or(0);
    // Skip the cite key
    let after_key = entry[start..].find(',').map(|i| start + i + 1).unwrap_or(start);
    let end = entry.rfind('}').unwrap_or(entry.len());

    if after_key >= end { return fields; }

    let content = &entry[after_key..end];

    // Parse field = {value} or field = "value" or field = number
    let field_re = Regex::new(r"(\w+)\s*=\s*").unwrap();
    let mut last_end = 0;

    let matches: Vec<_> = field_re.find_iter(content).collect();

    for (i, m) in matches.iter().enumerate() {
        let field_name = content[m.start()..m.end()]
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let value_start = m.end();
        let value_end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            content.len()
        };

        let raw_value = content[value_start..value_end].trim();
        let value = extract_value(raw_value);

        if !field_name.is_empty() && !value.is_empty() {
            fields.insert(field_name, value);
        }
    }

    fields
}

fn extract_value(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches(',').trim();

    if trimmed.starts_with('{') {
        // Extract content between braces
        let mut depth = 0;
        let mut result = String::new();
        for ch in trimmed.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    if depth > 1 { result.push(ch); }
                }
                '}' => {
                    depth -= 1;
                    if depth > 0 { result.push(ch); }
                    if depth == 0 { break; }
                }
                _ => { if depth > 0 { result.push(ch); } }
            }
        }
        result
    } else if trimmed.starts_with('"') {
        // Extract content between quotes
        let inner = trimmed.trim_start_matches('"');
        inner.find('"')
            .map(|end| inner[..end].to_string())
            .unwrap_or_else(|| inner.to_string())
    } else {
        // Bare value (number or string constant)
        trimmed.to_string()
    }
}

fn clean_bibtex_value(s: &str) -> String {
    s.replace("{\\&}", "&")
        .replace("\\&", "&")
        .replace("{", "")
        .replace("}", "")
        .replace("~", " ")
        .replace("\\textendash", "-")
        .replace("--", "\u{2013}") // en-dash
        .trim()
        .to_string()
}

fn parse_authors(author_str: &str) -> Vec<String> {
    author_str
        .split(" and ")
        .map(|a| {
            let trimmed = a.trim();
            // Handle "Last, First" format
            if let Some(comma_pos) = trimmed.find(',') {
                let last = trimmed[..comma_pos].trim();
                let first = trimmed[comma_pos + 1..].trim();
                format!("{} {}", first, last)
            } else {
                trimmed.to_string()
            }
        })
        .filter(|a| !a.is_empty())
        .collect()
}

fn add_field(output: &mut String, name: &str, value: &Option<String>) {
    if let Some(v) = value {
        if !v.is_empty() {
            output.push_str(&format!("  {} = {{{}}},\n", name, v));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_bibtex() {
        let bib = r#"
@article{smith2024deep,
  title = {Deep Learning for Research},
  author = {Smith, John and Doe, Jane},
  year = {2024},
  journal = {Nature},
  volume = {42},
  pages = {1--10},
  doi = {10.1234/test}
}
"#;
        let refs = parse_bibtex(bib).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].title, "Deep Learning for Research");
        assert_eq!(refs[0].authors, vec!["John Smith", "Jane Doe"]);
        assert_eq!(refs[0].year, Some(2024));
        assert_eq!(refs[0].cite_key, "smith2024deep");
    }

    #[test]
    fn test_roundtrip() {
        let bib = r#"@book{knuth1997art,
  title = {The Art of Computer Programming},
  author = {Donald Knuth},
  year = {1997},
  publisher = {Addison-Wesley},
}
"#;
        let refs = parse_bibtex(bib).unwrap();
        let exported = export_bibtex(&refs);
        let reimported = parse_bibtex(&exported).unwrap();
        assert_eq!(refs[0].title, reimported[0].title);
        assert_eq!(refs[0].authors, reimported[0].authors);
    }
}
