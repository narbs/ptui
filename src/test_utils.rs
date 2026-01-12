#[cfg(test)]
pub mod helpers {
    use crate::config::{ChafaConfig, ConverterConfig, Jp2aConfig, PTuiConfig};
    use crate::file_browser::FileItem;
    use std::fs;
    use std::path::Path;
    use std::time::UNIX_EPOCH;
    use tempfile::TempDir;

    pub fn create_test_config() -> PTuiConfig {
        PTuiConfig {
            converter: ConverterConfig {
                chafa: ChafaConfig {
                    format: "ansi".to_string(),
                    colors: "full".to_string(),
                },
                jp2a: Jp2aConfig {
                    colors: true,
                    invert: false,
                    dither: "none".to_string(),
                    chars: None,
                },
                graphical: crate::config::GraphicalConfig::default(),
                selected: "chafa".to_string(),
            },
            locale: Some("en".to_string()),
            slideshow_delay_ms: Some(1000),
            slideshow_transitions: Some(crate::config::SlideshowTransitionConfig::default()),
            chafa: None,
        }
    }

    pub fn create_test_file_item(name: &str, is_directory: bool) -> FileItem {
        FileItem::new(
            name.to_string(),
            format!("/test/path/{}", name),
            is_directory,
            UNIX_EPOCH,
        )
    }

    pub fn create_test_image_file_item(name: &str) -> FileItem {
        create_test_file_item(&format!("{}.jpg", name), false)
    }

    pub fn create_test_text_file_item(name: &str) -> FileItem {
        create_test_file_item(&format!("{}.txt", name), false)
    }

    pub fn create_test_directory_item(name: &str) -> FileItem {
        create_test_file_item(name, true)
    }

    pub struct TestFileSystem {
        pub temp_dir: TempDir,
    }

    impl TestFileSystem {
        pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let temp_dir = TempDir::new()?;
            Ok(Self { temp_dir })
        }

        pub fn create_file(
            &self,
            name: &str,
            content: &str,
        ) -> Result<String, Box<dyn std::error::Error>> {
            let file_path = self.temp_dir.path().join(name);
            fs::write(&file_path, content)?;
            Ok(file_path.to_string_lossy().to_string())
        }

        pub fn create_directory(&self, name: &str) -> Result<String, Box<dyn std::error::Error>> {
            let dir_path = self.temp_dir.path().join(name);
            fs::create_dir_all(&dir_path)?;
            Ok(dir_path.to_string_lossy().to_string())
        }

        pub fn create_test_image(&self, name: &str) -> Result<String, Box<dyn std::error::Error>> {
            let content = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00\xFF\xDB\x00C\x00\x08\x06\x06\x07\x06\x05\x08\x07\x07\x07\t\t\x08\n\x0C\x14\r\x0C\x0B\x0B\x0C\x19\x12\x13\x0F\x14\x1D\x1A\x1F\x1E\x1D\x1A\x1C\x1C $.\' \",#\x1C\x1C(7),01444\x1F\'9=82<.342\xFF\xC0\x00\x11\x08\x00\x01\x00\x01\x01\x01\x11\x00\x02\x11\x01\x03\x11\x01\xFF\xC4\x00\x14\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\xFF\xC4\x00\x14\x10\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xFF\xDA\x00\x0C\x03\x01\x00\x02\x11\x03\x11\x00\x3F\x00\xAA\xFF\xD9";
            self.create_binary_file(name, content)
        }

        pub fn create_binary_file(
            &self,
            name: &str,
            content: &[u8],
        ) -> Result<String, Box<dyn std::error::Error>> {
            let file_path = self.temp_dir.path().join(name);
            fs::write(&file_path, content)?;
            Ok(file_path.to_string_lossy().to_string())
        }

        pub fn get_path(&self) -> &Path {
            self.temp_dir.path()
        }
    }

    pub fn assert_file_exists(path: &str) {
        assert!(Path::new(path).exists(), "File should exist: {}", path);
    }
}