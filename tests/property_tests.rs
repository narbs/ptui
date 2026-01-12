use proptest::prelude::*;
use ptui::*;
use std::time::UNIX_EPOCH;
use tempfile::TempDir;

proptest! {
    #[test]
    fn test_file_item_filename_variants(
        name in "[a-zA-Z0-9._-]{1,50}",
        ext in prop::option::of("[a-zA-Z]{1,10}"),
        is_dir in any::<bool>(),
    ) {
        let filename = if let Some(extension) = ext {
            format!("{}.{}", name, extension)
        } else {
            name
        };
        
        let file_item = file_browser::FileItem::new(
            filename.clone(),
            format!("/path/{}", filename),
            is_dir,
            UNIX_EPOCH,
        );
        
        prop_assert_eq!(&file_item.name, &filename);
        prop_assert_eq!(file_item.is_directory, is_dir);
        
        if is_dir {
            prop_assert!(!file_item.is_image());
            prop_assert!(!file_item.is_text_file());
            prop_assert!(!file_item.can_preview());
        }
    }

    #[test]
    fn test_ui_layout_dimensions(
        width in 10u16..500u16,
        height in 10u16..200u16,
    ) {
        let mut layout = ui::UILayout::new();
        let area = ratatui::layout::Rect::new(0, 0, width, height);
        
        let (file_area, preview_area, debug_area) = layout.calculate_layout(area);
        
        prop_assert!(file_area.width > 0);
        prop_assert!(preview_area.width > 0);
        prop_assert!(debug_area.height > 0);
        
        prop_assert_eq!(file_area.width + preview_area.width, width);
        prop_assert_eq!(file_area.height + debug_area.height, height);
        
        prop_assert!(layout.preview_width <= preview_area.width);
        prop_assert!(layout.preview_height <= preview_area.height);
    }

    #[test]
    fn test_config_serialization_roundtrip(
        locale in prop::option::of("[a-z]{2}"),
        delay_ms in prop::option::of(100u64..10000u64),
        format in "[a-z]{3,10}",
        colors in "[a-z0-9]{1,10}",
        converter_selected in "(chafa|jp2a)",
    ) {
        let config = config::PTuiConfig {
            converter: config::ConverterConfig {
                chafa: config::ChafaConfig {
                    format: format.clone(),
                    colors: colors.clone(),
                },
                jp2a: config::Jp2aConfig::default(),
                graphical: config::GraphicalConfig::default(),
                selected: converter_selected.clone(),
            },
            locale: locale.clone(),
            slideshow_delay_ms: delay_ms,
            slideshow_transitions: Some(config::SlideshowTransitionConfig::default()),
            chafa: None,
        };
        
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: config::PTuiConfig = serde_json::from_str(&json).unwrap();
        
        prop_assert_eq!(deserialized.converter.chafa.format, format);
        prop_assert_eq!(deserialized.converter.chafa.colors, colors);
        prop_assert_eq!(deserialized.converter.selected, converter_selected);
        prop_assert_eq!(deserialized.locale, locale);
        prop_assert_eq!(deserialized.slideshow_delay_ms, delay_ms);
    }

    #[test]
    fn test_file_browser_navigation_bounds(
        file_count in 1usize..100usize,
        selected_index in 0usize..200usize,
        max_visible in 5usize..50usize,
    ) {
        use tempfile::TempDir;
        
        let temp_dir = TempDir::new().unwrap();
        for i in 0..file_count {
            std::fs::write(temp_dir.path().join(format!("file{}.txt", i)), "content").unwrap();
        }
        
        let mut browser = file_browser::FileBrowser::new_with_dir(temp_dir.path()).unwrap();
        browser.update_max_visible_files(max_visible);
        
        let safe_index = selected_index % browser.files.len();
        browser.set_selected_index(safe_index);
        
        prop_assert!(browser.selected_index < browser.files.len());
        prop_assert!(browser.scroll_offset <= browser.files.len());
        
        browser.move_up();
        prop_assert!(browser.selected_index < browser.files.len());
        
        browser.move_down();
        prop_assert!(browser.selected_index < browser.files.len());
        
        browser.page_up();
        prop_assert!(browser.selected_index < browser.files.len());
        
        browser.page_down();
        prop_assert!(browser.selected_index < browser.files.len());
    }

    #[test]
    fn test_layout_size_adjustments(
        initial_size in 10u16..90u16,
        increment in 1u16..50u16,
        min_divider in 5u16..20u16,
    ) {
        let mut layout = ui::UILayout::new();
        layout.preview_size = initial_size;
        layout.min_divider_percent = min_divider;
        
        let max_size = 100 - min_divider;
        
        if layout.can_increase_size() {
            let old_size = layout.preview_size;
            layout.increase_size(increment);
            prop_assert!(layout.preview_size >= old_size);
            prop_assert!(layout.preview_size <= max_size);
        }
        
        if layout.can_decrease_size() {
            let old_size = layout.preview_size;
            layout.decrease_size(increment);
            prop_assert!(layout.preview_size <= old_size);
            prop_assert!(layout.preview_size >= min_divider);
        }
    }

    #[test]
    fn test_converter_config_variants(
        chafa_format in "[a-z]{4,10}",
        chafa_colors in "[a-z0-9]{1,8}",
        jp2a_colors in any::<bool>(),
        jp2a_invert in any::<bool>(),
        selected in "(chafa|jp2a|unknown)",
    ) {
        let config = config::PTuiConfig {
            converter: config::ConverterConfig {
                chafa: config::ChafaConfig {
                    format: chafa_format.clone(),
                    colors: chafa_colors.clone(),
                },
                jp2a: config::Jp2aConfig {
                    colors: jp2a_colors,
                    invert: jp2a_invert,
                    dither: "none".to_string(),
                    chars: None,
                },
                graphical: config::GraphicalConfig::default(),
                selected: selected.clone(),
            },
            ..Default::default()
        };
        
        let converter = converter::create_converter(&config);
        
        match selected.as_str() {
            "jp2a" => prop_assert_eq!(converter.get_name(), "jp2a"),
            _ => prop_assert_eq!(converter.get_name(), "chafa"), // chafa is default fallback
        }
    }

    #[test]
    fn test_directory_display_truncation(
        path_segments in prop::collection::vec("[a-zA-Z0-9]{1,20}", 1..10),
    ) {
        use tempfile::TempDir;
        
        let long_path = format!("/{}", path_segments.join("/"));
        
        let temp_dir = TempDir::new().unwrap();
        let mut browser = file_browser::FileBrowser::new_with_dir(temp_dir.path()).unwrap();
        browser.current_dir = long_path.clone();
        
        let display = browser.get_current_dir_display();
        
        prop_assert!(display.len() <= 30, "Display should be truncated to 30 chars or less");
        
        if long_path.len() > 30 {
            prop_assert!(display.starts_with("..."), "Long paths should be truncated with ...");
        } else {
            prop_assert_eq!(display, long_path, "Short paths should not be truncated");
        }
    }

    #[test]
    fn test_slideshow_delay_configuration(
        delay_ms in prop::option::of(50u64..30000u64),
    ) {
        let config = config::PTuiConfig {
            slideshow_delay_ms: delay_ms,
            ..Default::default()
        };
        
        let actual_delay = config.get_slideshow_delay_ms();
        
        match delay_ms {
            Some(ms) => prop_assert_eq!(actual_delay, ms),
            None => prop_assert_eq!(actual_delay, 2000), // default value
        }
    }

    #[test]
    fn test_content_based_file_detection(
        filename in "[a-zA-Z0-9_-]{1,20}",
        file_type in "(jpeg_image|png_image|svg_image|text_file|binary_unknown)",
    ) {
        let temp_dir = TempDir::new().unwrap();
        let full_filename = format!("{}.test", filename); // Use generic extension
        let file_path = temp_dir.path().join(&full_filename);
        
        // Create file with appropriate content based on file_type
        match file_type.as_str() {
            "jpeg_image" => {
                let jpeg_content = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00";
                std::fs::write(&file_path, jpeg_content).unwrap();
                let file_item = file_browser::FileItem::new(
                    full_filename,
                    file_path.to_string_lossy().to_string(),
                    false,
                    UNIX_EPOCH,
                );
                prop_assert!(file_item.is_image());
                prop_assert!(file_item.can_preview());
            },
            "png_image" => {
                let png_content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
                std::fs::write(&file_path, png_content).unwrap();
                let file_item = file_browser::FileItem::new(
                    full_filename,
                    file_path.to_string_lossy().to_string(),
                    false,
                    UNIX_EPOCH,
                );
                prop_assert!(file_item.is_image());
                prop_assert!(file_item.can_preview());
            },
            "svg_image" => {
                let svg_content = "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
                std::fs::write(&file_path, svg_content).unwrap();
                let file_item = file_browser::FileItem::new(
                    full_filename,
                    file_path.to_string_lossy().to_string(),
                    false,
                    UNIX_EPOCH,
                );
                prop_assert!(file_item.is_image());
                prop_assert!(file_item.can_preview());
            },
            "text_file" => {
                let text_content = "Hello, this is text content!";
                std::fs::write(&file_path, text_content).unwrap();
                let file_item = file_browser::FileItem::new(
                    full_filename,
                    file_path.to_string_lossy().to_string(),
                    false,
                    UNIX_EPOCH,
                );
                prop_assert!(file_item.is_text_file());
                prop_assert!(file_item.can_preview());
            },
            "binary_unknown" => {
                let binary_content = b"\x00\x01\x02\x03\x04\x05"; // Unknown binary format
                std::fs::write(&file_path, binary_content).unwrap();
                let file_item = file_browser::FileItem::new(
                    full_filename,
                    file_path.to_string_lossy().to_string(),
                    false,
                    UNIX_EPOCH,
                );
                prop_assert!(!file_item.is_image());
                prop_assert!(!file_item.is_text_file());
                prop_assert!(!file_item.can_preview());
            },
            _ => unreachable!()
        }
    }
}