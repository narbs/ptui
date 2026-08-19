//! The main loop only draws when the app asks it to, so an action whose result is invisible
//! until the next keypress is indistinguishable from one that did nothing.
//!
//! The tests here change the process working directory, which `ChafaTui::new()` reads, so
//! they take a lock rather than running in parallel and pulling it out from under each other.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ptui::app::ChafaTui;
use ratatui::{Terminal, backend::TestBackend};
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;

/// The working directory is process-wide, so only one test may own it at a time.
static CWD: Mutex<()> = Mutex::new(());

/// ptui needs chafa and identify to render at all, but a contributor without them should
/// get a skipped test rather than a failure.
fn rendering_tools_available() -> bool {
    let ok = |cmd: &str, flag: &str| {
        Command::new(cmd)
            .arg(flag)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    ok("chafa", "--version") && ok("identify", "-version")
}

/// ImageMagick 7 ships `magick`, 6 ships `convert`; either can make the sample.
fn generate_image(path: &std::path::Path) -> bool {
    ["magick", "convert"].iter().any(|cmd| {
        Command::new(cmd)
            .args(["-size", "8x8", "xc:red"])
            .arg(path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            && path.exists()
    })
}

fn press(app: &mut ChafaTui, c: char) -> bool {
    let key = KeyEvent::new_with_kind(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press);
    let _ = app.handle_key_event(key);
    app.needs_redraw()
}

#[test]
fn actions_that_report_something_ask_for_a_redraw() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    if !rendering_tools_available() {
        eprintln!("skipping: chafa or identify is not installed");
        return;
    }

    let temp = TempDir::new().unwrap();
    std::fs::create_dir(temp.path().join("subdir")).unwrap();

    // A real image, because the ascii save runs it through chafa. Generated with the
    // ImageMagick that ptui already requires, rather than committed as a fixture or read
    // from the sample folders, which are gitignored and absent from a fresh clone.
    if !generate_image(&temp.path().join("sample.jpg")) {
        eprintln!("skipping: could not generate a test image with ImageMagick");
        return;
    }

    std::env::set_current_dir(temp.path()).unwrap();

    let mut app = ChafaTui::new().unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

    // A real draw gives the layout its dimensions; without one the ascii save is handed
    // a zero-sized preview and fails before it can report anything.
    term.draw(|f| app.draw(f)).unwrap();
    app.needs_redraw();

    // Directories sort first, so step onto the image itself.
    press(&mut app, 'j');
    term.draw(|f| app.draw(f)).unwrap();
    app.needs_redraw();

    let before = std::fs::read_dir(temp.path()).unwrap().count();
    let redrew = press(&mut app, 'i');
    let after = std::fs::read_dir(temp.path()).unwrap().count();

    assert!(after > before, "the ascii file should have been written");
    assert!(
        redrew,
        "saving an ascii file adds it to the listing, which the user cannot see until the \
         next redraw"
    );

    // Refusing to delete a directory is a message and nothing else, and a message that is
    // not drawn reads as the key having done nothing at all.
    press(&mut app, 'k');
    term.draw(|f| app.draw(f)).unwrap();
    app.needs_redraw();
    assert!(
        press(&mut app, 'x'),
        "the 'cannot delete directories' message needs a redraw to appear"
    );
}

#[test]
fn ctrl_c_quits_rather_than_opening_the_copy_dialog() {
    let _cwd = CWD.lock().unwrap_or_else(|e| e.into_inner());
    if !rendering_tools_available() {
        eprintln!("skipping: chafa or identify is not installed");
        return;
    }

    let temp = TempDir::new().unwrap();
    if !generate_image(&temp.path().join("sample.jpg")) {
        eprintln!("skipping: could not generate a test image with ImageMagick");
        return;
    }
    std::env::set_current_dir(temp.path()).unwrap();

    let ctrl_c = KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        KeyEventKind::Press,
    );

    let mut app = ChafaTui::new().unwrap();
    assert!(
        app.handle_key_event(ctrl_c).is_err(),
        "Ctrl+C should quit; without a modifier guard it matches the copy binding instead"
    );

    // And it gets out from under a dialog too, which is the state a user is most likely to
    // be reaching for it in.
    let mut app = ChafaTui::new().unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    press(&mut app, 'j');
    press(&mut app, 'c'); // open the copy dialog
    term.draw(|f| app.draw(f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        screen.contains("Copy File"),
        "the copy dialog should be open"
    );

    assert!(
        app.handle_key_event(ctrl_c).is_err(),
        "Ctrl+C should quit even with a dialog open"
    );

    // A plain c still copies, so the guard did not swallow the ordinary binding.
    let mut app = ChafaTui::new().unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    press(&mut app, 'j');
    assert!(
        app.handle_key_event(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            KeyEventKind::Press
        ))
        .is_ok(),
        "plain c should still open the copy dialog"
    );
}
