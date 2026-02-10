//! Image handling for high-resolution formats
//!
//! Supports TIFF, EXR, HDR, and standard image formats with proper
//! tone mapping and color space handling.

use std::path::Path;
use std::io::{BufReader, Cursor};
use std::fs::File;
use thiserror::Error;
use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::GenericImageView;

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("TIFF error: {0}")]
    Tiff(#[from] tiff::TiffError),
    #[error("EXR error: {0}")]
    Exr(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Invalid image data")]
    InvalidData,
}

pub type Result<T> = std::result::Result<T, ImageError>;

/// Image metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bit_depth: u32,
    pub channels: u32,
    pub color_space: String,
    pub has_alpha: bool,
    pub is_hdr: bool,
    pub file_size_bytes: u64,
}

/// Tone mapping options for HDR images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToneMapOptions {
    /// Exposure adjustment (-5 to +5 stops)
    pub exposure: f32,
    /// Gamma correction (typically 2.2)
    pub gamma: f32,
    /// Use Reinhard tone mapping
    pub reinhard: bool,
    /// Highlight compression
    pub highlight_compression: f32,
}

impl Default for ToneMapOptions {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            gamma: 2.2,
            reinhard: true,
            highlight_compression: 1.0,
        }
    }
}

/// Image manager for loading and processing images
pub struct ImageManager {
    tone_map_options: ToneMapOptions,
}

impl ImageManager {
    pub fn new() -> Self {
        Self {
            tone_map_options: ToneMapOptions::default(),
        }
    }

    /// Get image info without loading full data
    pub fn get_info(&self, path: &Path) -> Result<ImageInfo> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let file_size = std::fs::metadata(path)?.len();

