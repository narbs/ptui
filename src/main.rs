mod app;
mod config;
mod converter;
mod fast_image_loader;
mod file_browser;
mod localization;
mod preview;
mod transitions;
mod ui;
mod viuer_protocol;

#[cfg(test)]
mod test_utils;

use app::ChafaTui;
use clap::Command;
use config::PTuiConfig;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let _matches = Command::new("ptui")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Picture TUI - Terminal-based image viewer")
        .get_matches();

    // Create app
    let mut app = ChafaTui::new()?;
    
    // Start config file watcher
    let config_watcher_rx = match PTuiConfig::start_config_watcher() {
        Ok(rx) => Some(rx),
        Err(e) => {
            eprintln!("Warning: Failed to start config file watcher: {}", e);
            None
        }
    };
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Rate limiting for resize events
    let mut last_resize_event = Instant::now();
    let min_resize_interval = Duration::from_millis(100);
    
    // Main loop
    loop {
        // Update slideshow timing
        app.update_slideshow();
        
        // Update transitions and check if redraw is needed
        let _need_redraw = app.update_transitions();
        
        // Check for config file changes
        if let Some(ref config_rx) = config_watcher_rx
            && let Ok(config_result) = config_rx.try_recv()
        {
                match config_result {
                    Ok(new_config) => {
                        if let Err(e) = app.handle_config_reload(new_config) {
                            eprintln!("Error reloading config: {}", e);
                        }
                    }
                    Err(error_msg) => {
                        eprintln!("Config watcher error: {}", error_msg);
                    }
                }
            }
        
        // Only render when state has changed
        if app.needs_redraw() {
            // Clear Kitty graphics if switching from graphical to text mode
            app.clear_graphics_if_needed();
        terminal.draw(|f| app.draw(f))?;
        }
        
        // Handle events with timeout for slideshow
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.handle_key_event(key).is_err() {
                        break;
                    }
                }
                Event::Resize(width, height) => {
                    let now = Instant::now();
                    if now.duration_since(last_resize_event) >= min_resize_interval {
                        last_resize_event = now;
                        app.handle_resize(width, height);
                    }
                }
                _ => {}
            }
        }
    }
    
    // Cleanup: Clear screen and delete any lingering Kitty protocol images
    use std::io::Write;
    // Clear screen first
    let _ = execute!(terminal.backend_mut(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All));
    // Delete all Kitty protocol images
    let delete_all_cmd = "\x1b_Ga=d,d=a\x1b\\";
    let _ = std::io::stdout().write_all(delete_all_cmd.as_bytes());
    let _ = std::io::stdout().flush();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}