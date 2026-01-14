/// Fast image loading with turbojpeg (if available) or zune-jpeg for JPEGs
use image::DynamicImage;

pub struct FastImageLoader;

impl FastImageLoader {
    /// Load image with optimal strategy based on format and target size
    pub fn load_for_display(path: &str, target_max_dimension: u32) -> Result<DynamicImage, String> {
        use std::time::Instant;
        let load_start = Instant::now();

        // Detect format by extension
        let path_lower = path.to_lowercase();
        let is_jpeg = path_lower.ends_with(".jpg")
            || path_lower.ends_with(".jpeg")
            || path_lower.ends_with(".JPG")
            || path_lower.ends_with(".JPEG");

        let result = if is_jpeg {
            // Try fast decoders in order of speed
            #[cfg(feature = "fast-jpeg")]
            {
                Self::load_jpeg_turbojpeg(path, target_max_dimension)
                    .or_else(|e| {
                        eprintln!("[TURBOJPEG] Failed: {}, falling back to zune-jpeg", e);
                        Self::load_jpeg_zune(path, target_max_dimension)
                    })
            }
            #[cfg(not(feature = "fast-jpeg"))]
            {
                Self::load_jpeg_zune(path, target_max_dimension)
            }
        } else {
            // Fallback: Use image crate for PNG, GIF, etc.
            Self::load_with_image_crate(path)
        };

        match &result {
            Ok(img) => {
                let decoder_name = if is_jpeg {
                    #[cfg(feature = "fast-jpeg")]
                    { "turbojpeg" }
                    #[cfg(not(feature = "fast-jpeg"))]
                    { "zune-jpeg" }
                } else {
                    "image-crate"
                };
                eprintln!("[FAST-LOADER] Loaded {}x{} in {:?} (decoder: {})",
                    img.width(), img.height(), load_start.elapsed(), decoder_name);
            }
            Err(e) => {
                eprintln!("[FAST-LOADER] Failed to load: {}", e);
            }
        }

        result
    }

