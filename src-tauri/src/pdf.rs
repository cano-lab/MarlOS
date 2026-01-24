//! PDF handling with PDFium for accurate rendering and measurement
//!
//! Uses Google's PDFium library for pixel-perfect PDF rendering.
//! Provides coordinate transformation for accurate real-world measurements.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("PDFium error: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("PDF not loaded")]
    NotLoaded,
    #[error("Invalid page number: {0}")]
    InvalidPage(u32),
    #[error("Lock error")]
    Lock,
    #[error("Library not found")]
    LibraryNotFound,
}

pub type Result<T> = std::result::Result<T, PdfError>;

/// Page dimensions and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    /// Page index (0-based)
    pub index: u32,
    /// Width in points (72 points = 1 inch)
    pub width_points: f32,
    /// Height in points
    pub height_points: f32,
    /// Width in inches
    pub width_inches: f32,
    /// Height in inches
    pub height_inches: f32,
    /// Width in millimeters
    pub width_mm: f32,
    /// Height in millimeters
    pub height_mm: f32,
    /// Page rotation (0, 90, 180, 270)
    pub rotation: i32,
}

/// PDF document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfInfo {
    pub path: String,
    pub page_count: u32,
    pub pages: Vec<PageInfo>,
    pub title: Option<String>,
    pub author: Option<String>,
}

/// Rendered page as base64 image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPage {
    pub page_index: u32,
    /// Base64 encoded PNG image
    pub image_data: String,
    /// Rendered width in pixels
    pub width_px: u32,
    /// Rendered height in pixels
    pub height_px: u32,
    /// DPI used for rendering
    pub dpi: f32,
    /// Scale factor (pixels per point)
    pub scale: f32,
}

/// Measurement result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    /// Distance in points
    pub points: f64,
    /// Distance in inches
    pub inches: f64,
    /// Distance in millimeters
    pub mm: f64,
    /// Distance in centimeters
    pub cm: f64,
}

/// Point in PDF coordinates (points from bottom-left)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPoint {
    pub x: f64,
    pub y: f64,
}

/// Stored document state (just bytes and metadata, no Pdfium references)
struct LoadedDocument {
    path: PathBuf,
    bytes: Vec<u8>,
    info: PdfInfo,
}

/// PDF Manager state - stores document data, creates Pdfium on demand
pub struct PdfState {
    current_document: Option<LoadedDocument>,
}

impl PdfState {
    pub fn new() -> Self {
        Self {
            current_document: None,
        }
    }
}

/// Thread-safe PDF Manager wrapper
pub struct PdfManager {
    state: Mutex<PdfState>,
}

// Implement Send + Sync for PdfManager since we only store raw bytes
unsafe impl Send for PdfManager {}
unsafe impl Sync for PdfManager {}

impl PdfManager {
    /// Create a new PDF manager
    pub fn new() -> Result<Self> {
        // Just verify we can load PDFium at startup
        let _ = Self::create_pdfium()?;
        log::info!("PDFium library verified successfully");

        Ok(Self {
            state: Mutex::new(PdfState::new()),
        })
    }

    /// Create a Pdfium instance (called per-operation)
    fn create_pdfium() -> Result<Pdfium> {
        let bindings = Pdfium::bind_to_library(
            Pdfium::pdfium_platform_library_name_at_path("./")
        )
        .or_else(|_| Pdfium::bind_to_library(
            Pdfium::pdfium_platform_library_name_at_path("./lib/")
        ))
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| {
            log::error!("Failed to load PDFium library: {:?}", e);
            PdfError::LibraryNotFound
        })?;

