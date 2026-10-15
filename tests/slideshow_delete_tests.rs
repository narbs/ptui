//! Deleting the image on screen during a slideshow deletes that image, not the file selected
//! in the browser, and the slideshow carries on with the next image rather than dropping
//! back to the browser's preview of whatever file followed it.
//!
//! The tests here change the process working directory, which `ChafaTui::new()` reads, so
//! they take a lock rather than running in parallel and pulling it out from under each other.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ptui::app::ChafaTui;
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;

/// The working directory is process-wide, so only one test may own it at a time.
static CWD: Mutex<()> = Mutex::new(());

fn press(app: &mut ChafaTui, c: char) {
    let key = KeyEvent::new_with_kind(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press);
    let _ = app.handle_key_event(key);
}

/// Files that start with the PNG signature, which is all ptui checks to call them images.
fn make_images(dir: &Path, names: &[&str]) {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&[0u8; 64]);
    for name in names {
        std::fs::write(dir.join(name), &bytes).unwrap();
    }
}

/// An app opened on `dir`, drawn once so the browser knows how many rows it shows.
fn open_app(dir: &Path) -> (ChafaTui, Terminal<TestBackend>) {
    std::env::set_current_dir(dir).unwrap();
    let mut app = ChafaTui::new().unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    (app, term)
}

#[test]
fn deleting_in_a_slideshow_moves_on_to_the_next_image() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_images(temp.path(), &["a.png", "b.png", "d.png"]);
    std::fs::write(temp.path().join("c.txt"), "text").unwrap();
    let (mut app, _term) = open_app(temp.path());

    press(&mut app, ' ');
    assert_eq!(app.slideshow_file_name(), Some("a.png"));
    press(&mut app, 'j');
    assert_eq!(app.slideshow_file_name(), Some("b.png"));

    // The browser still has a.png selected, but b.png is the one on screen.
    press(&mut app, 'x');
    press(&mut app, 'y');
    assert!(temp.path().join("a.png").exists());
    assert!(!temp.path().join("b.png").exists());

    // Still in the slideshow, past the text file to the next image.
    assert_eq!(app.slideshow_file_name(), Some("d.png"));

    // Deleting the last image falls back to the one before it.
    press(&mut app, 'x');
    press(&mut app, 'y');
    assert!(!temp.path().join("d.png").exists());
    assert_eq!(app.slideshow_file_name(), Some("a.png"));
}

#[test]
fn deleting_the_last_image_in_a_slideshow_ends_it() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_images(temp.path(), &["a.png"]);
    std::fs::write(temp.path().join("b.txt"), "text").unwrap();
    let (mut app, _term) = open_app(temp.path());

    press(&mut app, ' ');
    assert_eq!(app.slideshow_file_name(), Some("a.png"));

    press(&mut app, 'x');
    press(&mut app, 'y');
    assert!(!temp.path().join("a.png").exists());
    assert_eq!(app.slideshow_file_name(), None);
    assert_eq!(app.selected_file_name(), Some("b.txt"));
}
