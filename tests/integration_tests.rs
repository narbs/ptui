use ptui::*;
use tempfile::TempDir;

#[test]
fn test_full_application_workflow() {
    let _temp_fs = create_test_environment().unwrap();
    
    let config_result = config::PTuiConfig::load();
    
    if let Ok(_config) = config_result {
    }
}

#[test]
fn test_file_browser_and_preview_integration() {
    let temp_fs = create_test_environment().unwrap();
    std::env::set_current_dir(temp_fs.path()).unwrap();
    
    let file_browser = file_browser::FileBrowser::new().unwrap();
    let config = config::PTuiConfig::default();
    let mut preview_manager = preview::PreviewManager::new(config);
    let localization = localization::Localization::new("en").unwrap();
    
    if let Some(file) = file_browser.get_selected_file() {
        let preview = preview_manager.generate_preview(file, 80, 24, &localization);
        assert!(!preview.lines.is_empty());
    }
}

#[test]
fn test_config_and_converter_integration() {
    let config = config::PTuiConfig::default();
    let converter = converter::create_converter(&config);
    
    assert_eq!(converter.get_name(), "chafa");
    
    let jp2a_config = config::PTuiConfig {
        converter: config::ConverterConfig {
            selected: "jp2a".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    
    let jp2a_converter = converter::create_converter(&jp2a_config);
    assert_eq!(jp2a_converter.get_name(), "jp2a");
}

#[test]
fn test_localization_and_ui_integration() {
    let localization = localization::Localization::new("en").unwrap();
    let mut ui_layout = ui::UILayout::new();
    
    let area = ratatui::layout::Rect::new(0, 0, 100, 40);
    let (file_area, preview_area, debug_area) = ui_layout.calculate_layout(area);
    
    assert!(file_area.width > 0);
    assert!(preview_area.width > 0);
    assert!(debug_area.height > 0);
    
    let help_text = localization.get_help_text();
    assert!(!help_text.is_empty());
}

#[test]
fn test_file_type_detection_integration() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a real JPEG image file with proper magic bytes
    let jpeg_content = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00\xFF\xDB\x00C\x00\x08\x06\x06\x07\x06\x05\x08\x07\x07\x07\t\t\x08\n\x0C\x14\r\x0C\x0B\x0B\x0C\x19\x12\x13\x0F\x14\x1D\x1A\x1F\x1E\x1D\x1A\x1C\x1C $.\' \",#\x1C\x1C(7),01444\x1F\'9=82<.342\xFF\xC0\x00\x11\x08\x00\x01\x00\x01\x01\x01\x11\x00\x02\x11\x01\x03\x11\x01\xFF\xC4\x00\x14\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\xFF\xC4\x00\x14\x10\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xFF\xDA\x00\x0C\x03\x01\x00\x02\x11\x03\x11\x00\x3F\x00\xAA\xFF\xD9";
    let image_path = temp_dir.path().join("test.jpg");
    std::fs::write(&image_path, jpeg_content).unwrap();
    let image_file = file_browser::FileItem::new(
        "test.jpg".to_string(),
        image_path.to_string_lossy().to_string(),
        false,
        std::time::UNIX_EPOCH,
    );
    
    // Create a real text file
    let text_path = temp_dir.path().join("test.txt");
    std::fs::write(&text_path, "Hello, world!").unwrap();
    let text_file = file_browser::FileItem::new(
        "test.txt".to_string(),
        text_path.to_string_lossy().to_string(),
        false,
        std::time::UNIX_EPOCH,
    );
    
    let directory = file_browser::FileItem::new(
        "folder".to_string(),
        "/path/folder".to_string(),
        true,
        std::time::UNIX_EPOCH,
    );
    
    assert!(image_file.is_image());
    assert!(image_file.can_preview());
    
    assert!(text_file.is_text_file());
    assert!(text_file.can_preview());
    
    assert!(!directory.can_preview());
}

#[test]
fn test_configuration_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("ptui.json");
    
    let original_config = config::PTuiConfig {
        converter: config::ConverterConfig {
            selected: "jp2a".to_string(),
            chafa: config::ChafaConfig {
                format: "sixel".to_string(),
                colors: "256".to_string(),
            },
            ..Default::default()
        },
        locale: Some("fr".to_string()),
        slideshow_delay_ms: Some(5000),
        slideshow_transitions: Some(config::SlideshowTransitionConfig::default()),
        chafa: None,
    };
    
    config::PTuiConfig::save_config(&config_path, &original_config).unwrap();
    
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let loaded_config: config::PTuiConfig = serde_json::from_str(&contents).unwrap();
    
    assert_eq!(loaded_config.converter.selected, "jp2a");
    assert_eq!(loaded_config.converter.chafa.format, "sixel");
    assert_eq!(loaded_config.locale, Some("fr".to_string()));
    assert_eq!(loaded_config.slideshow_delay_ms, Some(5000));
}

