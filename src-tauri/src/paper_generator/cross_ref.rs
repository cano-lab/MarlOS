//! Cross-Reference System
//!
//! Resolves cross-references in paper content:
//! - `{{fig:label}}` → "Figure 1"
//! - `{{tab:label}}` → "Table 2"
//! - `{{sec:label}}` → "Section 3.1"
//! - `{{eq:label}}` → "Equation 4"

use std::collections::HashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// A numbered item that can be cross-referenced
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberedItem {
    pub label: String,
    pub kind: ItemKind,
    pub number: String,
    pub caption: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Figure,
    Table,
    Equation,
    Section,
}

impl ItemKind {
    pub fn prefix(&self) -> &str {
        match self {
            ItemKind::Figure => "Figure",
            ItemKind::Table => "Table",
            ItemKind::Equation => "Eq.",
            ItemKind::Section => "Section",
        }
    }

    pub fn marker(&self) -> &str {
        match self {
            ItemKind::Figure => "fig",
            ItemKind::Table => "tab",
            ItemKind::Equation => "eq",
            ItemKind::Section => "sec",
        }
    }
}

/// Registry of all numbered items in a paper
#[derive(Debug, Clone, Default)]
pub struct CrossRefRegistry {
    items: HashMap<String, NumberedItem>,
    figure_count: usize,
    table_count: usize,
    equation_count: usize,
}

impl CrossRefRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a figure and get its number
    pub fn register_figure(&mut self, label: &str, caption: &str) -> String {
        self.figure_count += 1;
        let number = self.figure_count.to_string();
        self.items.insert(format!("fig:{}", label), NumberedItem {
            label: label.to_string(),
            kind: ItemKind::Figure,
            number: number.clone(),
            caption: caption.to_string(),
        });
        number
    }

    /// Register a table and get its number
    pub fn register_table(&mut self, label: &str, caption: &str) -> String {
        self.table_count += 1;
        let number = self.table_count.to_string();
        self.items.insert(format!("tab:{}", label), NumberedItem {
            label: label.to_string(),
            kind: ItemKind::Table,
            number: number.clone(),
            caption: caption.to_string(),
        });
        number
    }

    /// Register an equation and get its number
    pub fn register_equation(&mut self, label: &str) -> String {
        self.equation_count += 1;
        let number = self.equation_count.to_string();
        self.items.insert(format!("eq:{}", label), NumberedItem {
            label: label.to_string(),
            kind: ItemKind::Equation,
            number: number.clone(),
            caption: String::new(),
        });
        number
    }

    /// Register a section heading with its number
    pub fn register_section(&mut self, label: &str, number: &str, title: &str) {
        self.items.insert(format!("sec:{}", label), NumberedItem {
            label: label.to_string(),
            kind: ItemKind::Section,
            number: number.to_string(),
            caption: title.to_string(),
        });
    }

    /// Resolve all `{{kind:label}}` references in content
    pub fn resolve(&self, content: &str) -> String {
        let re = Regex::new(r"\{\{(fig|tab|eq|sec):([^}]+)\}\}").unwrap();

        re.replace_all(content, |caps: &regex::Captures| {
            let key = format!("{}:{}", &caps[1], &caps[2]);
            match self.items.get(&key) {
                Some(item) => format!("{} {}", item.kind.prefix(), item.number),
                None => format!("**??{}**", key),
            }
        }).to_string()
    }

    /// Get all items of a given kind
    pub fn items_of_kind(&self, kind: ItemKind) -> Vec<&NumberedItem> {
        let mut items: Vec<_> = self.items.values()
            .filter(|i| i.kind == kind)
            .collect();
        items.sort_by(|a, b| {
            a.number.parse::<usize>().unwrap_or(0)
                .cmp(&b.number.parse::<usize>().unwrap_or(0))
        });
        items
    }
}

/// Scan paper content for figure/table/equation definitions and auto-number them.
/// Definitions use the syntax:
/// - `{{def:fig:label|Caption text}}` for figures
/// - `{{def:tab:label|Caption text}}` for tables
/// - `{{def:eq:label}}` for equations
pub fn auto_number_content(content: &str) -> (String, CrossRefRegistry) {
    let mut registry = CrossRefRegistry::new();

    let def_re = Regex::new(r"\{\{def:(fig|tab|eq):([^|}]+)(?:\|([^}]*))?\}\}").unwrap();

    // First pass: register all definitions
    for caps in def_re.captures_iter(content) {
        let kind = &caps[1];
        let label = &caps[2];
        let caption = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        match kind {
            "fig" => { registry.register_figure(label, caption); }
            "tab" => { registry.register_table(label, caption); }
            "eq" => { registry.register_equation(label); }
            _ => {}
        }
    }

    // Second pass: replace definitions with formatted output
    let result = def_re.replace_all(content, |caps: &regex::Captures| {
        let kind = &caps[1];
        let label = &caps[2];
        let key = format!("{}:{}", kind, label);

        match registry.items.get(&key) {
            Some(item) => {
                if item.caption.is_empty() {
                    format!("**{} {}**", item.kind.prefix(), item.number)
                } else {
                    format!("**{} {}: {}**", item.kind.prefix(), item.number, item.caption)
                }
            }
            None => caps[0].to_string(),
        }
    }).to_string();

    // Third pass: resolve references
    let result = registry.resolve(&result);

    (result, registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_crossref() {
        let mut reg = CrossRefRegistry::new();
        reg.register_figure("arch", "System Architecture");
        reg.register_figure("results", "Experimental Results");
        reg.register_table("comparison", "Method Comparison");

        let content = "As shown in {{fig:arch}}, the system uses... See {{tab:comparison}} and {{fig:results}}.";
        let resolved = reg.resolve(content);

        assert!(resolved.contains("Figure 1"));
        assert!(resolved.contains("Figure 2"));
        assert!(resolved.contains("Table 1"));
    }

    #[test]
    fn test_unresolved_ref() {
        let reg = CrossRefRegistry::new();
        let resolved = reg.resolve("See {{fig:missing}}.");
        assert!(resolved.contains("**??fig:missing**"));
    }

    #[test]
    fn test_auto_number() {
        let content = r#"
{{def:fig:arch|System Architecture}}

The architecture is shown in {{fig:arch}}.

{{def:tab:results|Benchmark Results}}

Results are in {{tab:results}}.
"#;
        let (resolved, registry) = auto_number_content(content);

        assert!(resolved.contains("**Figure 1: System Architecture**"));
        assert!(resolved.contains("Figure 1"));
        assert!(resolved.contains("**Table 1: Benchmark Results**"));
        assert!(resolved.contains("Table 1"));
        assert_eq!(registry.items_of_kind(ItemKind::Figure).len(), 2); // def counts
    }

    #[test]
    fn test_section_ref() {
        let mut reg = CrossRefRegistry::new();
        reg.register_section("intro", "1", "Introduction");
        reg.register_section("method", "2.1", "Data Collection");

        let resolved = reg.resolve("In {{sec:intro}}, we discuss... {{sec:method}} describes...");
        assert!(resolved.contains("Section 1"));
        assert!(resolved.contains("Section 2.1"));
    }
}
