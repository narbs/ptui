use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime state that persists between sessions but is not user-facing configuration.
///
/// This deliberately lives outside the config directory: `PTuiConfig::start_config_watcher`
/// watches `~/.config/ptui/` and reloading the config clears the preview cache, so writing
/// state there after every copy/move would cause a visible redraw hiccup.
/// Whether ptui may write XMP sidecars into a particular folder.
///
/// Creating files in someone's photo folder as a side effect of a keypress deserves an
/// explicit yes, so the answer is asked once per folder and remembered here.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidecarConsent {
    Allow,
    Deny,
}

/// Where a rating should go, once the global preference and the folder's remembered answer
/// have both been taken into account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingDestination {
    /// Write an XMP sidecar: the shared, syncing record.
    Sidecar,
    /// Keep it in ptui's own store, because a sidecar is unwanted or impossible here.
    Fallback,
    /// Ask about this folder first.
    Ask,
}

/// Decide where a rating belongs.
///
/// A remembered answer for the folder always wins over the global preference, so a user who
/// allowed sidecars in one folder and refused them in another gets what they asked for in
/// each, whatever the default says.
pub fn rating_destination(
    stars: &crate::config::StarsConfig,
    consent: Option<SidecarConsent>,
) -> RatingDestination {
    if stars.never_writes_sidecars() {
        return RatingDestination::Fallback;
    }
    match consent {
        Some(SidecarConsent::Allow) => RatingDestination::Sidecar,
        Some(SidecarConsent::Deny) => RatingDestination::Fallback,
        None if stars.always_writes_sidecars() => RatingDestination::Sidecar,
        None => RatingDestination::Ask,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct PTuiState {
    /// Last directory a file was copied or moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transfer_destination: Option<String>,

    /// Per-folder answers to the sidecar prompt, keyed by absolute path.
    ///
    /// This is remembered state rather than user preference, which is why it lives here and
    /// not in `ptui.json`: a growing list of paths would be noise in a hand-edited config,
    /// and writing to the config directory would wake the config watcher on every new folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sidecar_dirs: BTreeMap<String, SidecarConsent>,

    /// Ratings for files ptui cannot write a sidecar for, keyed by absolute path.
    ///
    /// Used when a folder is read-only, the user declined sidecars there, or the file is not
    /// an image. Ratings kept here are private to ptui and do not sync -- that is the cost of
    /// the fallback, and the reason sidecars are preferred wherever they are possible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ratings: BTreeMap<String, u8>,
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

    /// The remembered answer for `dir`, if it has been asked about before.
    pub fn sidecar_consent(&self, dir: &Path) -> Option<SidecarConsent> {
        self.sidecar_dirs
            .get(&dir.to_string_lossy().into_owned())
            .copied()
    }

    pub fn set_sidecar_consent(&mut self, dir: &Path, consent: SidecarConsent) {
        self.sidecar_dirs
            .insert(dir.to_string_lossy().into_owned(), consent);
    }

    /// The privately stored rating for `path`, used where a sidecar is not an option.
    pub fn fallback_rating(&self, path: &Path) -> Option<u8> {
        self.ratings
            .get(&path.to_string_lossy().into_owned())
            .copied()
    }

    /// Store a private rating, or forget it when the rating is cleared.
    pub fn set_fallback_rating(&mut self, path: &Path, rating: u8) {
        let key = path.to_string_lossy().into_owned();
        if rating == 0 {
            self.ratings.remove(&key);
        } else {
            self.ratings.insert(key, rating);
        }
    }

    /// Carry a privately stored rating to a file's new location.
    ///
    /// The store is keyed by absolute path, so a rating kept here has to follow the file the
    /// way a sidecar does. Without this a move leaves the rating stranded under a path that
    /// no longer exists and the file reads as unrated, and a copy silently loses a rating
    /// that the sidecar backend would have carried across.
    pub fn transfer_rating(&mut self, from: &Path, to: &Path, move_it: bool) {
        let Some(rating) = self.fallback_rating(from) else {
            return;
        };

        self.set_fallback_rating(to, rating);
        if move_it {
            self.set_fallback_rating(from, 0);
        }
    }

    /// Every privately stored rating for files directly inside `dir`.
    pub fn fallback_ratings_in(&self, dir: &Path) -> Vec<(String, u8)> {
        self.ratings
            .iter()
            .filter_map(|(path, rating)| {
                let path = Path::new(path);
                if path.parent() != Some(dir) {
                    return None;
                }
                let name = path.file_name()?.to_string_lossy().into_owned();
                Some((name, *rating))
            })
            .collect()
    }

    /// Drop ratings for files that no longer exist.
    ///
    /// The private store is the one place ptui can tidy after itself; sidecars orphaned on
    /// disk are another tool's business, but these entries are ptui's alone and would
    /// otherwise accumulate for the life of the install.
    pub fn prune_missing_ratings(&mut self) {
        self.ratings.retain(|path, _| Path::new(path).exists());
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
    #[test]
    fn remembers_sidecar_consent_per_folder() {
        let mut state = PTuiState::default();
        assert_eq!(state.sidecar_consent(Path::new("/photos")), None);

        state.set_sidecar_consent(Path::new("/photos"), SidecarConsent::Allow);
        state.set_sidecar_consent(Path::new("/shared"), SidecarConsent::Deny);

        assert_eq!(
            state.sidecar_consent(Path::new("/photos")),
            Some(SidecarConsent::Allow)
        );
        assert_eq!(
            state.sidecar_consent(Path::new("/shared")),
            Some(SidecarConsent::Deny)
        );
        assert_eq!(state.sidecar_consent(Path::new("/elsewhere")), None);
    }

    #[test]
    fn clearing_a_fallback_rating_forgets_it() {
        let mut state = PTuiState::default();
        state.set_fallback_rating(Path::new("/photos/a.jpg"), 4);
        assert_eq!(state.fallback_rating(Path::new("/photos/a.jpg")), Some(4));

        // A zero is the absence of a rating, not a rating of zero worth storing.
        state.set_fallback_rating(Path::new("/photos/a.jpg"), 0);
        assert_eq!(state.fallback_rating(Path::new("/photos/a.jpg")), None);
        assert!(state.ratings.is_empty());
    }

    #[test]
    fn fallback_ratings_are_scoped_to_one_folder() {
        let mut state = PTuiState::default();
        state.set_fallback_rating(Path::new("/photos/a.jpg"), 5);
        state.set_fallback_rating(Path::new("/photos/b.jpg"), 3);
        state.set_fallback_rating(Path::new("/photos/nested/c.jpg"), 1);

        let mut found = state.fallback_ratings_in(Path::new("/photos"));
        found.sort();

        assert_eq!(
            found,
            vec![("a.jpg".to_string(), 5), ("b.jpg".to_string(), 3)],
            "a nested folder's ratings must not leak into its parent's listing"
        );
    }

    #[test]
    fn pruning_drops_ratings_for_missing_files() {
        let temp = TempDir::new().unwrap();
        let present = temp.path().join("here.jpg");
        fs::write(&present, "x").unwrap();

        let mut state = PTuiState::default();
        state.set_fallback_rating(&present, 5);
        state.set_fallback_rating(&temp.path().join("gone.jpg"), 4);

        state.prune_missing_ratings();

        assert_eq!(state.fallback_rating(&present), Some(5));
        assert_eq!(state.fallback_rating(&temp.path().join("gone.jpg")), None);
    }

    #[test]
    fn state_written_before_ratings_existed_still_loads() {
        // Configs and state files predate this feature, so an old file must not be
        // discarded wholesale just because it lacks the new keys.
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        fs::write(&path, r#"{"last_transfer_destination":"/home/user/pics"}"#).unwrap();

        let loaded = PTuiState::load_from(&path);
        assert_eq!(
            loaded.get_last_transfer_destination(),
            Some(PathBuf::from("/home/user/pics"))
        );
        assert!(loaded.ratings.is_empty());
        assert!(loaded.sidecar_dirs.is_empty());
    }
    fn stars(mode: &str) -> crate::config::StarsConfig {
        crate::config::StarsConfig {
            sidecars: mode.to_string(),
        }
    }

    #[test]
    fn unasked_folders_prompt_by_default() {
        assert_eq!(
            rating_destination(&stars("ask"), None),
            RatingDestination::Ask
        );
    }

    #[test]
    fn a_remembered_answer_settles_the_folder() {
        assert_eq!(
            rating_destination(&stars("ask"), Some(SidecarConsent::Allow)),
            RatingDestination::Sidecar
        );
        assert_eq!(
            rating_destination(&stars("ask"), Some(SidecarConsent::Deny)),
            RatingDestination::Fallback
        );
    }

    #[test]
    fn never_overrides_even_an_allowed_folder() {
        // Turning sidecars off globally has to mean off, or the setting is a lie.
        assert_eq!(
            rating_destination(&stars("never"), Some(SidecarConsent::Allow)),
            RatingDestination::Fallback
        );
    }

    #[test]
    fn always_skips_the_prompt_but_still_honours_a_refusal() {
        assert_eq!(
            rating_destination(&stars("always"), None),
            RatingDestination::Sidecar
        );
        assert_eq!(
            rating_destination(&stars("always"), Some(SidecarConsent::Deny)),
            RatingDestination::Fallback,
            "a folder the user said no to stays off limits"
        );
    }

    #[test]
    fn an_unrecognised_mode_falls_back_to_asking() {
        // A typo in the config should not silently start writing files.
        assert_eq!(
            rating_destination(&stars("maybe"), None),
            RatingDestination::Ask
        );
    }
    #[test]
    fn moving_a_file_takes_its_private_rating_with_it() {
        let mut state = PTuiState::default();
        let from = Path::new("/photos/a.jpg");
        let to = Path::new("/keepers/a.jpg");
        state.set_fallback_rating(from, 4);

        state.transfer_rating(from, to, true);

        assert_eq!(
            state.fallback_rating(to),
            Some(4),
            "rating followed the file"
        );
        assert_eq!(
            state.fallback_rating(from),
            None,
            "nothing left stranded under the old path"
        );
    }

    #[test]
    fn copying_a_file_rates_the_copy_and_keeps_the_original() {
        let mut state = PTuiState::default();
        let from = Path::new("/photos/a.jpg");
        let to = Path::new("/elsewhere/a.jpg");
        state.set_fallback_rating(from, 2);

        state.transfer_rating(from, to, false);

        assert_eq!(state.fallback_rating(to), Some(2));
        assert_eq!(state.fallback_rating(from), Some(2));
    }

    #[test]
    fn transferring_an_unrated_file_stores_nothing() {
        let mut state = PTuiState::default();
        state.transfer_rating(Path::new("/photos/a.jpg"), Path::new("/b/a.jpg"), true);

        assert!(
            state.ratings.is_empty(),
            "no phantom entry for an unrated file"
        );
    }
}