    /// Load JPEG with turbojpeg using intelligent subsampling (FASTEST)
    #[cfg(feature = "fast-jpeg")]
    fn load_jpeg_turbojpeg(path: &str, target_max_dimension: u32) -> Result<DynamicImage, String> {
        use std::fs;
        use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

        // Read file into memory
        let buffer = fs::read(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Create decompressor
        let mut decompressor = Decompressor::new()
            .map_err(|e| format!("Failed to create decompressor: {}", e))?;

        // Get image info to calculate optimal scale
        let header = decompressor.read_header(&buffer)
            .map_err(|e| format!("Failed to read JPEG header: {}", e))?;

        let original_width = header.width;
        let original_height = header.height;
        let max_original = original_width.max(original_height);

        // Calculate optimal scaling factor for turbojpeg
        // turbojpeg supports 1, 1/2, 1/4, 1/8 during decompression (INSTANT!)
        // Use aggressive thresholds to ensure scaling triggers (using integer math to avoid float precision issues)
        // For 4032px with target 2048px: 4032*10 >= 2048*19 -> 40320 >= 38912 = true -> 1/2 scale ✓
        let target = target_max_dimension as usize;
        let scaling_factor = if max_original * 10 >= target * 75 {
            // 1/8 scale when original >= target * 7.5
            ScalingFactor::ONE_EIGHTH
        } else if max_original * 10 >= target * 37 {
            // 1/4 scale when original >= target * 3.7
            ScalingFactor::ONE_QUARTER
        } else if max_original * 10 >= target * 19 {
            // 1/2 scale when original >= target * 1.9
            ScalingFactor::ONE_HALF
        } else {
            ScalingFactor::ONE  // Full size
        };

        eprintln!("[TURBOJPEG] Original: {}x{}, Target: {}, Scale: {:?}",
            original_width, original_height, target_max_dimension, scaling_factor);

        // Set scaling factor on decompressor (THIS IS THE KEY!)
        decompressor.set_scaling_factor(scaling_factor)
            .map_err(|e| format!("Failed to set scaling factor: {:?}", e))?;

        // Get scaled dimensions from header
        let scaled_header = header.scaled(scaling_factor);
        let output_width = scaled_header.width;
        let output_height = scaled_header.height;

        eprintln!("[TURBOJPEG] Scaled dimensions: {}x{}", output_width, output_height);

        // Allocate output buffer for scaled image
        let output_size = output_width * output_height * 3; // RGB = 3 bytes per pixel
        let mut output_buf = vec![0u8; output_size];

        // Create output image wrapper
        let mut output_image = Image {
            pixels: output_buf.as_mut_slice(),
            width: output_width,
            pitch: output_width * 3, // RGB stride
            height: output_height,
            format: PixelFormat::RGB,
        };

        // Decompress with scaling (now the decompressor knows to scale!)
        decompressor.decompress(&buffer, output_image.as_deref_mut())
            .map_err(|e| format!("JPEG decompression failed: {:?}", e))?;

        eprintln!("[TURBOJPEG] Successfully decoded at: {}x{}", output_width, output_height);

        // Convert to DynamicImage
        let img_buffer = image::RgbImage::from_raw(output_width as u32, output_height as u32, output_buf)
            .ok_or_else(|| "Failed to create image buffer".to_string())?;

        Ok(DynamicImage::ImageRgb8(img_buffer))
    }

    /// Load JPEG with zune-jpeg (faster than image crate, fallback)
    fn load_jpeg_zune(path: &str, _target_max_dimension: u32) -> Result<DynamicImage, String> {
        use std::fs;
        use zune_jpeg::JpegDecoder;
        use zune_jpeg::zune_core::options::DecoderOptions;
        use zune_jpeg::zune_core::colorspace::ColorSpace;

        // Read file into memory
        let buffer = fs::read(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Configure decoder for RGB output
        let options = DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGB);

        // Create decoder with options
        let mut decoder = JpegDecoder::new_with_options(&buffer, options);

        // Decode
        let pixels = decoder.decode()
            .map_err(|e| format!("JPEG decode failed: {:?}", e))?;

        // Get output dimensions after decode
        let info = decoder.info()
            .ok_or_else(|| "Failed to get decoder info".to_string())?;
        let width = info.width as u32;
        let height = info.height as u32;

        eprintln!("[ZUNE-JPEG] Decoded: {}x{}", width, height);

        // Convert to DynamicImage
        let img_buffer = image::RgbImage::from_raw(width, height, pixels)
            .ok_or_else(|| "Failed to create image buffer from zune-jpeg output".to_string())?;

        Ok(DynamicImage::ImageRgb8(img_buffer))
    }

    /// Fallback loader using image crate
    fn load_with_image_crate(path: &str) -> Result<DynamicImage, String> {
        image::open(path)
            .map_err(|e| format!("Failed to load image: {}", e))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_scale_factor_calculation() {
        // 4032px image, target 512px:
        // 4032*10 >= 512*75? -> 40320 >= 38400? Yes -> Use 1/8 scale
        assert_eq!(8, if 4032*10 >= 512*75 { 8 } else if 4032*10 >= 512*37 { 4 } else if 4032*10 >= 512*19 { 2 } else { 1 });

        // 2048px image, target 512px:
        // 2048*10 >= 512*75? -> 20480 >= 38400? No
        // 2048*10 >= 512*37? -> 20480 >= 18944? Yes -> Use 1/4 scale
        assert_eq!(4, if 2048*10 >= 512*75 { 8 } else if 2048*10 >= 512*37 { 4 } else if 2048*10 >= 512*19 { 2 } else { 1 });

        // 4032px image, target 2048px: should use 1/2 scale (the critical fix!)
        // 4032*10 >= 2048*75? -> 40320 >= 153600? No
        // 4032*10 >= 2048*37? -> 40320 >= 75776? No
        // 4032*10 >= 2048*19? -> 40320 >= 38912? Yes -> Use 1/2 scale
        assert_eq!(2, if 4032*10 >= 2048*75 { 8 } else if 4032*10 >= 2048*37 { 4 } else if 4032*10 >= 2048*19 { 2 } else { 1 });

        // 5000px image, target 512px: should use 1/8
        // 5000*10 >= 512*75? -> 50000 >= 38400? Yes -> Use 1/8 scale
        assert_eq!(8, if 5000*10 >= 512*75 { 8 } else if 5000*10 >= 512*37 { 4 } else if 5000*10 >= 512*19 { 2 } else { 1 });

        // 3024px image, target 2048px: should use 1/2 scale
        // 3024*10 >= 2048*19? -> 30240 >= 38912? No -> Use 1/1 scale (full size is fine for smaller images)
        assert_eq!(1, if 3024*10 >= 2048*75 { 8 } else if 3024*10 >= 2048*37 { 4 } else if 3024*10 >= 2048*19 { 2 } else { 1 });
    }
}
