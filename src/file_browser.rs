use content_inspector::{ContentType, inspect};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

// Buffer size for reading file content for magic byte detection and content inspection
// Most image formats need only a few bytes for magic byte detection:
// - JPEG: 3 bytes (0xFF, 0xD8, 0xFF)
// - PNG: 8 bytes (0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
// - GIF: 6 bytes ("GIF87a" or "GIF89a")
// - WebP: 12 bytes ("RIFF" + 4 byte size + "WEBP")
// - BMP: 2 bytes (0x42, 0x4D)
// - TIFF: 4 bytes (0x49, 0x49, 0x2A, 0x00 or 0x4D, 0x4D, 0x00, 0x2A)
// - SVG: may need more bytes due to XML declarations, comments, and DOCTYPE declarations
//   before the <svg tag appears. Using 512 bytes to handle SVG files reliably.
// - Text encoding detection also works well within this range
// Using 512 bytes provides better SVG detection while maintaining good performance
const CONTENT_DETECTION_BUFFER_SIZE: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum SortMode {
    Name,
    DateNewestFirst,
    DateOldestFirst,
}

#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub modified: SystemTime,
}

impl FileItem {
    pub fn new(name: String, path: String, is_directory: bool, modified: SystemTime) -> Self {
        Self {
            name,
            path,
            is_directory,
            modified,
        }
    }

