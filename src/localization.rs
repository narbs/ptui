use fluent::{FluentArgs, FluentBundle, FluentResource};
use std::error::Error;
use unic_langid::LanguageIdentifier;

// Use embedded locales
include!(concat!(env!("OUT_DIR"), "/locales.rs"));

const DEFAULT_LOCALE: &str = "en";

pub struct Localization {
    bundle: FluentBundle<FluentResource>,
    current_locale: String,
}

impl Localization {
    pub fn new(locale: &str) -> Result<Self, Box<dyn Error>> {
        let langid: LanguageIdentifier = locale
            .parse()
            .unwrap_or_else(|_| DEFAULT_LOCALE.parse().unwrap());
        let mut bundle = FluentBundle::new(vec![langid]);

        let locales_map = get_embedded_locales();
        let resource_content = locales_map
            .get(locale)
            .or_else(|| locales_map.get(DEFAULT_LOCALE))
            .ok_or("Locale not found")?;
            
        let resource = FluentResource::try_new(resource_content.to_string())
            .map_err(|e| format!("Failed to load resource: {:?}", e))?;
            
        bundle
            .add_resource(resource)
            .map_err(|e| format!("Failed to add resource: {:?}", e))?;
            
        Ok(Self { 
            bundle,
            current_locale: locale.to_string(),
        })
    }

    pub fn get(&self, key: &str) -> String {
        let args = FluentArgs::new();
        if let Some(message) = self.bundle.get_message(key)
            && let Some(pattern) = message.value() {
                let mut errors = vec![];
                let value = self.bundle.format_pattern(pattern, Some(&args), &mut errors);
                return value.to_string();
            }
        key.to_string()
    }

    pub fn get_with_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let empty_args = FluentArgs::new();
        let args_ref = args.unwrap_or(&empty_args);
        
        if let Some(message) = self.bundle.get_message(key)
            && let Some(pattern) = message.value() {
                let mut errors = vec![];
                let value = self.bundle.format_pattern(pattern, Some(args_ref), &mut errors);
                return value.to_string();
            }
        key.to_string()
    }

    pub fn get_help_text(&self) -> String {
        format!(
            "{}\n\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.get("select_image_to_preview"),
            self.get("keys_navigation"),
            self.get("keys_page_navigation"),
            self.get("keys_jump_navigation"),
            self.get("keys_home_end_navigation"),
            self.get("keys_sort"),
            self.get("keys_enter_directory"),
            self.get("keys_backspace_parent_dir"),
            self.get("keys_resize_window"),
            self.get("keys_refresh_image"),
            self.get("keys_save_ascii"),
            self.get("keys_delete_file"),
            self.get("keys_open_in_browser"),
            self.get("keys_slideshow"),
            self.get("keys_text_scroll"),
            self.get("keys_help_toggle"),
            self.get("keys_quit")
        )
    }

    pub fn current_locale(&self) -> &str {
        &self.current_locale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localization_creation_valid_locale() {
        let localization = Localization::new("en").unwrap();
        assert!(!localization.get("select_image_to_preview").is_empty());
    }

    #[test]
    fn test_localization_creation_invalid_locale_fallback() {
        let localization = Localization::new("invalid_locale").unwrap();
        assert!(!localization.get("select_image_to_preview").is_empty());
    }

    #[test]
    fn test_localization_get_existing_key() {
        let localization = Localization::new("en").unwrap();
        let message = localization.get("select_image_to_preview");
        assert!(!message.is_empty());
        assert_ne!(message, "select_image_to_preview");
    }

    #[test]
    fn test_localization_get_nonexistent_key() {
        let localization = Localization::new("en").unwrap();
        let message = localization.get("nonexistent_key");
        assert_eq!(message, "nonexistent_key");
    }

    #[test]
    fn test_localization_help_text_contains_key_info() {
        let localization = Localization::new("en").unwrap();
        let help_text = localization.get_help_text();
        
        assert!(!help_text.is_empty());
        assert!(help_text.contains(&localization.get("keys_navigation")));
        assert!(help_text.contains(&localization.get("keys_quit")));
        assert!(help_text.contains(&localization.get("keys_help_toggle")));
        assert!(help_text.contains(&localization.get("keys_text_scroll")));
        
        // Specifically test that the scrolling keys are mentioned
        assert!(help_text.contains("u: Scroll text up"));
        assert!(help_text.contains("Space: Scroll text down"));
    }

    #[test]
    fn test_localization_help_text_structure() {
        let localization = Localization::new("en").unwrap();
        let help_text = localization.get_help_text();
        
        let lines: Vec<&str> = help_text.lines().collect();
        assert!(lines.len() >= 10);
    }

    #[rstest::rstest]
    #[case("en")]
    #[case("de")]
    #[case("es")]
    #[case("fr")]
    #[case("ja")]
    #[case("zh")]
    fn test_localization_supported_locales(#[case] locale: &str) {
        let result = Localization::new(locale);
        assert!(result.is_ok(), "Locale {} should be supported", locale);
        
        let localization = result.unwrap();
        let message = localization.get("select_image_to_preview");
        assert!(!message.is_empty(), "Locale {} should have messages", locale);
    }

    #[test]
    fn test_default_locale_constant() {
        assert_eq!(DEFAULT_LOCALE, "en");
        let localization = Localization::new(DEFAULT_LOCALE).unwrap();
        assert!(!localization.get("select_image_to_preview").is_empty());
    }

    #[test]
    fn test_localization_keys_consistency() {
        let localization = Localization::new("en").unwrap();
        
        let expected_keys = [
            "select_image_to_preview",
            "keys_navigation",
            "keys_page_navigation",
            "keys_jump_navigation",
            "keys_home_end_navigation",
            "keys_sort",
            "keys_enter_directory",
            "keys_backspace_parent_dir",
            "keys_resize_window",
            "keys_refresh_image",
            "keys_save_ascii",
            "keys_delete_file",
            "keys_open_in_browser",
            "keys_slideshow",
            "keys_help_toggle",
            "keys_quit",
        ];
        
        for key in &expected_keys {
            let message = localization.get(key);
            assert!(!message.is_empty(), "Key {} should have a message", key);
            assert_ne!(message, *key, "Key {} should be translated", key);
        }
    }

    #[test]
    fn test_fluent_args_empty() {
        let localization = Localization::new("en").unwrap();
        
        let simple_message = localization.get("keys_quit");
        assert!(!simple_message.is_empty());
    }

    #[test]
    fn test_localization_bundle_functionality() {
        let localization = Localization::new("en").unwrap();
        
        let message1 = localization.get("keys_navigation");
        let message2 = localization.get("keys_navigation");
        
        assert_eq!(message1, message2);
    }
}