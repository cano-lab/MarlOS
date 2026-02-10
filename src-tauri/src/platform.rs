//! Platform-specific utilities for MarlOS
//!
//! Handles differences between desktop (Windows, macOS, Linux) and mobile (iOS, Android).

use std::path::PathBuf;

/// Get the application data directory for the current platform
pub fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        // Android: Use internal storage
        // The actual path is provided by Tauri at runtime
        std::env::var("INTERNAL_STORAGE")
            .map(PathBuf::from)
            .ok()
            .or_else(|| Some(PathBuf::from("/data/data/com.marlos.app/files")))
    }

    #[cfg(target_os = "ios")]
    {
        // iOS: Use Application Support directory
        dirs::data_dir().map(|d| d.join("com.marlos.app"))
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Desktop: Use standard data directory
        dirs::data_dir().map(|d| d.join("com.marlos.app"))
    }
}

/// Get the cache directory for the current platform
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        std::env::var("CACHE_DIR")
            .map(PathBuf::from)
            .ok()
            .or_else(|| Some(PathBuf::from("/data/data/com.marlos.app/cache")))
    }

    #[cfg(target_os = "ios")]
    {
        dirs::cache_dir().map(|d| d.join("com.marlos.app"))
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        dirs::cache_dir().map(|d| d.join("com.marlos.app"))
    }
}

/// Check if running on mobile
pub fn is_mobile() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// Check if running on desktop
pub fn is_desktop() -> bool {
    !is_mobile()
}

/// Get the default database path
pub fn default_db_path() -> PathBuf {
    app_data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("objects.db")
}

/// Get the sessions directory (Claude Code sessions - desktop only)
pub fn sessions_dir() -> Option<PathBuf> {
    if is_mobile() {
        // Mobile doesn't have direct access to Claude Code sessions
        None
    } else {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }
}

/// Platform capabilities
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// Whether local LLM (LM Studio/Ollama) is available
    pub local_llm: bool,
    /// Whether session import is available
    pub session_import: bool,
    /// Whether file system access is available
    pub file_system: bool,
    /// Whether PDF rendering is available
    pub pdf_rendering: bool,
    /// Maximum recommended database size (MB)
    pub max_db_size_mb: usize,
}

impl PlatformCapabilities {
    pub fn current() -> Self {
        if is_mobile() {
            Self {
                local_llm: false,
                session_import: false,
                file_system: false, // Limited on mobile
                pdf_rendering: true,
                max_db_size_mb: 500, // Conservative for mobile
            }
        } else {
            Self {
                local_llm: true,
                session_import: true,
                file_system: true,
                pdf_rendering: true,
                max_db_size_mb: 10000, // 10GB on desktop
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_data_dir() {
        let dir = app_data_dir();
        assert!(dir.is_some());
    }

    #[test]
    fn test_platform_detection() {
        // On desktop, is_mobile should be false
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            assert!(!is_mobile());
            assert!(is_desktop());
        }
    }

    #[test]
    fn test_capabilities() {
        let caps = PlatformCapabilities::current();
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            assert!(caps.local_llm);
            assert!(caps.session_import);
        }
    }
}
