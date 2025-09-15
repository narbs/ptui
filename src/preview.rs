use crate::config::PTuiConfig;
use crate::converter::{self, AsciiConverter};
use crate::file_browser::FileItem;
use crate::localization::Localization;
use ansi_to_tui::IntoText;
use ratatui::text::Text;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;

pub struct PreviewManager {
    cache: HashMap<String, Text<'static>>,
    converter: Box<dyn AsciiConverter>,
    pub debug_info: String,
}

impl PreviewManager {
    pub fn new(config: PTuiConfig) -> Self {
        let converter = converter::create_converter(&config);
        Self {
            cache: HashMap::new(),
            converter,
            debug_info: String::new(),
        }
    }

    pub fn get_debug_info(&self) -> &str {
        &self.debug_info
    }

    pub fn converter_supports_transitions(&self) -> bool {
        self.converter.supports_transitions()
    }


    pub fn set_message(&mut self, message: String) {
        self.debug_info = message;
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub fn remove_from_cache(&mut self, file: &FileItem, width: u16, height: u16) {
        let cache_key = format!("{}:{}x{}", file.path, width, height);
        self.cache.remove(&cache_key);
    }

    pub fn save_ascii_to_file(&mut self, file: &FileItem, width: u16, height: u16, localization: &Localization) -> Result<String, String> {
        if !file.is_image() {
            return Err(localization.get("selected_file_not_image").to_string());
        }

        // Generate output filename with .ascii extension
        let path = Path::new(&file.path);
        let output_path = if let Some(stem) = path.file_stem() {
            if let Some(parent) = path.parent() {
                parent.join(format!("{}.ascii", stem.to_string_lossy()))
            } else {
                Path::new(&format!("{}.ascii", stem.to_string_lossy())).to_path_buf()
            }
        } else {
            return Err("Could not determine output filename".to_string());
        };

        // Check if file already exists
        if output_path.exists() {
            return Err(format!("File already exists: {}", output_path.display()));
        }

        // Generate ASCII content using selected converter
        let (converter_width, converter_height) = self.calculate_converter_dimensions(&file.path, width, height, localization);
        let ascii_content = self.generate_ascii_content(&file.path, converter_width, converter_height)?;

        // Save to file
        match fs::write(&output_path, ascii_content) {
            Ok(_) => Ok(format!("{} {}", localization.get("saved_to"), output_path.display())),
            Err(e) => Err(format!("Failed to write file: {}", e)),
        }
    }

    fn generate_ascii_content(&self, path: &str, width: u16, height: u16) -> Result<String, String> {
        self.converter.convert_image(path, width, height)
    }

    pub fn generate_preview(&mut self, file: &FileItem, width: u16, height: u16, text_scroll_offset: usize, localization: &Localization) -> Text<'static> {
        if file.is_directory {
            self.debug_info = localization.get("directory_selected");
            return Text::from(localization.get("directory_selected"));
        }

        if file.is_image() {
            self.generate_image_preview(&file.path, width, height, localization)
        } else if file.is_ascii_file() {
            self.debug_info = format!("{}{}", localization.get("ascii_file_prefix"), file.name);
            self.generate_ascii_preview(&file.path, text_scroll_offset)
        } else if file.is_text_file() {
            self.debug_info = format!("{}{}", localization.get("text_file_prefix"), file.name);
            self.generate_text_preview(&file.path, text_scroll_offset, height)
        } else {
            self.debug_info = localization.get("file_type_not_supported");
            Text::from(localization.get("not_supported_file_type"))
        }
    }