    pub fn is_image(&self) -> bool {
        if self.is_directory {
            return false;
        }

        // Read only the first few bytes for content inspection - sufficient for magic bytes and basic detection
        if let Ok(mut file) = std::fs::File::open(&self.path) {
            let mut buffer = [0u8; CONTENT_DETECTION_BUFFER_SIZE];
            if let Ok(bytes_read) = file.read(&mut buffer) {
                let sample = &buffer[..bytes_read];
                match inspect(sample) {
                    ContentType::BINARY => {
                        // For binary files, check if it's a known image format by magic bytes
                        if sample.len() >= 4 {
                            // Check for common image magic bytes
                            if sample.starts_with(&[0xFF, 0xD8, 0xFF]) {
                                // JPEG
                                return true;
                            }
                            if sample.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                                // PNG
                                return true;
                            }
                            if sample.starts_with(b"GIF8") {
                                // GIF
                                return true;
                            }
                            if sample.starts_with(b"RIFF")
                                && sample.len() >= 12
                                && &sample[8..12] == b"WEBP"
                            {
                                // WebP
                                return true;
                            }
                            if sample.starts_with(&[0x42, 0x4D]) {
                                // BMP
                                return true;
                            }
                            if sample.starts_with(&[0x49, 0x49, 0x2A, 0x00])
                                || sample.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
                            {
                                // TIFF
                                return true;
                            }
                        }
                        false
                    }
                    ContentType::UTF_8 => {
                        // Check if it's SVG (XML-based image format)
                        let content = String::from_utf8_lossy(sample);
                        // Look for various SVG indicators in the content
                        content.contains("<svg")
                            || content.contains("</svg>")
                            || content.contains("<SVG")
                            || content.contains("</SVG>")
                            || (content.contains("<?xml") && content.to_lowercase().contains("svg"))
                    }
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn is_ascii_file(&self) -> bool {
        if self.is_directory {
            return false;
        }

        if let Some(ext) = self.path.rsplit('.').next() {
            ext.to_lowercase() == "ascii"
        } else {
            false
        }
    }

    pub fn is_text_file(&self) -> bool {
        if self.is_directory {
            return false;
        }

        // Skip ASCII files by extension check only (cheap operation)
        // NOTE: Caller must check is_image() before calling this method to avoid redundant file reads
        if self.is_ascii_file() {
            return false;
        }

        // Read only the first few bytes for content inspection - sufficient for text encoding detection
        if let Ok(mut file) = std::fs::File::open(&self.path) {
            let mut buffer = [0u8; CONTENT_DETECTION_BUFFER_SIZE];
            if let Ok(bytes_read) = file.read(&mut buffer) {
                let sample = &buffer[..bytes_read];
                match inspect(sample) {
                    ContentType::UTF_8 | ContentType::UTF_8_BOM => true,
                    ContentType::UTF_16LE | ContentType::UTF_16BE => true,
                    ContentType::UTF_32LE | ContentType::UTF_32BE => true,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn can_preview(&self) -> bool {
        self.is_image() || self.is_text_file() || self.is_ascii_file()
    }
}

pub struct FileBrowser {
    pub current_dir: String,
    pub files: Vec<FileItem>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub max_visible_files: usize,
    pub sort_mode: SortMode,
    // Stack to track the last selected file in each directory for navigation
    dir_stack: Vec<(String, usize)>, // (directory_path, selected_index)
}

impl FileBrowser {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let current_dir = std::env::current_dir()?.to_string_lossy().into_owned();
        Self::new_with_dir(current_dir)
    }

    pub fn new_with_dir<P: AsRef<Path>>(dir: P) -> Result<Self, Box<dyn Error>> {
        let current_dir = dir.as_ref().to_string_lossy().into_owned();
        let mut browser = Self {
            current_dir,
            files: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            max_visible_files: 20,
            sort_mode: SortMode::Name,
            dir_stack: Vec::new(),
        };
        browser.refresh_files()?;
        Ok(browser)
    }

    pub fn refresh_files(&mut self) -> Result<(), Box<dyn Error>> {
        self.files.clear();

        let entries = fs::read_dir(&self.current_dir)?;

        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();

            let mut is_directory = file_type.is_dir();

            // Handle symlinks that point to directories
            if !is_directory
                && let Ok(metadata) = fs::symlink_metadata(&path)
                && metadata.file_type().is_symlink()
                && let Ok(target_metadata) = fs::metadata(&path)
            {
                is_directory = target_metadata.is_dir();
            }

            // Get modification time
            let modified = entry
                .metadata()?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);

            self.files.push(FileItem::new(
                entry.file_name().to_string_lossy().into_owned(),
                path.to_string_lossy().into_owned(),
                is_directory,
                modified,
            ));
        }

        self.sort_files();
        Ok(())
    }

    fn sort_files(&mut self) {
        self.files.sort_by(|a, b| {
            // Always put directories first
            if a.is_directory && !b.is_directory {
                std::cmp::Ordering::Less
            } else if !a.is_directory && b.is_directory {
                std::cmp::Ordering::Greater
            } else {
                // Both are directories or both are files
                match self.sort_mode {
                    SortMode::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortMode::DateNewestFirst => b.modified.cmp(&a.modified), // Newest first
                    SortMode::DateOldestFirst => a.modified.cmp(&b.modified), // Oldest first
                }
            }
        });
    }

    pub fn get_selected_file(&self) -> Option<&FileItem> {
        self.files.get(self.selected_index)
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.files.len().saturating_sub(1) {
            self.selected_index += 1;

            if self.selected_index >= self.scroll_offset + self.max_visible_files {
                self.scroll_offset = self
                    .selected_index
                    .saturating_sub(self.max_visible_files - 1);
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;

            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn page_down(&mut self) {
        if self.files.is_empty() {
            return;
        }

        let page_size = if self.max_visible_files > 0 {
            self.max_visible_files
        } else {
            10
        };
        let new_index = (self.selected_index + page_size).min(self.files.len() - 1);

        // If we're already near the end, jump to the last item
        if new_index == self.files.len() - 1 {
            self.selected_index = self.files.len() - 1;
        } else {
            self.selected_index = new_index;
        }

        // Update scroll to keep selection visible
        self.update_scroll_for_selection();
    }

    pub fn page_up(&mut self) {
        if self.files.is_empty() {
            return;
        }

        let page_size = if self.max_visible_files > 0 {
            self.max_visible_files
        } else {
            10
        };

        // If we're already near the top, jump to the first item
        if self.selected_index <= page_size {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.saturating_sub(page_size);
        }

        // Update scroll to keep selection visible
        self.update_scroll_for_selection();
    }

    fn update_scroll_for_selection(&mut self) {
        if self.selected_index < self.scroll_offset {
            // Selection is above visible area, scroll up
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + self.max_visible_files {
            // Selection is below visible area, scroll down
            self.scroll_offset = self
                .selected_index
                .saturating_sub(self.max_visible_files - 1);
        }
    }

    pub fn jump_forward(&mut self) {
        if self.files.is_empty() {
            return;
        }

        let jump_size = 10;
        let new_index = (self.selected_index + jump_size).min(self.files.len() - 1);
        self.selected_index = new_index;

        // Update scroll to keep selection visible
        self.update_scroll_for_selection();
    }

    pub fn jump_backward(&mut self) {
        if self.files.is_empty() {
            return;
        }

        let jump_size = 10;
        self.selected_index = self.selected_index.saturating_sub(jump_size);

        // Update scroll to keep selection visible
        self.update_scroll_for_selection();
    }

    pub fn move_to_start(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn move_to_end(&mut self) {
        if !self.files.is_empty() {
            self.selected_index = self.files.len() - 1;
            // Update scroll to keep selection visible at the bottom
            self.update_scroll_for_selection();
        }
    }

    pub fn sort_by_name(&mut self) {
        if self.sort_mode == SortMode::Name {
            return; // Already sorted by name
        }

        // Remember the currently selected file
        let selected_file = self.get_selected_file().map(|f| f.path.clone());

        self.sort_mode = SortMode::Name;
        self.sort_files();

        // Find the file again and update selection
        if let Some(selected_path) = selected_file {
            self.find_and_select_file(&selected_path);
        }
    }

    pub fn sort_by_date(&mut self) -> &'static str {
        // Remember the currently selected file
        let selected_file = self.get_selected_file().map(|f| f.path.clone());

        // Toggle between date sorting modes and return appropriate message key
        let message_key = match self.sort_mode {
            SortMode::DateNewestFirst => {
                self.sort_mode = SortMode::DateOldestFirst;
                "date_sort_oldest_first"
            }
            SortMode::DateOldestFirst => {
                self.sort_mode = SortMode::DateNewestFirst;
                "date_sort_newest_first"
            }
            SortMode::Name => {
                self.sort_mode = SortMode::DateNewestFirst; // Default to newest first when switching from name
                "date_sort_newest_first"
            }
        };

        self.sort_files();

        // Find the file again and update selection
        if let Some(selected_path) = selected_file {
            self.find_and_select_file(&selected_path);
        }

        message_key
    }

    fn find_and_select_file(&mut self, file_path: &str) {
        if let Some(index) = self.files.iter().position(|f| f.path == file_path) {
            self.selected_index = index;
            self.center_on_selection();
        }
    }

    pub fn enter_directory(&mut self) -> Result<bool, Box<dyn Error>> {
        // Check if current selection is a directory without borrowing conflicts
        let is_dir = if let Some(file) = self.get_selected_file() {
            file.is_directory
        } else {
            false
        };

        if is_dir {
            // Save current directory and selection to stack before entering new dir
            let current_dir = self.current_dir.clone();
            let selected_index = self.selected_index;

            self.dir_stack.push((current_dir, selected_index));

            // Now get the actual file for path access (this is safe)
            if let Some(file) = self.get_selected_file() {
                self.current_dir = file.path.clone();
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.refresh_files()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn go_to_parent(&mut self) -> Result<bool, Box<dyn Error>> {
        if let Some(parent) = Path::new(&self.current_dir).parent() {
            // Try to restore previous selection from stack when going back up
            let restored_selection = if let Some((prev_dir, prev_index)) = self.dir_stack.pop() {
                // Verify we're actually returning to the expected parent directory
                let expected_parent = parent.to_string_lossy().into_owned();

                // Simple string comparison - this should work for most cases
                if expected_parent == prev_dir {
                    Some(prev_index)
                } else {
                    None // Different path - don't restore selection
                }
            } else {
                None // No previous selection in stack
            };

            self.current_dir = parent.to_string_lossy().into_owned();
            self.scroll_offset = 0;
            self.refresh_files()?;

            // Restore the previously selected index if available and matches, but ensure it's valid
            let mut restored_index = 0;
            if let Some(index) = restored_selection {
                if index < self.files.len() {
                    restored_index = index;
                }
            }

            self.selected_index = restored_index;

            // Make sure we have a valid selection after refresh
            if !self.files.is_empty() && self.selected_index >= self.files.len() {
                self.selected_index = 0;
            }

            // Center the restored selection on screen
            self.center_on_selection();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_max_visible_files(&mut self, max_visible: usize) {
        self.max_visible_files = max_visible;

        // Ensure scroll offset is valid
        if self.scroll_offset >= self.files.len() {
            self.scroll_offset = 0;
        }
    }

    pub fn set_selected_index(&mut self, index: usize) {
        if index < self.files.len() {
            self.selected_index = index;
            self.center_on_selection();
        }
    }

    pub fn center_on_selection(&mut self) {
        if self.max_visible_files == 0 {
            return;
        }

        // Calculate the optimal scroll offset to center the selection
        let half_visible = self.max_visible_files / 2;

        if self.selected_index >= half_visible {
            self.scroll_offset = self.selected_index.saturating_sub(half_visible);
        } else {
            self.scroll_offset = 0;
        }

        // Ensure we don't scroll past the end
        let max_scroll = self.files.len().saturating_sub(self.max_visible_files);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    pub fn get_display_files(&self) -> impl Iterator<Item = (usize, &FileItem)> {
        self.files
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.max_visible_files)
    }

    pub fn get_current_dir_display(&self) -> String {
        if self.current_dir.len() > 30 {
            format!("...{}", &self.current_dir[self.current_dir.len() - 27..])
        } else {
            self.current_dir.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::helpers::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_file_item_creation() {
        let item = FileItem::new(
            "test.jpg".to_string(),
            "/path/to/test.jpg".to_string(),
            false,
            UNIX_EPOCH,
        );

        assert_eq!(item.name, "test.jpg");
        assert_eq!(item.path, "/path/to/test.jpg");
        assert!(!item.is_directory);
        assert_eq!(item.modified, UNIX_EPOCH);
    }

    #[test]
    fn test_file_item_is_image() {
        let temp_fs = TestFileSystem::new().unwrap();

        // Create real image file with JPEG magic bytes
        let jpeg_path = temp_fs.create_test_image("test.jpg").unwrap();
        let jpeg_item = FileItem::new("test.jpg".to_string(), jpeg_path, false, UNIX_EPOCH);
        assert!(jpeg_item.is_image(), "Should detect JPEG as image");

        // Create PNG file with PNG magic bytes
        let png_content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde";
        let png_path = temp_fs.create_binary_file("test.png", png_content).unwrap();
        let png_item = FileItem::new("test.png".to_string(), png_path, false, UNIX_EPOCH);
        assert!(png_item.is_image(), "Should detect PNG as image");

        // Create SVG file
        let svg_content = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"></svg>";
        let svg_path = temp_fs.create_file("test.svg", svg_content).unwrap();
        let svg_item = FileItem::new("test.svg".to_string(), svg_path, false, UNIX_EPOCH);
        assert!(svg_item.is_image(), "Should detect SVG as image");

        let dir_item = create_test_file_item("test.jpg", true);
        assert!(!dir_item.is_image(), "Directory should not be image");

        let text_path = temp_fs.create_file("test.txt", "Hello world").unwrap();
        let text_item = FileItem::new("test.txt".to_string(), text_path, false, UNIX_EPOCH);
        assert!(!text_item.is_image(), "Text file should not be image");
    }

    #[test]
    fn test_file_item_is_text_file() {
        let temp_fs = TestFileSystem::new().unwrap();

        // Test various text file types with actual content
        let text_files = [
            ("test.txt", "Hello world"),
            ("config.json", "{\"key\": \"value\"}"),
            ("README.md", "# Test"),
            ("code.rs", "fn main() {}"),
            ("script.py", "print('hello')"),
            ("style.css", "body { color: red; }"),
        ];

        for (filename, content) in &text_files {
            let path = temp_fs.create_file(filename, content).unwrap();
            let item = FileItem::new(filename.to_string(), path, false, UNIX_EPOCH);
            assert!(
                item.is_text_file(),
                "Should detect {} as text file",
                filename
            );
        }

        let dir_item = create_test_file_item("test.txt", true);
        assert!(
            !dir_item.is_text_file(),
            "Directory should not be text file"
        );

        let image_path = temp_fs.create_test_image("test.jpg").unwrap();
        let image_item = FileItem::new("test.jpg".to_string(), image_path, false, UNIX_EPOCH);
        assert!(
            !image_item.is_text_file(),
            "Image file should not be text file"
        );
    }

    #[test]
    fn test_file_item_is_ascii_file() {
        let ascii_item = create_test_file_item("test.ascii", false);
        assert!(ascii_item.is_ascii_file());

        let dir_item = create_test_file_item("test.ascii", true);
        assert!(!dir_item.is_ascii_file());

        let other_item = create_test_file_item("test.txt", false);
        assert!(!other_item.is_ascii_file());
    }

    #[test]
    fn test_file_item_can_preview() {
        let temp_fs = TestFileSystem::new().unwrap();

        // Test image file
        let image_path = temp_fs.create_test_image("photo.jpg").unwrap();
        let image_item = FileItem::new("photo.jpg".to_string(), image_path, false, UNIX_EPOCH);
        assert!(image_item.can_preview());

        // Test text file
        let text_path = temp_fs.create_file("document.txt", "Hello world").unwrap();
        let text_item = FileItem::new("document.txt".to_string(), text_path, false, UNIX_EPOCH);
        assert!(text_item.can_preview());

        // Test ascii file
        let ascii_path = temp_fs
            .create_file("art.ascii", "ASCII art content")
            .unwrap();
        let ascii_item = FileItem::new("art.ascii".to_string(), ascii_path, false, UNIX_EPOCH);
        assert!(ascii_item.can_preview());

        let dir_item = create_test_directory_item("folder");
        assert!(!dir_item.can_preview());

        // Test unknown binary file
        let unknown_content = b"\x00\x01\x02\x03\x04\x05"; // Binary content with no known signature
        let unknown_path = temp_fs
            .create_binary_file("unknown.xyz", unknown_content)
            .unwrap();
        let unknown_item =
            FileItem::new("unknown.xyz".to_string(), unknown_path, false, UNIX_EPOCH);
        assert!(!unknown_item.can_preview());
    }

    #[test]
    fn test_sort_mode_equality() {
        assert_eq!(SortMode::Name, SortMode::Name);
        assert_eq!(SortMode::DateNewestFirst, SortMode::DateNewestFirst);
        assert_eq!(SortMode::DateOldestFirst, SortMode::DateOldestFirst);
        assert_ne!(SortMode::Name, SortMode::DateNewestFirst);
        assert_ne!(SortMode::DateNewestFirst, SortMode::DateOldestFirst);
    }

    #[test]
    fn test_file_browser_creation() {
        let temp_fs = TestFileSystem::new().unwrap();

        let browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        assert_eq!(browser.selected_index, 0);
        assert_eq!(browser.scroll_offset, 0);
        assert_eq!(browser.max_visible_files, 20);
        assert_eq!(browser.sort_mode, SortMode::Name);
    }

    #[test]
    fn test_file_browser_refresh_files() {
        let temp_fs = TestFileSystem::new().unwrap();
        temp_fs.create_file("test1.txt", "content1").unwrap();
        temp_fs.create_file("test2.jpg", "content2").unwrap();
        temp_fs.create_directory("subdir").unwrap();

        let browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        assert!(browser.files.len() >= 3);

        let dir_count = browser.files.iter().filter(|f| f.is_directory).count();
        let file_count = browser.files.iter().filter(|f| !f.is_directory).count();

        assert_eq!(dir_count, 1);
        assert_eq!(file_count, 2);
    }

    #[test]
    fn test_file_browser_navigation() {
        let temp_fs = TestFileSystem::new().unwrap();
        temp_fs.create_file("file1.txt", "content").unwrap();
        temp_fs.create_file("file2.txt", "content").unwrap();
        temp_fs.create_file("file3.txt", "content").unwrap();

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        assert_eq!(browser.selected_index, 0);

        browser.move_down();
        assert_eq!(browser.selected_index, 1);

        browser.move_down();
        assert_eq!(browser.selected_index, 2);

        browser.move_up();
        assert_eq!(browser.selected_index, 1);

        browser.move_up();
        assert_eq!(browser.selected_index, 0);

        browser.move_up();
        assert_eq!(browser.selected_index, 0);
    }

    #[test]
    fn test_file_browser_page_navigation() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..50 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        browser.update_max_visible_files(10);

        assert_eq!(browser.selected_index, 0);

        browser.page_down();
        assert_eq!(browser.selected_index, 10);

        browser.page_down();
        assert_eq!(browser.selected_index, 20);

        browser.page_up();
        assert_eq!(browser.selected_index, 10);

        browser.page_up();
        assert_eq!(browser.selected_index, 0);
    }

    #[test]
    fn test_file_browser_jump_navigation() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..30 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        assert_eq!(browser.selected_index, 0);

        browser.jump_forward();
        assert_eq!(browser.selected_index, 10);

        browser.jump_forward();
        assert_eq!(browser.selected_index, 20);

        browser.jump_backward();
        assert_eq!(browser.selected_index, 10);

        browser.jump_backward();
        assert_eq!(browser.selected_index, 0);
    }

    #[test]
    fn test_file_browser_sorting() {
        let temp_fs = TestFileSystem::new().unwrap();

        std::thread::sleep(Duration::from_millis(10));
        temp_fs.create_file("zebra.txt", "content").unwrap();

        std::thread::sleep(Duration::from_millis(10));
        temp_fs.create_file("alpha.txt", "content").unwrap();

        temp_fs.create_directory("beta_dir").unwrap();

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        browser.sort_by_name();
        assert_eq!(browser.sort_mode, SortMode::Name);

        let first_file = browser.files.iter().find(|f| !f.is_directory).unwrap();
        assert_eq!(first_file.name, "alpha.txt");

        browser.sort_by_date();
        assert_eq!(browser.sort_mode, SortMode::DateNewestFirst);

        let first_file = browser.files.iter().find(|f| !f.is_directory).unwrap();
        assert_eq!(first_file.name, "alpha.txt");
    }

    #[test]
    fn test_file_browser_date_sort_toggle() {
        let temp_fs = TestFileSystem::new().unwrap();

        // Create files with different timestamps
        std::thread::sleep(Duration::from_millis(10));
        temp_fs.create_file("first.txt", "content").unwrap();

        std::thread::sleep(Duration::from_millis(10));
        temp_fs.create_file("second.txt", "content").unwrap();

        std::thread::sleep(Duration::from_millis(10));
        temp_fs.create_file("third.txt", "content").unwrap();

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        // Initially should be sorted by name
        assert_eq!(browser.sort_mode, SortMode::Name);

        // First press of 'd' should sort by date newest first
        browser.sort_by_date();
        assert_eq!(browser.sort_mode, SortMode::DateNewestFirst);

        // Find newest file (should be first in the list)
        let first_file = browser.files.iter().find(|f| !f.is_directory).unwrap();
        assert_eq!(first_file.name, "third.txt"); // Newest file

        // Second press of 'd' should toggle to oldest first
        browser.sort_by_date();
        assert_eq!(browser.sort_mode, SortMode::DateOldestFirst);

        // Find oldest file (should be first in the list now)
        let first_file = browser.files.iter().find(|f| !f.is_directory).unwrap();
        assert_eq!(first_file.name, "first.txt"); // Oldest file

        // Third press of 'd' should toggle back to newest first
        browser.sort_by_date();
        assert_eq!(browser.sort_mode, SortMode::DateNewestFirst);

        // Find newest file again (should be first in the list)
        let first_file = browser.files.iter().find(|f| !f.is_directory).unwrap();
        assert_eq!(first_file.name, "third.txt"); // Newest file
    }

    #[test]
    fn test_file_browser_directory_navigation() {
        let temp_fs = TestFileSystem::new().unwrap();
        temp_fs.create_directory("subdir").unwrap();
        temp_fs.create_file("subdir/nested.txt", "content").unwrap();

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        let subdir_index = browser
            .files
            .iter()
            .position(|f| f.name == "subdir")
            .unwrap();
        browser.set_selected_index(subdir_index);

        let entered = browser.enter_directory().unwrap();
        assert!(entered);
        assert!(browser.current_dir.ends_with("subdir"));

        let nested_file_exists = browser.files.iter().any(|f| f.name == "nested.txt");
        assert!(nested_file_exists);

        let went_back = browser.go_to_parent().unwrap();
        assert!(went_back);
        assert!(!browser.current_dir.ends_with("subdir"));
    }

    #[test]
    fn test_file_browser_get_display_files() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..15 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        browser.update_max_visible_files(5);
        browser.set_selected_index(10);

        let display_files: Vec<_> = browser.get_display_files().collect();
        assert_eq!(display_files.len(), 5);

        assert!(display_files.iter().any(|(i, _)| *i == 10));
    }

    #[test]
    fn test_file_browser_center_on_selection() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..20 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        browser.update_max_visible_files(10);

        browser.set_selected_index(15);
        browser.center_on_selection();

        let half_visible = browser.max_visible_files / 2;
        let expected_offset = 15_usize.saturating_sub(half_visible);
        assert_eq!(browser.scroll_offset, expected_offset);
    }

    #[test]
    fn test_get_current_dir_display_truncation() {
        let temp_fs = TestFileSystem::new().unwrap();
        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        browser.current_dir =
            "/very/long/path/that/exceeds/thirty/characters/for/testing/truncation".to_string();
        let display = browser.get_current_dir_display();

        assert!(display.starts_with("..."));
        assert!(display.len() <= 30);

        browser.current_dir = "/short/path".to_string();
        let display = browser.get_current_dir_display();
        assert_eq!(display, "/short/path");
    }

    #[test]
    fn test_update_max_visible_files() {
        let temp_fs = TestFileSystem::new().unwrap();
        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        browser.update_max_visible_files(15);
        assert_eq!(browser.max_visible_files, 15);

        browser.scroll_offset = 100;
        browser.update_max_visible_files(10);
        assert_eq!(browser.scroll_offset, 0);
    }

    #[test]
    fn test_get_selected_file() {
        let temp_fs = TestFileSystem::new().unwrap();
        temp_fs.create_file("test.txt", "content").unwrap();

        let browser_result = FileBrowser::new_with_dir(temp_fs.get_path());

        if let Ok(browser) = browser_result
            && let Some(selected) = browser.get_selected_file()
        {
            assert!(!selected.name.is_empty());
        }
    }

    #[test]
    fn test_empty_directory() {
        let temp_fs = TestFileSystem::new().unwrap();

        let browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        browser.get_display_files().for_each(|_| {});

        assert!(browser.files.is_empty() || browser.files.iter().all(|f| f.name.starts_with('.')));
    }

    #[test]
    fn test_file_browser_navigation_bounds() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..10 {
            temp_fs
                .create_file(&format!("file{}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        let max_index = browser.files.len().saturating_sub(1);

        for test_index in [0, max_index / 2, max_index] {
            browser.set_selected_index(test_index);
            assert!(browser.selected_index <= max_index);

            browser.move_up();
            assert!(browser.selected_index <= max_index);

            browser.move_down();
            assert!(browser.selected_index <= max_index);
        }
    }

    #[test]
    fn test_move_to_start() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..10 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        // Move to middle
        browser.set_selected_index(5);
        assert_eq!(browser.selected_index, 5);

        // Test move to start
        browser.move_to_start();
        assert_eq!(browser.selected_index, 0);
        assert_eq!(browser.scroll_offset, 0);

        // Test move to start when already at start
        browser.move_to_start();
        assert_eq!(browser.selected_index, 0);
        assert_eq!(browser.scroll_offset, 0);
    }

    #[test]
    fn test_move_to_end() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..10 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        browser.update_max_visible_files(5);

        // Start at beginning
        assert_eq!(browser.selected_index, 0);

        // Test move to end
        browser.move_to_end();
        let expected_last_index = browser.files.len() - 1;
        assert_eq!(browser.selected_index, expected_last_index);

        // Test move to end when already at end
        browser.move_to_end();
        assert_eq!(browser.selected_index, expected_last_index);
    }

    #[test]
    fn test_home_end_navigation_empty_list() {
        let temp_fs = TestFileSystem::new().unwrap();
        let mut browser = FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();

        // Test on empty directory (should not crash)
        browser.move_to_start();
        browser.move_to_end();

        // Should remain at 0 if no files
        if browser.files.is_empty() {
            assert_eq!(browser.selected_index, 0);
        }
    }

    #[test]
    fn test_file_browser_scrolling() {
        let temp_fs = TestFileSystem::new().unwrap();
        for i in 0..25 {
            temp_fs
                .create_file(&format!("file{:02}.txt", i), "content")
                .unwrap();
        }

        let browser_result = FileBrowser::new_with_dir(temp_fs.get_path());

        if let Ok(mut browser) = browser_result {
            browser.update_max_visible_files(10);

            assert_eq!(browser.scroll_offset, 0);

            for _ in 0..15 {
                browser.move_down();
            }

            if browser.files.len() > 10 {
                assert!(browser.scroll_offset > 0);
            }
            assert!(browser.selected_index >= browser.scroll_offset);
            assert!(browser.selected_index < browser.scroll_offset + browser.max_visible_files);
        }
    }
}
