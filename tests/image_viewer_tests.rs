//! Enter on an image opens it full-screen, like a slideshow that only moves when the user
//! steps through it. Enter on a directory still opens the directory.
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

fn press(app: &mut ChafaTui, code: KeyCode) {
    let key = KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press);
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
fn enter_on_an_image_opens_it_full_screen_and_steps_by_hand() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_images(temp.path(), &["a.png", "b.png", "d.png"]);
    std::fs::write(temp.path().join("c.txt"), "text").unwrap();
    let (mut app, mut term) = open_app(temp.path());

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.slideshow_file_name(), Some("a.png"));

    // The status bar names the viewer, not the slideshow.
    term.draw(|f| app.draw(f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(screen.contains("Image Viewer"));

    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.slideshow_file_name(), Some("b.png"));
    press(&mut app, KeyCode::Right);
    assert_eq!(app.slideshow_file_name(), Some("d.png"));
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.slideshow_file_name(), Some("b.png"));

    // Enter again goes back to the list, on the image last shown.
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.slideshow_file_name(), None);
    assert_eq!(app.selected_file_name(), Some("b.png"));
}

#[test]
fn enter_on_a_directory_still_opens_it() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    make_images(&temp.path().join("sub"), &["inner.png"]);
    let (mut app, _term) = open_app(temp.path());

    assert_eq!(app.selected_file_name(), Some("sub"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.slideshow_file_name(), None);
    assert!(app.selected_file_name().is_some_and(|n| n != "sub"));
}
