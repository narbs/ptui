use crate::config::PTuiConfig;
use crate::converter;
use crate::file_browser::FileBrowser;
use crate::localization::Localization;
use crate::preview::{PreviewContent, PreviewManager};
use crate::ratings::{self, MAX_RATING};
use crate::state::{PTuiState, RatingDestination, SidecarConsent, rating_destination};
use crate::transfer::{self, Resolution, Stage, TransferAction, TransferDialog, TransferMode};
use crate::transitions::TransitionManager;
use crate::ui::{UILayout, UIRenderer};
use ansi_to_tui::IntoText;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Text;
use std::error::Error;
use std::path::PathBuf;
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
    // Copy/move destination dialog state
    transfer_dialog: Option<TransferDialog>,
    // Sidecar consent dialog state: the rating awaiting the user's answer
    pending_rating: Option<u8>,
    // Name of the sidecar the prompt is asking permission to create
    pending_sidecar_name: Option<String>,
    // Persisted state (last copy/move destination, sidecar consent, fallback ratings)
    app_state: PTuiState,
    // Star rating preferences
    stars_config: crate::config::StarsConfig,
    // Dirty flag for render optimization
    needs_redraw: bool,
}

impl ChafaTui {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let loaded = PTuiConfig::load()?;
        let config = loaded.config;
        Self::check_required_applications(&config)?;

        let locale = config.get_locale();
        let slideshow_delay = Duration::from_millis(config.get_slideshow_delay_ms());

        println!("Using locale: {}", locale);

        let localization = Localization::new(&locale)?;
        let file_browser = FileBrowser::new()?;
        let mut preview_manager = PreviewManager::new(config.clone());
        let transition_manager = TransitionManager::new(config.get_slideshow_transitions());

