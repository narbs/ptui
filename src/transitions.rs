use crate::config::SlideshowTransitionConfig;
use ratatui::text::Text;
use std::time::{Duration, Instant};
use ansi_to_tui::IntoText;

pub struct TransitionManager {
    config: SlideshowTransitionConfig,
    transition_start_time: Option<Instant>,
    cached_frames: Vec<Text<'static>>,
    current_frame_index: usize,
    total_transition_duration: Duration,
}

impl TransitionManager {
    pub fn new(config: SlideshowTransitionConfig) -> Self {
        let total_duration = Duration::from_millis(config.frame_duration_ms * 20); // 20 frames total
        Self {
            config,
            transition_start_time: None,
            cached_frames: Vec::new(),
            current_frame_index: 0,
            total_transition_duration: total_duration,
        }
    }

    pub fn update_config(&mut self, config: SlideshowTransitionConfig) {
        self.total_transition_duration = Duration::from_millis(config.frame_duration_ms * 20);
        self.config = config;
        // Clear any ongoing transition when config changes
        self.reset_transition();
    }

    pub fn get_effect_name(&self) -> &str {
        &self.config.effect
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Start a new transition animation from one content to another
    pub fn start_transition(&mut self, from_content: &Text, to_content: &Text) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Convert Text to string for terani
        let _from_str = self.text_to_string(from_content);
        let to_str = self.text_to_string(to_content);

        // Start the transition
        self.transition_start_time = Some(Instant::now());
        self.cached_frames.clear();
        self.current_frame_index = 0;
        
        // Pre-render all frames for smooth playback using our simulation
        self.prerender_transition_frames(&to_str);
        true
    }

    /// Get the current transition frame, or None if transition is complete
    pub fn get_current_transition_frame(&mut self) -> Option<&Text<'static>> {
        if !self.is_in_transition() {
            return None;
        }

