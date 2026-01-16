use crate::config::PTuiConfig;
use crate::converter;
use crate::file_browser::FileBrowser;
use crate::localization::Localization;
use crate::preview::{PreviewContent, PreviewManager};
use crate::transitions::TransitionManager;
use crate::ui::{UILayout, UIRenderer};
use ansi_to_tui::IntoText;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Text;
use std::error::Error;
use std::process::Command;
use std::time::{Duration, Instant};

const DIVIDER_PERCENT_INCREMENT: u16 = 2;

const EMBEDDED_LOGO: &str = r#"

     OooOOo.  oOoOOoOOo O       o ooOoOOo
     O     `O     o     o       O    O
     o      O     o     O       o    o
     O     .o     O     o       o    O
     oOooOO'      o     o       O    o
     o            O     O       O    O
     O            O     `o     Oo    O
     o'           o'     `OoooO'O ooOOoOo


{app_subtitle}
v{version}"#;

pub struct ChafaTui {
    file_browser: FileBrowser,
    preview_manager: PreviewManager,
    transition_manager: TransitionManager,
    ui_layout: UILayout,
    localization: Localization,
    preview_content: Option<PreviewContent>,
    is_preview_image: bool,
    is_text_file: bool,
    terminal_width: u16,
    terminal_height: u16,
    show_help_on_startup: bool,
    show_help_toggle: bool,
    ascii_logo: Option<Text<'static>>,
    // Text file scrolling state
    text_scroll_offset: usize,
    // Slideshow state
    is_slideshow_mode: bool,
    slideshow_start_index: usize,
    slideshow_current_index: usize,
    slideshow_last_change: Instant,
    slideshow_delay: Duration,
    slideshow_image_files: Vec<usize>, // Indices of image files only
    slideshow_previous_content: Option<PreviewContent>,
    // Delete confirmation dialog state
    show_delete_confirmation: bool,
    delete_target_file: Option<String>,
    // Dirty flag for render optimization
    needs_redraw: bool,
}

impl ChafaTui {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let config = PTuiConfig::load()?;
        Self::check_required_applications(&config)?;

        let locale = config.get_locale();
        let slideshow_delay = Duration::from_millis(config.get_slideshow_delay_ms());

        println!("Using locale: {}", locale);

        let localization = Localization::new(&locale)?;
        let file_browser = FileBrowser::new()?;
        let mut preview_manager = PreviewManager::new(config.clone());
        let transition_manager = TransitionManager::new(config.get_slideshow_transitions());

        // Set initial ready message
        preview_manager.debug_info = localization.get("ptui_ready");
        let ascii_logo = Self::load_ascii_logo();

        let mut app = Self {
            file_browser,
            preview_manager,
            transition_manager,
            ui_layout: UILayout::new(),
            localization,
            preview_content: None,
            is_preview_image: false,
            is_text_file: false,
            terminal_width: 80,
            terminal_height: 24,
            show_help_on_startup: true,
            show_help_toggle: false,
            ascii_logo,
            // Text file scrolling state
            text_scroll_offset: 0,
            // Slideshow state
            is_slideshow_mode: false,
            slideshow_start_index: 0,
            slideshow_current_index: 0,
            slideshow_last_change: Instant::now(),
            slideshow_delay,
            slideshow_image_files: Vec::new(),
            slideshow_previous_content: None,
            // Delete confirmation dialog state
            show_delete_confirmation: false,
            delete_target_file: None,
            // Dirty flag for render optimization
            needs_redraw: true,
        };

        app.update_preview();
        Ok(app)
    }

    fn check_required_applications(config: &PTuiConfig) -> Result<(), Box<dyn Error>> {
        // Check selected converter availability
        let selected_converter = &config.converter.selected;
        if let Err(e) = converter::check_converter_availability(selected_converter) {
            eprintln!("Error: {} is required but {}.", selected_converter, e);
            eprintln!(
                "Please install {} before running this application.",
                selected_converter
            );
            return Err(format!("{} not available", selected_converter).into());
        }

        // Check if identify is available (from ImageMagick) - always required for dimension detection
        let identify_result = Command::new("identify").arg("-version").output();
        if identify_result.is_err() || !identify_result.unwrap().status.success() {
            eprintln!(
                "Error: identify application (from ImageMagick) is required but not found in PATH."
            );
            eprintln!("Please install ImageMagick before running this application.");
            return Err("identify not found".into());
        }

        println!("Using converter: {}", selected_converter);
        Ok(())
    }

    fn load_ascii_logo() -> Option<Text<'static>> {
        // Use embedded logo instead of reading from file
        match EMBEDDED_LOGO.into_text() {
            Ok(text) => Some(text),
            Err(_) => {
                eprintln!("Warning: Failed to parse embedded ASCII logo");
                None
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        // Handle delete confirmation dialog first if it's showing
        if self.show_delete_confirmation {
            self.handle_delete_confirmation(key)?;
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Err("Quit".into()),
            KeyCode::Down | KeyCode::Char('j') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.move_down();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.move_up();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::PageDown => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.page_down();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::PageUp => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.page_up();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.page_down();
                self.update_preview();
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.page_up();
                self.update_preview();
            }
            KeyCode::Char('u') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                if self.is_text_file_selected() {
                    self.scroll_text_up();
                }
            }
            KeyCode::Char('f') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.jump_forward();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::Char('b') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.jump_backward();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::Char('d') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                let message_key = self.file_browser.sort_by_date();
                let message = self.localization.get(message_key);
                self.preview_manager.set_message(message.to_string());
                self.update_preview();
            }
            KeyCode::Char('n') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.sort_by_name();
                self.update_preview();
            }
            KeyCode::Enter => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                if self.file_browser.enter_directory()? {
                    self.preview_manager.clear_cache();
                    self.update_preview();
                }
            }
            KeyCode::Backspace => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                if self.file_browser.go_to_parent()? {
                    self.preview_manager.clear_cache();
                    self.update_preview();
                }
            }
            KeyCode::Char('r') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.refresh_current_preview();
            }
            KeyCode::Char('[') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.ui_layout.decrease_size(DIVIDER_PERCENT_INCREMENT);
                self.update_preview();
            }
            KeyCode::Char(']') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.ui_layout.increase_size(DIVIDER_PERCENT_INCREMENT);
                self.update_preview();
            }
            KeyCode::Char('s') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.save_ascii_file();
            }
            KeyCode::Char('x') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                // Show delete confirmation dialog
                self.show_delete_dialog();
            }
            KeyCode::Char('o') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.open_in_system_browser();
            }
            KeyCode::Char(' ') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                // Priority: text scrolling first, then slideshow
                if self.is_text_file_selected() {
                    self.scroll_text_down();
                } else if self.is_slideshow_mode {
                    self.exit_slideshow_mode();
                } else {
                    self.enter_slideshow_mode();
                }
            }
            KeyCode::Char('?') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = !self.show_help_toggle;
                self.update_preview();
            }
            KeyCode::Right => {
                if self.is_slideshow_mode {
                    self.advance_slideshow();
                } else {
                    // Normal navigation - right arrow same as down arrow
                    self.show_help_on_startup = false;
                    self.show_help_toggle = false;
                    self.file_browser.move_down();
                    self.update_preview();
                }
            }
            KeyCode::Left => {
                if self.is_slideshow_mode {
                    self.slideshow_go_backward();
                } else {
                    // Normal navigation - left arrow same as up arrow
                    self.show_help_on_startup = false;
                    self.show_help_toggle = false;
                    self.file_browser.move_up();
                    self.update_preview();
                }
            }
            KeyCode::Home => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.move_to_start();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::End => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.file_browser.move_to_end();
                self.reset_text_scroll();
                self.update_preview();
            }
            KeyCode::Tab => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.cycle_converter();
            }
            _ => {
                // Exit slideshow on any other key if in slideshow mode
                if self.is_slideshow_mode {
                    self.exit_slideshow_mode();
                }
            }
        }
        Ok(())
    }

    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
        self.update_preview();
        self.needs_redraw = true;
    }

    pub fn handle_config_reload(&mut self, new_config: PTuiConfig) -> Result<(), Box<dyn Error>> {
        // Check if locale has changed and needs reloading
        let current_locale = self.localization.current_locale();
        let new_locale = new_config.get_locale();

        if current_locale != new_locale {
            // Reload localization
            self.localization = Localization::new(&new_locale)?;
            self.preview_manager.debug_info =
                format!("Config reloaded | Locale changed to: {}", new_locale);
        } else {
            self.preview_manager.debug_info = "Config reloaded".to_string();
        }

        // Update slideshow delay
        self.slideshow_delay = Duration::from_millis(new_config.get_slideshow_delay_ms());

        // Update transition manager config
        self.transition_manager
            .update_config(new_config.get_slideshow_transitions());

        // Update preview manager config (for converter settings)
        self.preview_manager.update_config(new_config);

        // Clear cache to force regeneration with new settings
        self.preview_manager.clear_cache();

        // Update preview to reflect changes
        self.update_preview();
        self.needs_redraw = true;

        Ok(())
    }

    pub fn needs_redraw(&mut self) -> bool {
        if self.needs_redraw {
            self.needs_redraw = false;
            true
        } else {
            false
        }
    }

    fn update_preview(&mut self) {
        if self.show_help_on_startup || self.show_help_toggle {
            self.preview_content = None;
            self.is_preview_image = false;
            self.is_text_file = false;
        } else if let Some(file) = self.file_browser.get_selected_file() {
            self.is_text_file = file.is_text_file();
            self.preview_content = Some(self.preview_manager.generate_preview(
                file,
                self.ui_layout.preview_width,
                self.ui_layout.preview_height,
                self.text_scroll_offset,
                &self.localization,
            ));
            // Only treat actual image files as images for UI rendering (centered alignment)
            // ASCII files should be left-aligned like text files
            self.is_preview_image = file.is_image();
        } else {
            self.is_text_file = false;
            self.preview_content = None;
            self.is_preview_image = false;
        }
        self.needs_redraw = true;
    }

    fn refresh_current_preview(&mut self) {
        if let Some(file) = self.file_browser.get_selected_file()
            && file.can_preview()
        {
            self.preview_manager.remove_from_cache(
                file,
                self.ui_layout.preview_width,
                self.ui_layout.preview_height,
            );
            self.update_preview();
        }
    }

    fn save_ascii_file(&mut self) {
        if let Some(file) = self.file_browser.get_selected_file() {
            match self.preview_manager.save_ascii_to_file(
                file,
                self.ui_layout.preview_width,
                self.ui_layout.preview_height,
                &self.localization,
            ) {
                Ok(success_msg) => {
                    // Update debug info with success message
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | {}", current_debug, success_msg);

                    // Refresh file list to show the new ASCII file
                    if let Err(e) = self.file_browser.refresh_files() {
                        let current_debug = self.preview_manager.get_debug_info();
                        self.preview_manager.debug_info = format!(
                            "{} | WARNING: Failed to refresh file list: {}",
                            current_debug, e
                        );
                    }
                }
                Err(error_msg) => {
                    // Update debug info with error message
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | ERROR: {}", current_debug, error_msg);
                }
            }
        } else {
            // Update debug info when no file is selected
            let current_debug = self.preview_manager.get_debug_info();
            self.preview_manager.debug_info =
                format!("{} | ERROR: No file selected", current_debug);
        }
    }

    fn show_delete_dialog(&mut self) {
        if let Some(file) = self.file_browser.get_selected_file() {
            if file.is_directory {
                // Don't allow deleting directories
                let current_debug = self.preview_manager.get_debug_info();
                self.preview_manager.debug_info =
                    format!("{} | ERROR: Cannot delete directories", current_debug);
                return;
            }

            self.show_delete_confirmation = true;
            self.delete_target_file = Some(file.name.clone());
            self.needs_redraw = true;
        } else {
            let current_debug = self.preview_manager.get_debug_info();
            self.preview_manager.debug_info =
                format!("{} | ERROR: No file selected", current_debug);
        }
    }

    fn handle_delete_confirmation(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // User confirmed deletion
                if let Some(file_name) = &self.delete_target_file {
                    self.delete_current_file(file_name.clone())?;
                }
                self.hide_delete_dialog();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // User canceled deletion
                self.hide_delete_dialog();
            }
            _ => {
                // Ignore other keys
            }
        }
        Ok(())
    }

    fn hide_delete_dialog(&mut self) {
        self.show_delete_confirmation = false;
        self.delete_target_file = None;
        self.needs_redraw = true;
    }

    fn delete_current_file(&mut self, file_name: String) -> Result<(), Box<dyn Error>> {
        if let Some(file) = self.file_browser.get_selected_file() {
            let file_path = &file.path;

            match std::fs::remove_file(file_path) {
                Ok(()) => {
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | Deleted: {}", current_debug, file_name);

                    // Refresh file list to remove deleted file
                    if let Err(e) = self.file_browser.refresh_files() {
                        let current_debug = self.preview_manager.get_debug_info();
                        self.preview_manager.debug_info = format!(
                            "{} | WARNING: Failed to refresh file list: {}",
                            current_debug, e
                        );
                    }

                    // Update preview after refresh
                    self.update_preview();
                }
                Err(e) => {
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info = format!(
                        "{} | ERROR: Failed to delete {}: {}",
                        current_debug, file_name, e
                    );
                }
            }
        }
        Ok(())
    }

    fn open_in_system_browser(&mut self) {
        if let Some(file) = self.file_browser.get_selected_file() {
            let file_path = std::path::Path::new(&file.path);
            let target_path = if file.is_directory {
                // If it's a directory, open the directory itself
                file_path
            } else {
                // If it's a file, open the parent directory and select the file
                file_path.parent().unwrap_or(file_path)
            };

            let result = self.open_path_in_system_browser(
                target_path,
                if file.is_directory {
                    None
                } else {
                    Some(file_path)
                },
            );

            match result {
                Ok(()) => {
                    let message = if file.is_directory {
                        self.localization.get("opened_directory_in_browser")
                    } else {
                        self.localization.get("opened_file_in_browser")
                    };
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | {}: {}", current_debug, message, file.name);
                }
                Err(e) => {
                    let error_msg = self.localization.get("failed_to_open_in_browser");
                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | {}: {}", current_debug, error_msg, e);
                }
            }
        } else {
            let error_msg = self.localization.get("no_file_selected");
            let current_debug = self.preview_manager.get_debug_info();
            self.preview_manager.debug_info = format!("{} | {}", current_debug, error_msg);
        }
    }

    #[cfg(target_os = "macos")]
    fn open_path_in_system_browser(
        &self,
        dir_path: &std::path::Path,
        file_path: Option<&std::path::Path>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(file) = file_path {
            // On macOS, we can use 'open -R' to reveal the file in Finder
            Command::new("open")
                .args(["-R", &file.to_string_lossy()])
                .spawn()?;
        } else {
            // Open directory normally
            Command::new("open").arg(dir_path).spawn()?;
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn open_path_in_system_browser(
        &self,
        dir_path: &std::path::Path,
        file_path: Option<&std::path::Path>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(file) = file_path {
            // On Windows, we can use explorer.exe /select to open and highlight the file
            Command::new("explorer")
                .args(&["/select,", &file.to_string_lossy()])
                .spawn()?;
        } else {
            Command::new("explorer").arg(dir_path).spawn()?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn open_path_in_system_browser(
        &self,
        dir_path: &std::path::Path,
        file_path: Option<&std::path::Path>,
    ) -> Result<(), Box<dyn Error>> {
        // Try different file managers with file selection support where available
        let file_managers_with_selection = [
            ("nautilus", vec!["--select"]),
            ("dolphin", vec!["--select"]),
            ("thunar", vec![]), // Thunar doesn't have file selection, but we'll try to open the file directly
        ];

        let file_managers_basic = ["xdg-open", "pcmanfm"];

        // First try file managers that support file selection
        if let Some(file) = file_path {
            for (manager, args) in &file_managers_with_selection {
                if Command::new("which")
                    .arg(manager)
                    .output()?
                    .status
                    .success()
                {
                    let mut cmd = Command::new(manager);

                    if !args.is_empty() {
                        // Use selection argument with the file path
                        cmd.args(args).arg(file);
                    } else if *manager == "thunar" {
                        // For thunar, try to open the file directly, then fall back to directory
                        if Command::new("thunar").arg(file).spawn().is_err() {
                            Command::new("thunar").arg(dir_path).spawn()?;
                        }
                        return Ok(());
                    }

                    if cmd.spawn().is_ok() {
                        return Ok(());
                    }
                }
            }
        }

        // Fall back to basic file managers (just open directory)
        for manager in &file_managers_basic {
            if Command::new("which")
                .arg(manager)
                .output()?
                .status
                .success()
            {
                Command::new(manager).arg(dir_path).spawn()?;
                return Ok(());
            }
        }

        // Last resort: try all the managers we know about for directory opening
        let all_managers = ["nautilus", "dolphin", "thunar", "pcmanfm"];
        for manager in &all_managers {
            if Command::new("which")
                .arg(manager)
                .output()?
                .status
                .success()
            {
                Command::new(manager).arg(dir_path).spawn()?;
                return Ok(());
            }
        }

        Err("No suitable file manager found".into())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    fn open_path_in_system_browser(
        &self,
        _dir_path: &std::path::Path,
        _file_path: Option<&std::path::Path>,
    ) -> Result<(), Box<dyn Error>> {
        Err("Opening system file browser not supported on this platform".into())
    }

    fn enter_slideshow_mode(&mut self) {
        // Build list of image files starting from current selection
        self.slideshow_image_files.clear();
        self.slideshow_start_index = self.file_browser.selected_index;

        // Find all image files in the current directory
        for (i, file) in self.file_browser.files.iter().enumerate() {
            if file.is_image() {
                self.slideshow_image_files.push(i);
            }
        }

        if self.slideshow_image_files.is_empty() {
            // No images to show slideshow
            return;
        }

        // Find the position of current selection in image files list
        if let Some(pos) = self
            .slideshow_image_files
            .iter()
            .position(|&i| i == self.slideshow_start_index)
        {
            self.slideshow_current_index = pos;
        } else {
            // Current selection is not an image, start with first image
            self.slideshow_current_index = 0;
            // Update slideshow_start_index to the first image for consistency
            if !self.slideshow_image_files.is_empty() {
                self.slideshow_start_index = self.slideshow_image_files[0];
            }
        }

        self.is_slideshow_mode = true;
        self.slideshow_last_change = Instant::now();
        self.update_slideshow_preview();
    }

    fn exit_slideshow_mode(&mut self) {
        self.is_slideshow_mode = false;

        // Select the current slideshow file in the file browser
        if !self.slideshow_image_files.is_empty()
            && self.slideshow_current_index < self.slideshow_image_files.len()
        {
            let current_file_index = self.slideshow_image_files[self.slideshow_current_index];
            self.file_browser.set_selected_index(current_file_index);
        } else {
            // Fallback to original selection if something went wrong
            self.file_browser
                .set_selected_index(self.slideshow_start_index);
        }

        self.update_preview();
    }

    fn advance_slideshow(&mut self) {
        if !self.is_slideshow_mode || self.slideshow_image_files.is_empty() {
            return;
        }

        // Store current content for potential transition
        self.slideshow_previous_content = self.preview_content.clone();

        self.slideshow_current_index =
            (self.slideshow_current_index + 1) % self.slideshow_image_files.len();
        self.slideshow_last_change = Instant::now();
        self.update_slideshow_preview();

        // Check if we should start a transition effect
        // Transitions only work with Text content (ASCII art), not graphical content
        if self.transition_manager.is_enabled()
            && self.preview_manager.converter_supports_transitions()
            && let (Some(prev_content), Some(new_content)) =
                (&self.slideshow_previous_content, &self.preview_content)
            && let (PreviewContent::Text(prev_text), PreviewContent::Text(new_text)) =
                (prev_content, new_content)
            && self
                .transition_manager
                .start_transition(prev_text, new_text)
        {
            // Successfully started transition
            let current_debug = self.preview_manager.get_debug_info();
            self.preview_manager.debug_info = format!(
                "{} | Starting {} transition",
                current_debug,
                self.transition_manager.get_effect_name()
            );
        }
    }

    fn slideshow_go_backward(&mut self) {
        if !self.is_slideshow_mode || self.slideshow_image_files.is_empty() {
            return;
        }

        // Store current content for potential transition
        self.slideshow_previous_content = self.preview_content.clone();

        // Go backward with wrap-around (if at 0, go to last image)
        if self.slideshow_current_index == 0 {
            self.slideshow_current_index = self.slideshow_image_files.len() - 1;
        } else {
            self.slideshow_current_index -= 1;
        }
        self.slideshow_last_change = Instant::now();
        self.update_slideshow_preview();

        // Check if we should start a transition effect (same as advance_slideshow)
        // Transitions only work with Text content (ASCII art), not graphical content
        if self.transition_manager.is_enabled()
            && self.preview_manager.converter_supports_transitions()
            && let (Some(prev_content), Some(new_content)) =
                (&self.slideshow_previous_content, &self.preview_content)
            && let (PreviewContent::Text(prev_text), PreviewContent::Text(new_text)) =
                (prev_content, new_content)
            && self
                .transition_manager
                .start_transition(prev_text, new_text)
        {
            // Successfully started transition
            let current_debug = self.preview_manager.get_debug_info();
            self.preview_manager.debug_info = format!(
                "{} | Starting {} transition",
                current_debug,
                self.transition_manager.get_effect_name()
            );
        }
    }

    fn update_slideshow_preview(&mut self) {
        if !self.is_slideshow_mode || self.slideshow_image_files.is_empty() {
            return;
        }

        let file_index = self.slideshow_image_files[self.slideshow_current_index];
        if let Some(file) = self.file_browser.files.get(file_index) {
            self.preview_content = Some(self.preview_manager.generate_preview(
                file,
                self.terminal_width.saturating_sub(4),
                self.terminal_height.saturating_sub(4),
                0, // No text scrolling in slideshow mode
                &self.localization,
            ));
            self.is_preview_image = true;
        }
    }

    pub fn update_slideshow(&mut self) {
        if self.is_slideshow_mode && self.slideshow_last_change.elapsed() >= self.slideshow_delay {
            // Only advance slideshow if no transition is in progress
            if !self.transition_manager.is_in_transition() {
                self.advance_slideshow();
                self.needs_redraw = true;
            }
        }
    }

    /// Update transitions and return true if a redraw is needed
    pub fn update_transitions(&mut self) -> bool {
        if self.transition_manager.is_in_transition() {
            // Check if transition frame has changed
            let _current_frame = self.transition_manager.get_current_transition_frame();
            // A frame change or completion indicates we need to redraw
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // Update terminal dimensions
        self.terminal_width = size.width;
        self.terminal_height = size.height;

        if self.is_slideshow_mode {
            // Check if we have a transition in progress
            let transition_content: Option<PreviewContent>;
            let display_content = if let Some(transition_frame) =
                self.transition_manager.get_current_transition_frame()
            {
                transition_content = Some(PreviewContent::Text(transition_frame.clone()));
                transition_content.as_ref()
            } else {
                self.preview_content.as_ref()
            };

            // Render full-screen slideshow
            UIRenderer::render_slideshow(
                f,
                size,
                display_content,
                &self.localization,
                self.slideshow_current_index + 1,
                self.slideshow_image_files.len(),
            );
        } else {
            // Regular UI layout
            // Calculate layout
            let (file_area, preview_area, debug_area) = self.ui_layout.calculate_layout(size);

            // Render components
            UIRenderer::render_file_browser(f, file_area, &mut self.file_browser, true);

            UIRenderer::render_preview(
                f,
                preview_area,
                self.preview_content.as_ref(),
                &self.localization,
                self.ascii_logo.as_ref(),
                self.is_text_file
            );

            UIRenderer::render_debug_pane(
                f,
                debug_area,
                self.preview_manager.get_debug_info(),
                &self.localization,
            );
        }

        // Render delete confirmation dialog overlay if needed
        if self.show_delete_confirmation
            && let Some(ref file_name) = self.delete_target_file
        {
            UIRenderer::render_delete_confirmation_dialog(f, size, file_name, &self.localization);
        }
    }

    fn is_text_file_selected(&self) -> bool {
        if let Some(file) = self.file_browser.get_selected_file() {
            file.is_text_file() && !file.is_directory
        } else {
            false
        }
    }

    fn scroll_text_up(&mut self) {
        let scroll_amount = (self.ui_layout.preview_height as usize / 2).max(1);
        self.text_scroll_offset = self.text_scroll_offset.saturating_sub(scroll_amount);
        self.update_preview();
    }

    fn scroll_text_down(&mut self) {
        let scroll_amount = (self.ui_layout.preview_height as usize / 2).max(1);
        self.text_scroll_offset += scroll_amount;
        self.update_preview();
    }

    fn reset_text_scroll(&mut self) {
        self.text_scroll_offset = 0;
    }

    /// Cycle through available converters in order: chafa -> jp2a -> graphical -> chafa
    fn cycle_converter(&mut self) {
        let current_converter = &self.preview_manager.converter.get_name();
        let new_converter = match *current_converter {
            "chafa" => "jp2a",
            "jp2a" => "graphical",
            _ => "chafa", // Default to chafa for graphical or unknown
        };

        // Create a new config with the updated converter selection
        let mut new_config = self.preview_manager.config.clone();
        new_config.converter.selected = new_converter.to_string();

        // Update preview manager with new converter
        self.preview_manager.update_config(new_config);

        // Clear cache and refresh preview
        self.preview_manager.clear_cache();
        self.update_preview();

        // Show feedback in debug info
        let message = format!("Converter switched to: {}", new_converter);
        self.preview_manager.debug_info = message;
    }

    /// Clear Kitty graphics protocol images from the terminal
    /// This should be called when switching from graphical to text mode
    pub fn clear_graphics_if_needed(&self) {
        // If we're not showing graphical content, clear any lingering images
        // Only do this in non-test environments to avoid interfering with test output
        #[cfg(not(test))]
        {
            // Check if current preview is text-based (not graphical)
            let is_current_graphical =
                matches!(&self.preview_content, Some(PreviewContent::Graphical(_)));

            if !is_current_graphical {
                use std::io::Write;
                // Send Kitty protocol command to delete all images
                let delete_all_cmd = "\x1b_Ga=d,d=a\x1b\\";
                let _ = std::io::stdout().write_all(delete_all_cmd.as_bytes());
                let _ = std::io::stdout().flush();
            }
        }
    }
}
