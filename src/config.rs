use notify::{Event, EventKind, RecursiveMode, Watcher, event::ModifyKind};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const DEFAULT_LOCALE: &str = "en";

// Thread-safe lazy initialization of config directory
// This prevents thread contention when multiple tests access the home directory simultaneously
static CONFIG_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(dirs::config_dir);

fn get_config_dir() -> Result<PathBuf, Box<dyn Error>> {
    match CONFIG_DIR.as_ref() {
        Some(dir) => Ok(dir.clone()),
        None => Err("Could not determine config directory".into()),
    }
}

#[derive(Serialize, Debug, Clone, Deserialize, PartialEq)]
pub struct ChafaConfig {
    pub format: String,
    pub colors: String,
}

impl Default for ChafaConfig {
    fn default() -> Self {
        Self {
            format: "ansi".to_string(),
            colors: "full".to_string(),
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct Jp2aConfig {
    pub colors: bool,
    pub invert: bool,
    pub dither: String,
    pub chars: Option<String>,
}

impl Default for Jp2aConfig {
    fn default() -> Self {
        Self {
            colors: true,
            invert: false,
            dither: "none".to_string(), // Note: jp2a doesn't support dithering, this field is ignored
            chars: None, // Use jp2a default character set
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct GraphicalConfig {
    pub filter_type: String,
    #[serde(default = "default_max_dimension")]
    pub max_dimension: u32,
    /// Auto-calculate max_dimension based on terminal size (default: true)
    #[serde(default = "default_auto_resize")]
    pub auto_resize: bool,
}

fn default_max_dimension() -> u32 {
    384 // Good balance between quality and speed for manual override
}

fn default_auto_resize() -> bool {
    true
}

impl Default for GraphicalConfig {
    fn default() -> Self {
        Self {
            filter_type: "lanczos3".to_string(),
            max_dimension: default_max_dimension(),
            auto_resize: default_auto_resize(),
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct ConverterConfig {
    pub chafa: ChafaConfig,
    pub jp2a: Jp2aConfig,
    pub graphical: GraphicalConfig,
    pub selected: String, // "chafa", "jp2a", "graphical"
}

impl Default for ConverterConfig {
    fn default() -> Self {
        Self {
            chafa: ChafaConfig::default(),
            jp2a: Jp2aConfig::default(),
            graphical: GraphicalConfig::default(),
            selected: "chafa".to_string(),
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct SlideshowTransitionConfig {
    pub enabled: bool,
    pub effect: String, // "scattering", "typewriter", "scrolling_left", "scrolling_right", "climbing"
    pub frame_duration_ms: u64,
}

impl Default for SlideshowTransitionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            effect: "scattering".to_string(),
            frame_duration_ms: 50,
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct PTuiConfig {
    pub converter: ConverterConfig,
    pub locale: Option<String>,
    pub slideshow_delay_ms: Option<u64>,
    pub slideshow_transitions: Option<SlideshowTransitionConfig>,
    // Keep the old chafa field for backward compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chafa: Option<ChafaConfig>,
}

impl Default for PTuiConfig {
    fn default() -> Self {
        Self {
            converter: ConverterConfig::default(),
            locale: Some(DEFAULT_LOCALE.to_string()),
            slideshow_delay_ms: Some(2000), // Default 2 seconds
            slideshow_transitions: Some(SlideshowTransitionConfig::default()),
            chafa: None, // Deprecated, use converter.chafa instead
        }
    }
}

impl PTuiConfig {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let config_dir = get_config_dir()?;
        let config_path = config_dir.join("ptui").join("ptui.json");
        
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)?;
            if let Ok(mut config) = serde_json::from_str::<PTuiConfig>(&contents) {
                // Handle backward compatibility: migrate old chafa config to new format
                if let Some(old_chafa) = config.chafa.take() {
                    config.converter.chafa = old_chafa;
                    // Save updated config to migrate to new format
                    let _ = Self::save_config(&config_path, &config);
                }
                println!("Loaded config from: {:?}", config_path);
                return Ok(config);
            }
        }
        
        Self::create_default_config(&config_path)
    }

    fn create_default_config(config_path: &Path) -> Result<Self, Box<dyn Error>> {
        fs::create_dir_all(config_path.parent().unwrap())?;
        let default_config = Self::default();
        Self::save_config(config_path, &default_config)?;
        println!("Created default config file: {:?}", config_path);
        
        Ok(default_config)
    }

    pub fn save_config(config_path: &Path, config: &PTuiConfig) -> Result<(), Box<dyn Error>> {
        let json_content = serde_json::to_string_pretty(config)?;
        fs::write(config_path, json_content)?;
        Ok(())
    }

    pub fn get_locale(&self) -> String {
        self.locale
            .clone()
            .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
    }

    pub fn get_slideshow_delay_ms(&self) -> u64 {
        self.slideshow_delay_ms.unwrap_or(2000)
    }

    pub fn get_slideshow_transitions(&self) -> SlideshowTransitionConfig {
        self.slideshow_transitions.clone().unwrap_or_default()
    }

    pub fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
        let config_dir = get_config_dir()?;
        Ok(config_dir.join("ptui").join("ptui.json"))
    }

    pub fn try_reload_from_file(config_path: &Path) -> Result<PTuiConfig, Box<dyn Error>> {
        if !config_path.exists() {
            return Err("Config file does not exist".into());
        }

        let contents = fs::read_to_string(config_path)?;
        
        // First validate that it's valid JSON
        let _json_value: serde_json::Value = serde_json::from_str(&contents)?;
        
        // Then try to deserialize into PTuiConfig
        let mut config = serde_json::from_str::<PTuiConfig>(&contents)?;
        
        // Handle backward compatibility: migrate old chafa config to new format
        if let Some(old_chafa) = config.chafa.take() {
            config.converter.chafa = old_chafa;
        }
        
        Ok(config)
    }

    pub fn start_config_watcher()
    -> Result<mpsc::Receiver<Result<PTuiConfig, String>>, Box<dyn Error>> {
        let config_path = Self::get_config_path()?;
        let (tx, rx) = mpsc::channel();
        let config_path_clone = config_path.clone();
        let tx_clone = tx.clone();
        
        thread::spawn(move || {
            let mut watcher =
                match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Only react to modify events (file content changes)
                        if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
                            // Small delay to ensure file write is complete
                            thread::sleep(Duration::from_millis(100));
                            
                            match PTuiConfig::try_reload_from_file(&config_path_clone) {
                                Ok(new_config) => {
                                    if tx_clone.send(Ok(new_config)).is_err() {
                                        // Channel closed, exit watcher
                                    }
                                }
                                Err(e) => {
                                        if tx_clone
                                            .send(Err(format!("Failed to reload config: {}", e)))
                                            .is_err()
                                        {
                                        // Channel closed, exit watcher
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if tx_clone.send(Err(format!("Watch error: {}", e))).is_err() {
                            // Channel closed, exit watcher
                        }
                    }
                }
            }) {
                Ok(watcher) => watcher,
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to create watcher: {}", e)));
                    return;
                }
            };
            
            // Watch the config directory (not just the file, as editors often replace files)
            if let Some(config_dir) = config_path.parent()
                && let Err(e) = watcher.watch(config_dir, RecursiveMode::NonRecursive)
            {
                    let _ = tx.send(Err(format!("Failed to watch config directory: {}", e)));
                    return;
                }
            
            // Keep the watcher alive by running an infinite loop
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        });
        
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::helpers::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_chafa_config_default() {
        let config = ChafaConfig::default();
        assert_eq!(config.format, "ansi");
        assert_eq!(config.colors, "full");
    }

    #[test]
    fn test_jp2a_config_default() {
        let config = Jp2aConfig::default();
        assert!(config.colors);
        assert!(!config.invert);
        assert_eq!(config.dither, "none");
        assert_eq!(config.chars, None);
    }

    #[test]
    fn test_converter_config_default() {
        let config = ConverterConfig::default();
        assert_eq!(config.selected, "chafa");
        assert_eq!(config.chafa.format, "ansi");
        assert!(config.jp2a.colors);
    }

    #[test]
    fn test_ptui_config_default() {
        let config = PTuiConfig::default();
        assert_eq!(config.converter.selected, "chafa");
        assert_eq!(config.locale, Some("en".to_string()));
        assert_eq!(config.slideshow_delay_ms, Some(2000));
        assert_eq!(config.chafa, None);
    }

    #[test]
    fn test_get_locale_with_value() {
        let config = PTuiConfig {
            locale: Some("fr".to_string()),
            ..Default::default()
        };
        assert_eq!(config.get_locale(), "fr");
    }

    #[test]
    fn test_get_locale_default() {
        let config = PTuiConfig {
            locale: None,
            ..Default::default()
        };
        assert_eq!(config.get_locale(), DEFAULT_LOCALE);
    }

    #[test]
    fn test_get_slideshow_delay_with_value() {
        let config = PTuiConfig {
            slideshow_delay_ms: Some(5000),
            ..Default::default()
        };
        assert_eq!(config.get_slideshow_delay_ms(), 5000);
    }

    #[test]
    fn test_get_slideshow_delay_default() {
        let config = PTuiConfig {
            slideshow_delay_ms: None,
            ..Default::default()
        };
        assert_eq!(config.get_slideshow_delay_ms(), 2000);
    }

    #[test]
    fn test_config_serialization() {
        let config = create_test_config();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PTuiConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(config.converter.selected, deserialized.converter.selected);
        assert_eq!(config.locale, deserialized.locale);
        assert_eq!(config.slideshow_delay_ms, deserialized.slideshow_delay_ms);
    }

    #[test]
    fn test_load_nonexistent_config_creates_default() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ptui").join("ptui.json");
        
        let config = PTuiConfig::create_default_config(&config_path).unwrap();
        
        assert_eq!(config.converter.selected, "chafa");
        assert_eq!(config.locale, Some("en".to_string()));
        assert_file_exists(&config_path.to_string_lossy());
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ptui.json");
        
        let original_config = PTuiConfig {
            converter: ConverterConfig {
                selected: "jp2a".to_string(),
                ..Default::default()
            },
            locale: Some("de".to_string()),
            slideshow_delay_ms: Some(3000),
            slideshow_transitions: Some(SlideshowTransitionConfig::default()),
            chafa: None,
        };
        
        PTuiConfig::save_config(&config_path, &original_config).unwrap();
        
        let contents = fs::read_to_string(&config_path).unwrap();
        let loaded_config: PTuiConfig = serde_json::from_str(&contents).unwrap();
        
        assert_eq!(loaded_config.converter.selected, "jp2a");
        assert_eq!(loaded_config.locale, Some("de".to_string()));
        assert_eq!(loaded_config.slideshow_delay_ms, Some(3000));
    }

    #[test]
    fn test_backward_compatibility_migration() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ptui.json");
        
        let old_config_json = r#"{
            "chafa": {
                "format": "sixel",
                "colors": "256"
            },
            "locale": "ja",
            "slideshow_delay_ms": 1500,
            "converter": {
                "chafa": {
                    "format": "ansi",
                    "colors": "full"
                },
                "jp2a": {
                    "colors": true,
                    "invert": false,
                    "dither": "none",
                    "chars": null
                },
                "graphical": {
                    "filter_type": "lanczos3",
                    "max_dimension": 768
                },
                "selected": "chafa"
            }
        }"#;
        
        fs::write(&config_path, old_config_json).unwrap();
        
        let contents = fs::read_to_string(&config_path).unwrap();
        let mut config: PTuiConfig = serde_json::from_str(&contents).unwrap();
        
        if let Some(old_chafa) = config.chafa.take() {
            config.converter.chafa = old_chafa;
            PTuiConfig::save_config(&config_path, &config).unwrap();
        }
        
        assert_eq!(config.converter.chafa.format, "sixel");
        assert_eq!(config.converter.chafa.colors, "256");
        assert_eq!(config.locale, Some("ja".to_string()));
        assert_eq!(config.slideshow_delay_ms, Some(1500));
    }

    #[test]
    fn test_invalid_config_falls_back_to_default() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ptui.json");
        
        fs::write(&config_path, "invalid json content").unwrap();
        
        let config = PTuiConfig::create_default_config(&config_path).unwrap();
        assert_eq!(config.converter.selected, "chafa");
        assert_eq!(config.locale, Some("en".to_string()));
    }

    #[test]
    fn test_config_path_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_config_path = temp_dir
            .path()
            .join("deep")
            .join("nested")
            .join("ptui.json");
        
        let config = PTuiConfig::create_default_config(&nested_config_path).unwrap();
        
        assert_file_exists(&nested_config_path.to_string_lossy());
        assert_eq!(config.converter.selected, "chafa");
    }

    #[rstest::rstest]
    #[case("ansi", "full")]
    #[case("sixel", "256")]
    #[case("kitty", "16")]
    fn test_chafa_config_variations(#[case] format: &str, #[case] colors: &str) {
        let config = ChafaConfig {
            format: format.to_string(),
            colors: colors.to_string(),
        };
        
        assert_eq!(config.format, format);
        assert_eq!(config.colors, colors);
    }

    #[rstest::rstest]
    #[case(true, false, "none", None)]
    #[case(false, true, "floyd", Some("@%#*+=-:. ".to_string()))]
    fn test_jp2a_config_variations(
        #[case] colors: bool,
        #[case] invert: bool,
        #[case] dither: &str,
        #[case] chars: Option<String>,
    ) {
        let config = Jp2aConfig {
            colors,
            invert,
            dither: dither.to_string(),
            chars: chars.clone(),
        };
        
        assert_eq!(config.colors, colors);
        assert_eq!(config.invert, invert);
        assert_eq!(config.dither, dither);
        assert_eq!(config.chars, chars);
    }
}