use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime state that persists between sessions but is not user-facing configuration.
///
/// This deliberately lives outside the config directory: `PTuiConfig::start_config_watcher`
/// watches `~/.config/ptui/` and reloading the config clears the preview cache, so writing
/// state there after every copy/move would cause a visible redraw hiccup.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct PTuiState {
    /// Last directory a file was copied or moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transfer_destination: Option<String>,
}

impl PTuiState {
    /// Load persisted state, falling back to defaults if it is missing or unreadable.
    /// State is a convenience, so failures are never fatal.
    pub fn load() -> Self {
        match Self::state_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Persist state, ignoring failures (a read-only data dir must not break transfers).
    pub fn save(&self) {
        if let Some(path) = Self::state_path() {
            let _ = self.save_to(&path);
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn state_path() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join("ptui").join("state.json"))
    }

    pub fn get_last_transfer_destination(&self) -> Option<PathBuf> {
        self.last_transfer_destination.as_ref().map(PathBuf::from)
    }

    pub fn set_last_transfer_destination(&mut self, path: &Path) {
        self.last_transfer_destination = Some(path.to_string_lossy().into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_state_has_no_destination() {
        let state = PTuiState::default();
        assert_eq!(state.get_last_transfer_destination(), None);
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("state.json");

        let mut state = PTuiState::default();
        state.set_last_transfer_destination(Path::new("/home/user/pics"));
        state.save_to(&path).unwrap();

        let loaded = PTuiState::load_from(&path);
        assert_eq!(
            loaded.get_last_transfer_destination(),
            Some(PathBuf::from("/home/user/pics"))
        );
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let temp = TempDir::new().unwrap();
        let loaded = PTuiState::load_from(&temp.path().join("missing.json"));
        assert_eq!(loaded, PTuiState::default());
    }

    #[test]
    fn test_load_invalid_json_returns_default() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        fs::write(&path, "not json at all").unwrap();

        assert_eq!(PTuiState::load_from(&path), PTuiState::default());
    }

    #[test]
    fn test_state_path_is_outside_config_dir() {
        if let (Some(state_path), Some(config_dir)) = (PTuiState::state_path(), dirs::config_dir())
        {
            assert!(
                !state_path.starts_with(config_dir),
                "state file must not live in the watched config directory"
            );
        }
    }
}
