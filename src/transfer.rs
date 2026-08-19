use crate::ratings;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Candidate directory names to probe for a "Projects" bookmark.
/// The first one that exists under the home directory is used.
const PROJECT_DIR_CANDIDATES: [&str; 4] = ["Projects", "projects", "dev", "code"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Copy,
    Move,
}

impl TransferMode {
    pub fn title_key(&self) -> &'static str {
        match self {
            TransferMode::Copy => "copy_dialog_title",
            TransferMode::Move => "move_dialog_title",
        }
    }

    pub fn prompt_key(&self) -> &'static str {
        match self {
            TransferMode::Copy => "copy_file_prompt",
            TransferMode::Move => "move_file_prompt",
        }
    }

    pub fn success_key(&self) -> &'static str {
        match self {
            TransferMode::Copy => "transfer_copied",
            TransferMode::Move => "transfer_moved",
        }
    }
}

/// A destination shortcut shown in the dialog. `label_key` is a localization key.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub label_key: &'static str,
    pub path: PathBuf,
}

/// Reasons a chosen destination cannot be used. Each maps to a localization key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    NotFound,
    NotADirectory,
    SameDirectory,
}

impl TransferError {
    pub fn message_key(&self) -> &'static str {
        match self {
            TransferError::NotFound => "transfer_error_not_found",
            TransferError::NotADirectory => "transfer_error_not_a_directory",
            TransferError::SameDirectory => "transfer_error_same_directory",
        }
    }
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message_key())
    }
}

impl Error for TransferError {}

/// Which step of the dialog the user is on.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// Numbered list of bookmarks; the final number is "enter a path".
    ChooseDestination,
    /// Free-text path entry.
    EnterPath { input: String },
    /// The destination already holds a file with the same name. A clear destination
    /// needs no confirmation - choosing it transfers straight away.
    ConfirmOverwrite { dest: PathBuf },
}

#[derive(Debug, Clone)]
pub struct TransferDialog {
    pub mode: TransferMode,
    pub source: PathBuf,
    pub file_name: String,
    /// Directory the file browser is currently showing; relative paths resolve against it.
    pub current_dir: PathBuf,
    pub bookmarks: Vec<Bookmark>,
    pub stage: Stage,
    /// Inline error shown in the dialog; the dialog stays open so the user can retry.
    pub error: Option<&'static str>,
}

impl TransferDialog {
    pub fn new(
        mode: TransferMode,
        source: PathBuf,
        file_name: String,
        current_dir: PathBuf,
        last_used: Option<&Path>,
    ) -> Self {
        Self {
            mode,
            source,
            file_name,
            current_dir,
            bookmarks: build_bookmarks(last_used),
            stage: Stage::ChooseDestination,
            error: None,
        }
    }

    /// The number the user presses to switch to free-text path entry.
    /// Always one past the last bookmark, so it is the final entry in the list.
    pub fn custom_path_number(&self) -> usize {
        self.bookmarks.len() + 1
    }

    /// Resolve a 1-based selection from the numbered list.
    /// Returns `None` when the number is out of range.
    pub fn select_number(&self, number: usize) -> Option<Selection> {
        if number == 0 {
            return None;
        }
        if number == self.custom_path_number() {
            return Some(Selection::CustomPath);
        }
        self.bookmarks
            .get(number - 1)
            .map(|bookmark| Selection::Bookmark(bookmark.path.clone()))
    }

