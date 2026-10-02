//! r re-reads the folder as well as the preview, so files added or removed by other programs
//! show up without leaving and re-entering it, and the selection stays on the same file.
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

/// Text files rather than images, so the tests need neither chafa nor ImageMagick.
fn make_files(dir: &Path, names: &[&str]) {
    for name in names {
        std::fs::write(dir.join(name), "text").unwrap();
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
fn refresh_keeps_the_selection_on_the_same_file_when_others_appear() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_files(temp.path(), &["b.txt", "d.txt", "f.txt"]);
    let (mut app, _term) = open_app(temp.path());

    press(&mut app, 'j');
    assert_eq!(app.selected_file_name(), Some("d.txt"));

    // Files sorting ahead of the selection shift its index, so a position-based refresh
    // would land on the wrong file.
    make_files(temp.path(), &["a.txt", "c.txt"]);
    press(&mut app, 'r');
    assert_eq!(app.selected_file_name(), Some("d.txt"));

    // And the new files are in the listing.
    press(&mut app, 'k');
    assert_eq!(app.selected_file_name(), Some("c.txt"));
}

#[test]
fn refresh_moves_to_the_next_file_when_the_selected_one_was_deleted() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_files(temp.path(), &["a.txt", "b.txt", "c.txt", "d.txt"]);
    let (mut app, _term) = open_app(temp.path());

    press(&mut app, 'j');
    assert_eq!(app.selected_file_name(), Some("b.txt"));

    std::fs::remove_file(temp.path().join("b.txt")).unwrap();
    press(&mut app, 'r');
    assert_eq!(app.selected_file_name(), Some("c.txt"));
}

#[test]
fn refresh_moves_to_the_previous_file_when_the_last_one_was_deleted() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_files(temp.path(), &["a.txt", "b.txt", "c.txt"]);
    let (mut app, _term) = open_app(temp.path());

    press(&mut app, 'j');
    press(&mut app, 'j');
    assert_eq!(app.selected_file_name(), Some("c.txt"));

    std::fs::remove_file(temp.path().join("c.txt")).unwrap();
    press(&mut app, 'r');
    assert_eq!(app.selected_file_name(), Some("b.txt"));
}

#[test]
fn refresh_copes_with_the_folder_being_emptied() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let temp = TempDir::new().unwrap();
    make_files(temp.path(), &["a.txt", "b.txt"]);
    let (mut app, mut term) = open_app(temp.path());

    std::fs::remove_file(temp.path().join("a.txt")).unwrap();
    std::fs::remove_file(temp.path().join("b.txt")).unwrap();
    press(&mut app, 'r');
    assert_eq!(app.selected_file_name(), None);
    term.draw(|f| app.draw(f)).unwrap();
}
