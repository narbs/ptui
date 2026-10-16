//! ? during a slideshow shows help full-screen and holds the slideshow; any key but q puts
//! it away, back on the same image, and the slideshow carries on.
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

fn screen(app: &mut ChafaTui, term: &mut Terminal<TestBackend>) -> String {
    term.draw(|f| app.draw(f)).unwrap();
    term.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn help_in_a_slideshow_pauses_it_and_any_key_resumes() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_images(temp.path(), &["a.png", "b.png"]);
    let (mut app, mut term) = open_app(temp.path());

    press(&mut app, ' ');
    assert_eq!(app.slideshow_file_name(), Some("a.png"));

    press(&mut app, '?');
    let shown = screen(&mut app, &mut term);
    assert!(shown.contains("Cycle converters"));
    assert!(!shown.contains("Slideshow Mode"));
    assert_eq!(app.slideshow_file_name(), Some("a.png"));

    // The key that closes help does only that: j does not also move to the next image.
    press(&mut app, 'j');
    let shown = screen(&mut app, &mut term);
    assert!(!shown.contains("Cycle converters"));
    assert!(shown.contains("Slideshow Mode"));
    assert_eq!(app.slideshow_file_name(), Some("a.png"));

    press(&mut app, 'j');
    assert_eq!(app.slideshow_file_name(), Some("b.png"));
}
