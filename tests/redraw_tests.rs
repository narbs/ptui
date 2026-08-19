//! The main loop only draws when the app asks it to, so an action whose result is invisible
//! until the next keypress is indistinguishable from one that did nothing.
//!
//! This file holds a single test because it changes the process working directory, which
//! `ChafaTui::new()` reads; a second test running in parallel would see it move underneath.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ptui::app::ChafaTui;
use ratatui::{Terminal, backend::TestBackend};
use std::process::Command;
use tempfile::TempDir;

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