    /// Full path the file would end up at for a given destination directory.
    pub fn target_path(&self, dest_dir: &Path) -> PathBuf {
        dest_dir.join(&self.file_name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Bookmark(PathBuf),
    CustomPath,
}

/// What a key press in the dialog resolved to. The dialog updates its own stage;
/// anything that touches the wider app is reported back to the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum TransferAction {
    None,
    Close,
    /// Validate this destination and move to a confirmation step.
    Propose(PathBuf),
    /// Destination is confirmed; perform the copy or move.
    Execute(PathBuf),
}

/// Advance the dialog for a key press and report what the app must do next.
pub fn handle_key(dialog: &mut TransferDialog, key: KeyEvent) -> TransferAction {
    match &dialog.stage {
        Stage::ChooseDestination => match key.code {
            KeyCode::Esc => TransferAction::Close,
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let number = c.to_digit(10).unwrap_or(0) as usize;
                match dialog.select_number(number) {
                    Some(Selection::CustomPath) => {
                        dialog.error = None;
                        dialog.stage = Stage::EnterPath {
                            input: String::new(),
                        };
                        TransferAction::None
                    }
                    Some(Selection::Bookmark(path)) => TransferAction::Propose(path),
                    None => TransferAction::None,
                }
            }
            _ => TransferAction::None,
        },
        Stage::EnterPath { input } => match key.code {
            KeyCode::Esc => {
                dialog.error = None;
                dialog.stage = Stage::ChooseDestination;
                TransferAction::None
            }
            KeyCode::Enter => {
                if input.trim().is_empty() {
                    TransferAction::None
                } else {
                    TransferAction::Propose(expand_path(input, &dialog.current_dir))
                }
            }
            KeyCode::Backspace => {
                let mut new_input = input.clone();
                new_input.pop();
                dialog.error = None;
                dialog.stage = Stage::EnterPath { input: new_input };
                TransferAction::None
            }
            // Ctrl-U clears the line, as in a shell prompt.
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                dialog.error = None;
                dialog.stage = Stage::EnterPath {
                    input: String::new(),
                };
                TransferAction::None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut new_input = input.clone();
                new_input.push(c);
                dialog.error = None;
                dialog.stage = Stage::EnterPath { input: new_input };
                TransferAction::None
            }
            _ => TransferAction::None,
        },
        Stage::ConfirmOverwrite { dest } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                TransferAction::Execute(dest.clone())
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => TransferAction::Close,
            _ => TransferAction::None,
        },
    }
}

/// What a validated destination leads to.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Nothing would be lost - transfer immediately.
    Transfer(PathBuf),
    /// A file of the same name is already there - ask before replacing it.
    ConfirmOverwrite(PathBuf),
}

/// Validate a chosen destination, or return the error to display while keeping the
/// current stage. Only an overwrite needs confirming; a clear destination transfers
/// as soon as it is chosen.
pub fn resolve_destination(
    dialog: &TransferDialog,
    dest: PathBuf,
) -> Result<Resolution, TransferError> {
    validate_destination(&dest, &dialog.source)?;

    if dialog.target_path(&dest).exists() {
        Ok(Resolution::ConfirmOverwrite(dest))
    } else {
        Ok(Resolution::Transfer(dest))
    }
}

/// Build the bookmark list: last-used first (when it still exists), then the standard
/// user directories. Entries that do not exist on disk are omitted, so the numbering
/// stays dense and the "enter a path" entry is always the final number.
pub fn build_bookmarks(last_used: Option<&Path>) -> Vec<Bookmark> {
    let mut bookmarks: Vec<Bookmark> = Vec::new();

    if let Some(path) = last_used
        && path.is_dir()
    {
        bookmarks.push(Bookmark {
            label_key: "transfer_bookmark_last_used",
            path: path.to_path_buf(),
        });
    }

    let candidates: [(&'static str, Option<PathBuf>); 5] = [
        ("transfer_bookmark_home", dirs::home_dir()),
        ("transfer_bookmark_desktop", dirs::desktop_dir()),
        ("transfer_bookmark_downloads", dirs::download_dir()),
        ("transfer_bookmark_documents", dirs::document_dir()),
        ("transfer_bookmark_pictures", dirs::picture_dir()),
    ];

    for (label_key, path) in candidates {
        if let Some(path) = path {
            push_unique(&mut bookmarks, label_key, path);
        }
    }

    // "Projects" is not a standard user directory, so probe a few common names.
    if let Some(home) = dirs::home_dir() {
        for candidate in PROJECT_DIR_CANDIDATES {
            let path = home.join(candidate);
            if path.is_dir() {
                push_unique(&mut bookmarks, "transfer_bookmark_projects", path);
                break;
            }
        }
    }

    bookmarks
}

fn push_unique(bookmarks: &mut Vec<Bookmark>, label_key: &'static str, path: PathBuf) {
    if !path.is_dir() {
        return;
    }
    if bookmarks.iter().any(|existing| existing.path == path) {
        return;
    }
    bookmarks.push(Bookmark { label_key, path });
}

/// Expand `~` and resolve relative paths against the directory being browsed.
pub fn expand_path(input: &str, current_dir: &Path) -> PathBuf {
    let trimmed = input.trim();

    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }

    if let Some(rest) = trimmed.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }

    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    }
}