        // Set initial ready message, or say why the config on disk is not in use. The
        // file is left alone in that case, so this message is the only sign of it.
        preview_manager.debug_info = match &loaded.parse_error {
            Some(error) => {
                let args = fluent::fluent_args!["error" => error.as_str()];
                localization.get_with_args("config_unreadable", Some(&args))
            }
            None => localization.get("ptui_ready"),
        };
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
            // Copy/move destination dialog state
            transfer_dialog: None,
            pending_rating: None,
            pending_sidecar_name: None,
            app_state: PTuiState::load(),
            stars_config: config.get_stars(),
            // Dirty flag for render optimization
            needs_redraw: true,
        };

        app.apply_fallback_ratings();
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
        // Ctrl+C quits, ahead of the dialogs, because that is what it means in a terminal
        // and a user reaching for it wants out rather than a cancelled field. Raw mode
        // suppresses the signal, so it arrives here as an ordinary key. Without the guard
        // it would match the plain 'c' arm below and open the copy dialog instead.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Err("Quit".into());
        }

        // Handle delete confirmation dialog first if it's showing
        if self.show_delete_confirmation {
            self.handle_delete_confirmation(key)?;
            return Ok(());
        }

        // Handle the copy/move destination dialog if it's showing
        if self.transfer_dialog.is_some() {
            self.handle_transfer_key(key);
            return Ok(());
        }

        // Handle the one-time sidecar prompt if it's showing
        if self.pending_rating.is_some() {
            self.handle_sidecar_consent(key);
            return Ok(());
        }

        match key.code {
            // Ctrl+C is handled above, before the dialogs.
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
                let message_key = self.file_browser.sort_by_name();
                let message = self.localization.get(message_key);
                self.preview_manager.set_message(message.to_string());
                self.update_preview();
            }
            KeyCode::Enter => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                if self.file_browser.enter_directory()? {
                    self.preview_manager.clear_cache();
                    self.apply_fallback_ratings();
                    self.update_preview();
                }
            }
            KeyCode::Backspace => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                if self.file_browser.go_to_parent()? {
                    self.preview_manager.clear_cache();
                    self.apply_fallback_ratings();
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
                let message_key = self.file_browser.sort_by_rating();
                let message = self.localization.get(message_key);
                self.preview_manager.set_message(message.to_string());
                self.update_preview();
            }
            KeyCode::Char('i') => {
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
            KeyCode::Char('c') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.show_transfer_dialog(TransferMode::Copy);
            }
            KeyCode::Char('m') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.show_transfer_dialog(TransferMode::Move);
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
            KeyCode::Char(c @ '0'..='5') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                // `as u8 - b'0'` is safe here: the pattern admits only ASCII digits.
                self.rate_selected(c as u8 - b'0');
            }
            KeyCode::Char('*') => {
                self.show_help_on_startup = false;
                self.show_help_toggle = false;
                self.rate_selected(MAX_RATING);
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
        // Re-read the rating from disk as well as the image. Another tool may have changed
        // it in the meantime -- a sidecar is a shared record, and ptui is not its only writer.
        self.reload_selected_rating();

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

    /// Re-read the selected file's rating, preferring its sidecar over the private store.
    fn reload_selected_rating(&mut self) {
        let Some(file) = self.file_browser.get_selected_file() else {
            return;
        };
        let path = PathBuf::from(&file.path);
        if file.is_directory {
            return;
        }

        let rating = ratings::read_rating(&path)
            .or_else(|| self.app_state.fallback_rating(&path))
            .unwrap_or(0);
        self.file_browser.set_selected_rating(rating);
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
                    self.append_message(success_msg);

                    // Refresh file list to show the new ASCII file. The saved file sorts
                    // into the listing and shifts the indices, so reselect by name to stay
                    // on the image that was saved.
                    let fallback_names = self.file_browser.selection_fallback_names();

                    if let Err(e) = self.file_browser.refresh_files() {
                        self.append_message(format!("WARNING: Failed to refresh file list: {}", e));
                    }
                    self.apply_fallback_ratings();
                    if !self.file_browser.select_first_available(&fallback_names) {
                        self.clamp_selection();
                    }

                    // The listing gained a file and the selection may have shifted, so the
                    // preview has to be rebuilt for the pane to show the new state.
                    self.update_preview();
                }
                Err(error_msg) => {
                    self.append_message(format!("ERROR: {}", error_msg));
                }
            }
        } else {
            self.append_message("ERROR: No file selected".to_string());
        }
    }

    fn show_delete_dialog(&mut self) {
        // Copied out before reporting anything, so the browser borrow does not outlive it.
        let Some((name, is_directory)) = self
            .file_browser
            .get_selected_file()
            .map(|f| (f.name.clone(), f.is_directory))
        else {
            self.append_message("ERROR: No file selected".to_string());
            return;
        };

        if is_directory {
            // Don't allow deleting directories
            self.append_message("ERROR: Cannot delete directories".to_string());
            return;
        }

        self.show_delete_confirmation = true;
        self.delete_target_file = Some(name);
        self.needs_redraw = true;
    }

    fn handle_delete_confirmation(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
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
            let sidecar_source = PathBuf::from(file_path);

            match std::fs::remove_file(file_path) {
                Ok(()) => {
                    // Remove the rating alongside the file, so ptui does not leave behind
                    // the orphaned sidecars that tools which only write them accumulate.
                    if let Err(e) = ratings::remove_sidecar(&sidecar_source) {
                        self.append_message(format!("WARNING: Failed to remove sidecar: {}", e));
                    }
                    self.app_state.set_fallback_rating(&sidecar_source, 0);
                    self.app_state.save();

                    let current_debug = self.preview_manager.get_debug_info();
                    self.preview_manager.debug_info =
                        format!("{} | Deleted: {}", current_debug, file_name);

                    // Refresh file list to remove deleted file. The deleted file is gone
                    // from the fallback list, so the selection lands on the file that
                    // followed it, by name rather than by a position the refresh may have
                    // shifted.
                    let fallback_names = self.file_browser.selection_fallback_names();

                    if let Err(e) = self.file_browser.refresh_files() {
                        let current_debug = self.preview_manager.get_debug_info();
                        self.preview_manager.debug_info = format!(
                            "{} | WARNING: Failed to refresh file list: {}",
                            current_debug, e
                        );
                    }
                    self.apply_fallback_ratings();
                    if !self.file_browser.select_first_available(&fallback_names) {
                        self.clamp_selection();
                    }

                    // Update preview after refresh
                    self.update_preview();
                }
                Err(e) => {
                    self.append_message(format!("ERROR: Failed to delete {}: {}", file_name, e));
                }
            }
        }
        Ok(())
    }

    /// True while any modal dialog is open. Graphical previews must not be drawn then,
    /// because the Kitty/iTerm2 image layer sits above the text cells.
    fn dialog_active(&self) -> bool {
        self.show_delete_confirmation
            || self.transfer_dialog.is_some()
            || self.pending_rating.is_some()
    }

    /// Show a message in the debug pane.
    ///
    /// This sets the redraw flag, because the main loop only draws when something asks it
    /// to: a message posted without one sits invisible until the user happens to press
    /// another key, which reads as the action having done nothing.
    fn append_message(&mut self, message: String) {
        let current_debug = self.preview_manager.get_debug_info();
        self.preview_manager.debug_info = format!("{} | {}", current_debug, message);
        self.needs_redraw = true;
    }

    fn show_transfer_dialog(&mut self, mode: TransferMode) {
        let Some(file) = self.file_browser.get_selected_file() else {
            let message = self.localization.get("no_file_selected");
            self.append_message(message);
            return;
        };

        if file.is_directory {
            let message = self.localization.get("cannot_transfer_directory");
            self.append_message(message);
            return;
        }

        let source = PathBuf::from(&file.path);
        let file_name = file.name.clone();
        let current_dir = PathBuf::from(&self.file_browser.current_dir);
        let last_used = self.app_state.get_last_transfer_destination();

        self.transfer_dialog = Some(TransferDialog::new(
            mode,
            source,
            file_name,
            current_dir,
            last_used.as_deref(),
        ));
        self.needs_redraw = true;
    }

    fn handle_transfer_key(&mut self, key: KeyEvent) {
        let action = match self.transfer_dialog.as_mut() {
            Some(dialog) => transfer::handle_key(dialog, key),
            None => return,
        };

        match action {
            TransferAction::None => {}
            TransferAction::Close => self.transfer_dialog = None,
            TransferAction::Propose(dest) => self.propose_transfer_destination(dest),
            TransferAction::Execute(dest) => self.execute_transfer(dest),
        }

        self.needs_redraw = true;
    }

    /// Act on a chosen destination: transfer straight away unless it would replace an
    /// existing file. Validation errors keep the dialog open so the path can be corrected.
    fn propose_transfer_destination(&mut self, dest: PathBuf) {
        let Some(dialog) = self.transfer_dialog.as_ref() else {
            return;
        };

        let resolution = transfer::resolve_destination(dialog, dest);

        match resolution {
            Ok(Resolution::Transfer(dest)) => self.execute_transfer(dest),
            Ok(Resolution::ConfirmOverwrite(dest)) => {
                if let Some(dialog) = self.transfer_dialog.as_mut() {
                    dialog.error = None;
                    dialog.stage = Stage::ConfirmOverwrite { dest };
                }
            }
            Err(error) => {
                if let Some(dialog) = self.transfer_dialog.as_mut() {
                    dialog.error = Some(error.message_key());
                }
            }
        }
    }

    fn execute_transfer(&mut self, dest: PathBuf) {
        let Some(dialog) = self.transfer_dialog.take() else {
            return;
        };

        match transfer::perform(dialog.mode, &dialog.source, &dest) {
            Ok(target) => {
                self.app_state.set_last_transfer_destination(&dest);
                // A sidecar travels with the file inside transfer::perform. A rating held
                // in the private store has to be moved here instead, or it would be left
                // behind under a path that no longer exists.
                self.app_state.transfer_rating(
                    &dialog.source,
                    &target,
                    dialog.mode == TransferMode::Move,
                );
                self.app_state.save();

                let message = format!(
                    "{}: {} -> {}",
                    self.localization.get(dialog.mode.success_key()),
                    dialog.file_name,
                    target.display()
                );
                self.append_message(message);

                // A move removes the file from the current listing, so its cached preview
                // is no longer reachable.
                if dialog.mode == TransferMode::Move {
                    self.preview_manager.clear_cache();
                }

                // Reselect by name rather than by index. The listing is re-read from
                // disk, so anything added or removed in the background since the dialog
                // opened would otherwise shift the selection onto an unrelated file.
                // After a copy the first candidate is the file itself; after a move it
                // has gone, so the selection lands on the file that followed it.
                let fallback_names = self.file_browser.selection_fallback_names();

                if let Err(e) = self.file_browser.refresh_files() {
                    self.append_message(format!("WARNING: Failed to refresh file list: {}", e));
                }
                self.apply_fallback_ratings();
                if !self.file_browser.select_first_available(&fallback_names) {
                    self.clamp_selection();
                }
                self.update_preview();
            }
            Err(e) => {
                let message = format!("{}: {}", self.localization.get("transfer_failed"), e);
                self.append_message(message);
            }
        }
    }

    /// Keep the selection inside the file list after entries disappear.
    fn clamp_selection(&mut self) {
        let file_count = self.file_browser.files.len();
        if file_count == 0 {
            self.file_browser.selected_index = 0;
            self.file_browser.scroll_offset = 0;
        } else if self.file_browser.selected_index >= file_count {
            self.file_browser.selected_index = file_count - 1;
            self.file_browser.center_on_selection();
        }
    }

    fn open_in_system_browser(&mut self) {
        // Copy out what is needed before reporting anything: the selected file borrows the
        // browser, and append_message needs the whole of self.
        let Some((path, name, is_directory)) = self
            .file_browser
            .get_selected_file()
            .map(|f| (PathBuf::from(&f.path), f.name.clone(), f.is_directory))
        else {
            let error_msg = self.localization.get("no_file_selected");
            self.append_message(error_msg);
            return;
        };

        // A directory opens itself; a file opens its parent with the file selected.
        let target_path = if is_directory {
            path.as_path()
        } else {
            path.parent().unwrap_or(path.as_path())
        };
        let select = if is_directory {
            None
        } else {
            Some(path.as_path())
        };

        match self.open_path_in_system_browser(target_path, select) {
            Ok(()) => {
                let message = self.localization.get(if is_directory {
                    "opened_directory_in_browser"
                } else {
                    "opened_file_in_browser"
                });
                self.append_message(format!("{}: {}", message, name));
            }
            Err(e) => {
                let error_msg = self.localization.get("failed_to_open_in_browser");
                self.append_message(format!("{}: {}", error_msg, e));
            }
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
        self.update_slideshow_preview();
        self.slideshow_last_change = Instant::now();
        self.needs_redraw = true;
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

        self.needs_redraw = true;
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

        self.needs_redraw = true;
    }

    fn update_slideshow_preview(&mut self) {
        if !self.is_slideshow_mode || self.slideshow_image_files.is_empty() {
            return;
        }

        let file_index = self.slideshow_image_files[self.slideshow_current_index];
        if let Some(file) = self.file_browser.files.get(file_index) {
            // Slideshow uses full screen minus status bar (3 rows)
            self.preview_content = Some(self.preview_manager.generate_preview(
                file,
                self.terminal_width,
                self.terminal_height.saturating_sub(3),
                0, // No text scrolling in slideshow mode
                &self.localization,
            ));
            self.is_preview_image = true;
        }
    }

    pub fn update_slideshow(&mut self) {
        // A dialog is a question about the image on screen, so the slideshow holds still
        // until it is answered rather than moving on to a different one.
        if self.dialog_active() {
            return;
        }
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

            // Don't render graphical preview when dialog is showing (graphics layer sits above text)
            let preview_to_render = if self.dialog_active() {
                None
            } else {
                self.preview_content.as_ref()
            };

            UIRenderer::render_preview(
                f,
                preview_area,
                preview_to_render,
                &self.localization,
                self.ascii_logo.as_ref(),
                self.is_text_file,
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

        // Render copy/move destination dialog overlay if needed
        if let Some(ref dialog) = self.transfer_dialog {
            UIRenderer::render_transfer_dialog(f, size, dialog, &self.localization);
        }

        // Render the one-time sidecar prompt overlay if needed
        if self.pending_rating.is_some()
            && let Some(ref sidecar_name) = self.pending_sidecar_name
        {
            UIRenderer::render_sidecar_consent_dialog(f, size, sidecar_name, &self.localization);
        }
    }

    /// Apply privately stored ratings to the current listing.
    ///
    /// Called after every folder read, because the private store covers files whose folder
    /// has no sidecar of its own and the browser cannot know about it.
    fn apply_fallback_ratings(&mut self) {
        let dir = PathBuf::from(&self.file_browser.current_dir);
        let entries = self.app_state.fallback_ratings_in(&dir);
        self.file_browser.apply_fallback_ratings(&entries);
    }

    /// Handle a rating key (0-5), asking about sidecars in this folder if need be.
    fn rate_selected(&mut self, rating: u8) {
        let rating = rating.min(MAX_RATING);

        // A slideshow advances without moving the browser's selection, so point the
        // selection at the image actually on screen before rating it. Rating during a
        // slideshow is the natural way to cull a shoot, and it must hit the right file.
        if self.is_slideshow_mode
            && let Some(index) = self
                .slideshow_image_files
                .get(self.slideshow_current_index)
                .copied()
        {
            self.file_browser.set_selected_index(index);
        }

        let Some(file) = self.file_browser.get_selected_file() else {
            self.append_message("ERROR: No file selected".to_string());
            return;
        };
        if file.is_directory {
            self.append_message(self.localization.get("cannot_rate_directory").to_string());
            return;
        }

        let path = PathBuf::from(&file.path);
        let Some(dir) = path.parent().map(|p| p.to_path_buf()) else {
            return;
        };

        let consent = self.app_state.sidecar_consent(&dir);
        match rating_destination(&self.stars_config, consent) {
            RatingDestination::Sidecar => self.write_sidecar_rating(rating),
            RatingDestination::Fallback => self.store_fallback_rating(rating),
            RatingDestination::Ask => {
                // Creating files in someone's photo folder as a side effect of a keypress
                // deserves an explicit yes, asked once per folder.
                self.pending_rating = Some(rating);
                self.pending_sidecar_name = Some(
                    self.file_browser
                        .sidecar_naming()
                        .path_for(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                self.needs_redraw = true;
            }
        }
    }

    /// Write the rating to an XMP sidecar, falling back to the private store on failure.
    ///
    /// A read-only folder is the common case here, and losing the rating outright would be a
    /// worse answer than keeping it somewhere that at least ptui can read.
    fn write_sidecar_rating(&mut self, rating: u8) {
        let Some(file) = self.file_browser.get_selected_file() else {
            return;
        };
        let path = PathBuf::from(&file.path);
        let naming = self.file_browser.sidecar_naming();

        match ratings::set_rating(&path, rating, naming) {
            Ok(_) => {
                self.file_browser.set_selected_rating(rating);
                // A sidecar supersedes anything previously kept privately for this file.
                self.app_state.set_fallback_rating(&path, 0);
                self.app_state.save();
                self.announce_rating(rating);
                self.needs_redraw = true;
            }
            Err(e) => {
                self.append_message(format!(
                    "{}: {}",
                    self.localization.get("rating_sidecar_failed"),
                    e
                ));
                self.store_fallback_rating(rating);
            }
        }
    }

    /// Keep the rating in ptui's own store, for folders where a sidecar is not an option.
    fn store_fallback_rating(&mut self, rating: u8) {
        let Some(file) = self.file_browser.get_selected_file() else {
            return;
        };
        let path = PathBuf::from(&file.path);

        self.app_state.set_fallback_rating(&path, rating);
        self.app_state.prune_missing_ratings();
        self.app_state.save();
        self.file_browser.set_selected_rating(rating);
        self.announce_rating(rating);
        self.needs_redraw = true;
    }

    fn announce_rating(&mut self, rating: u8) {
        let message = if rating == 0 {
            self.localization.get("rating_cleared").to_string()
        } else {
            let args = fluent::fluent_args!["stars" => rating.to_string()];
            self.localization
                .get_with_args("rating_set", Some(&args))
                .to_string()
        };
        self.append_message(message);
    }

    /// Answer the one-time sidecar prompt for the current folder.
    fn handle_sidecar_consent(&mut self, key: KeyEvent) {
        let Some(rating) = self.pending_rating else {
            return;
        };
        let dir = PathBuf::from(&self.file_browser.current_dir);

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.app_state
                    .set_sidecar_consent(&dir, SidecarConsent::Allow);
                self.app_state.save();
                self.dismiss_sidecar_prompt();
                self.write_sidecar_rating(rating);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Declining is remembered too, so the prompt does not reappear on the next
                // keypress in a folder the user has already said no to.
                self.app_state
                    .set_sidecar_consent(&dir, SidecarConsent::Deny);
                self.app_state.save();
                self.dismiss_sidecar_prompt();
                self.store_fallback_rating(rating);
            }
            KeyCode::Esc => {
                // Escape cancels the rating outright rather than answering for the folder.
                self.dismiss_sidecar_prompt();
            }
            _ => {}
        }
    }

    fn dismiss_sidecar_prompt(&mut self) {
        self.pending_rating = None;
        self.pending_sidecar_name = None;
        self.needs_redraw = true;
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
            // Check if current preview is graphical (either Graphical or Kitty)
            let is_current_graphical = matches!(
                &self.preview_content,
                Some(PreviewContent::Graphical(_)) | Some(PreviewContent::Kitty(_))
            );

            // Clear graphics if not graphical content, or if delete dialog is showing
            // (dialog needs to appear above the graphics layer)
            if !is_current_graphical || self.dialog_active() {
                use std::io::Write;
                // Send Kitty protocol command to delete all images
                let delete_all_cmd = "\x1b_Ga=d,d=a\x1b\\";
                let _ = std::io::stdout().write_all(delete_all_cmd.as_bytes());
                let _ = std::io::stdout().flush();
            }
        }
    }

    /// Render Kitty graphics after ratatui's frame is drawn
    /// This must be called AFTER terminal.draw() to avoid being overwritten
    pub fn render_kitty_post_draw(&mut self) {
        #[cfg(not(test))]
        {
            // Don't render graphics when a modal dialog is showing
            if self.dialog_active() {
                return;
            }

            // Check if we have a Kitty preview to render
            if let Some(PreviewContent::Kitty(ref kitty_rc)) = self.preview_content {
                let mut kitty = kitty_rc.borrow_mut();

                // Reset rendered flag to allow re-rendering
                kitty.rendered = false;

                // Calculate position based on mode
                let (render_x, render_y, width, height) = if self.is_slideshow_mode {
                    // Slideshow mode: full screen with status bar at bottom
                    let image_area = ratatui::layout::Rect::new(
                        0,
                        0,
                        self.terminal_width,
                        self.terminal_height.saturating_sub(3), // Reserve 3 rows for status bar
                    );

                    let img_aspect = kitty.img_width as f32 / kitty.img_height as f32;
                    let font_width = kitty.font_size.0 as f32;
                    let font_height = kitty.font_size.1 as f32;
                    let char_aspect = font_height / font_width;

                    let display_width_cells =
                        (image_area.height as f32 * img_aspect * char_aspect) as u16;

                    let (w, h) = if display_width_cells <= image_area.width {
                        (display_width_cells, image_area.height)
                    } else {
                        let display_height =
                            (image_area.width as f32 / img_aspect / char_aspect) as u16;
                        (image_area.width, display_height.min(image_area.height))
                    };

                    let x_offset = (image_area.width.saturating_sub(w)) / 2;
                    let y_offset = (image_area.height.saturating_sub(h)) / 2;
                    (image_area.x + x_offset, image_area.y + y_offset, w, h)
                } else {
                    // Normal mode: use preview area from layout
                    let (_, preview_area, _) = self.ui_layout.calculate_layout(
                        ratatui::layout::Rect::new(0, 0, self.terminal_width, self.terminal_height),
                    );

                    // Account for border
                    let inner_area = ratatui::layout::Rect::new(
                        preview_area.x + 1,
                        preview_area.y + 1,
                        preview_area.width.saturating_sub(2),
                        preview_area.height.saturating_sub(2),
                    );

                    let img_aspect = kitty.img_width as f32 / kitty.img_height as f32;
                    let font_width = kitty.font_size.0 as f32;
                    let font_height = kitty.font_size.1 as f32;
                    let char_aspect = font_height / font_width;

                    let display_width_cells =
                        (inner_area.height as f32 * img_aspect * char_aspect) as u16;

                    let (w, h) = if display_width_cells <= inner_area.width {
                        (display_width_cells, inner_area.height)
                    } else {
                        let display_height =
                            (inner_area.width as f32 / img_aspect / char_aspect) as u16;
                        (inner_area.width, display_height.min(inner_area.height))
                    };

                    let x_offset = (inner_area.width.saturating_sub(w)) / 2;
                    let y_offset = (inner_area.height.saturating_sub(h)) / 2;
                    (inner_area.x + x_offset, inner_area.y + y_offset, w, h)
                };

                // Update display dimensions
                kitty.display_width = width as u32;
                kitty.display_height = height as u32;

                // Render the Kitty image (now AFTER ratatui has flushed)
                if let Err(e) = crate::preview::PreviewManager::print_kitty_image(
                    &mut kitty, render_x, render_y,
                ) {
                    eprintln!("[KITTY] Post-draw render error: {}", e);
                }
            }
        }
    }
}
