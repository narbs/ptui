use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ptui::file_browser::FileBrowser;
use ptui::state::PTuiState;
use ptui::transfer::{
    self, Resolution, Stage, TransferAction, TransferDialog, TransferError, TransferMode,
    handle_key,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn dialog_for(mode: TransferMode, source: &Path, last_used: Option<&Path>) -> TransferDialog {
    TransferDialog::new(
        mode,
        source.to_path_buf(),
        source.file_name().unwrap().to_string_lossy().into_owned(),
        source.parent().unwrap().to_path_buf(),
        last_used,
    )
}

/// Mirrors the app's dialog loop: keys drive the dialog, a clear destination transfers
/// immediately, and one that would replace a file waits for the overwrite confirmation.
fn drive(dialog: &mut TransferDialog, keys: &[KeyEvent]) -> Option<PathBuf> {
    for k in keys {
        match handle_key(dialog, *k) {
            TransferAction::None => {}
            TransferAction::Close => return None,
            TransferAction::Propose(dest) => match transfer::resolve_destination(dialog, dest) {
                Ok(Resolution::Transfer(dest)) => {
                    return Some(transfer::perform(dialog.mode, &dialog.source, &dest).unwrap());
                }
                Ok(Resolution::ConfirmOverwrite(dest)) => {
                    dialog.error = None;
                    dialog.stage = Stage::ConfirmOverwrite { dest };
                }
                Err(error) => dialog.error = Some(error.message_key()),
            },
            TransferAction::Execute(dest) => {
                return Some(transfer::perform(dialog.mode, &dialog.source, &dest).unwrap());
            }
        }
    }
    None
}

fn type_path(path: &Path) -> Vec<KeyEvent> {
    path.to_string_lossy()
        .chars()
        .map(|c| key(KeyCode::Char(c)))
        .collect()
}

#[test]
fn test_copy_to_typed_path_end_to_end() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");

    let mut dialog = dialog_for(TransferMode::Copy, &source, None);

    // Choose the final numbered entry (free-text path) and type the destination. Nothing
    // is there to replace, so Enter completes the copy without a further confirmation.
    let mut keys = vec![key(KeyCode::Char(
        char::from_digit(dialog.custom_path_number() as u32, 10).unwrap(),
    ))];
    keys.extend(type_path(dest_dir.path()));
    keys.push(key(KeyCode::Enter));

    let target = drive(&mut dialog, &keys).expect("copy should complete");

    assert_eq!(target, dest_dir.path().join("sunset.jpg"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "image-bytes");
    assert!(source.exists(), "copy must leave the source in place");
}

#[test]
fn test_move_to_bookmark_end_to_end() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");

    // The remembered destination is always the first entry in the list.
    let mut dialog = dialog_for(TransferMode::Move, &source, Some(dest_dir.path()));

    let target = drive(&mut dialog, &[key(KeyCode::Char('1'))]).expect("move should complete");

    assert_eq!(target, dest_dir.path().join("sunset.jpg"));
    assert!(!source.exists(), "move must remove the source");
}

#[test]
fn test_existing_target_requires_overwrite_confirmation() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "new");
    write_file(dest_dir.path(), "sunset.jpg", "old");

    let mut dialog = dialog_for(TransferMode::Copy, &source, Some(dest_dir.path()));

    drive(&mut dialog, &[key(KeyCode::Char('1'))]);
    assert!(
        matches!(dialog.stage, Stage::ConfirmOverwrite { .. }),
        "an existing file must trigger the overwrite step, got {:?}",
        dialog.stage
    );

    // Declining leaves the destination untouched.
    assert_eq!(
        handle_key(&mut dialog, key(KeyCode::Char('n'))),
        TransferAction::Close
    );
    assert_eq!(
        fs::read_to_string(dest_dir.path().join("sunset.jpg")).unwrap(),
        "old"
    );
}

#[test]
fn test_missing_folder_keeps_dialog_open_with_error() {
    let source_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");
    let missing = source_dir.path().join("no-such-folder");

    let mut dialog = dialog_for(TransferMode::Copy, &source, None);

    let mut keys = vec![key(KeyCode::Char(
        char::from_digit(dialog.custom_path_number() as u32, 10).unwrap(),
    ))];
    keys.extend(type_path(&missing));
    keys.push(key(KeyCode::Enter));

    assert!(drive(&mut dialog, &keys).is_none());
    assert_eq!(dialog.error, Some(TransferError::NotFound.message_key()));
    assert!(
        matches!(dialog.stage, Stage::EnterPath { .. }),
        "the typed path must stay editable after an error"
    );
}

