/// Fast image loading with turbojpeg (if available) or zune-jpeg for JPEGs
use image::DynamicImage;

pub struct FastImageLoader;

/// How much to shrink a JPEG by while decoding it, as the denominator of a fraction.
///
/// turbojpeg can decode at 1, 1/2, 1/4 or 1/8 scale essentially for free, so the largest
/// reduction that still leaves the image at or above the target is chosen.
///
/// Kept out of the decode path itself so it can be tested without a JPEG or the optional
/// turbojpeg dependency. Only the turbojpeg path can act on it, so it is gated to match.
#[cfg(any(feature = "fast-jpeg", test))]
pub fn scale_divisor(max_original: usize, target_max_dimension: u32) -> u32 {
    if max_original > (target_max_dimension * 8) as usize {
        8
    } else if max_original > (target_max_dimension * 4) as usize {
        4
    } else if max_original > (target_max_dimension * 2) as usize {
        2
    } else {
        1
    }
}

impl FastImageLoader {
    /// Load image with optimal strategy based on format and target size
    #[allow(unused_variables)]
    pub fn load_for_display(path: &str, target_max_dimension: u32) -> Result<DynamicImage, String> {
        #[cfg(all(not(test), feature = "debug-output"))]
        use std::time::Instant;
        #[cfg(all(not(test), feature = "debug-output"))]
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
                Self::load_jpeg_turbojpeg(path, target_max_dimension).or_else(|e| {
                    #[cfg(not(test))]
                    eprintln!("[TURBOJPEG] Failed: {}, falling back to image crate", e);
                    Self::load_with_image_crate(path)
                })
            }
            #[cfg(not(feature = "fast-jpeg"))]
            {
                // Fallback to image crate when turbojpeg is not available
                Self::load_with_image_crate(path)
            }
        } else {
            // Fallback: Use image crate for PNG, GIF, etc.
            Self::load_with_image_crate(path)
        };

        #[cfg(all(not(test), feature = "debug-output"))]
        match &result {
            Ok(img) => {
                #[cfg(all(not(test), feature = "debug-output"))]
                let decoder_name = if is_jpeg {
                    #[cfg(feature = "fast-jpeg")]
                    {
                        "turbojpeg"
                    }
                    #[cfg(not(feature = "fast-jpeg"))]
                    {
                        "image-crate"
                    }
                } else {
                    "image-crate"
                };
                #[cfg(all(not(test), feature = "debug-output"))]
                eprintln!(
                    "[FAST-LOADER] Loaded {}x{} in {:?} (decoder: {})",
                    img.width(),
                    img.height(),
                    load_start.elapsed(),
                    decoder_name
                );
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
        let buffer = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

        // Create decompressor
        let mut decompressor =
            Decompressor::new().map_err(|e| format!("Failed to create decompressor: {}", e))?;

        // Get image info to calculate optimal scale
        let header = decompressor
            .read_header(&buffer)
            .map_err(|e| format!("Failed to read JPEG header: {}", e))?;

        let original_width = header.width;
        let original_height = header.height;
        let max_original = original_width.max(original_height);

        // Calculate optimal scaling factor for turbojpeg
        // turbojpeg supports 1, 1/2, 1/4, 1/8 during decompression (INSTANT!)
        let scaling_factor = match scale_divisor(max_original, target_max_dimension) {
            8 => ScalingFactor::ONE_EIGHTH,
            4 => ScalingFactor::ONE_QUARTER,
            2 => ScalingFactor::ONE_HALF,
            _ => ScalingFactor::ONE,
        };

        #[cfg(all(not(test), feature = "debug-output"))]
        eprintln!(
            "[TURBOJPEG] Original: {}x{}, Target: {}, Scale: {:?}",
            original_width, original_height, target_max_dimension, scaling_factor
        );

        // Set scaling factor on decompressor (THIS IS THE KEY!)
        decompressor
            .set_scaling_factor(scaling_factor)
            .map_err(|e| format!("Failed to set scaling factor: {:?}", e))?;

        // Get scaled dimensions from header
        let scaled_header = header.scaled(scaling_factor);
        let output_width = scaled_header.width;
        let output_height = scaled_header.height;

        #[cfg(all(not(test), feature = "debug-output"))]
        eprintln!(
            "[TURBOJPEG] Scaled dimensions: {}x{}",
            output_width, output_height
        );

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
        decompressor
            .decompress(&buffer, output_image.as_deref_mut())
            .map_err(|e| format!("JPEG decompression failed: {:?}", e))?;

        #[cfg(all(not(test), feature = "debug-output"))]
        eprintln!(
            "[TURBOJPEG] Successfully decoded at: {}x{}",
            output_width, output_height
        );

        // Convert to DynamicImage
        let img_buffer =
            image::RgbImage::from_raw(output_width as u32, output_height as u32, output_buf)
                .ok_or_else(|| "Failed to create image buffer".to_string())?;

        Ok(DynamicImage::ImageRgb8(img_buffer))
    }

    /// Fallback loader using image crate
    fn load_with_image_crate(path: &str) -> Result<DynamicImage, String> {
        image::open(path).map_err(|e| format!("Failed to load image: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::scale_divisor;

    /// The previous version of this test inlined the comparisons with literal numbers, so it
    /// asserted only that Rust can do arithmetic; it never touched the loader.
    #[rstest::rstest]
    // 4032 is over 512*4 (2048) but not over 512*8 (4096).
    #[case(4032, 512, 4)]
    // Exactly 512*4, and the comparison is strict, so it falls to the next step down.
    #[case(2048, 512, 2)]
    #[case(5000, 512, 8)]
    // At or below the target, no reduction at all.
    #[case(512, 512, 1)]
    #[case(100, 512, 1)]
    // Boundaries: strictly greater is required to step up.
    #[case(1024, 512, 1)]
    #[case(1025, 512, 2)]
    #[case(4096, 512, 4)]
    #[case(4097, 512, 8)]
    fn picks_the_largest_free_reduction(
        #[case] max_original: usize,
        #[case] target: u32,
        #[case] expected: u32,
    ) {
        assert_eq!(scale_divisor(max_original, target), expected);
    }

    #[test]
    fn never_reduces_below_the_target() {
        // Decoding smaller than the target would lose detail the preview needs.
        for max_original in [600usize, 1000, 1500, 3000, 6000, 12000] {
            let target = 512u32;
            let divisor = scale_divisor(max_original, target);
            let decoded = max_original / divisor as usize;

            assert!(
                decoded >= target as usize || divisor == 1,
                "{}px reduced by 1/{} gives {}px, below the {}px target",
                max_original,
                divisor,
                decoded,
                target
            );
        }
    }
}
