//! Tier Classifier - Auto-detect security tier for content
//!
//! Automatically classifies content into security tiers based on:
//! - File paths (e.g., .env, credentials.*, *.key)
//! - Content patterns (e.g., API keys, passwords, tokens)
//! - Data type (e.g., logs vs documents)

use regex::Regex;
use crate::memory::{MemoryType, SecurityTier};

/// Classifier for determining security tiers
pub struct TierClassifier {
    /// Patterns that indicate Sealed tier (secrets)
    sealed_path_patterns: Vec<Regex>,
    sealed_content_patterns: Vec<Regex>,
    /// Patterns that indicate Guarded tier (user data)
    guarded_path_patterns: Vec<Regex>,
}

impl TierClassifier {
    pub fn new() -> Self {
        Self {
            // File paths that should be Sealed
            sealed_path_patterns: vec![
                Regex::new(r"(?i)\.env(\.[a-z]+)?$").unwrap(),           // .env, .env.local
                Regex::new(r"(?i)credentials?\.(json|yaml|yml|toml)$").unwrap(),
                Regex::new(r"(?i)secrets?\.(json|yaml|yml|toml)$").unwrap(),
                Regex::new(r"(?i)\.(pem|key|p12|pfx|jks)$").unwrap(),    // Certificate/key files
                Regex::new(r"(?i)(^|/)\.ssh/").unwrap(),                 // SSH directory
                Regex::new(r"(?i)(^|/)\.aws/").unwrap(),                 // AWS config
                Regex::new(r"(?i)(^|/)\.kube/config").unwrap(),          // Kubernetes config
                Regex::new(r"(?i)id_rsa").unwrap(),                      // SSH private keys
                Regex::new(r"(?i)\.npmrc$").unwrap(),                    // NPM auth tokens
                Regex::new(r"(?i)\.netrc$").unwrap(),                    // Network credentials
            ],
            // Content patterns that indicate Sealed tier
            sealed_content_patterns: vec![
                // API keys and tokens
                Regex::new(r"(?i)(api[_\-]?key|apikey)\s*[:=]\s*['\x22]?[a-zA-Z0-9_\-]{20,}").unwrap(),
                Regex::new(r"(?i)(auth[_\-]?token|access[_\-]?token)\s*[:=]\s*['\x22]?[a-zA-Z0-9_\-]{20,}").unwrap(),
                Regex::new(r"(?i)(secret[_\-]?key|private[_\-]?key)\s*[:=]\s*['\x22]?[a-zA-Z0-9_\-]{20,}").unwrap(),
                // Passwords
                Regex::new(r"(?i)password\s*[:=]\s*['\x22]?[^\s'\x22]{8,}").unwrap(),
                Regex::new(r"(?i)(db_pass|database_password|mysql_password|postgres_password)\s*[:=]").unwrap(),
                // AWS
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),                // AWS Access Key ID
                Regex::new(r"(?i)aws_secret_access_key\s*[:=]").unwrap(),
                // Private keys
                Regex::new(r"-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----").unwrap(),
                // JWT secrets
                Regex::new(r"(?i)jwt[_\-]?secret\s*[:=]").unwrap(),
                // Database connection strings with passwords
                Regex::new(r"(?i)(mongodb|postgres|mysql|redis)://[^:]+:[^@]+@").unwrap(),
                // GitHub/GitLab tokens
                Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36,}").unwrap(),     // GitHub tokens
                Regex::new(r"glpat-[a-zA-Z0-9_\-]{20,}").unwrap(),       // GitLab tokens
                // Stripe
                Regex::new(r"sk_live_[0-9a-zA-Z]{24,}").unwrap(),
                // Slack
                Regex::new(r"xox[baprs]-[0-9]{10,13}-[0-9]{10,13}-[a-zA-Z0-9]{24}").unwrap(),
            ],
            // File paths that should be Guarded (user data)
            guarded_path_patterns: vec![
                Regex::new(r"(?i)\.(md|txt|doc|docx|pdf)$").unwrap(),    // Documents
                Regex::new(r"(?i)(notes?|journal|diary|personal)/").unwrap(),
                Regex::new(r"(?i)/home/").unwrap(),                      // User home directories
                Regex::new(r"(?i)/users/[^/]+/(documents|desktop)/").unwrap(),
            ],
        }
    }

    /// Classify content based on file path
    pub fn classify_by_path(&self, path: &str) -> Option<SecurityTier> {
        // Check for Sealed patterns first (highest priority)
        for pattern in &self.sealed_path_patterns {
            if pattern.is_match(path) {
                return Some(SecurityTier::Sealed);
            }
        }

        // Check for Guarded patterns
        for pattern in &self.guarded_path_patterns {
            if pattern.is_match(path) {
                return Some(SecurityTier::Guarded);
            }
        }

        None // No specific classification - use default
    }

    /// Classify content based on content text
    pub fn classify_by_content(&self, content: &str) -> Option<SecurityTier> {
        // Check for Sealed patterns (secrets in content)
        for pattern in &self.sealed_content_patterns {
            if pattern.is_match(content) {
                return Some(SecurityTier::Sealed);
            }
        }

        None // No specific classification
    }

    /// Full classification considering path, content, and memory type
    pub fn classify(
        &self,
        content: &str,
        path: Option<&str>,
        memory_type: MemoryType,
    ) -> SecurityTier {
        // 1. Check path first
        if let Some(p) = path {
            if let Some(tier) = self.classify_by_path(p) {
                return tier;
            }
        }

        // 2. Check content for secrets
        if let Some(tier) = self.classify_by_content(content) {
            return tier;
        }

        // 3. Default based on memory type
        match memory_type {
            MemoryType::Event => SecurityTier::Open,    // Logs/events are Open
            MemoryType::Command => SecurityTier::Open,  // Commands are Open
            MemoryType::Search => SecurityTier::Open,   // Search queries are Open
            MemoryType::Document => SecurityTier::Guarded, // User documents are Guarded
            MemoryType::Note => SecurityTier::Guarded,  // Notes are Guarded
        }
    }

    /// Check if content contains potential secrets (for warnings)
    pub fn contains_secrets(&self, content: &str) -> bool {
        self.classify_by_content(content) == Some(SecurityTier::Sealed)
    }

    /// Get a description of why content was classified as Sealed
    pub fn get_sealed_reason(&self, content: &str, path: Option<&str>) -> Option<String> {
        if let Some(p) = path {
            for pattern in &self.sealed_path_patterns {
                if pattern.is_match(p) {
                    return Some(format!("File path matches sensitive pattern: {}", pattern.as_str()));
                }
            }
        }

        for pattern in &self.sealed_content_patterns {
            if pattern.is_match(content) {
                return Some(format!("Content matches secret pattern: {}", pattern.as_str()));
            }
        }

        None
    }
}

