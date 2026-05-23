//! Pandoc subprocess wrapper.
//!
//! Pandoc is treated as an opaque external tool: we spawn it, hand it
//! markdown via a file path, and consume its HTML output. We never link
//! against pandoc, never modify it, never extend it.
//!
//! Phase A scope: produce structured HTML from one markdown file using the
//! invocation recommended in the typesetter handoff doc:
//!
//! ```text
//! pandoc input.md \
//!   --from markdown+smart+footnotes+pipe_tables \
//!   --to html5 \
//!   --section-divs \
//!   --id-prefix=ch \
//!   --output -
//! ```
//!
//! `--section-divs` is load-bearing: it wraps every heading-bounded section
//! in a `<section>` element with the heading's id, which is what Phase B's
//! structure parser will consume.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum PandocError {
    #[error("pandoc binary not found — install pandoc and ensure it's on PATH (or set MARLOS_PANDOC)")]
    NotFound,
    #[error("pandoc IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("pandoc exited with status {status}: {stderr}")]
    NonZeroExit { status: i32, stderr: String },
    #[error("input file not found: {0}")]
    InputMissing(PathBuf),
    #[error("pandoc produced invalid UTF-8 output")]
    InvalidUtf8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PandocConvertResult {
    /// HTML5 fragment body (not a full standalone document).
    pub html: String,
    /// Anything pandoc emitted on stderr — usually warnings.
    pub stderr_warnings: String,
    /// Resolved path to the pandoc binary that produced this output.
    pub pandoc_path: String,
    /// Pandoc version string (e.g. "pandoc 3.1.13").
    pub pandoc_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PandocConvertOptions {
    /// Override the default markdown extensions. Phase A uses
    /// `markdown+smart+footnotes+pipe_tables`.
    pub from_format: Option<String>,
    /// Defaults to `html5`.
    pub to_format: Option<String>,
    /// Defaults to `ch` per the handoff doc — gives stable heading ids.
    pub id_prefix: Option<String>,
    /// If true, pandoc emits a standalone document (`<html><head>...`). For
    /// V1 we only need fragments — the typesetter wraps them in its own
    /// templated HTML.
    pub standalone: bool,
    /// Optional `--metadata-file=...` (typically `book.toml` rendered into
    /// pandoc-readable YAML/JSON in a later phase).
    pub metadata_file: Option<PathBuf>,
    /// Math rendering format. Valid values:
    ///   - `Some("katex")` (default) — emit `.math` spans referencing
    ///     client-side KaTeX (used by the on-screen preview where we
    ///     run KaTeX in JS).
    ///   - `Some("mathml")` — emit native MathML; rendered without JS
    ///     by Chromium 109+, used by the PDF export path.
    ///   - `Some("")` or other — pass nothing; math stays as raw TeX.
    pub math_format: Option<String>,
}

pub struct PandocConverter;

impl PandocConverter {
    /// Resolve a pandoc binary, preferring the `MARLOS_PANDOC` env var, then
    /// `pandoc` on PATH.
    pub fn resolve_binary() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MARLOS_PANDOC") {
            let path = PathBuf::from(p);
            if path.exists() {
                return Some(path);
            }
        }
        which_on_path("pandoc")
    }

    /// Convert a markdown file to HTML. The file is read by pandoc itself —
    /// we do not inline its contents.
    pub async fn convert_file(
        input: &Path,
        opts: &PandocConvertOptions,
    ) -> Result<PandocConvertResult, PandocError> {
        if !input.is_file() {
            return Err(PandocError::InputMissing(input.to_path_buf()));
        }
        let pandoc = Self::resolve_binary().ok_or(PandocError::NotFound)?;

        let from = opts
            .from_format
            .clone()
            .unwrap_or_else(|| {
                // link_attributes lets the writer add `{.class width=4in}`
                // to images for figure layout/sizing (Phase J/K).
                // Implicitly on for plain markdown but pinned here so
                // a future from_format override doesn't drop it.
                // lists_without_preceding_blankline: the manuscript writes
                // breakdown lists right under a lead-in line ("What's in
                // it:") with no blank line; without this they get swallowed
                // into the paragraph instead of rendering as a list.
                // -yaml_metadata_block: the manuscript uses `---` as
                // thematic-break dividers; without this, pandoc reads a
                // `---` after a blank line as a YAML metadata block and
                // fails parsing the prose (or an image's `!`) as YAML.
                "markdown+smart+footnotes+pipe_tables+link_attributes+lists_without_preceding_blankline-yaml_metadata_block".to_string()
            });
        let to = opts.to_format.clone().unwrap_or_else(|| "html5".to_string());
        let id_prefix = opts.id_prefix.clone().unwrap_or_else(|| "ch".to_string());

        let mut cmd = Command::new(&pandoc);
        cmd.arg(input)
            .args(["--from", &from])
            .args(["--to", &to])
            .arg("--section-divs")
            .args(["--id-prefix", &id_prefix])
            .args(["--output", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Math rendering: katex spans (default for on-screen preview)
        // or native MathML (used by the PDF export — Chromium renders
        // it without JS).
        match opts.math_format.as_deref() {
            Some("mathml") => {
                cmd.arg("--mathml");
            }
            Some("") | Some("none") => {
                // pass nothing — math stays as raw TeX
            }
            _ => {
                cmd.arg("--katex");
            }
        }

        if !opts.standalone {
            cmd.arg("--standalone=false");
        } else {
            cmd.arg("--standalone");
        }

        if let Some(meta) = &opts.metadata_file {
            cmd.arg(format!("--metadata-file={}", meta.display()));
        }

        let output = cmd.output().await.map_err(PandocError::Io)?;

        if !output.status.success() {
            return Err(PandocError::NonZeroExit {
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let html = String::from_utf8(output.stdout).map_err(|_| PandocError::InvalidUtf8)?;
        let stderr_warnings = String::from_utf8_lossy(&output.stderr).to_string();
        let pandoc_version = Self::version(&pandoc).await.unwrap_or_default();

        Ok(PandocConvertResult {
            html,
            stderr_warnings,
            pandoc_path: pandoc.display().to_string(),
            pandoc_version,
        })
    }

    /// Convert a raw markdown string. Pandoc reads from stdin.
    pub async fn convert_str(
        markdown: &str,
        opts: &PandocConvertOptions,
    ) -> Result<PandocConvertResult, PandocError> {
        let pandoc = Self::resolve_binary().ok_or(PandocError::NotFound)?;

        let from = opts
            .from_format
            .clone()
            .unwrap_or_else(|| {
                // link_attributes lets the writer add `{.class width=4in}`
                // to images for figure layout/sizing (Phase J/K).
                // Implicitly on for plain markdown but pinned here so
                // a future from_format override doesn't drop it.
                // lists_without_preceding_blankline: the manuscript writes
                // breakdown lists right under a lead-in line ("What's in
                // it:") with no blank line; without this they get swallowed
                // into the paragraph instead of rendering as a list.
                // -yaml_metadata_block: the manuscript uses `---` as
                // thematic-break dividers; without this, pandoc reads a
                // `---` after a blank line as a YAML metadata block and
                // fails parsing the prose (or an image's `!`) as YAML.
                "markdown+smart+footnotes+pipe_tables+link_attributes+lists_without_preceding_blankline-yaml_metadata_block".to_string()
            });
        let to = opts.to_format.clone().unwrap_or_else(|| "html5".to_string());
        let id_prefix = opts.id_prefix.clone().unwrap_or_else(|| "ch".to_string());

        let mut cmd = Command::new(&pandoc);
        cmd.args(["--from", &from])
            .args(["--to", &to])
            .arg("--section-divs")
            .args(["--id-prefix", &id_prefix])
            .arg(if opts.standalone { "--standalone" } else { "--standalone=false" })
            .args(["--output", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match opts.math_format.as_deref() {
            Some("mathml") => {
                cmd.arg("--mathml");
            }
            Some("") | Some("none") => {}
            _ => {
                cmd.arg("--katex");
            }
        }

        let mut child = cmd.spawn().map_err(PandocError::Io)?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(markdown.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }
        let output = child.wait_with_output().await.map_err(PandocError::Io)?;

        if !output.status.success() {
            return Err(PandocError::NonZeroExit {
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let html = String::from_utf8(output.stdout).map_err(|_| PandocError::InvalidUtf8)?;
        let stderr_warnings = String::from_utf8_lossy(&output.stderr).to_string();
        let pandoc_version = Self::version(&pandoc).await.unwrap_or_default();

        Ok(PandocConvertResult {
            html,
            stderr_warnings,
            pandoc_path: pandoc.display().to_string(),
            pandoc_version,
        })
    }

    pub async fn version(pandoc: &Path) -> Result<String, PandocError> {
        let output = Command::new(pandoc).arg("--version").output().await?;
        if !output.status.success() {
            return Err(PandocError::NonZeroExit {
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
        Ok(first_line)
    }

    /// Cheap status check used by the UI to detect whether typesetter
    /// features are usable on this machine.
    pub async fn probe() -> PandocProbe {
        match Self::resolve_binary() {
            Some(path) => match Self::version(&path).await {
                Ok(v) => PandocProbe {
                    available: true,
                    path: Some(path.display().to_string()),
                    version: Some(v),
                    error: None,
                },
                Err(e) => PandocProbe {
                    available: false,
                    path: Some(path.display().to_string()),
                    version: None,
                    error: Some(e.to_string()),
                },
            },
            None => PandocProbe {
                available: false,
                path: None,
                version: None,
                error: Some("pandoc not found on PATH".to_string()),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PandocProbe {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exe = if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    for dir in std::env::split_paths(&path) {
        let candidate: PathBuf = Path::new(&dir).join(&exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
