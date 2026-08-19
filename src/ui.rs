use crate::file_browser::FileBrowser;
use crate::localization::Localization;
use crate::preview::PreviewContent;
use crate::transfer::{Stage, TransferDialog};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use ratatui_image::{Resize, StatefulImage};
use std::path::Path;

/// Share of the terminal width given to the file browser on a roomy screen.
const FILE_BROWSER_WIDTH_PERCENT: u16 = 12;

/// Columns the file pane should not fall below, before the rating column is added.
///
/// A flat percentage scales the wrong way on a narrow terminal: the borders, the file-type
/// icon and the rating column cost a constant number of columns, so a small percentage of a
/// small width leaves almost nothing for the name itself. This floor keeps roughly a dozen
/// characters of file name readable at any width.
const MIN_FILE_BROWSER_COLUMNS: u16 = 18;

/// Ceiling on the file pane's share, so the floor cannot crowd out the preview on a tiny
/// terminal where the minimum would otherwise exceed the whole width.
const MAX_FILE_BROWSER_PERCENT: u16 = 60;

/// Columns the rating indicator occupies in the file list.
///
/// The file pane is widened by exactly this much so that adding ratings did not quietly
/// cost every file name three characters. A fixed offset rather than a larger percentage,
/// because the indicator costs the same three columns whatever the terminal width.
const RATING_COLUMN_WIDTH: u16 = 3;

pub struct UILayout {
    pub preview_size: u16,
    pub min_divider_percent: u16,
    pub preview_width: u16,
    pub preview_height: u16,
}

impl Default for UILayout {
    fn default() -> Self {
        Self::new()
    }
}

impl UILayout {
    pub fn new() -> Self {
        Self {
            preview_size: 0,
            min_divider_percent: 10,
            preview_width: 0,
            preview_height: 0,
        }
    }

    pub fn calculate_layout(&mut self, area: Rect) -> (Rect, Rect, Rect) {
        // The file pane takes a fixed share of the width, raised on narrow terminals to
        // whatever a readable pane needs. Expressed as a minimum percentage rather than a
        // minimum column count so that [ and ] keep working normally from wherever the
        // floor puts the divider, instead of appearing dead until the percentage catches up.
        // Deliberately rounded down: the exact floor is applied to the column count below,
        // and a percentage that rounded up would land a column above it at some widths and
        // on it at others, making the pane jitter by a column as the terminal is resized.
        let floor_percent = (MIN_FILE_BROWSER_COLUMNS * 100)
            .checked_div(area.width)
            .unwrap_or(FILE_BROWSER_WIDTH_PERCENT);
        let file_browser_width = FILE_BROWSER_WIDTH_PERCENT
            .max(floor_percent)
            .min(MAX_FILE_BROWSER_PERCENT);

        self.min_divider_percent = file_browser_width;

        // Initialize preview size on first draw
        if self.preview_size == 0 {
            self.preview_size = file_browser_width;
        }

        // Shrinking the terminal can raise the floor above the current divider.
        self.preview_size = self.preview_size.max(file_browser_width);

        // Main vertical layout with debug pane at bottom
        // Use flexible debug pane height for small screens
        let debug_height = if area.height > 10 { 3 } else { 1 };
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(area.height.saturating_sub(debug_height)), // Main content area
                Constraint::Length(debug_height),                          // Debug pane
            ])
            .split(area);

        // Horizontal layout for file browser and preview. The file pane takes its share as
        // a percentage, plus the fixed width of the rating column, and the preview takes
        // whatever is left so the two always add up to the full width.
        let content_width = main_chunks[0].width;
        let file_browser_cells = (content_width * self.preview_size / 100)
            .max(MIN_FILE_BROWSER_COLUMNS)
            .saturating_add(RATING_COLUMN_WIDTH)
            .clamp(1, content_width.saturating_sub(1).max(1));

        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(file_browser_cells), Constraint::Min(0)])
            .split(main_chunks[0]);

        // Update preview dimensions
        self.preview_width = content_chunks[1].width.saturating_sub(2);
        self.preview_height = content_chunks[1].height.saturating_sub(1);

        (content_chunks[0], content_chunks[1], main_chunks[1])
    }

    pub fn can_increase_size(&self) -> bool {
        self.preview_size < (100 - self.min_divider_percent)
    }

    pub fn can_decrease_size(&self) -> bool {
        self.preview_size > self.min_divider_percent
    }

    pub fn increase_size(&mut self, increment: u16) {
        if self.can_increase_size() {
            self.preview_size = (self.preview_size + increment).min(100 - self.min_divider_percent);
        }
    }

    pub fn decrease_size(&mut self, increment: u16) {
        if self.can_decrease_size() {
            self.preview_size = self
                .preview_size
                .saturating_sub(increment)
                .max(self.min_divider_percent);
        }
    }
}

/// Helper function to create a centered rect
fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min((r.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min((r.height.saturating_sub(height)) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min((r.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min((r.width.saturating_sub(width)) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Width of the copy/move dialog, clamped to the terminal in `render_transfer_dialog`.
const TRANSFER_DIALOG_WIDTH: u16 = 64;
/// Column width reserved for bookmark labels ("Downloads", "Documents", ...).
const TRANSFER_LABEL_WIDTH: usize = 14;

/// Render a path for display, abbreviating the home directory as `~`.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if path == home {
            return "~".to_string();
        }
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.to_string_lossy());
        }
    }
    path.to_string_lossy().into_owned()
}

/// Truncate from the left, keeping the most specific part of a path visible.
pub fn shorten_path_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return chars[chars.len() - max_chars..].iter().collect();
    }
    let tail: String = chars[chars.len() - (max_chars - 3)..].iter().collect();
    format!("...{}", tail)
}