impl Default for TierClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_file_detection() {
        let classifier = TierClassifier::new();

        assert_eq!(classifier.classify_by_path(".env"), Some(SecurityTier::Sealed));
        assert_eq!(classifier.classify_by_path(".env.local"), Some(SecurityTier::Sealed));
        assert_eq!(classifier.classify_by_path("/app/.env.production"), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_key_file_detection() {
        let classifier = TierClassifier::new();

        assert_eq!(classifier.classify_by_path("server.key"), Some(SecurityTier::Sealed));
        assert_eq!(classifier.classify_by_path("cert.pem"), Some(SecurityTier::Sealed));
        assert_eq!(classifier.classify_by_path("/home/user/.ssh/id_rsa"), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_api_key_detection() {
        let classifier = TierClassifier::new();

        let content = "const API_KEY = 'sk-1234567890abcdefghijklmnop';";
        assert_eq!(classifier.classify_by_content(content), Some(SecurityTier::Sealed));

        let content2 = "api_key: abcdefghij1234567890";
        assert_eq!(classifier.classify_by_content(content2), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_password_detection() {
        let classifier = TierClassifier::new();

        let content = "password=mysecretpassword123";
        assert_eq!(classifier.classify_by_content(content), Some(SecurityTier::Sealed));

        let content2 = "DB_PASS: supersecret";
        assert_eq!(classifier.classify_by_content(content2), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_aws_key_detection() {
        let classifier = TierClassifier::new();

        let content = "AWS Access Key: AKIAIOSFODNN7EXAMPLE";
        assert_eq!(classifier.classify_by_content(content), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_private_key_detection() {
        let classifier = TierClassifier::new();

        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQ...";
        assert_eq!(classifier.classify_by_content(content), Some(SecurityTier::Sealed));
    }

    #[test]
    fn test_document_classification() {
        let classifier = TierClassifier::new();

        assert_eq!(classifier.classify_by_path("/notes/todo.md"), Some(SecurityTier::Guarded));
        assert_eq!(classifier.classify_by_path("journal/2024-01-01.txt"), Some(SecurityTier::Guarded));
    }

    #[test]
    fn test_safe_content() {
        let classifier = TierClassifier::new();

        let content = "This is a normal log message with no secrets";
        assert_eq!(classifier.classify_by_content(content), None);

        let content2 = "Error: Connection timeout after 30 seconds";
        assert_eq!(classifier.classify_by_content(content2), None);
    }

    #[test]
    fn test_full_classification() {
        let classifier = TierClassifier::new();

        // Event with safe content -> Open
        assert_eq!(
            classifier.classify("Connection established", None, MemoryType::Event),
            SecurityTier::Open
        );

        // Document without secrets -> Guarded
        assert_eq!(
            classifier.classify("My personal notes", Some("notes.md"), MemoryType::Document),
            SecurityTier::Guarded
        );

        // Event with secret content -> Sealed (content overrides type)
        assert_eq!(
            classifier.classify("api_key=secret123456789012345", None, MemoryType::Event),
            SecurityTier::Sealed
        );

        // .env file -> Sealed (path overrides everything)
        assert_eq!(
            classifier.classify("DEBUG=true", Some(".env"), MemoryType::Document),
            SecurityTier::Sealed
        );
    }

    #[test]
    fn test_contains_secrets() {
        let classifier = TierClassifier::new();

        assert!(classifier.contains_secrets("password=secret123"));
        assert!(classifier.contains_secrets("AKIAIOSFODNN7EXAMPLE"));
        assert!(!classifier.contains_secrets("Hello world"));
    }
}
