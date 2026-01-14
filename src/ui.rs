use crate::file_browser::FileBrowser;
use crate::localization::Localization;
use crate::preview::PreviewContent;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use ratatui_image::{Resize, StatefulImage};

const WIDE_SCREEN_WIDTH_PERCENT: u16 = 10;
const NARROW_SCREEN_WIDTH_PERCENT: u16 = 15;
const NARROW_SCREEN_CHAR_CUTOFF: u16 = 120;

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
        // Determine file browser width based on screen size
        let file_browser_width = if area.width > NARROW_SCREEN_CHAR_CUTOFF {
            WIDE_SCREEN_WIDTH_PERCENT
        } else {
            NARROW_SCREEN_WIDTH_PERCENT
        };
        
        self.min_divider_percent = file_browser_width;
        
        // Initialize preview size on first draw
        if self.preview_size == 0 {
            self.preview_size = file_browser_width;
        }

        // Main vertical layout with debug pane at bottom
        // Use flexible debug pane height for small screens
        let debug_height = if area.height > 10 { 3 } else { 1 };
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(area.height.saturating_sub(debug_height)),     // Main content area
                Constraint::Length(debug_height),   // Debug pane
            ])
            .split(area);

        // Horizontal layout for file browser and preview
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.preview_size),
                Constraint::Percentage(100 - self.preview_size),
            ])
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
            self.preview_size = self.preview_size.saturating_sub(increment).max(self.min_divider_percent);
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
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                
                ListItem::new(content).style(style)
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
    ) {
        // Clear the preview area first to prevent artifacts when switching between text files
        use ratatui::widgets::Clear;
        f.render_widget(Clear, area);

        match preview_content {
            Some(PreviewContent::Text(text)) => {
                let preview_block = Block::default()
                    .title(format!("🖼️ {}", localization.get("image_preview")))
                    .borders(Borders::ALL);

                let preview_paragraph = Paragraph::new(text.clone())
                    .block(preview_block)
                    .wrap(Wrap { trim: false });

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

                eprintln!("[UI] Rendering graphical preview in area: {}x{} cells (image: {}x{}px)",
                    inner_area.width, inner_area.height,
                    graphical_borrow.img_width, graphical_borrow.img_height);

                // Use Fit with Nearest filter for fast scaling
                use image::imageops::FilterType;
                use std::time::Instant;
                let widget_start = Instant::now();
                let image_widget = StatefulImage::new(None).resize(Resize::Fit(Some(FilterType::Nearest)));
                f.render_stateful_widget(image_widget, inner_area, &mut graphical_borrow.protocol);
                eprintln!("[UI] render_stateful_widget took: {:?}", widget_start.elapsed());
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
                    },
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
                    localized_content = localized_content.replace("{app_subtitle}", &localization.get("app_subtitle"));
                }
                if localized_content.contains("{version}") {
                    localized_content = localized_content.replace("{version}", env!("CARGO_PKG_VERSION"));
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

    pub fn render_debug_pane(f: &mut Frame, area: Rect, debug_info: &str, localization: &Localization) {
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
                Constraint::Min(1),     // Image area
                Constraint::Length(3),  // Status bar
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

                // Use Fit with Nearest filter for fast scaling
                use image::imageops::FilterType;
                let image_widget = StatefulImage::new(None).resize(Resize::Fit(Some(FilterType::Nearest)));
                f.render_stateful_widget(image_widget, chunks[0], &mut graphical_borrow.protocol);
            }
            None => {
                let content = Text::from(localization.get("no_file_selected"));
        let image_paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(image_paragraph, chunks[0]);
            }
        }

        // Render status bar
        let status_text = format!(
            "🎞️ {} | {} {}/{} | {}",
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
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        f.render_widget(status_paragraph, chunks[1]);
    }

    pub fn render_delete_confirmation_dialog(
        f: &mut Frame,
        area: Rect,
        file_name: &str,
        localization: &Localization,
    ) {
        use fluent::fluent_args;
        use ratatui::widgets::{Block, Borders, Paragraph, Clear};
        use ratatui::layout::{Alignment};
        use ratatui::style::{Color, Style, Modifier};

        // Calculate centered dialog position
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let dialog_height = 5.min(area.height.saturating_sub(4));
        
        let popup_area = centered_rect(dialog_width, dialog_height, area);

        // Clear the area where the dialog will be rendered
        f.render_widget(Clear, popup_area);

        // Create the dialog message with the file name
        let args = fluent_args!["file" => file_name];
        let prompt = localization.get_with_args("delete_file_prompt", Some(&args));
        let instructions = localization.get("delete_confirmation_instructions");

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
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));

        f.render_widget(dialog_paragraph, popup_area);
    }

    /// Calculate a horizontally-centered area for an image based on its aspect ratio
    #[allow(dead_code)]
    fn calculate_centered_image_area(area: Rect, img_width: u32, img_height: u32) -> Rect {
        if img_width == 0 || img_height == 0 {
            return area;
        }

        // Character cell aspect ratio approximation
        // Terminal characters are typically ~2:1 (height:width) in pixel dimensions
        // So 1 row of chars = 2 columns worth of pixels
        let char_aspect = 2.0;

        // Calculate image aspect ratio
        let img_aspect = img_width as f32 / img_height as f32;

        // Calculate fitted dimensions in character cells
        let area_aspect = (area.width as f32) / (area.height as f32 * char_aspect);

        let (fitted_width, fitted_height) = if img_aspect > area_aspect {
            // Image is wider - fit to width
            let fitted_width = area.width;
            let fitted_height = (area.width as f32 / img_aspect / char_aspect) as u16;
            (fitted_width, fitted_height.min(area.height))
        } else {
            // Image is taller - fit to height
            let fitted_height = area.height;
            let fitted_width = (area.height as f32 * char_aspect * img_aspect) as u16;
            (fitted_width.min(area.width), fitted_height)
        };

        // Center horizontally by calculating offset
        let x_offset = (area.width.saturating_sub(fitted_width)) / 2;

        Rect {
            x: area.x + x_offset,
            y: area.y,
            width: fitted_width,
            height: fitted_height,
        }
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
        
        assert_eq!(layout.min_divider_percent, WIDE_SCREEN_WIDTH_PERCENT);
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
        
        assert_eq!(layout.min_divider_percent, NARROW_SCREEN_WIDTH_PERCENT);
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
    #[case(80, NARROW_SCREEN_WIDTH_PERCENT)]
    #[case(100, NARROW_SCREEN_WIDTH_PERCENT)]
    #[case(120, NARROW_SCREEN_WIDTH_PERCENT)]
    #[case(130, WIDE_SCREEN_WIDTH_PERCENT)]
    #[case(200, WIDE_SCREEN_WIDTH_PERCENT)]
    fn test_ui_layout_screen_width_logic(#[case] width: u16, #[case] expected_percent: u16) {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, width, 40);
        
        layout.calculate_layout(area);
        
        assert_eq!(layout.min_divider_percent, expected_percent);
    }

    #[test]
    fn test_ui_renderer_file_browser_empty() {
        let temp_fs = TestFileSystem::new().unwrap();
        
        let mut file_browser = crate::file_browser::FileBrowser::new_with_dir(temp_fs.get_path()).unwrap();
        let area = Rect::new(0, 0, 50, 20);
        
        let backend = ratatui::backend::TestBackend::new(50, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        
        terminal.draw(|f| {
            UIRenderer::render_file_browser(f, area, &mut file_browser, true);
        }).unwrap();
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
        
        terminal.draw(|f| {
            UIRenderer::render_preview(f, area, Some(&preview), &localization, None);
        }).unwrap();
    }

    #[test]
    fn test_ui_renderer_preview_without_content() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let area = Rect::new(0, 0, 50, 20);
        
        let backend = ratatui::backend::TestBackend::new(50, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        
        terminal.draw(|f| {
            UIRenderer::render_preview(f, area, None, &localization, None);
        }).unwrap();
    }

    #[test]
    fn test_ui_renderer_debug_pane() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let debug_info = "Test debug information";
        let area = Rect::new(0, 0, 50, 5);
        
        let backend = ratatui::backend::TestBackend::new(50, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        
        terminal.draw(|f| {
            UIRenderer::render_debug_pane(f, area, debug_info, &localization);
        }).unwrap();
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
        
        terminal.draw(|f| {
            UIRenderer::render_slideshow(f, area, Some(&preview), &localization, 3, 10);
        }).unwrap();
    }

    #[test]
    fn test_ui_renderer_localize_logo_text() {
        let localization = crate::localization::Localization::new("en").unwrap();
        let mut logo = Text::default();
        logo.lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::from("Test {app_subtitle} v{version} Logo")
        ]));
        
        let localized = UIRenderer::localize_logo_text(&logo, &localization);
        
        let content = &localized.lines[0].spans[0].content;
        assert!(content.contains(&localization.get("app_subtitle")));
        assert!(!content.contains("{app_subtitle}"));
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
        assert!(!content.contains("{version}"));
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