    fn generate_image_preview(&mut self, path: &str, width: u16, height: u16, localization: &Localization) -> Text<'static> {
        let cache_key = format!("{}:{}x{}", path, width, height);
        
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.clone();
        }

        let (converter_width, converter_height) = self.calculate_converter_dimensions(path, width, height, localization);
        
        let result = self.render_with_converter(path, converter_width, converter_height);
        self.cache.insert(cache_key, result.clone());
        result
    }

    fn generate_ascii_preview(&self, path: &str, scroll_offset: usize) -> Text<'static> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Parse ANSI codes in ASCII files and convert to Text
                match content.as_bytes().into_text() {
                    Ok(mut text) => {
                        // Apply scroll offset to ASCII files as well
                        if scroll_offset > 0 && scroll_offset < text.lines.len() {
                            text.lines.drain(0..scroll_offset);
                        }
                        text
                    }
                    Err(_) => {
                        // If ANSI parsing fails, display as plain text with scroll offset
                        let lines: Vec<&str> = content.lines().collect();
                        let scrolled_lines: Vec<String> = if scroll_offset < lines.len() {
                            lines.into_iter().skip(scroll_offset).map(|s| s.to_string()).collect()
                        } else {
                            vec!["(End of file)".to_string()]
                        };
                        Text::from(scrolled_lines.join("\n"))
                    }
                }
            }
            Err(_) => Text::from("Error: Could not read ASCII file"),
        }
    }

    fn generate_text_preview(&self, path: &str, scroll_offset: usize, visible_height: u16) -> Text<'static> {
        match std::fs::File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut all_lines: Vec<String> = Vec::new();
                
                // Read all lines first
                for line in reader.lines() {
                    if let Ok(content) = line {
                        all_lines.push(content);
                        
                        // Still limit total lines to prevent excessive memory usage
                        if all_lines.len() > 10000 {
                            all_lines.push("... (file too large for scrolling, showing first 10000 lines)".to_string());
                            break;
                        }
                    } else {
                        all_lines.push("Error reading file".to_string());
                        break;
                    }
                }
                
                // Apply scroll offset and visible height
                let display_lines = if scroll_offset >= all_lines.len() {
                    // If scrolled past the end, show "end of file" message
                    vec!["(End of file)".to_string()]
                } else {
                    // Take lines starting from scroll_offset
                    let end_line = (scroll_offset + visible_height as usize).min(all_lines.len());
                    all_lines[scroll_offset..end_line].to_vec()
                };
                
                Text::from(display_lines.join("\n"))
            }
            Err(_) => Text::from("Error: Could not open file"),
        }
    }

    fn calculate_converter_dimensions(&mut self, path: &str, max_width: u16, max_height: u16, localization: &Localization) -> (u16, u16) {
        let (img_width, img_height) = ImageDimensions::get_dimensions(path);
        
        self.debug_info = format!(
            "{}{}",
            localization.get("image_file_prefix"),
            std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        
        if img_width == 0 || img_height == 0 {
            self.debug_info = format!("{} | Using fallback dimensions", self.debug_info);
            return (max_width, max_height);
        }

        let char_aspect_ratio_height = 3.0;
        let effective_max_width = max_width;
        let effective_max_height = max_height;
        
        let img_aspect_ratio = img_width as f32 / img_height as f32;
        
        let width_constrained_width = effective_max_width;
        let width_constrained_height = 
            ((width_constrained_width as f32) / img_aspect_ratio) as u16;
        
        let height_constrained_height = effective_max_height;
        let height_constrained_width = 
            ((height_constrained_height as f32) * img_aspect_ratio * char_aspect_ratio_height) as u16;
        
        let (final_width, final_height) = if width_constrained_height <= effective_max_height {
            (width_constrained_width, width_constrained_height)
        } else {
            (height_constrained_width.min(effective_max_width), height_constrained_height)
        };
        
        (final_width, final_height)
    }

    fn render_with_converter(&mut self, path: &str, width: u16, height: u16) -> Text<'static> {
        match self.converter.convert_image(path, width, height) {
            Ok(output) => {
                match output.as_bytes().into_text() {
                    Ok(text) => text,
                    Err(_) => Text::from("Failed to parse ANSI output"),
                }
            }
            Err(e) => {
                self.debug_info = format!("{} error: {}", self.converter.get_name(), e);
                Text::from(format!("Failed to execute {}: {}", self.converter.get_name(), e))
            }
        }
    }

    pub fn update_config(&mut self, config: PTuiConfig) {
        self.converter = converter::create_converter(&config);
        // Clear cache since converter settings changed
        self.clear_cache();
    }
}

struct ImageDimensions;

