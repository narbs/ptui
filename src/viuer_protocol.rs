/// Custom Kitty protocol implementation using viuer's fast encoding approach
/// while remaining compatible with ratatui-image's StatefulProtocol trait.
///
/// This combines:
/// - ratatui-image's protocol detection and trait interface
/// - viuer's faster RGBA8 encoding and simpler escape sequences

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, Rgb};
use ratatui::{buffer::Buffer, layout::Rect};
use ratatui_image::{protocol::StatefulProtocol, Resize};

#[derive(Clone)]
pub struct ViuerKittyProtocol {
    /// The source image
    image: DynamicImage,
    /// Currently encoded escape sequence (cached)
    escape_sequence: String,
    /// The current rectangle that the image occupies
    rect: Rect,
    /// Track if we need to retransmit
    needs_retransmit: bool,
    /// Unique ID for this image in the Kitty protocol (stored for future use)
    #[allow(dead_code)]
    unique_id: u8,
    /// Maximum dimension for downscaling (configurable)
    max_dimension: u32,
}

impl ViuerKittyProtocol {
    #[allow(dead_code)]
    pub fn new(image: DynamicImage, unique_id: u8) -> Self {
        Self::new_with_config(image, unique_id, 1024)
    }

    pub fn new_with_config(image: DynamicImage, unique_id: u8, max_dimension: u32) -> Self {
        Self {
            image,
            escape_sequence: String::new(),
            rect: Rect::default(),
            needs_retransmit: true,
            unique_id,
            max_dimension,
        }
    }

    /// Encode the image using viuer's faster RGBA8 approach
    fn encode_image(&self, img: &DynamicImage, width: u16, height: u16) -> String {
        let rgba = img.to_rgba8();
        let raw = rgba.as_raw();

        // Pre-allocate result string to avoid reallocations
        // Base64 is 33% larger, plus escape codes overhead
        let estimated_size = (raw.len() * 4) / 3 + 1000;
        let mut result = String::with_capacity(estimated_size);

        // Encode in chunks to match Kitty's 4096-byte chunk expectation
        let chunk_size = 4096;
        let chunks: Vec<_> = raw.chunks(chunk_size).collect();
        let total_chunks = chunks.len();

        // First chunk with metadata
        if let Some(first_chunk) = chunks.first() {
            let encoded_chunk = general_purpose::STANDARD.encode(first_chunk);

            result.push_str(&format!(
                "\x1b_Gf=32,a=T,t=d,s={},v={},c={},r={},m={};{}\x1b\\",
                img.width(),
                img.height(),
                width,
                height,
                if total_chunks > 1 { 1 } else { 0 },
                encoded_chunk
            ));

            // Remaining chunks
            for (i, chunk) in chunks.iter().skip(1).enumerate() {
                let encoded_chunk = general_purpose::STANDARD.encode(chunk);
                let is_last = i == total_chunks - 2; // -2 because we skip(1)
                result.push_str(&format!(
                    "\x1b_Gm={};{}\x1b\\",
                    if is_last { 0 } else { 1 },
                    encoded_chunk
                ));
            }
        }

        result
    }

    /// Calculate the best fit dimensions for the image
    fn calculate_dimensions(&self, area: Rect) -> (u16, u16) {
        let img_width = self.image.width();
        let img_height = self.image.height();

        if area.width == 0 || area.height == 0 {
            return (0, 0);
        }

        // Terminal cells are roughly 2:1 aspect ratio (height:width)
        // So we need to account for this when fitting images
        let char_aspect_ratio = 2.0;

        let available_width = area.width as f32;
        let available_height = area.height as f32 * char_aspect_ratio;

        let img_aspect = img_width as f32 / img_height as f32;
        let available_aspect = available_width / available_height;

        let (fit_width, fit_height) = if img_aspect > available_aspect {
            // Image is wider, constrain by width
            let w = available_width;
            let h = w / img_aspect;
            (w, h)
        } else {
            // Image is taller, constrain by height
            let h = available_height;
            let w = h * img_aspect;
            (w, h)
        };

        // Convert back to terminal cells
        let cell_width = fit_width.min(area.width as f32) as u16;
        let cell_height = (fit_height / char_aspect_ratio).min(area.height as f32) as u16;

        (cell_width.max(1), cell_height.max(1))
    }
}

impl StatefulProtocol for ViuerKittyProtocol {
    fn needs_resize(&mut self, _resize: &Resize, area: Rect) -> Option<Rect> {
        if self.needs_retransmit {
            return Some(area);
        }

        // Check if the current rect doesn't match the area
        if self.rect.width != area.width || self.rect.height != area.height {
            Some(area)
        } else {
            None
        }
    }