        match ext.as_str() {
            "exr" => self.get_exr_info(path, file_size),
            "tif" | "tiff" => self.get_tiff_info(path, file_size),
            "hdr" => self.get_hdr_info(path, file_size),
            _ => self.get_standard_info(path, file_size),
        }
    }

    /// Load and render image to PNG bytes for display
    pub fn render_to_png(&self, path: &Path, max_dimension: Option<u32>) -> Result<Vec<u8>> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let rgba_image = match ext.as_str() {
            "exr" => self.load_exr(path)?,
            "tif" | "tiff" => self.load_tiff(path)?,
            "hdr" => self.load_hdr(path)?,
            _ => self.load_standard(path)?,
        };

        // Resize if needed
        let final_image = if let Some(max_dim) = max_dimension {
            let (w, h) = rgba_image.dimensions();
            if w > max_dim || h > max_dim {
                let scale = max_dim as f32 / w.max(h) as f32;
                let new_w = (w as f32 * scale) as u32;
                let new_h = (h as f32 * scale) as u32;
                image::imageops::resize(&rgba_image, new_w, new_h, image::imageops::FilterType::Lanczos3)
            } else {
                rgba_image
            }
        } else {
            rgba_image
        };

        // Encode to PNG
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        final_image.write_to(&mut cursor, image::ImageFormat::Png)?;

        Ok(png_bytes)
    }

    /// Render to base64 PNG for web display
    pub fn render_to_base64(&self, path: &Path, max_dimension: Option<u32>) -> Result<String> {
        let png_bytes = self.render_to_png(path, max_dimension)?;
        Ok(BASE64.encode(&png_bytes))
    }

    /// Set tone mapping options
    pub fn set_tone_map_options(&mut self, options: ToneMapOptions) {
        self.tone_map_options = options;
    }

    // ========================================================================
    // EXR Support
    // ========================================================================

    fn get_exr_info(&self, path: &Path, file_size: u64) -> Result<ImageInfo> {
        use exr::prelude::*;

        // Read just the metadata
        let meta = MetaData::read_from_file(path, false)
            .map_err(|e| ImageError::Exr(e.to_string()))?;

        let header = &meta.headers[0];
        let data_window = header.layer_size;

        Ok(ImageInfo {
            path: path.to_string_lossy().to_string(),
            width: data_window.width() as u32,
            height: data_window.height() as u32,
            format: "EXR".to_string(),
            bit_depth: 32, // EXR uses 16 or 32-bit float
            channels: header.channels.list.len() as u32,
            color_space: "Linear".to_string(),
            has_alpha: header.channels.list.iter().any(|c| c.name.to_string() == "A"),
            is_hdr: true,
            file_size_bytes: file_size,
        })
    }

    fn load_exr(&self, path: &Path) -> Result<image::RgbaImage> {
        use exr::prelude::*;

        // Copy tone mapping options before the closure to avoid lifetime issues
        let exposure = self.tone_map_options.exposure;
        let gamma = self.tone_map_options.gamma;
        let reinhard = self.tone_map_options.reinhard;
        let highlight_compression = self.tone_map_options.highlight_compression;

        // Inline tone mapping function that doesn't capture self
        let tone_map_value = move |v: f32| -> f32 {
            let exposure_scale = 2.0_f32.powf(exposure);
            let v = v * exposure_scale;
            let v = if reinhard {
                v / (1.0 + v * highlight_compression)
            } else {
                v.min(1.0)
            };
            v.powf(1.0 / gamma)
        };

        // Read the full EXR image
        let exr_image = read_first_rgba_layer_from_file(
            path,
            |resolution, _| {
                image::RgbaImage::new(resolution.width() as u32, resolution.height() as u32)
            },
            move |rgba_image, position, (r, g, b, a): (f32, f32, f32, f32)| {
                let (r, g, b) = (
                    tone_map_value(r),
                    tone_map_value(g),
                    tone_map_value(b),
                );
                rgba_image.put_pixel(
                    position.x() as u32,
                    position.y() as u32,
                    image::Rgba([
                        (r * 255.0).clamp(0.0, 255.0) as u8,
                        (g * 255.0).clamp(0.0, 255.0) as u8,
                        (b * 255.0).clamp(0.0, 255.0) as u8,
                        (a * 255.0).clamp(0.0, 255.0) as u8,
                    ]),
                );
            },
        ).map_err(|e| ImageError::Exr(e.to_string()))?;

        Ok(exr_image.layer_data.channel_data.pixels)
    }

    // ========================================================================
    // TIFF Support
    // ========================================================================

    fn get_tiff_info(&self, path: &Path, file_size: u64) -> Result<ImageInfo> {
        let file = File::open(path)?;
        let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file))?;

        let (width, height) = decoder.dimensions()?;
        let color_type = decoder.colortype()?;

        let (channels, has_alpha) = match color_type {
            tiff::ColorType::Gray(_) => (1, false),
            tiff::ColorType::GrayA(_) => (2, true),
            tiff::ColorType::RGB(_) => (3, false),
            tiff::ColorType::RGBA(_) => (4, true),
            tiff::ColorType::CMYK(_) => (4, false),
            tiff::ColorType::YCbCr(_) => (3, false),
            _ => (3, false),
        };

        let bit_depth = match color_type {
            tiff::ColorType::Gray(bits) => bits as u32,
            tiff::ColorType::GrayA(bits) => bits as u32,
            tiff::ColorType::RGB(bits) => bits as u32,
            tiff::ColorType::RGBA(bits) => bits as u32,
            _ => 8,
        };

        Ok(ImageInfo {
            path: path.to_string_lossy().to_string(),
            width,
            height,
            format: "TIFF".to_string(),
            bit_depth,
            channels,
            color_space: "sRGB".to_string(),
            has_alpha,
            is_hdr: bit_depth > 8,
            file_size_bytes: file_size,
        })
    }

    fn load_tiff(&self, path: &Path) -> Result<image::RgbaImage> {
        let file = File::open(path)?;
        let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file))?;

        let (width, height) = decoder.dimensions()?;
        let color_type = decoder.colortype()?;

        // Read the image data
        let result = decoder.read_image()?;

        let rgba = match result {
            tiff::decoder::DecodingResult::U8(data) => {
                self.tiff_u8_to_rgba(&data, width, height, color_type)?
            }
            tiff::decoder::DecodingResult::U16(data) => {
                self.tiff_u16_to_rgba(&data, width, height, color_type)?
            }
            tiff::decoder::DecodingResult::U32(data) => {
                self.tiff_u32_to_rgba(&data, width, height, color_type)?
            }
            tiff::decoder::DecodingResult::F32(data) => {
                self.tiff_f32_to_rgba(&data, width, height, color_type)?
            }
            tiff::decoder::DecodingResult::F64(data) => {
                self.tiff_f64_to_rgba(&data, width, height, color_type)?
            }
            _ => return Err(ImageError::UnsupportedFormat("Unsupported TIFF pixel format".to_string())),
        };

        Ok(rgba)
    }

    fn tiff_u8_to_rgba(&self, data: &[u8], width: u32, height: u32, color_type: tiff::ColorType) -> Result<image::RgbaImage> {
        let mut rgba = image::RgbaImage::new(width, height);
        let channels = match color_type {
            tiff::ColorType::Gray(_) => 1,
            tiff::ColorType::GrayA(_) => 2,
            tiff::ColorType::RGB(_) => 3,
            tiff::ColorType::RGBA(_) => 4,
            _ => 3,
        };

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * channels as u32) as usize;
                let pixel = match channels {
                    1 => image::Rgba([data[idx], data[idx], data[idx], 255]),
                    2 => image::Rgba([data[idx], data[idx], data[idx], data[idx + 1]]),
                    3 => image::Rgba([data[idx], data[idx + 1], data[idx + 2], 255]),
                    4 => image::Rgba([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]),
                    _ => image::Rgba([0, 0, 0, 255]),
                };
                rgba.put_pixel(x, y, pixel);
            }
        }

        Ok(rgba)
    }

    fn tiff_u16_to_rgba(&self, data: &[u16], width: u32, height: u32, color_type: tiff::ColorType) -> Result<image::RgbaImage> {
        let mut rgba = image::RgbaImage::new(width, height);
        let channels = match color_type {
            tiff::ColorType::Gray(_) => 1,
            tiff::ColorType::GrayA(_) => 2,
            tiff::ColorType::RGB(_) => 3,
            tiff::ColorType::RGBA(_) => 4,
            _ => 3,
        };

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * channels as u32) as usize;
                // Convert 16-bit to 8-bit
                let to_u8 = |v: u16| (v >> 8) as u8;

                let pixel = match channels {
                    1 => {
                        let g = to_u8(data[idx]);
                        image::Rgba([g, g, g, 255])
                    }
                    2 => {
                        let g = to_u8(data[idx]);
                        image::Rgba([g, g, g, to_u8(data[idx + 1])])
                    }
                    3 => image::Rgba([to_u8(data[idx]), to_u8(data[idx + 1]), to_u8(data[idx + 2]), 255]),
                    4 => image::Rgba([to_u8(data[idx]), to_u8(data[idx + 1]), to_u8(data[idx + 2]), to_u8(data[idx + 3])]),
                    _ => image::Rgba([0, 0, 0, 255]),
                };
                rgba.put_pixel(x, y, pixel);
            }
        }

        Ok(rgba)
    }

    fn tiff_u32_to_rgba(&self, data: &[u32], width: u32, height: u32, color_type: tiff::ColorType) -> Result<image::RgbaImage> {
        let mut rgba = image::RgbaImage::new(width, height);
        let channels = match color_type {
            tiff::ColorType::Gray(_) => 1,
            tiff::ColorType::GrayA(_) => 2,
            tiff::ColorType::RGB(_) => 3,
            tiff::ColorType::RGBA(_) => 4,
            _ => 3,
        };

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * channels as u32) as usize;
                // Convert 32-bit to 8-bit
                let to_u8 = |v: u32| (v >> 24) as u8;

                let pixel = match channels {
                    1 => {
                        let g = to_u8(data[idx]);
                        image::Rgba([g, g, g, 255])
                    }
                    2 => {
                        let g = to_u8(data[idx]);
                        image::Rgba([g, g, g, to_u8(data[idx + 1])])
                    }
                    3 => image::Rgba([to_u8(data[idx]), to_u8(data[idx + 1]), to_u8(data[idx + 2]), 255]),
                    4 => image::Rgba([to_u8(data[idx]), to_u8(data[idx + 1]), to_u8(data[idx + 2]), to_u8(data[idx + 3])]),
                    _ => image::Rgba([0, 0, 0, 255]),
                };
                rgba.put_pixel(x, y, pixel);
            }
        }

        Ok(rgba)
    }

    fn tiff_f32_to_rgba(&self, data: &[f32], width: u32, height: u32, color_type: tiff::ColorType) -> Result<image::RgbaImage> {
        let mut rgba = image::RgbaImage::new(width, height);
        let channels = match color_type {
            tiff::ColorType::Gray(_) => 1,
            tiff::ColorType::GrayA(_) => 2,
            tiff::ColorType::RGB(_) => 3,
            tiff::ColorType::RGBA(_) => 4,
            _ => 3,
        };

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * channels as u32) as usize;

                let pixel = match channels {
                    1 => {
                        let (g, _, _) = self.tone_map(data[idx], data[idx], data[idx]);
                        let g = (g * 255.0).clamp(0.0, 255.0) as u8;
                        image::Rgba([g, g, g, 255])
                    }
                    2 => {
                        let (g, _, _) = self.tone_map(data[idx], data[idx], data[idx]);
                        let g = (g * 255.0).clamp(0.0, 255.0) as u8;
                        let a = (data[idx + 1] * 255.0).clamp(0.0, 255.0) as u8;
                        image::Rgba([g, g, g, a])
                    }
                    3 => {
                        let (r, g, b) = self.tone_map(data[idx], data[idx + 1], data[idx + 2]);
                        image::Rgba([
                            (r * 255.0).clamp(0.0, 255.0) as u8,
                            (g * 255.0).clamp(0.0, 255.0) as u8,
                            (b * 255.0).clamp(0.0, 255.0) as u8,
                            255,
                        ])
                    }
                    4 => {
                        let (r, g, b) = self.tone_map(data[idx], data[idx + 1], data[idx + 2]);
                        image::Rgba([
                            (r * 255.0).clamp(0.0, 255.0) as u8,
                            (g * 255.0).clamp(0.0, 255.0) as u8,
                            (b * 255.0).clamp(0.0, 255.0) as u8,
                            (data[idx + 3] * 255.0).clamp(0.0, 255.0) as u8,
                        ])
                    }
                    _ => image::Rgba([0, 0, 0, 255]),
                };
                rgba.put_pixel(x, y, pixel);
            }
        }

        Ok(rgba)
    }

    fn tiff_f64_to_rgba(&self, data: &[f64], width: u32, height: u32, color_type: tiff::ColorType) -> Result<image::RgbaImage> {
        // Convert f64 to f32 and use the f32 path
        let f32_data: Vec<f32> = data.iter().map(|&v| v as f32).collect();
        self.tiff_f32_to_rgba(&f32_data, width, height, color_type)
    }

    // ========================================================================
    // HDR Support (Radiance RGBE)
    // ========================================================================

    fn get_hdr_info(&self, path: &Path, file_size: u64) -> Result<ImageInfo> {
        // Use the image crate to load HDR
        let img = image::open(path)?;
        let (width, height) = (img.width(), img.height());

        Ok(ImageInfo {
            path: path.to_string_lossy().to_string(),
            width,
            height,
            format: "HDR".to_string(),
            bit_depth: 32,
            channels: 3,
            color_space: "Linear".to_string(),
            has_alpha: false,
            is_hdr: true,
            file_size_bytes: file_size,
        })
    }

    fn load_hdr(&self, path: &Path) -> Result<image::RgbaImage> {
        // Load as Rgb32F and convert with tone mapping
        let img = image::open(path)?;

        // Get as RGB32F if possible, otherwise convert
        let rgb32f = img.to_rgb32f();
        let (width, height) = (rgb32f.width(), rgb32f.height());

        let mut rgba = image::RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = rgb32f.get_pixel(x, y);
                let (r, g, b) = self.tone_map(pixel[0], pixel[1], pixel[2]);
                rgba.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        (r * 255.0).clamp(0.0, 255.0) as u8,
                        (g * 255.0).clamp(0.0, 255.0) as u8,
                        (b * 255.0).clamp(0.0, 255.0) as u8,
                        255,
                    ]),
                );
            }
        }

        Ok(rgba)
    }

    // ========================================================================
    // Standard Image Support (PNG, JPG, WebP, etc.)
    // ========================================================================

    fn get_standard_info(&self, path: &Path, file_size: u64) -> Result<ImageInfo> {
        let img = image::open(path)?;
        let (width, height) = img.dimensions();

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_uppercase())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        let (channels, has_alpha) = match img.color() {
            image::ColorType::L8 | image::ColorType::L16 => (1, false),
            image::ColorType::La8 | image::ColorType::La16 => (2, true),
            image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => (3, false),
            image::ColorType::Rgba8 | image::ColorType::Rgba16 | image::ColorType::Rgba32F => (4, true),
            _ => (3, false),
        };

        let bit_depth = match img.color() {
            image::ColorType::L8 | image::ColorType::La8 | image::ColorType::Rgb8 | image::ColorType::Rgba8 => 8,
            image::ColorType::L16 | image::ColorType::La16 | image::ColorType::Rgb16 | image::ColorType::Rgba16 => 16,
            image::ColorType::Rgb32F | image::ColorType::Rgba32F => 32,
            _ => 8,
        };

        Ok(ImageInfo {
            path: path.to_string_lossy().to_string(),
            width,
            height,
            format: ext,
            bit_depth,
            channels,
            color_space: "sRGB".to_string(),
            has_alpha,
            is_hdr: bit_depth > 8,
            file_size_bytes: file_size,
        })
    }

    fn load_standard(&self, path: &Path) -> Result<image::RgbaImage> {
        let img = image::open(path)?;
        Ok(img.to_rgba8())
    }

    // ========================================================================
    // Tone Mapping
    // ========================================================================

    /// Apply tone mapping to HDR values
    fn tone_map(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let opts = &self.tone_map_options;

        // Apply exposure
        let exposure_scale = 2.0_f32.powf(opts.exposure);
        let r = r * exposure_scale;
        let g = g * exposure_scale;
        let b = b * exposure_scale;

        // Apply tone mapping
        let (r, g, b) = if opts.reinhard {
            // Reinhard tone mapping with highlight compression
            let compress = |v: f32| v / (1.0 + v * opts.highlight_compression);
            (compress(r), compress(g), compress(b))
        } else {
            // Simple clamp
            (r.min(1.0), g.min(1.0), b.min(1.0))
        };

        // Apply gamma correction
        let gamma_correct = |v: f32| v.powf(1.0 / opts.gamma);
        (gamma_correct(r), gamma_correct(g), gamma_correct(b))
    }
}

impl Default for ImageManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported image formats
pub fn supported_extensions() -> Vec<&'static str> {
    vec![
        // HDR formats
        "exr", "hdr",
        // High bit-depth
        "tif", "tiff",
        // Standard formats
        "png", "jpg", "jpeg", "webp", "gif", "bmp", "ico", "pnm", "pbm", "pgm", "ppm",
    ]
}

/// Check if a file extension is supported
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| supported_extensions().contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tone_mapping() {
        let manager = ImageManager::new();

        // Test that values > 1.0 get compressed
        let (r, g, b) = manager.tone_map(2.0, 2.0, 2.0);
        assert!(r < 1.0);
        assert!(g < 1.0);
        assert!(b < 1.0);
    }

    #[test]
    fn test_supported_extensions() {
        assert!(is_supported(Path::new("test.exr")));
        assert!(is_supported(Path::new("test.tiff")));
        assert!(is_supported(Path::new("test.png")));
        assert!(!is_supported(Path::new("test.txt")));
    }
}
