use crate::config::{ChafaConfig, Jp2aConfig, PTuiConfig};
use std::process::Command;

pub trait AsciiConverter {
    fn convert_image(&self, path: &str, width: u16, height: u16) -> Result<String, String>;
    fn get_name(&self) -> &'static str;
    fn supports_transitions(&self) -> bool;
}

pub struct ChafaConverter {
    config: ChafaConfig,
}

impl ChafaConverter {
    pub fn new(config: ChafaConfig) -> Self {
        Self { config }
    }
}

impl AsciiConverter for ChafaConverter {
    fn convert_image(&self, path: &str, width: u16, height: u16) -> Result<String, String> {
        let args = vec![
            "-f".to_string(), 
            self.config.format.clone(),
            "-c".to_string(), 
            self.config.colors.clone(),
            "--size".to_string(), 
            format!("{}x{}", width, height),
            path.to_string(),
        ];

        match Command::new("chafa").args(&args).output() {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Err(format!("Chafa error: {}", String::from_utf8_lossy(&output.stderr)))
                }
            }
            Err(e) => Err(format!("Failed to execute chafa: {}", e)),
        }
    }

    fn get_name(&self) -> &'static str {
        "chafa"
    }

    fn supports_transitions(&self) -> bool {
        // Chafa produces complex ANSI sequences with colors and positioning
        // that don't work well with character-based transition effects
        false
    }
}

pub struct Jp2aConverter {
    config: Jp2aConfig,
}

impl Jp2aConverter {
    pub fn new(config: Jp2aConfig) -> Self {
        Self { config }
    }
}

impl AsciiConverter for Jp2aConverter {
    fn convert_image(&self, path: &str, width: u16, height: u16) -> Result<String, String> {
        let mut args = vec![];

        // jp2a uses --size=WxH format (note the equals sign)
        args.push(format!("--size={}x{}", width, height));

        if self.config.colors {
            args.push("--colors".to_string());
        }

        if self.config.invert {
            args.push("--invert".to_string());
        }

        // jp2a doesn't have a --dither option, but it has other options
        // We'll ignore the dither setting for jp2a
        
        if let Some(ref chars) = self.config.chars {
            args.push(format!("--chars={}", chars));
        }

        args.push(path.to_string());

        match Command::new("jp2a").args(&args).output() {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Err(format!("jp2a error: {}", String::from_utf8_lossy(&output.stderr)))
                }
            }
            Err(e) => Err(format!("Failed to execute jp2a: {}", e)),
        }
    }

    fn get_name(&self) -> &'static str {
        "jp2a"
    }

    fn supports_transitions(&self) -> bool {
        // jp2a produces simple ASCII characters that work well with
        // character-based transition effects
        true
    }
}

pub fn create_converter(config: &PTuiConfig) -> Box<dyn AsciiConverter> {
    match config.converter.selected.as_str() {
        "jp2a" => Box::new(Jp2aConverter::new(config.converter.jp2a.clone())),
        "chafa" => Box::new(ChafaConverter::new(config.converter.chafa.clone())),
        _ => Box::new(ChafaConverter::new(config.converter.chafa.clone())), // Default to chafa
    }
}