    fn resize_encode(&mut self, _resize: &Resize, _background_color: Option<Rgb<u8>>, area: Rect) {
        use std::time::Instant;

        if area.width == 0 || area.height == 0 {
            return;
        }

        let total_start = Instant::now();
        let (width, height) = self.calculate_dimensions(area);

        // Downscaling based on config (default 768px for fast encoding)
        // Base64 encoding is the bottleneck - smaller images = faster encoding
        // Quality is still excellent for terminal display
        let needs_downscale = self.image.width() > self.max_dimension || self.image.height() > self.max_dimension;

        let img_to_encode = if needs_downscale {
            let scale = (self.max_dimension as f32 / self.image.width().max(self.image.height()) as f32).min(1.0);
            let new_width = (self.image.width() as f32 * scale) as u32;
            let new_height = (self.image.height() as f32 * scale) as u32;

            let resize_start = Instant::now();
            // Use fastest filter - Nearest is 10x faster than Triangle/Lanczos
            let resized = self.image.resize_exact(new_width, new_height, image::imageops::FilterType::Nearest);
            eprintln!("[TIMING] Resize {}x{} -> {}x{}: {:?}",
                self.image.width(), self.image.height(), new_width, new_height, resize_start.elapsed());
            resized
        } else {
            self.image.clone()
        };

        let encode_start = Instant::now();
        self.escape_sequence = self.encode_image(&img_to_encode, width, height);
        eprintln!("[TIMING] Base64 encode ({}x{} = {}MB): {:?}",
            img_to_encode.width(), img_to_encode.height(),
            (img_to_encode.width() * img_to_encode.height() * 4) / 1_000_000,
            encode_start.elapsed());

        self.rect = Rect::new(0, 0, width, height);
        self.needs_retransmit = false;

        eprintln!("[TIMING] resize_encode TOTAL: {:?}", total_start.elapsed());
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if self.escape_sequence.is_empty() {
            return;
        }

        // Clear the terminal screen area directly to prevent text ghosting
        // We need to write directly to stdout because previous text frames are already on the terminal
        use std::io::Write;
        let mut clear_area = String::new();
        for row in 0..area.height {
            // Position cursor at the start of each row in the preview area
            let position = format!("\x1b[{};{}H", area.top() + row + 1, area.left() + 1);
            clear_area.push_str(&position);
            // Clear to end of line (or write spaces for exact width)
            clear_area.push_str(&" ".repeat(area.width as usize));
        }
        let _ = std::io::stdout().write_all(clear_area.as_bytes());
        let _ = std::io::stdout().flush();

        // First, clear all cells in the buffer to prevent text ghosting
        // This ensures old text content doesn't show through
        for y in 0..area.height {
            for x in 0..area.width {
                buf[(area.left() + x, area.top() + y)].set_symbol(" ");
            }
        }

        // Clear the screen area by deleting all images with action 'a=d,d=a' (delete all)
        // This ensures old images don't remain visible
        let delete_all_cmd = "\x1b_Ga=d,d=a\x1b\\";

        // Write the delete-all command followed by the new image escape sequence
        let full_sequence = format!("{}{}", delete_all_cmd, &self.escape_sequence);

        // Write into the first cell of the area
        // The Kitty protocol will handle the actual image placement
        if area.width > 0 && area.height > 0 {
            buf[(area.left(), area.top())]
                .set_symbol(&full_sequence);

            // Mark other cells as skipped to prevent overwrites
            for y in 0..area.height.min(self.rect.height) {
                for x in 0..area.width.min(self.rect.width) {
                    if x > 0 || y > 0 {
                        buf[(area.left() + x, area.top() + y)].set_skip(true);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn test_viuer_protocol_creation() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
        let protocol = ViuerKittyProtocol::new(img, 1);

        assert_eq!(protocol.unique_id, 1);
        assert!(protocol.needs_retransmit);
        assert_eq!(protocol.rect.width, 0);
        assert_eq!(protocol.rect.height, 0);
    }

    #[test]
    fn test_calculate_dimensions() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(200, 100));
        let protocol = ViuerKittyProtocol::new(img, 1);

        let area = Rect::new(0, 0, 80, 24);
        let (width, height) = protocol.calculate_dimensions(area);

        assert!(width > 0);
        assert!(height > 0);
        assert!(width <= area.width);
        assert!(height <= area.height);
    }

    #[test]
    fn test_needs_resize_initial() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
        let mut protocol = ViuerKittyProtocol::new(img, 1);

        let area = Rect::new(0, 0, 40, 20);
        let resize = Resize::Fit(None);

        assert!(protocol.needs_resize(&resize, area).is_some());
    }

    #[test]
    fn test_encode_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(2, 2));
        let protocol = ViuerKittyProtocol::new(img.clone(), 1);

        let encoded = protocol.encode_image(&img, 10, 10);

        assert!(encoded.contains("\x1b_G"));
        assert!(encoded.contains("f=32")); // RGBA8 format
        assert!(encoded.contains("a=T")); // Direct placement
        assert!(encoded.contains("\x1b\\"));
    }
}