#[test]
fn test_transfer_into_same_folder_is_refused() {
    let source_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");

    let mut dialog = dialog_for(TransferMode::Move, &source, Some(source_dir.path()));

    drive(&mut dialog, &[key(KeyCode::Char('1'))]);

    assert_eq!(
        dialog.error,
        Some(TransferError::SameDirectory.message_key())
    );
    assert_eq!(dialog.stage, Stage::ChooseDestination);
    assert!(source.exists());
}

#[test]
fn test_escape_cancels_without_touching_files() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");

    let mut dialog = dialog_for(TransferMode::Move, &source, Some(dest_dir.path()));

    assert!(drive(&mut dialog, &[key(KeyCode::Esc)]).is_none());
    assert!(source.exists());
    assert!(fs::read_dir(dest_dir.path()).unwrap().next().is_none());
}

#[test]
fn test_last_destination_persists_and_leads_the_next_dialog() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let state_dir = TempDir::new().unwrap();
    let state_path = state_dir.path().join("state.json");
    let source = write_file(source_dir.path(), "sunset.jpg", "image-bytes");

    // First transfer records the destination, as the app does after a successful copy.
    let mut state = PTuiState::default();
    state.set_last_transfer_destination(dest_dir.path());
    state.save_to(&state_path).unwrap();

    // A later session loads it and offers it as the first shortcut.
    let reloaded = PTuiState::load_from(&state_path);
    let dialog = dialog_for(
        TransferMode::Copy,
        &source,
        reloaded.get_last_transfer_destination().as_deref(),
    );

    assert_eq!(dialog.bookmarks[0].label_key, "transfer_bookmark_last_used");
    assert_eq!(dialog.bookmarks[0].path, dest_dir.path());
}

/// After a transfer the app captures fallback names, re-reads the directory, and reselects
/// by name. These cover that sequence against a directory that changed underneath it.
#[test]
fn test_selection_follows_the_copied_file_when_the_listing_shifts() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    for name in ["b.jpg", "c.jpg", "d.jpg"] {
        write_file(source_dir.path(), name, "x");
    }

    let mut browser = FileBrowser::new_with_dir(source_dir.path()).unwrap();
    browser.update_max_visible_files(10);
    let selected = browser
        .files
        .iter()
        .position(|f| f.name == "c.jpg")
        .unwrap();
    browser.set_selected_index(selected);

    let fallback = browser.selection_fallback_names();
    let source = source_dir.path().join("c.jpg");
    transfer::perform(TransferMode::Copy, &source, dest_dir.path()).unwrap();

    // Something else adds a file ahead of the selection while the dialog was open.
    write_file(source_dir.path(), "a.jpg", "x");
    browser.refresh_files().unwrap();
    assert!(browser.select_first_available(&fallback));

    assert_eq!(
        browser.get_selected_file().unwrap().name,
        "c.jpg",
        "a copy should leave the selection on the file that was copied"
    );
}

#[test]
fn test_selection_follows_the_next_file_after_a_move_when_the_listing_shifts() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    for name in ["b.jpg", "c.jpg", "d.jpg"] {
        write_file(source_dir.path(), name, "x");
    }

    let mut browser = FileBrowser::new_with_dir(source_dir.path()).unwrap();
    browser.update_max_visible_files(10);
    let selected = browser
        .files
        .iter()
        .position(|f| f.name == "c.jpg")
        .unwrap();
    browser.set_selected_index(selected);

    let fallback = browser.selection_fallback_names();
    let source = source_dir.path().join("c.jpg");
    transfer::perform(TransferMode::Move, &source, dest_dir.path()).unwrap();

    write_file(source_dir.path(), "a.jpg", "x");
    browser.refresh_files().unwrap();
    assert!(browser.select_first_available(&fallback));

    assert_eq!(
        browser.get_selected_file().unwrap().name,
        "d.jpg",
        "a move should leave the selection on the file that followed it"
    );
}