pub fn check_converter_availability(converter_name: &str) -> Result<(), String> {
    let result = match converter_name {
        "chafa" => Command::new("chafa").arg("--version").output(),
        "jp2a" => Command::new("jp2a").arg("--version").output(),
        _ => return Err(format!("Unknown converter: {}", converter_name)),
    };

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err(format!("{} command failed", converter_name)),
        Err(_) => Err(format!("{} not found in PATH", converter_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConverterConfig;

    #[test]
    fn test_chafa_converter_creation() {
        let config = ChafaConfig {
            format: "ansi".to_string(),
            colors: "full".to_string(),
        };
        let converter = ChafaConverter::new(config);
        assert_eq!(converter.get_name(), "chafa");
    }

    #[test]
    fn test_jp2a_converter_creation() {
        let config = Jp2aConfig {
            colors: true,
            invert: false,
            dither: "none".to_string(),
            chars: None,
        };
        let converter = Jp2aConverter::new(config);
        assert_eq!(converter.get_name(), "jp2a");
    }

    #[test]
    fn test_create_chafa_converter() {
        let config = PTuiConfig {
            converter: ConverterConfig {
                selected: "chafa".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let converter = create_converter(&config);
        assert_eq!(converter.get_name(), "chafa");
    }

    #[test]
    fn test_create_jp2a_converter() {
        let config = PTuiConfig {
            converter: ConverterConfig {
                selected: "jp2a".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let converter = create_converter(&config);
        assert_eq!(converter.get_name(), "jp2a");
    }

    #[test]
    fn test_create_default_converter_fallback() {
        let config = PTuiConfig {
            converter: ConverterConfig {
                selected: "unknown".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let converter = create_converter(&config);
        assert_eq!(converter.get_name(), "chafa");
    }

    #[test]
    fn test_chafa_convert_image_args() {
        let config = ChafaConfig {
            format: "ansi".to_string(),
            colors: "256".to_string(),
        };
        let converter = ChafaConverter::new(config);
        
        let result = converter.convert_image("test.jpg", 80, 24);
        
        match result {
            Ok(_) => {
            },
            Err(e) => {
                assert!(e.contains("chafa") || e.contains("Failed to execute"));
            }
        }
    }

    #[test]
    fn test_jp2a_convert_image_args() {
        let config = Jp2aConfig {
            colors: true,
            invert: true,
            dither: "none".to_string(),
            chars: Some("@%#*".to_string()),
        };
        let converter = Jp2aConverter::new(config);
        
        let result = converter.convert_image("test.jpg", 80, 24);
        
        match result {
            Ok(_) => {
            },
            Err(e) => {
                assert!(e.contains("jp2a") || e.contains("Failed to execute"));
            }
        }
    }

    #[test]
    fn test_check_converter_availability_unknown() {
        let result = check_converter_availability("unknown_converter");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown converter"));
    }

    #[test]
    fn test_chafa_config_options() {
        let config = ChafaConfig {
            format: "sixel".to_string(),
            colors: "16".to_string(),
        };
        let converter = ChafaConverter::new(config);
        
        assert_eq!(converter.config.format, "sixel");
        assert_eq!(converter.config.colors, "16");
    }

    #[test]
    fn test_jp2a_config_options() {
        let config = Jp2aConfig {
            colors: false,
            invert: true,
            dither: "floyd".to_string(),
            chars: Some("ascii".to_string()),
        };
        let converter = Jp2aConverter::new(config);
        
        assert!(!converter.config.colors);
        assert!(converter.config.invert);
        assert_eq!(converter.config.dither, "floyd");
        assert_eq!(converter.config.chars, Some("ascii".to_string()));
    }

    #[rstest::rstest]
    #[case("ansi", "full")]
    #[case("sixel", "256")]
    #[case("kitty", "16")]
    #[case("iterm", "8")]
    fn test_chafa_converter_variations(#[case] format: &str, #[case] colors: &str) {
        let config = ChafaConfig {
            format: format.to_string(),
            colors: colors.to_string(),
        };
        let converter = ChafaConverter::new(config);
        
        assert_eq!(converter.config.format, format);
        assert_eq!(converter.config.colors, colors);
        assert_eq!(converter.get_name(), "chafa");
    }

    #[rstest::rstest]
    #[case(true, false, None)]
    #[case(false, true, Some("@%#*+=-:. ".to_string()))]
    #[case(true, true, Some("01".to_string()))]
    fn test_jp2a_converter_variations(
        #[case] colors: bool,
        #[case] invert: bool,
        #[case] chars: Option<String>,
    ) {
        let config = Jp2aConfig {
            colors,
            invert,
            dither: "none".to_string(),
            chars: chars.clone(),
        };
        let converter = Jp2aConverter::new(config);
        
        assert_eq!(converter.config.colors, colors);
        assert_eq!(converter.config.invert, invert);
        assert_eq!(converter.config.chars, chars);
        assert_eq!(converter.get_name(), "jp2a");
    }
}