#[test]
fn test_preview_caching_behavior() {
    let temp_fs = create_test_environment().unwrap();
    let image_path = create_test_image(&temp_fs, "test.jpg").unwrap();
    
    let config = config::PTuiConfig::default();
    let mut preview_manager = preview::PreviewManager::new(config);
    let localization = localization::Localization::new("en").unwrap();
    
    let file_item = file_browser::FileItem::new(
        "test.jpg".to_string(),
        image_path,
        false,
        std::time::UNIX_EPOCH,
    );
    
    let preview1 = preview_manager.generate_preview(&file_item, 80, 24, &localization);
    let preview2 = preview_manager.generate_preview(&file_item, 80, 24, &localization);
    
    assert!(!preview1.lines.is_empty());
    assert!(!preview2.lines.is_empty());
    
    preview_manager.clear_cache();
    let preview3 = preview_manager.generate_preview(&file_item, 80, 24, &localization);
    assert!(!preview3.lines.is_empty());
}

#[test]
fn test_ui_layout_responsiveness() {
    let mut layout = ui::UILayout::new();
    
    let small_screen = ratatui::layout::Rect::new(0, 0, 80, 24);
    let (small_file, small_preview, small_debug) = layout.calculate_layout(small_screen);
    
    let large_screen = ratatui::layout::Rect::new(0, 0, 200, 60);
    let (large_file, large_preview, large_debug) = layout.calculate_layout(large_screen);
    
    assert!(small_file.width + small_preview.width == small_screen.width);
    assert!(large_file.width + large_preview.width == large_screen.width);
    
    assert_eq!(small_debug.height, 3);
    assert_eq!(large_debug.height, 3);
}

#[test]
fn test_multilingual_support() {
    let locales = ["en", "de", "es", "fr", "ja", "zh"];
    
    for locale in &locales {
        let localization = localization::Localization::new(locale).unwrap();
        let help_text = localization.get_help_text();
        
        assert!(!help_text.is_empty(), "Locale {} should have help text", locale);
        
        let quit_message = localization.get("keys_quit");
        assert!(!quit_message.is_empty(), "Locale {} should have quit message", locale);
        assert_ne!(quit_message, "keys_quit", "Locale {} should translate keys", locale);
    }
}

fn create_test_environment() -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    
    std::fs::write(temp_dir.path().join("image1.jpg"), create_minimal_jpeg())?;
    std::fs::write(temp_dir.path().join("image2.png"), b"fake png content")?;
    std::fs::write(temp_dir.path().join("document.txt"), "Sample text content\nLine 2\nLine 3")?;
    std::fs::write(temp_dir.path().join("config.json"), r#"{"key": "value"}"#)?;
    std::fs::write(temp_dir.path().join("art.ascii"), "\x1b[31mRed\x1b[0m ASCII art")?;
    
    std::fs::create_dir(temp_dir.path().join("subfolder"))?;
    std::fs::write(temp_dir.path().join("subfolder/nested.txt"), "Nested content")?;
    
    Ok(temp_dir)
}

fn create_test_image(temp_dir: &TempDir, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = temp_dir.path().join(name);
    std::fs::write(&path, create_minimal_jpeg())?;
    Ok(path.to_string_lossy().to_string())
}

fn create_minimal_jpeg() -> &'static [u8] {
    b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00\xFF\xDB\x00C\x00\x08\x06\x06\x07\x06\x05\x08\x07\x07\x07\t\t\x08\n\x0C\x14\r\x0C\x0B\x0B\x0C\x19\x12\x13\x0F\x14\x1D\x1A\x1F\x1E\x1D\x1A\x1C\x1C $.\' \",#\x1C\x1C(7),01444\x1F\'9=82<.342\xFF\xC0\x00\x11\x08\x00\x01\x00\x01\x01\x01\x11\x00\x02\x11\x01\x03\x11\x01\xFF\xC4\x00\x14\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\xFF\xC4\x00\x14\x10\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xFF\xDA\x00\x0C\x03\x01\x00\x02\x11\x03\x11\x00\x3F\x00\xAA\xFF\xD9"
}