        // Calculate which frame should be displayed based on timing
        if let Some(start_time) = self.transition_start_time {
            let elapsed = start_time.elapsed();
            
            if elapsed >= self.total_transition_duration {
                // Transition complete
                self.reset_transition();
                return None;
            }
            
            let progress = elapsed.as_millis() as f32 / self.total_transition_duration.as_millis() as f32;
            let target_frame = (progress * (self.cached_frames.len() - 1) as f32) as usize;
            
            self.current_frame_index = target_frame.min(self.cached_frames.len().saturating_sub(1));
            self.cached_frames.get(self.current_frame_index)
        } else {
            None
        }
    }

    /// Check if we're currently in the middle of a transition
    pub fn is_in_transition(&self) -> bool {
        self.transition_start_time.is_some() && !self.cached_frames.is_empty()
    }

    /// Reset/stop the current transition
    pub fn reset_transition(&mut self) {
        self.transition_start_time = None;
        self.cached_frames.clear();
        self.current_frame_index = 0;
    }


    /// Pre-render all animation frames for smooth playback using terani-inspired effects
    fn prerender_transition_frames(&mut self, target_text: &str) {
        self.cached_frames.clear();
        
        // Create multiple frames for smooth animation (simulate terani effects)
        let num_frames = 20; // Configurable number of frames
        for i in 0..=num_frames {
            let progress = i as f32 / num_frames as f32;
            let frame_text = self.create_transition_frame(target_text, progress);
            
            if let Ok(text) = frame_text.into_text() {
                self.cached_frames.push(text);
            } else {
                // Fallback to simple text
                self.cached_frames.push(Text::from(target_text.to_string()));
                break;
            }
        }
    }

    /// Create a single transition frame with the given progress (0.0 to 1.0)
    fn create_transition_frame(&self, text: &str, progress: f32) -> String {
        match self.config.effect.as_str() {
            "scattering" => self.simulate_scattering_frame(text, progress),
            "typewriter" => self.simulate_typewriter_frame(text, progress),
            "scrolling_left" | "scrolling_right" => self.simulate_scrolling_frame(text, progress),
            "climbing" => self.simulate_climbing_frame(text, progress),
            _ => text.to_string(),
        }
    }

    fn simulate_scattering_frame(&self, text: &str, progress: f32) -> String {
        // Simple scattering simulation: gradually reveal characters
        let total_chars = text.chars().count();
        let visible_chars = (total_chars as f32 * progress) as usize;
        text.chars().take(visible_chars).collect()
    }

    fn simulate_typewriter_frame(&self, text: &str, progress: f32) -> String {
        // Typewriter effect: reveal characters one by one
        let total_chars = text.chars().count();
        let visible_chars = (total_chars as f32 * progress) as usize;
        text.chars().take(visible_chars).collect::<String>() + if progress < 1.0 { "█" } else { "" }
    }

    fn simulate_scrolling_frame(&self, text: &str, progress: f32) -> String {
        // Simple scrolling effect: shift text position
        let shift = (20.0 * (1.0 - progress)) as usize;
        let padding: String = " ".repeat(shift);
        format!("{}{}", padding, text)
    }

    fn simulate_climbing_frame(&self, text: &str, progress: f32) -> String {
        // Climbing effect: move text up gradually
        let lines_shift = (5.0 * (1.0 - progress)) as usize;
        let padding: String = "\n".repeat(lines_shift);
        format!("{}{}", padding, text)
    }

    fn text_to_string(&self, text: &Text) -> String {
        // Extract raw text content from ratatui Text
        // This is a simplified conversion - in practice, we may need
        // more sophisticated handling of ANSI sequences
        text.lines.iter()
            .map(|line| line.spans.iter()
                .map(|span| span.content.as_ref())
                .collect::<String>())
            .collect::<Vec<String>>()
            .join("\n")
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SlideshowTransitionConfig;
    use ratatui::text::Line;

    #[test]
    fn test_transition_manager_creation() {
        let config = SlideshowTransitionConfig::default();
        let manager = TransitionManager::new(config.clone());
        assert_eq!(manager.config.enabled, config.enabled);
    }

    #[test]
    fn test_is_enabled() {
        let config = SlideshowTransitionConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = TransitionManager::new(config);
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_is_disabled() {
        let config = SlideshowTransitionConfig {
            enabled: false,
            ..Default::default()
        };
        let manager = TransitionManager::new(config);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn test_start_transition_when_disabled() {
        let config = SlideshowTransitionConfig {
            enabled: false,
            effect: "scattering".to_string(),
            frame_duration_ms: 50,
        };
        let mut manager = TransitionManager::new(config);
        
        let text1 = Text::from("Hello");
        let text2 = Text::from("World");
        
        let result = manager.start_transition(&text1, &text2);
        assert!(!result);
        assert!(!manager.is_in_transition());
    }

    #[test]
    fn test_start_transition_when_enabled() {
        let config = SlideshowTransitionConfig {
            enabled: true,
            effect: "scattering".to_string(),
            frame_duration_ms: 50,
        };
        let mut manager = TransitionManager::new(config);
        
        let text1 = Text::from("Hello");
        let text2 = Text::from("World");
        
        let result = manager.start_transition(&text1, &text2);
        assert!(result);
        assert!(manager.is_in_transition());
    }

    #[test]
    fn test_text_to_string_simple() {
        let config = SlideshowTransitionConfig::default();
        let manager = TransitionManager::new(config);
        
        let text = Text::from("Hello World");
        let result = manager.text_to_string(&text);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_text_to_string_multiline() {
        let config = SlideshowTransitionConfig::default();
        let manager = TransitionManager::new(config);
        
        let text = Text::from(vec![
            Line::from("Line 1"),
            Line::from("Line 2"),
        ]);
        let result = manager.text_to_string(&text);
        assert_eq!(result, "Line 1\nLine 2");
    }


    #[test]
    fn test_update_config() {
        let initial_config = SlideshowTransitionConfig {
            enabled: false,
            effect: "scattering".to_string(),
            frame_duration_ms: 50,
        };
        let mut manager = TransitionManager::new(initial_config);
        
        let new_config = SlideshowTransitionConfig {
            enabled: true,
            effect: "typewriter".to_string(),
            frame_duration_ms: 100,
        };
        
        manager.update_config(new_config.clone());
        assert_eq!(manager.config.enabled, new_config.enabled);
        assert_eq!(manager.config.effect, new_config.effect);
        assert_eq!(manager.config.frame_duration_ms, new_config.frame_duration_ms);
        assert!(!manager.is_in_transition()); // Should reset transition
    }

    #[test]
    fn test_reset_transition() {
        let config = SlideshowTransitionConfig {
            enabled: true,
            effect: "typewriter".to_string(),
            frame_duration_ms: 50,
        };
        let mut manager = TransitionManager::new(config);
        
        let text1 = Text::from("Hello");
        let text2 = Text::from("World");
        
        manager.start_transition(&text1, &text2);
        assert!(manager.is_in_transition());
        
        manager.reset_transition();
        assert!(!manager.is_in_transition());
    }

    #[test]
    fn test_transition_frame_progression() {
        let config = SlideshowTransitionConfig {
            enabled: true,
            effect: "typewriter".to_string(),
            frame_duration_ms: 10, // Very fast for testing
        };
        let mut manager = TransitionManager::new(config);
        
        let text1 = Text::from("Hello");
        let text2 = Text::from("World");
        
        manager.start_transition(&text1, &text2);
        
        // Should have frames available initially
        let first_frame = manager.get_current_transition_frame();
        assert!(first_frame.is_some());
        
        // After sufficient time, transition should complete
        std::thread::sleep(std::time::Duration::from_millis(200));
        let final_frame = manager.get_current_transition_frame();
        // Transition should be complete (None returned)
        assert!(final_frame.is_none() || !manager.is_in_transition());
    }
}