/// The rating prefix for a file list row: a star and a digit when rated, blanks when not.
///
/// Always three columns wide so that rating a file does not shift every name in the folder.
fn rating_column(rating: u8) -> String {
    if rating == 0 {
        " ".repeat(RATING_COLUMN_WIDTH as usize)
    } else {
        format!("\u{2605}{} ", rating)
    }
}

/// Rows a line of text needs once wrapped to `width`, matching how Paragraph breaks on
/// word boundaries and hard-breaks words too long to fit. Widths are display columns, not
/// characters, so CJK text is measured correctly. Used to size dialogs to their content so
/// nothing is clipped in languages whose strings are longer than English.
pub fn wrapped_rows(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }

    let mut rows = 1;
    let mut used = 0;

    for word in text.split_whitespace() {
        let word_width = Span::from(word).width();

        // Fits on the current row after a space, so keep filling it.
        if used > 0 {
            if used + 1 + word_width <= width {
                used += 1 + word_width;
                continue;
            }
            rows += 1;
        }

        // A word wider than the line spills over onto further rows.
        rows += word_width.saturating_sub(1) / width;
        used = word_width - (word_width.saturating_sub(1) / width) * width;
    }

    rows
}

pub struct UIRenderer;

impl UIRenderer {
    pub fn render_file_browser(
        f: &mut Frame,
        area: Rect,
        file_browser: &mut FileBrowser,
        is_selected_highlighted: bool,
    ) {
        // Calculate visible file list dimensions and update browser
        let file_list_height = area.height.saturating_sub(2);
        file_browser.update_max_visible_files(file_list_height as usize);

        let file_list_items: Vec<ListItem> = file_browser
            .get_display_files()
            .map(|(i, file)| {
                let content = if file.is_directory {
                    format!("📁 {}", file.name)
                } else {
                    format!("🖼️ {}", file.name)
                };

                let style = if i == file_browser.selected_index && is_selected_highlighted {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                // A fixed-width rating column, blank when unrated, so file names stay
                // aligned whether or not anything in the folder has been rated.
                let rating = Span::styled(
                    rating_column(file.rating),
                    Style::default().fg(Color::Yellow),
                );

                ListItem::new(Line::from(vec![rating, Span::styled(content, style)])).style(style)
            })
            .collect();

        let file_list = List::new(file_list_items)
            .block(
                Block::default()
                    .title(format!("📁 {}", file_browser.get_current_dir_display()))
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().bg(Color::Blue));

        f.render_widget(file_list, area);
    }

    pub fn render_preview(
        f: &mut Frame,
        area: Rect,
        preview_content: Option<&PreviewContent>,
        localization: &Localization,
        ascii_logo: Option<&Text<'static>>,
        is_text_file: bool,
    ) {
        // Clear the preview area first to prevent artifacts when switching between text files
        use ratatui::widgets::Clear;
        f.render_widget(Clear, area);

        match preview_content {
            Some(PreviewContent::Text(text)) => {
                let preview_block = Block::default()
                    .title(format!("🖼️ {}", localization.get("image_preview")))
                    .borders(Borders::ALL);

                // For text previews, left-align them to avoid centering regular text files
                // This prevents regular text files from being centered while preserving
                // the previous behavior for ASCII art (which may still be centered via other means)
                let alignment = if is_text_file {
                    Alignment::Left
                } else {
                    Alignment::Center
                };
                let preview_paragraph = Paragraph::new(text.clone())
                    .block(preview_block)
                    .wrap(Wrap { trim: false })
                    .alignment(alignment);

                f.render_widget(preview_paragraph, area);
            }
            Some(PreviewContent::Graphical(graphical)) => {
                let preview_block = Block::default()
                    .title(format!("🖼️ {}", localization.get("image_preview")))
                    .borders(Borders::ALL);

                // Render block first
                f.render_widget(preview_block.clone(), area);

                // Calculate inner area (excluding borders)
                let inner_area = Rect {
                    x: area.x + 1,
                    y: area.y + 1,
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };

                // Use the cached protocol - no recreation needed!
                let mut graphical_borrow = graphical.borrow_mut();

                // Calculate centered area
                use crate::preview::TerminalGraphicsSupport;
                #[cfg(all(not(test), feature = "debug-output"))]
                eprintln!("[UI] Protocol type: {:?}", graphical_borrow.protocol_type);

                let centered_area = if graphical_borrow.protocol_type
                    == TerminalGraphicsSupport::Iterm2
                {
                    // iTerm2: Calculate exact cell dimensions based on pixel size and font size
                    // Guard against division by zero with fallback values
                    let font_width = (graphical_borrow.font_size.0 as u32).max(1);
                    let font_height = (graphical_borrow.font_size.1 as u32).max(1);

                    // Calculate how many cells the resized image needs
                    let needed_width_cells = graphical_borrow.img_width.div_ceil(font_width) as u16;
                    let needed_height_cells =
                        graphical_borrow.img_height.div_ceil(font_height) as u16;

                    // Clamp to available area
                    let width = needed_width_cells.min(inner_area.width);
                    let height = needed_height_cells.min(inner_area.height);

                    // Center within the available area
                    let x_offset = (inner_area.width.saturating_sub(width)) / 2;
                    let y_offset = (inner_area.height.saturating_sub(height)) / 2;

                    #[cfg(all(not(test), feature = "debug-output"))]
                    eprintln!(
                        "[UI] iTerm2: Image {}x{}px, Font {}x{}px, Needs {}x{} cells, Centered at +{}+{}",
                        graphical_borrow.img_width,
                        graphical_borrow.img_height,
                        font_width,
                        font_height,
                        width,
                        height,
                        x_offset,
                        y_offset
                    );

                    Rect {
                        x: inner_area.x + x_offset,
                        y: inner_area.y + y_offset,
                        width,
                        height,
                    }
                } else {
                    // Kitty/Ghostty: Fill vertical space, center horizontally
                    // Calculate display size based on image aspect ratio fitting to full height
                    // Guard against division by zero with fallback values
                    let img_height = graphical_borrow.img_height.max(1) as f32;
                    let img_aspect = graphical_borrow.img_width as f32 / img_height;

                    // Use full available height, calculate width from aspect ratio
                    let font_width = (graphical_borrow.font_size.0 as f32).max(1.0);
                    let font_height = (graphical_borrow.font_size.1 as f32).max(1.0);
                    let char_aspect = font_height / font_width;

                    // Calculate width needed to display at full height while preserving aspect ratio
                    // Account for character aspect ratio (cells are taller than wide in pixels)
                    let display_width =
                        (inner_area.height as f32 * img_aspect * char_aspect) as u16;

                    let (width, height) = if display_width <= inner_area.width {
                        // Image fits horizontally at full height
                        (display_width, inner_area.height)
                    } else {
                        // Image is too wide, fit to width instead
                        // Guard against division by zero
                        let safe_aspect = img_aspect.max(0.001);
                        let safe_char_aspect = char_aspect.max(0.001);
                        let display_height =
                            (inner_area.width as f32 / safe_aspect / safe_char_aspect) as u16;
                        (inner_area.width, display_height.min(inner_area.height))
                    };

                    // Center both horizontally and vertically
                    let x_offset = (inner_area.width.saturating_sub(width)) / 2;
                    let y_offset = (inner_area.height.saturating_sub(height)) / 2;

                    #[cfg(all(not(test), feature = "debug-output"))]
                    eprintln!(
                        "[UI] Kitty: Image {}x{}px (aspect {:.2}), Display {}x{} cells, Area {}x{}, offset +{}+{}",
                        graphical_borrow.img_width,
                        graphical_borrow.img_height,
                        img_aspect,
                        width,
                        height,
                        inner_area.width,
                        inner_area.height,
                        x_offset,
                        y_offset
                    );

                    Rect {
                        x: inner_area.x + x_offset,
                        y: inner_area.y + y_offset,
                        width,
                        height,
                    }
                };

                // Use Fit to fill available space
                let image_widget = StatefulImage::new().resize(Resize::Scale(None));
                f.render_stateful_widget(
                    image_widget,
                    centered_area,
                    &mut graphical_borrow.protocol,
                );
            }
            Some(PreviewContent::Kitty(_)) => {
                // Fast Kitty rendering - just draw the border block here
                // The actual image is rendered in render_kitty_post_draw() AFTER ratatui flushes
                let preview_block = Block::default()
                    .title(format!("🖼️ {}", localization.get("image_preview")))
                    .borders(Borders::ALL);
                f.render_widget(preview_block, area);
            }
            None => {
                // Show help text with logo if available
                let help_text = localization.get_help_text();
                let content = match ascii_logo {
                    Some(logo) => {
                        // Start with the logo and localize any placeholders
                        let mut combined = Self::localize_logo_text(logo, localization);

                        // Add spacing between logo and help text
                        combined.lines.push(ratatui::text::Line::from(""));
                        combined.lines.push(ratatui::text::Line::from(""));

                        // Add help text lines
                        let help_text_obj = Text::from(help_text);
                        for line in help_text_obj.lines {
                            combined.lines.push(line);
                        }
                        combined
                    }
                    None => Text::from(help_text),
                };

                let preview_block = Block::default()
                    .title(format!("🖼️ {}", localization.get("image_preview")))
                    .borders(Borders::ALL);

                let preview_paragraph = Paragraph::new(content)
                    .block(preview_block)
                    .wrap(Wrap { trim: false })
                    .alignment(Alignment::Left);

                f.render_widget(preview_paragraph, area);
            }
        }
    }

    fn localize_logo_text(logo: &Text<'static>, localization: &Localization) -> Text<'static> {
        let mut localized_logo = Text::default();

        for line in &logo.lines {
            let mut new_line = ratatui::text::Line::default();

            for span in &line.spans {
                let content = span.content.to_string();
                // Replace placeholders with localized subtitle and version
                let mut localized_content = content;
                if localized_content.contains("{app_subtitle}") {
                    localized_content = localized_content
                        .replace("{app_subtitle}", &localization.get("app_subtitle"));
                }
                if localized_content.contains("{version}") {
                    localized_content =
                        localized_content.replace("{version}", env!("CARGO_PKG_VERSION"));
                }

                new_line.spans.push(ratatui::text::Span {
                    content: localized_content.into(),
                    style: span.style,
                });
            }

            localized_logo.lines.push(new_line);
        }

        localized_logo
    }

    pub fn render_debug_pane(
        f: &mut Frame,
        area: Rect,
        debug_info: &str,
        localization: &Localization,
    ) {
        let debug_block = Block::default()
            .title(format!("🔍 {}", localization.get("messages")))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let debug_text = Paragraph::new(debug_info.to_string())
            .block(debug_block)
            .style(Style::default().fg(Color::Gray));

        f.render_widget(debug_text, area);
    }

    pub fn render_slideshow(
        f: &mut Frame,
        area: Rect,
        preview_content: Option<&PreviewContent>,
        localization: &Localization,
        current_image: usize,
        total_images: usize,
    ) {
        // Create full-screen slideshow layout with status bar at bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Image area
                Constraint::Length(3), // Status bar
            ])
            .split(area);

        // Render the image in full screen
        match preview_content {
            Some(PreviewContent::Text(text)) => {
                let image_paragraph = Paragraph::new(text.clone())
                    .block(Block::default().borders(Borders::NONE))
                    .alignment(Alignment::Center);
                f.render_widget(image_paragraph, chunks[0]);
            }
            Some(PreviewContent::Graphical(graphical)) => {
                // Use the cached protocol - no recreation needed!
                let mut graphical_borrow = graphical.borrow_mut();

                // Calculate centered area
                use crate::preview::TerminalGraphicsSupport;
                let centered_area = if graphical_borrow.protocol_type
                    == TerminalGraphicsSupport::Iterm2
                {
                    // iTerm2: Calculate exact cell dimensions
                    // Guard against division by zero with fallback values
                    let font_width = (graphical_borrow.font_size.0 as u32).max(1);
                    let font_height = (graphical_borrow.font_size.1 as u32).max(1);

                    let needed_width_cells = graphical_borrow.img_width.div_ceil(font_width) as u16;
                    let needed_height_cells =
                        graphical_borrow.img_height.div_ceil(font_height) as u16;

                    let width = needed_width_cells.min(chunks[0].width);
                    let height = needed_height_cells.min(chunks[0].height);

                    let x_offset = (chunks[0].width.saturating_sub(width)) / 2;
                    let y_offset = (chunks[0].height.saturating_sub(height)) / 2;

                    Rect {
                        x: chunks[0].x + x_offset,
                        y: chunks[0].y + y_offset,
                        width,
                        height,
                    }
                } else {
                    // Kitty/Ghostty: Fill vertical space, center horizontally
                    // Guard against division by zero with fallback values
                    let img_height = graphical_borrow.img_height.max(1) as f32;
                    let img_aspect = graphical_borrow.img_width as f32 / img_height;
                    let font_width = (graphical_borrow.font_size.0 as f32).max(1.0);
                    let font_height = (graphical_borrow.font_size.1 as f32).max(1.0);
                    let char_aspect = font_height / font_width;

                    let display_width = (chunks[0].height as f32 * img_aspect * char_aspect) as u16;

                    let (width, height) = if display_width <= chunks[0].width {
                        (display_width, chunks[0].height)
                    } else {
                        // Guard against division by zero
                        let safe_aspect = img_aspect.max(0.001);
                        let safe_char_aspect = char_aspect.max(0.001);
                        let display_height =
                            (chunks[0].width as f32 / safe_aspect / safe_char_aspect) as u16;
                        (chunks[0].width, display_height.min(chunks[0].height))
                    };

                    let x_offset = (chunks[0].width.saturating_sub(width)) / 2;

                    Rect {
                        x: chunks[0].x + x_offset,
                        y: chunks[0].y,
                        width,
                        height,
                    }
                };

                let image_widget = StatefulImage::new().resize(Resize::Scale(None));
                f.render_stateful_widget(
                    image_widget,
                    centered_area,
                    &mut graphical_borrow.protocol,
                );
            }
            Some(PreviewContent::Kitty(_)) => {
                // Fast Kitty rendering - image is rendered in render_kitty_post_draw()
                // after ratatui's frame is flushed, so nothing to do here
            }
            None => {
                let content = Text::from(localization.get("no_file_selected"));
                let image_paragraph = Paragraph::new(content)
                    .block(Block::default().borders(Borders::NONE))
                    .alignment(Alignment::Center);
                f.render_widget(image_paragraph, chunks[0]);
            }
        }

        // Render status bar - clear first to avoid artifacts from Kitty graphics
        f.render_widget(Clear, chunks[1]);

        let status_text = format!(
            "[>] {} | {} {}/{} | {}",
            localization.get("slideshow_mode"),
            localization.get("slideshow_image"),
            current_image,
            total_images,
            localization.get("slideshow_press_any_key")
        );

        let status_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow));

        let status_paragraph = Paragraph::new(status_text)
            .block(status_block)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(status_paragraph, chunks[1]);
    }

    pub fn render_delete_confirmation_dialog(
        f: &mut Frame,
        area: Rect,
        file_name: &str,
        localization: &Localization,
    ) {
        use fluent::fluent_args;
        use ratatui::layout::Alignment;
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        // Calculate centered dialog position
        let dialog_width = 56.min(area.width.saturating_sub(4));
        let text_width = dialog_width.saturating_sub(2) as usize;

        // Create the dialog message with the file name
        let args = fluent_args!["file" => file_name];
        let prompt = localization.get_with_args("delete_file_prompt", Some(&args));
        let instructions = localization.get("delete_confirmation_instructions");

        // Size to the wrapped content: borders, prompt, a blank line, instructions.
        let content_rows =
            wrapped_rows(&prompt, text_width) + 1 + wrapped_rows(&instructions, text_width);
        let dialog_height = (content_rows as u16 + 2).min(area.height.saturating_sub(2));

        let popup_area = centered_rect(dialog_width, dialog_height, area);

        // Clear the area where the dialog will be rendered
        f.render_widget(Clear, popup_area);

        let confirmation_text = format!("{}\n\n{}", prompt, instructions);

        // Create the dialog block
        let title = format!("⚠️  {}", localization.get("delete_confirmation_title"));
        let dialog_block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));

        // Create the dialog content
        let dialog_paragraph = Paragraph::new(confirmation_text)
            .block(dialog_block)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));

        f.render_widget(dialog_paragraph, popup_area);
    }

    /// Ask, once per folder, before ptui creates XMP sidecar files in it.
    ///
    /// Writing files into someone's photo folder as a side effect of a keypress is the kind
    /// of thing that deserves an explicit yes, and the prompt names the exact file so the
    /// answer is informed rather than a guess at what "sidecar" means.
    pub fn render_sidecar_consent_dialog(
        f: &mut Frame,
        area: Rect,
        sidecar_name: &str,
        localization: &Localization,
    ) {
        use fluent::fluent_args;

        let dialog_width = 62.min(area.width.saturating_sub(4));
        let text_width = dialog_width.saturating_sub(2) as usize;

        let args = fluent_args!["file" => sidecar_name];
        let prompt = localization.get_with_args("sidecar_consent_prompt", Some(&args));
        let explanation = localization.get("sidecar_consent_explanation");
        let instructions = localization.get("sidecar_consent_instructions");

        let content_rows = wrapped_rows(&prompt, text_width)
            + 1
            + wrapped_rows(&explanation, text_width)
            + 1
            + wrapped_rows(&instructions, text_width);
        let dialog_height = (content_rows as u16 + 2).min(area.height.saturating_sub(2));

        let popup_area = centered_rect(dialog_width, dialog_height, area);
        f.render_widget(Clear, popup_area);

        let body = format!("{}\n\n{}\n\n{}", prompt, explanation, instructions);

        let dialog_block = Block::default()
            .title(format!(
                "\u{2605}  {}",
                localization.get("sidecar_consent_title")
            ))
            .borders(Borders::ALL)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );

        let dialog_paragraph = Paragraph::new(body)
            .block(dialog_block)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White));

        f.render_widget(dialog_paragraph, popup_area);
    }

    /// Render the copy/move destination dialog.
    pub fn render_transfer_dialog(
        f: &mut Frame,
        area: Rect,
        dialog: &TransferDialog,
        localization: &Localization,
    ) {
        use fluent::fluent_args;

        let dialog_width = TRANSFER_DIALOG_WIDTH.min(area.width.saturating_sub(4));
        // Text width inside the borders, with a column of padding on each side.
        let text_width = dialog_width.saturating_sub(4) as usize;

        let args = fluent_args!["file" => dialog.file_name.as_str()];
        let mut lines: Vec<Line> = Vec::new();

        // Each arm fills in the body and yields the key instructions to show beneath it.
        let instructions_key = match &dialog.stage {
            Stage::ChooseDestination => {
                lines.push(Line::from(
                    localization.get_with_args(dialog.mode.prompt_key(), Some(&args)),
                ));
                lines.push(Line::from(""));

                for (index, bookmark) in dialog.bookmarks.iter().enumerate() {
                    let label = localization.get(bookmark.label_key);
                    let path = display_path(&bookmark.path);
                    let path_width = text_width.saturating_sub(TRANSFER_LABEL_WIDTH + 4);
                    lines.push(Line::from(format!(
                        "{:>2}  {:<width$}{}",
                        index + 1,
                        label,
                        shorten_path_text(&path, path_width),
                        width = TRANSFER_LABEL_WIDTH
                    )));
                }

                lines.push(Line::from(format!(
                    "{:>2}  {}",
                    dialog.custom_path_number(),
                    localization.get("transfer_enter_path")
                )));
                "transfer_choose_instructions"
            }
            Stage::EnterPath { input } => {
                lines.push(Line::from(
                    localization.get_with_args(dialog.mode.prompt_key(), Some(&args)),
                ));
                lines.push(Line::from(""));

                let label = localization.get("transfer_path_label");
                let available = text_width.saturating_sub(label.chars().count() + 2);
                lines.push(Line::from(format!(
                    "{} {}█",
                    label,
                    shorten_path_text(input, available)
                )));
                "transfer_input_instructions"
            }
            Stage::ConfirmOverwrite { dest } => {
                lines.push(Line::from(
                    localization.get_with_args("transfer_overwrite_prompt", Some(&args)),
                ));
                lines.push(Line::from(""));
                lines.push(Line::from(Self::destination_line(
                    dest,
                    text_width,
                    localization,
                )));
                "transfer_confirm_instructions"
            }
        };

        // Errors sit with the content they refer to, above the key instructions.
        if let Some(error_key) = dialog.error {
            lines.push(Line::from(Span::styled(
                localization.get(error_key),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(localization.get(instructions_key)));

        // Two rows of border plus the content, counting lines that wrap.
        let content_rows: usize = lines
            .iter()
            .map(|line| wrapped_rows(&line.to_string(), text_width))
            .sum();
        let dialog_height = (content_rows as u16 + 2).min(area.height.saturating_sub(2));
        let popup_area = centered_rect(dialog_width, dialog_height, area);

        f.render_widget(Clear, popup_area);

        let title = format!("📋 {}", localization.get(dialog.mode.title_key()));
        let dialog_block = Block::default().title(title).borders(Borders::ALL).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let dialog_paragraph = Paragraph::new(Text::from(lines))
            .block(dialog_block)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));

        f.render_widget(dialog_paragraph, popup_area);
    }

    fn destination_line(dest: &Path, text_width: usize, localization: &Localization) -> String {
        let label = localization.get("transfer_destination_label");
        let available = text_width.saturating_sub(label.chars().count() + 1);
        format!(
            "{} {}",
            label,
            shorten_path_text(&display_path(dest), available)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::helpers::*;
    use ratatui::layout::Rect;
    use ratatui::text::Text;

    #[test]
    fn test_ui_layout_creation() {
        let layout = UILayout::new();
        assert_eq!(layout.preview_size, 0);
        assert_eq!(layout.min_divider_percent, 10);
        assert_eq!(layout.preview_width, 0);
        assert_eq!(layout.preview_height, 0);
    }

    #[test]
    fn test_ui_layout_calculate_layout_wide_screen() {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, 150, 50);

        let (file_area, preview_area, debug_area) = layout.calculate_layout(area);

        // Wide enough that the base share already clears the floor.
        assert_eq!(layout.min_divider_percent, FILE_BROWSER_WIDTH_PERCENT);
        assert!(file_area.width > 0);
        assert!(preview_area.width > 0);
        assert!(debug_area.height == 3);
        assert_eq!(file_area.height + debug_area.height, area.height);
    }

    #[test]
    fn test_ui_layout_calculate_layout_narrow_screen() {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, 80, 30);

        let (file_area, preview_area, debug_area) = layout.calculate_layout(area);

        // Narrow enough that the floor raises the share above the base percentage.
        assert!(layout.min_divider_percent > FILE_BROWSER_WIDTH_PERCENT);
        assert!(file_area.width >= MIN_FILE_BROWSER_COLUMNS);
        assert!(file_area.width > 0);
        assert!(preview_area.width > 0);
        assert!(debug_area.height == 3);
    }

    #[test]
    fn test_ui_layout_preview_size_initialization() {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, 100, 40);

        assert_eq!(layout.preview_size, 0);

        layout.calculate_layout(area);

        assert!(layout.preview_size > 0);
        assert_eq!(layout.preview_size, layout.min_divider_percent);
    }

    #[test]
    fn test_ui_layout_can_increase_size() {
        let mut layout = UILayout::new();
        layout.preview_size = 50;
        layout.min_divider_percent = 10;

        assert!(layout.can_increase_size());

        layout.preview_size = 90;
        assert!(!layout.can_increase_size());
    }

    #[test]
    fn test_ui_layout_can_decrease_size() {
        let mut layout = UILayout::new();
        layout.preview_size = 50;
        layout.min_divider_percent = 10;

        assert!(layout.can_decrease_size());

        layout.preview_size = 10;
        assert!(!layout.can_decrease_size());
    }

    #[test]
    fn test_ui_layout_increase_size() {
        let mut layout = UILayout::new();
        layout.preview_size = 30;
        layout.min_divider_percent = 10;

        layout.increase_size(20);
        assert_eq!(layout.preview_size, 50);

        layout.increase_size(50);
        assert_eq!(layout.preview_size, 90);
    }

    #[test]
    fn test_ui_layout_decrease_size() {
        let mut layout = UILayout::new();
        layout.preview_size = 50;
        layout.min_divider_percent = 10;

        layout.decrease_size(20);
        assert_eq!(layout.preview_size, 30);

        layout.decrease_size(50);
        assert_eq!(layout.preview_size, 10);
    }

    #[test]
    fn test_ui_layout_size_bounds() {
        let mut layout = UILayout::new();
        layout.min_divider_percent = 15;
        layout.preview_size = 50;

        layout.increase_size(100);
        assert_eq!(layout.preview_size, 85);

        layout.decrease_size(100);
        assert_eq!(layout.preview_size, 15);
    }

    #[test]
    fn test_ui_layout_preview_dimensions_calculation() {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, 120, 40);

        let (_, preview_area, _) = layout.calculate_layout(area);

        assert_eq!(layout.preview_width, preview_area.width.saturating_sub(2));
        assert_eq!(layout.preview_height, preview_area.height.saturating_sub(1));
    }

    #[rstest::rstest]
    #[case(80, 22)]
    #[case(100, 18)]
    #[case(120, 15)]
    #[case(130, 13)]
    #[case(200, FILE_BROWSER_WIDTH_PERCENT)]
    fn test_ui_layout_screen_width_logic(#[case] width: u16, #[case] expected_percent: u16) {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, width, 40);

        layout.calculate_layout(area);

        assert_eq!(layout.min_divider_percent, expected_percent);
    }

    /// The file pane must never shrink as the terminal grows.
    ///
    /// It used to: a cutoff at 120 columns dropped the share from 15% to 10%, so a
    /// 140-column terminal gave a narrower pane than a 120-column one.
    #[test]
    fn test_file_pane_never_shrinks_as_the_terminal_widens() {
        let mut previous = 0;

        for width in 60u16..=320 {
            let mut layout = UILayout::new();
            let (file_area, preview_area, _) = layout.calculate_layout(Rect::new(0, 0, width, 40));

            assert!(
                file_area.width >= previous,
                "pane shrank from {} to {} at width {}",
                previous,
                file_area.width,
                width
            );
            assert!(
                preview_area.width > 0,
                "preview vanished at width {}",
                width
            );
            previous = file_area.width;
        }
    }

    #[test]
    fn test_narrow_terminals_keep_a_readable_file_pane() {
        for width in [60u16, 80, 100, 120, 140] {
            let mut layout = UILayout::new();
            let (file_area, _, _) = layout.calculate_layout(Rect::new(0, 0, width, 40));

            assert!(
                file_area.width >= MIN_FILE_BROWSER_COLUMNS,
                "pane was only {} columns at width {}",
                file_area.width,
                width
            );
        }
    }

    #[test]
    fn test_ui_renderer_file_browser_empty() {
        let temp_fs = TestFileSystem::new().unwrap();

        let mut file_browser =
            crate::file_browser::FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        let area = Rect::new(0, 0, 50, 20);

        let backend = ratatui::backend::TestBackend::new(50, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_file_browser(f, area, &mut file_browser, true);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_renderer_preview_with_content() {
        use crate::preview::PreviewContent;
        let localization = crate::localization::Localization::new("en").unwrap();
        let text = Text::from("Test preview content");
        let preview = PreviewContent::Text(text);
        let area = Rect::new(0, 0, 50, 20);

        let backend = ratatui::backend::TestBackend::new(50, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_preview(f, area, Some(&preview), &localization, None, false);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_renderer_preview_without_content() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let area = Rect::new(0, 0, 50, 20);

        let backend = ratatui::backend::TestBackend::new(50, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_preview(f, area, None, &localization, None, false);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_renderer_debug_pane() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let debug_info = "Test debug information";
        let area = Rect::new(0, 0, 50, 5);

        let backend = ratatui::backend::TestBackend::new(50, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_debug_pane(f, area, debug_info, &localization);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_renderer_slideshow() {
        use crate::preview::PreviewContent;
        let localization = crate::localization::Localization::new("en").unwrap();
        let text = Text::from("Slideshow content");
        let preview = PreviewContent::Text(text);
        let area = Rect::new(0, 0, 80, 30);

        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_slideshow(f, area, Some(&preview), &localization, 3, 10);
            })
            .unwrap();
    }

    #[test]
    fn test_ui_renderer_localize_logo_text() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let mut logo = Text::default();
        logo.lines
            .push(ratatui::text::Line::from(vec![ratatui::text::Span::from(
                "Test {app_subtitle} v{version} Logo",
            )]));

        let localized = UIRenderer::localize_logo_text(&logo, &localization);

        let content = &localized.lines[0].spans[0].content;
        assert!(content.contains(&localization.get("app_subtitle")));
        assert!(!content.contains("{app_subtitle}"));
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
        assert!(!content.contains("{version}"));
    }

    fn transfer_test_dialog(stage: crate::transfer::Stage) -> TransferDialog {
        use crate::transfer::TransferMode;
        use std::path::PathBuf;

        let mut dialog = TransferDialog::new(
            TransferMode::Copy,
            PathBuf::from("/src/sunset.jpg"),
            "sunset.jpg".to_string(),
            PathBuf::from("/src"),
            None,
        );
        dialog.stage = stage;
        dialog
    }

    fn render_transfer(dialog: &TransferDialog) -> ratatui::buffer::Buffer {
        let localization = crate::localization::Localization::new("en").unwrap();
        let backend = ratatui::backend::TestBackend::new(90, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_transfer_dialog(f, f.area(), dialog, &localization);
            })
            .unwrap();

        terminal.backend().buffer().clone()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_render_transfer_dialog_choose_destination_lists_numbers() {
        use crate::transfer::Stage;
        let dialog = transfer_test_dialog(Stage::ChooseDestination);
        let text = buffer_text(&render_transfer(&dialog));

        assert!(text.contains("sunset.jpg"));
        // The custom-path entry is always the final number in the list.
        assert!(text.contains(&format!(" {}  ", dialog.custom_path_number())));
    }

    #[test]
    fn test_render_transfer_dialog_enter_path_shows_input() {
        use crate::transfer::Stage;
        let dialog = transfer_test_dialog(Stage::EnterPath {
            input: "/tmp/keepers".to_string(),
        });
        let text = buffer_text(&render_transfer(&dialog));

        assert!(text.contains("/tmp/keepers"));
        assert!(text.contains('█'), "input line should show a cursor");
    }

    #[test]
    fn test_render_transfer_dialog_overwrite_shows_prompt() {
        use crate::transfer::Stage;
        use std::path::PathBuf;

        let localization = crate::localization::Localization::new("en").unwrap();
        let dialog = transfer_test_dialog(Stage::ConfirmOverwrite {
            dest: PathBuf::from("/tmp/keepers"),
        });
        let text = buffer_text(&render_transfer(&dialog));

        let overwrite_word = localization
            .get("transfer_overwrite_prompt")
            .split_whitespace()
            .last()
            .unwrap()
            .to_string();
        assert!(text.contains(&overwrite_word));
        assert!(
            text.contains("/tmp/keepers"),
            "the destination stays visible"
        );
    }

    #[test]
    fn test_render_transfer_dialog_shows_error() {
        use crate::transfer::{Stage, TransferError};

        let localization = crate::localization::Localization::new("en").unwrap();
        let mut dialog = transfer_test_dialog(Stage::EnterPath {
            input: "/nope".to_string(),
        });
        dialog.error = Some(TransferError::NotFound.message_key());

        let text = buffer_text(&render_transfer(&dialog));
        assert!(text.contains(&localization.get("transfer_error_not_found")));
    }

    #[test]
    fn test_render_transfer_dialog_fits_small_terminal() {
        use crate::transfer::Stage;

        let localization = crate::localization::Localization::new("en").unwrap();
        let dialog = transfer_test_dialog(Stage::ChooseDestination);
        let backend = ratatui::backend::TestBackend::new(30, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                UIRenderer::render_transfer_dialog(f, f.area(), &dialog, &localization);
            })
            .unwrap();
    }

    #[test]
    fn test_shorten_path_text_keeps_tail() {
        assert_eq!(shorten_path_text("/a/b/c", 10), "/a/b/c");
        assert_eq!(shorten_path_text("/very/long/path/name", 10), "...th/name");
        assert_eq!(shorten_path_text("/very/long/path", 2).chars().count(), 2);
    }

    #[test]
    fn test_display_path_abbreviates_home() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(display_path(&home), "~");
            assert_eq!(display_path(&home.join("pics")), "~/pics");
        }
        assert_eq!(display_path(Path::new("/tmp/x")), "/tmp/x");
    }

    #[test]
    fn test_wrapped_rows_counts_wrapping() {
        assert_eq!(wrapped_rows("short", 20), 1);
        assert_eq!(wrapped_rows("one two three four", 10), 2);
        assert_eq!(wrapped_rows("", 10), 1);
        assert_eq!(wrapped_rows("anything", 0), 1);
        // A single word longer than the line spills onto further rows.
        assert_eq!(wrapped_rows("abcdefghijkl", 4), 3);
        // CJK glyphs take two columns each.
        assert_eq!(wrapped_rows("確認", 4), 1);
        assert_eq!(wrapped_rows("確認", 2), 2);
    }

    #[rstest::rstest]
    #[case("en")]
    #[case("de")]
    #[case("es")]
    #[case("fr")]
    #[case("ja")]
    #[case("zh")]
    fn test_dialogs_are_not_clipped_in_any_locale(#[case] locale: &str) {
        use crate::transfer::Stage;
        use std::path::PathBuf;

        let localization = crate::localization::Localization::new(locale).unwrap();

        // The instructions are the longest string in either dialog; if its final word
        // survives rendering, the dialog was tall and wide enough for the whole message.
        let delete_instructions = localization.get("delete_confirmation_instructions");
        let transfer_instructions = localization.get("transfer_confirm_instructions");

        let backend = ratatui::backend::TestBackend::new(80, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                UIRenderer::render_delete_confirmation_dialog(
                    f,
                    f.area(),
                    "sunset.jpg",
                    &localization,
                );
            })
            .unwrap();
        let text = buffer_text(&terminal.backend().buffer().clone());
        assert!(
            dialog_text(&text).contains(&dialog_text(&delete_instructions)),
            "delete dialog clipped for {}: missing {:?}",
            locale,
            delete_instructions
        );

        let mut dialog = transfer_test_dialog(Stage::ConfirmOverwrite {
            dest: PathBuf::from("/home/user/Downloads"),
        });
        dialog.mode = crate::transfer::TransferMode::Move;

        let backend = ratatui::backend::TestBackend::new(80, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                UIRenderer::render_transfer_dialog(f, f.area(), &dialog, &localization);
            })
            .unwrap();
        let text = buffer_text(&terminal.backend().buffer().clone());
        assert!(
            dialog_text(&text).contains(&dialog_text(&transfer_instructions)),
            "transfer dialog clipped for {}: missing {:?}",
            locale,
            transfer_instructions
        );
    }

    /// Strip whitespace and the box-drawing border, so a message that wrapped across
    /// rows still matches the string it came from.
    fn dialog_text(text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_whitespace() && !"│─┌┐└┘".contains(*c))
            .collect()
    }

    #[test]
    fn test_ui_layout_constraints_consistency() {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, 100, 50);

        let (file_area, preview_area, debug_area) = layout.calculate_layout(area);

        assert_eq!(file_area.y, 0);
        assert_eq!(preview_area.y, 0);
        assert_eq!(debug_area.y, file_area.height);
        assert_eq!(file_area.x + file_area.width, preview_area.x);
        assert_eq!(file_area.width + preview_area.width, area.width);
    }

    #[test]
    fn test_ui_layout_minimum_dimensions() {
        let mut layout = UILayout::new();
        let small_area = Rect::new(0, 0, 10, 15);

        let (file_area, preview_area, debug_area) = layout.calculate_layout(small_area);

        assert!(file_area.width > 0);
        assert!(preview_area.width > 0);
        assert!(debug_area.height > 0);
    }
}