        Ok(Pdfium::new(bindings))
    }

    /// Open a PDF file
    pub fn open(&self, path: &str) -> Result<PdfInfo> {
        let path = Path::new(path);
        let bytes = std::fs::read(path)?;

        // Create Pdfium instance for this operation
        let pdfium = Self::create_pdfium()?;

        // Load document to get info
        let document = pdfium.load_pdf_from_byte_slice(&bytes, None)?;

        let page_count = document.pages().len() as u32;
        let mut pages = Vec::with_capacity(page_count as usize);

        for (i, page) in document.pages().iter().enumerate() {
            let width_points = page.width().value;
            let height_points = page.height().value;

            pages.push(PageInfo {
                index: i as u32,
                width_points,
                height_points,
                width_inches: width_points / 72.0,
                height_inches: height_points / 72.0,
                width_mm: width_points / 72.0 * 25.4,
                height_mm: height_points / 72.0 * 25.4,
                rotation: match page.rotation() {
                    Ok(r) => match r {
                        PdfPageRenderRotation::None => 0,
                        PdfPageRenderRotation::Degrees90 => 90,
                        PdfPageRenderRotation::Degrees180 => 180,
                        PdfPageRenderRotation::Degrees270 => 270,
                    },
                    Err(_) => 0,
                },
            });
        }

        // Try to get metadata
        let metadata = document.metadata();
        let title = metadata.get(PdfDocumentMetadataTagType::Title).map(|t| t.value().to_string());
        let author = metadata.get(PdfDocumentMetadataTagType::Author).map(|t| t.value().to_string());

        // Drop document before storing bytes (document borrows bytes)
        drop(document);

        let info = PdfInfo {
            path: path.display().to_string(),
            page_count,
            pages,
            title,
            author,
        };

        // Store for later use
        let mut state = self.state.lock().map_err(|_| PdfError::Lock)?;
        state.current_document = Some(LoadedDocument {
            path: path.to_path_buf(),
            bytes,
            info: info.clone(),
        });

        log::info!("Opened PDF: {} ({} pages)", path.display(), page_count);

        Ok(info)
    }

    /// Render a page at specified DPI
    pub fn render_page(&self, page_index: u32, dpi: f32) -> Result<RenderedPage> {
        let state = self.state.lock().map_err(|_| PdfError::Lock)?;
        let loaded = state.current_document.as_ref().ok_or(PdfError::NotLoaded)?;

        if page_index >= loaded.info.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }

        // Create Pdfium instance for this operation
        let pdfium = Self::create_pdfium()?;

        // Load document from bytes
        let document = pdfium.load_pdf_from_byte_slice(&loaded.bytes, None)?;
        let page = document.pages().get(page_index as u16)?;

        // Calculate render dimensions
        let scale = dpi / 72.0; // 72 points per inch
        let width_px = (page.width().value * scale) as u32;
        let height_px = (page.height().value * scale) as u32;

        // Render to bitmap
        let bitmap = page.render_with_config(&PdfRenderConfig::new()
            .set_target_width(width_px as i32)
            .set_target_height(height_px as i32)
            .render_form_data(true)
            .render_annotations(true)
        )?;

        // Convert to PNG
        let img = bitmap.as_image();
        let mut png_bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;

        let image_data = BASE64.encode(&png_bytes);

        Ok(RenderedPage {
            page_index,
            image_data,
            width_px,
            height_px,
            dpi,
            scale,
        })
    }

    /// Get current document info
    pub fn get_info(&self) -> Result<PdfInfo> {
        let state = self.state.lock().map_err(|_| PdfError::Lock)?;
        state.current_document.as_ref()
            .map(|d| d.info.clone())
            .ok_or(PdfError::NotLoaded)
    }

    /// Calculate distance between two points in PDF coordinates
    pub fn measure_distance(&self, page_index: u32, p1: PdfPoint, p2: PdfPoint) -> Result<Measurement> {
        let state = self.state.lock().map_err(|_| PdfError::Lock)?;
        let loaded = state.current_document.as_ref().ok_or(PdfError::NotLoaded)?;

        if page_index >= loaded.info.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }

        // Calculate distance in points
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let distance_points = (dx * dx + dy * dy).sqrt();

        // Convert to real-world units
        let inches = distance_points / 72.0;
        let mm = inches * 25.4;
        let cm = mm / 10.0;

        Ok(Measurement {
            points: distance_points,
            inches,
            mm,
            cm,
        })
    }

    /// Calculate area of a polygon in PDF coordinates
    pub fn measure_area(&self, page_index: u32, points: Vec<PdfPoint>) -> Result<Measurement> {
        let state = self.state.lock().map_err(|_| PdfError::Lock)?;
        let loaded = state.current_document.as_ref().ok_or(PdfError::NotLoaded)?;

        if page_index >= loaded.info.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }

        if points.len() < 3 {
            return Ok(Measurement {
                points: 0.0,
                inches: 0.0,
                mm: 0.0,
                cm: 0.0,
            });
        }

        // Shoelace formula for polygon area
        let mut area_points_sq = 0.0;
        let n = points.len();
        for i in 0..n {
            let j = (i + 1) % n;
            area_points_sq += points[i].x * points[j].y;
            area_points_sq -= points[j].x * points[i].y;
        }
        area_points_sq = (area_points_sq / 2.0).abs();

        // Convert to real-world units (square)
        let sq_inches = area_points_sq / (72.0 * 72.0);
        let sq_mm = sq_inches * 25.4 * 25.4;
        let sq_cm = sq_mm / 100.0;

        Ok(Measurement {
            points: area_points_sq,
            inches: sq_inches,
            mm: sq_mm,
            cm: sq_cm,
        })
    }

    /// Convert pixel coordinates to PDF points
    pub fn pixel_to_points(&self, page_index: u32, x_px: f64, y_px: f64, render_scale: f32) -> Result<PdfPoint> {
        let state = self.state.lock().map_err(|_| PdfError::Lock)?;
        let loaded = state.current_document.as_ref().ok_or(PdfError::NotLoaded)?;

        if page_index >= loaded.info.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }

        let page_info = &loaded.info.pages[page_index as usize];

        // Convert from pixel to points
        let x_points = x_px / render_scale as f64;
        // PDF coordinates are from bottom-left, but screen is from top-left
        let y_points = page_info.height_points as f64 - (y_px / render_scale as f64);

        Ok(PdfPoint {
            x: x_points,
            y: y_points,
        })
    }
}

impl Default for PdfManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize PDFium")
    }
}
