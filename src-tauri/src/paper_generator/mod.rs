//! Paper Generator - Research Paper Generation Pipeline
//!
//! Generates research papers from collected sources through a multi-stage pipeline:
//! Sources → Chunking → Extraction → Outline → Write Sections → Review → Export
//!
//! # Architecture
//!
//! ```text
//! ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
//! │ Sources  │───▶│ Chunking │───▶│ Extract  │───▶│ Outline  │───▶│ Write    │
//! │          │    │ Semantic │    │ Findings │    │ Generate │    │ Sections │
//! └──────────┘    └──────────┘    └──────────┘    └──────────┘    └──────────┘
//!                      ↓              ↓              ↓              ↓
//!                  Semantic       Key Findings   Structure      Content
//!                  Chunks         + Quotes       + Flow         + Citations
//! ```
//!
//! # Usage
//!
//! ```rust
//! use paper_generator::{Paper, PaperPipeline};
//!
//! let mut pipeline = PaperPipeline::new(
//!     ai_manager,
//!     semantic_search,
//!     Paper::new("My Research Paper", "What is the impact of X on Y?")
//! );
//!
//! pipeline.add_sources(source_ids).await?;
//! pipeline.chunk_sources().await?;
//! pipeline.extract_findings().await?;
//! pipeline.generate_outline(None).await?;
//! pipeline.write_all_sections().await?;
//!
//! let markdown = pipeline.export_markdown()?;
//! ```

pub mod paper;
pub mod chunk;
pub mod store;
pub mod pipeline;
pub mod citation_tracker;
pub mod export;
pub mod latex_export;
pub mod cross_ref;
pub mod templates;

// Re-export main types
pub use paper::*;
pub use chunk::*;
pub use store::*;
pub use pipeline::*;
pub use citation_tracker::*;
pub use export::*;
pub use latex_export::*;