/// Check a destination directory is usable for the given source file.
pub fn validate_destination(dest: &Path, source: &Path) -> Result<(), TransferError> {
    if !dest.exists() {
        return Err(TransferError::NotFound);
    }
    if !dest.is_dir() {
        return Err(TransferError::NotADirectory);
    }

    // Copying or moving a file into its own directory is a no-op at best and an
    // overwrite of the source at worst.
    if let Some(source_dir) = source.parent() {
        let same = match (fs::canonicalize(source_dir), fs::canonicalize(dest)) {
            (Ok(a), Ok(b)) => a == b,
            _ => source_dir == dest,
        };
        if same {
            return Err(TransferError::SameDirectory);
        }
    }

    Ok(())
}

/// Copy or move `source` into `dest_dir`, returning the resulting path.
///
/// Any XMP sidecar travels with the file. Filing an image away is exactly when a user is
/// most likely to be relying on its rating, so leaving the sidecar behind would lose the
/// rating at the worst moment.
pub fn perform(
    mode: TransferMode,
    source: &Path,
    dest_dir: &Path,
) -> Result<PathBuf, std::io::Error> {
    let file_name = source.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no file name")
    })?;
    let target = dest_dir.join(file_name);

    match mode {
        TransferMode::Copy => {
            fs::copy(source, &target)?;
        }
        TransferMode::Move => {
            // rename fails when source and destination are on different filesystems,
            // which is common when moving to an external drive; fall back to copy+remove.
            if fs::rename(source, &target).is_err() {
                fs::copy(source, &target)?;
                fs::remove_file(source)?;
            }
        }
    }

    // The image is already safely transferred, so a sidecar that cannot be written (a
    // read-only destination, say) costs a rating but must not fail the transfer.
    let _ = ratings::transfer_sidecar(source, &target, matches!(mode, TransferMode::Move));

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_build_bookmarks_includes_existing_last_used_first() {
        let temp = TempDir::new().unwrap();
        let bookmarks = build_bookmarks(Some(temp.path()));

        assert_eq!(bookmarks[0].label_key, "transfer_bookmark_last_used");
        assert_eq!(bookmarks[0].path, temp.path());
    }

    #[test]
    fn test_build_bookmarks_skips_missing_last_used() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("does-not-exist");
        let bookmarks = build_bookmarks(Some(&missing));

        assert!(
            bookmarks
                .iter()
                .all(|b| b.label_key != "transfer_bookmark_last_used")
        );
    }

    #[test]
    fn test_build_bookmarks_only_contains_existing_directories() {
        let bookmarks = build_bookmarks(None);
        for bookmark in &bookmarks {
            assert!(
                bookmark.path.is_dir(),
                "bookmark {:?} should exist",
                bookmark.path
            );
        }
    }

    #[test]
    fn test_every_entry_is_reachable_with_one_key_press() {
        let temp = TempDir::new().unwrap();
        let dialog = TransferDialog::new(
            TransferMode::Copy,
            temp.path().join("a.jpg"),
            "a.jpg".to_string(),
            temp.path().to_path_buf(),
            Some(temp.path()),
        );

        // Entries are chosen by a single digit, so the list must stay within 1-9.
        assert!(
            dialog.custom_path_number() <= 9,
            "too many bookmarks to select by number: {}",
            dialog.custom_path_number()
        );
    }

    #[test]
    fn test_build_bookmarks_has_no_duplicate_paths() {
        let temp = TempDir::new().unwrap();
        let bookmarks = build_bookmarks(Some(temp.path()));

        let mut paths: Vec<_> = bookmarks.iter().map(|b| b.path.clone()).collect();
        let total = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), total, "bookmarks should be unique");
    }

    #[test]
    fn test_custom_path_number_is_last_entry() {
        let temp = TempDir::new().unwrap();
        let dialog = TransferDialog::new(
            TransferMode::Copy,
            temp.path().join("a.jpg"),
            "a.jpg".to_string(),
            temp.path().to_path_buf(),
            None,
        );

        assert_eq!(dialog.custom_path_number(), dialog.bookmarks.len() + 1);
        assert_eq!(
            dialog.select_number(dialog.custom_path_number()),
            Some(Selection::CustomPath)
        );
        assert_eq!(dialog.select_number(dialog.custom_path_number() + 1), None);
        assert_eq!(dialog.select_number(0), None);
    }

    #[test]
    fn test_select_number_returns_bookmark_path() {
        let temp = TempDir::new().unwrap();
        let dialog = TransferDialog::new(
            TransferMode::Copy,
            temp.path().join("a.jpg"),
            "a.jpg".to_string(),
            temp.path().to_path_buf(),
            Some(temp.path()),
        );

        assert_eq!(
            dialog.select_number(1),
            Some(Selection::Bookmark(temp.path().to_path_buf()))
        );
    }

    #[test]
    fn test_expand_path_absolute() {
        let current = PathBuf::from("/some/dir");
        assert_eq!(expand_path("/tmp/foo", &current), PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn test_expand_path_relative_resolves_against_current_dir() {
        let current = PathBuf::from("/some/dir");
        assert_eq!(
            expand_path("sub/folder", &current),
            PathBuf::from("/some/dir/sub/folder")
        );
    }

    #[test]
    fn test_expand_path_trims_whitespace() {
        let current = PathBuf::from("/some/dir");
        assert_eq!(
            expand_path("  /tmp/foo  ", &current),
            PathBuf::from("/tmp/foo")
        );
    }

    #[test]
    fn test_expand_path_tilde() {
        let current = PathBuf::from("/some/dir");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_path("~", &current), home);
            assert_eq!(expand_path("~/pics", &current), home.join("pics"));
        }
    }

    #[test]
    fn test_validate_destination_ok() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let source = write_file(source_dir.path(), "a.txt", "hello");

        assert_eq!(validate_destination(dest_dir.path(), &source), Ok(()));
    }

    #[test]
    fn test_validate_destination_missing() {
        let source_dir = TempDir::new().unwrap();
        let source = write_file(source_dir.path(), "a.txt", "hello");
        let missing = source_dir.path().join("nope");

        assert_eq!(
            validate_destination(&missing, &source),
            Err(TransferError::NotFound)
        );
    }

    #[test]
    fn test_validate_destination_not_a_directory() {
        let dir = TempDir::new().unwrap();
        let source = write_file(dir.path(), "a.txt", "hello");
        let other = write_file(dir.path(), "b.txt", "world");

        assert_eq!(
            validate_destination(&other, &source),
            Err(TransferError::NotADirectory)
        );
    }

    #[test]
    fn test_validate_destination_same_directory() {
        let dir = TempDir::new().unwrap();
        let source = write_file(dir.path(), "a.txt", "hello");

        assert_eq!(
            validate_destination(dir.path(), &source),
            Err(TransferError::SameDirectory)
        );
    }

    #[test]
    fn test_perform_copy_keeps_source() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let source = write_file(source_dir.path(), "a.txt", "hello");

        let target = perform(TransferMode::Copy, &source, dest_dir.path()).unwrap();

        assert_eq!(target, dest_dir.path().join("a.txt"));
        assert!(source.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn test_perform_move_removes_source() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let source = write_file(source_dir.path(), "a.txt", "hello");

        let target = perform(TransferMode::Move, &source, dest_dir.path()).unwrap();

        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn test_perform_copy_overwrites_existing_target() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        let source = write_file(source_dir.path(), "a.txt", "new");
        write_file(dest_dir.path(), "a.txt", "old");

        let target = perform(TransferMode::Copy, &source, dest_dir.path()).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn test_target_path_uses_file_name() {
        let temp = TempDir::new().unwrap();
        let dialog = TransferDialog::new(
            TransferMode::Move,
            PathBuf::from("/src/a.jpg"),
            "a.jpg".to_string(),
            temp.path().to_path_buf(),
            None,
        );

        assert_eq!(
            dialog.target_path(Path::new("/dest")),
            PathBuf::from("/dest/a.jpg")
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn dialog_with_stage(dir: &Path, stage: Stage) -> TransferDialog {
        let mut dialog = TransferDialog::new(
            TransferMode::Copy,
            dir.join("a.txt"),
            "a.txt".to_string(),
            dir.to_path_buf(),
            None,
        );
        dialog.stage = stage;
        dialog
    }

    #[test]
    fn test_handle_key_esc_closes_from_list() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(temp.path(), Stage::ChooseDestination);

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Esc)),
            TransferAction::Close
        );
    }

    #[test]
    fn test_handle_key_final_number_switches_to_path_entry() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(temp.path(), Stage::ChooseDestination);
        let number = dialog.custom_path_number();

        // Only single-digit entries are reachable by one key press.
        if number <= 9 {
            let digit = char::from_digit(number as u32, 10).unwrap();
            assert_eq!(
                handle_key(&mut dialog, key(KeyCode::Char(digit))),
                TransferAction::None
            );
            assert!(matches!(dialog.stage, Stage::EnterPath { .. }));
        }
    }

    #[test]
    fn test_handle_key_bookmark_number_proposes_destination() {
        let temp = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let mut dialog = TransferDialog::new(
            TransferMode::Copy,
            temp.path().join("a.txt"),
            "a.txt".to_string(),
            temp.path().to_path_buf(),
            Some(dest.path()),
        );

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Char('1'))),
            TransferAction::Propose(dest.path().to_path_buf())
        );
    }

    #[test]
    fn test_handle_key_out_of_range_number_is_ignored() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(temp.path(), Stage::ChooseDestination);

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Char('0'))),
            TransferAction::None
        );
        assert_eq!(dialog.stage, Stage::ChooseDestination);
    }

    #[test]
    fn test_handle_key_typing_edits_path_input() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(
            temp.path(),
            Stage::EnterPath {
                input: String::new(),
            },
        );

        for c in "/tmp".chars() {
            handle_key(&mut dialog, key(KeyCode::Char(c)));
        }
        assert_eq!(
            dialog.stage,
            Stage::EnterPath {
                input: "/tmp".to_string()
            }
        );

        handle_key(&mut dialog, key(KeyCode::Backspace));
        assert_eq!(
            dialog.stage,
            Stage::EnterPath {
                input: "/tm".to_string()
            }
        );

        handle_key(
            &mut dialog,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        assert_eq!(
            dialog.stage,
            Stage::EnterPath {
                input: String::new()
            }
        );
    }

    #[test]
    fn test_handle_key_enter_on_empty_input_does_nothing() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(
            temp.path(),
            Stage::EnterPath {
                input: "   ".to_string(),
            },
        );

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Enter)),
            TransferAction::None
        );
    }

    #[test]
    fn test_handle_key_enter_proposes_expanded_path() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(
            temp.path(),
            Stage::EnterPath {
                input: "sub".to_string(),
            },
        );

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Enter)),
            TransferAction::Propose(temp.path().join("sub"))
        );
    }

    #[test]
    fn test_handle_key_esc_from_path_entry_returns_to_list() {
        let temp = TempDir::new().unwrap();
        let mut dialog = dialog_with_stage(
            temp.path(),
            Stage::EnterPath {
                input: "/tmp".to_string(),
            },
        );

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Esc)),
            TransferAction::None
        );
        assert_eq!(dialog.stage, Stage::ChooseDestination);
    }

    #[test]
    fn test_handle_key_overwrite_yes_executes() {
        let temp = TempDir::new().unwrap();
        let dest = PathBuf::from("/tmp/dest");
        let mut dialog =
            dialog_with_stage(temp.path(), Stage::ConfirmOverwrite { dest: dest.clone() });

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Char('Y'))),
            TransferAction::Execute(dest)
        );
    }

    #[test]
    fn test_handle_key_overwrite_enter_also_confirms() {
        let temp = TempDir::new().unwrap();
        let dest = PathBuf::from("/tmp/dest");
        let mut dialog =
            dialog_with_stage(temp.path(), Stage::ConfirmOverwrite { dest: dest.clone() });

        assert_eq!(
            handle_key(&mut dialog, key(KeyCode::Enter)),
            TransferAction::Execute(dest)
        );
    }

    #[test]
    fn test_handle_key_overwrite_no_or_esc_closes() {
        let temp = TempDir::new().unwrap();
        let dest = PathBuf::from("/tmp/dest");

        for code in [KeyCode::Char('n'), KeyCode::Char('N'), KeyCode::Esc] {
            let mut dialog =
                dialog_with_stage(temp.path(), Stage::ConfirmOverwrite { dest: dest.clone() });
            assert_eq!(handle_key(&mut dialog, key(code)), TransferAction::Close);
        }
    }

    #[test]
    fn test_clear_destination_transfers_without_confirmation() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        write_file(source_dir.path(), "a.txt", "hello");
        let dialog = dialog_with_stage(source_dir.path(), Stage::ChooseDestination);

        assert_eq!(
            resolve_destination(&dialog, dest_dir.path().to_path_buf()),
            Ok(Resolution::Transfer(dest_dir.path().to_path_buf()))
        );
    }

    #[test]
    fn test_existing_target_asks_before_overwriting() {
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();
        write_file(source_dir.path(), "a.txt", "hello");
        write_file(dest_dir.path(), "a.txt", "old");
        let dialog = dialog_with_stage(source_dir.path(), Stage::ChooseDestination);

        assert_eq!(
            resolve_destination(&dialog, dest_dir.path().to_path_buf()),
            Ok(Resolution::ConfirmOverwrite(dest_dir.path().to_path_buf()))
        );
    }

    #[test]
    fn test_resolve_destination_reports_validation_error() {
        let source_dir = TempDir::new().unwrap();
        write_file(source_dir.path(), "a.txt", "hello");
        let dialog = dialog_with_stage(source_dir.path(), Stage::ChooseDestination);

        assert_eq!(
            resolve_destination(&dialog, source_dir.path().join("missing")),
            Err(TransferError::NotFound)
        );
        assert_eq!(
            resolve_destination(&dialog, source_dir.path().to_path_buf()),
            Err(TransferError::SameDirectory)
        );
    }

    #[test]
    fn test_mode_localization_keys() {
        assert_eq!(TransferMode::Copy.title_key(), "copy_dialog_title");
        assert_eq!(TransferMode::Move.title_key(), "move_dialog_title");
        assert_eq!(TransferMode::Copy.prompt_key(), "copy_file_prompt");
        assert_eq!(TransferMode::Move.prompt_key(), "move_file_prompt");
        assert_eq!(TransferMode::Copy.success_key(), "transfer_copied");
        assert_eq!(TransferMode::Move.success_key(), "transfer_moved");
    }
}