impl ImageDimensions {
    fn get_dimensions(path: &str) -> (u32, u32) {
        if let Ok(output) = Command::new("identify")
            .args(["-format", "%w %h", path])
            .output()
            && output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = output_str.split_whitespace().collect();
                if parts.len() >= 2
                    && let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        return (w, h);
                    }
            }
        
        if let Ok(output) = Command::new("file").arg(path).output()
            && output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if let Some(dimensions) = Self::extract_dimensions_from_file_output(&output_str) {
                    return dimensions;
                }
            }
        
        (800, 600) // Default fallback
    }

    fn extract_dimensions_from_file_output(output: &str) -> Option<(u32, u32)> {
        let words: Vec<&str> = output.split_whitespace().collect();
        
        // First try: look for "width x height" pattern
        for i in 0..words.len().saturating_sub(2) {
            if let Ok(w) = words[i].parse::<u32>()
                && words.get(i + 1).is_some_and(|s| *s == "x" || *s == "×")
                    && let Some(h) = words.get(i + 2).and_then(|s| s.parse::<u32>().ok()) {
                        return Some((w, h));
                    }
        }
        
        // Second try: look for "widthxheight" in single words
        for word in &words {
            if let Some(x_pos) = word.find('x') {
                let (w_str, h_str) = word.split_at(x_pos);
                let h_str = &h_str[1..];
                if let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                    return Some((w, h));
                }
            }
            
            if let Some(x_pos) = word.find('×') {
                let (w_str, h_str) = word.split_at(x_pos);
                let h_str = &h_str[3..]; // Remove the '×' (3 bytes in UTF-8)
                if let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                    return Some((w, h));
                }
            }
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::helpers::*;
    use crate::localization::Localization;

    #[test]
    fn test_preview_manager_creation() {
        let config = create_test_config();
        let manager = PreviewManager::new(config);
        
        assert!(manager.cache.is_empty());
        assert_eq!(manager.debug_info, "");
        assert_eq!(manager.converter.get_name(), "chafa");
    }

    #[test]
    fn test_preview_manager_cache_operations() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let file_item = create_test_image_file_item("test");
        
        manager.remove_from_cache(&file_item, 80, 24);
        
        manager.clear_cache();
        assert!(manager.cache.is_empty());
    }

    #[test]
    fn test_preview_manager_directory_preview() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        let dir_item = create_test_directory_item("test_dir");
        
        let preview = manager.generate_preview(&dir_item, 80, 24, 0, &localization);
        
        assert_eq!(manager.debug_info, localization.get("directory_selected"));
        assert!(!preview.lines.is_empty());
    }

    #[test]
    fn test_preview_manager_text_file_preview() {
        let temp_fs = TestFileSystem::new().unwrap();
        let file_path = temp_fs.create_file("test.txt", "Line 1\nLine 2\nLine 3").unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "test.txt".to_string(),
            file_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let preview = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        
        assert!(manager.debug_info.contains("test.txt"));
        assert!(!preview.lines.is_empty());
    }

    #[test]
    fn test_preview_manager_ascii_file_preview() {
        let temp_fs = TestFileSystem::new().unwrap();
        let ascii_content = "\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m";
        let file_path = temp_fs.create_file("test.ascii", ascii_content).unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "test.ascii".to_string(),
            file_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let preview = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        
        assert!(manager.debug_info.contains("test.ascii"));
        assert!(!preview.lines.is_empty());
    }

    #[test]
    fn test_preview_manager_unsupported_file() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        let unsupported_item = create_test_file_item("test.xyz", false);
        
        let preview = manager.generate_preview(&unsupported_item, 80, 24, 0, &localization);
        
        assert_eq!(manager.debug_info, localization.get("file_type_not_supported"));
        assert!(!preview.lines.is_empty());
    }

    #[test]
    fn test_preview_manager_image_caching() {
        let temp_fs = TestFileSystem::new().unwrap();
        let image_path = temp_fs.create_test_image("test.jpg").unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "test.jpg".to_string(),
            image_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let _ = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        let cache_size_after_first = manager.cache.len();
        
        let _ = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        let cache_size_after_second = manager.cache.len();
        
        assert_eq!(cache_size_after_first, cache_size_after_second);
    }

    #[test]
    fn test_preview_manager_save_ascii_to_file() {
        let temp_fs = TestFileSystem::new().unwrap();
        let image_path = temp_fs.create_test_image("source.jpg").unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "source.jpg".to_string(),
            image_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let result = manager.save_ascii_to_file(&file_item, 80, 24, &localization);
        
        match result {
            Ok(message) => {
                assert!(message.contains(localization.get("saved_to").as_str()));
            },
            Err(e) => {
                assert!(e.contains("chafa") || e.contains("Failed"));
            }
        }
    }

    #[test]
    fn test_preview_manager_save_ascii_non_image_file() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        let text_item = create_test_text_file_item("document");
        
        let result = manager.save_ascii_to_file(&text_item, 80, 24, &localization);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not an image"));
    }

    #[test]
    fn test_preview_manager_text_file_scrolling() {
        let temp_fs = TestFileSystem::new().unwrap();
        let test_content = (0..50).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
        let file_path = temp_fs.create_file("scrollable.txt", &test_content).unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "scrollable.txt".to_string(),
            file_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        // Test scrolling from the beginning (scroll_offset = 0)
        let preview1 = manager.generate_preview(&file_item, 80, 10, 0, &localization);
        let content1 = preview1.lines.iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        
        // Test scrolling with offset
        let preview2 = manager.generate_preview(&file_item, 80, 10, 5, &localization);
        let content2 = preview2.lines.iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        
        // The first preview should start with "Line 0"
        assert!(content1.contains("Line 0"));
        // With height=10, we should only see lines 0-9, not line 10 or higher
        assert!(!content1.contains("Line 10"));
        
        // The second preview should start with "Line 5" due to scroll offset
        assert!(content2.contains("Line 5"));
        assert!(!content2.contains("Line 0"));
    }

    #[test]
    fn test_preview_manager_text_file_line_limit() {
        let temp_fs = TestFileSystem::new().unwrap();
        // Create a file with more than 10000 lines to trigger the limit
        let large_content = (0..10002).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
        let file_path = temp_fs.create_file("large.txt", &large_content).unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "large.txt".to_string(),
            file_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        // Test with a large height parameter to see if limit is reached
        let preview = manager.generate_preview(&file_item, 80, 15000, 0, &localization);
        let content = preview.lines.iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        
        // Should show the limit message since we have more than 10000 lines
        assert!(content.contains("file too large for scrolling"));
    }

    #[test]
    fn test_image_dimensions_fallback() {
        let (width, height) = ImageDimensions::get_dimensions("nonexistent_file.jpg");
        assert_eq!(width, 800);
        assert_eq!(height, 600);
    }

    #[test]
    fn test_image_dimensions_extract_from_file_output() {
        let output_with_x_separator = "test.jpg: JPEG image data 1920 x 1080 quality 85%";
        let result = ImageDimensions::extract_dimensions_from_file_output(output_with_x_separator);
        assert_eq!(result, Some((1920, 1080)));
        
        let output_without_dimensions = "test.jpg: ASCII text";
        let result = ImageDimensions::extract_dimensions_from_file_output(output_without_dimensions);
        assert_eq!(result, None);
        
        let output_with_unicode_x = "test.jpg: PNG image data 800 × 600 8-bit/color RGBA";
        let result = ImageDimensions::extract_dimensions_from_file_output(output_with_unicode_x);
        assert_eq!(result, Some((800, 600)));
        
        let output_with_compact_format = "test.jpg: JPEG 1920x1080 24-bit";
        let result = ImageDimensions::extract_dimensions_from_file_output(output_with_compact_format);
        assert_eq!(result, Some((1920, 1080)));
    }

    #[test]
    fn test_preview_manager_debug_info() {
        let config = create_test_config();
        let manager = PreviewManager::new(config);
        
        assert_eq!(manager.get_debug_info(), "");
    }

    #[test]
    fn test_preview_manager_set_message() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        
        assert_eq!(manager.get_debug_info(), "");
        
        manager.set_message("Test message".to_string());
        assert_eq!(manager.get_debug_info(), "Test message");
        
        manager.set_message("Another message".to_string());
        assert_eq!(manager.get_debug_info(), "Another message");
    }

    #[test]
    fn test_preview_manager_cache_key_format() {
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let file_item = create_test_image_file_item("test");
        
        manager.remove_from_cache(&file_item, 100, 50);
        
        assert!(manager.cache.is_empty());
    }

    #[test]
    fn test_preview_manager_calculate_converter_dimensions() {
        let temp_fs = TestFileSystem::new().unwrap();
        let image_path = temp_fs.create_test_image("test.jpg").unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let (width, height) = manager.calculate_converter_dimensions(&image_path, 80, 24, &localization);
        
        assert!(width > 0);
        assert!(height > 0);
        assert!(width <= 80);
        assert!(height <= 24);
    }

    #[rstest::rstest]
    #[case("jpg")]
    #[case("png")]
    #[case("gif")]
    fn test_preview_manager_image_extensions(#[case] ext: &str) {
        let temp_fs = TestFileSystem::new().unwrap();
        let image_path = temp_fs.create_test_image(&format!("test.{}", ext)).unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            format!("test.{}", ext),
            image_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let preview = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        assert!(!preview.lines.is_empty());
    }

    #[test]
    fn test_preview_manager_empty_file() {
        let temp_fs = TestFileSystem::new().unwrap();
        let file_path = temp_fs.create_file("empty.txt", "").unwrap();
        
        let config = create_test_config();
        let mut manager = PreviewManager::new(config);
        let localization = Localization::new("en").unwrap();
        
        let file_item = FileItem::new(
            "empty.txt".to_string(),
            file_path,
            false,
            std::time::UNIX_EPOCH,
        );
        
        let preview = manager.generate_preview(&file_item, 80, 24, 0, &localization);
        assert!(!preview.lines.is_empty());
    }
}