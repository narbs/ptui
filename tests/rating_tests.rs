//! End-to-end coverage for star ratings across the modules that carry them: the sidecar
//! writer, the file listing that reads them, and the transfer path that must not lose them.

use ptui::file_browser::FileBrowser;
use ptui::ratings::{self, SidecarNaming};
use ptui::state::{PTuiState, RatingDestination, SidecarConsent, rating_destination};
use ptui::transfer::{self, TransferMode};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn image(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, "stand-in for image bytes").unwrap();
    path
}

fn rating_of(browser: &FileBrowser, name: &str) -> u8 {
    browser
        .files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{} missing from listing", name))
        .rating
}

#[test]
fn rating_a_file_makes_it_visible_to_other_tools_and_to_ptui() {
    let temp = TempDir::new().unwrap();
    let photo = image(temp.path(), "photo.jpg");

    ratings::set_rating(&photo, 4, SidecarNaming::default()).unwrap();

    // Written where any XMP-aware program would look for it...
    let sidecar = temp.path().join("photo.jpg.xmp");
    assert!(sidecar.exists());
    assert!(fs::read_to_string(&sidecar).unwrap().contains("xmp:Rating"));

    // ...and read back into the listing without the sidecar itself showing up.
    let browser = FileBrowser::new_with_dir(temp.path()).unwrap();
    assert_eq!(browser.files.len(), 1);
    assert_eq!(rating_of(&browser, "photo.jpg"), 4);
}

#[test]
fn moving_a_rated_file_keeps_its_rating() {
    let temp = TempDir::new().unwrap();
    let dest = temp.path().join("keepers");
    fs::create_dir(&dest).unwrap();
    let photo = image(temp.path(), "photo.jpg");
    ratings::set_rating(&photo, 5, SidecarNaming::default()).unwrap();

    let moved = transfer::perform(TransferMode::Move, &photo, &dest).unwrap();

    assert_eq!(
        ratings::read_rating(&moved),
        Some(5),
        "rating followed the file"
    );
    assert!(
        !temp.path().join("photo.jpg.xmp").exists(),
        "nothing left behind"
    );

    let browser = FileBrowser::new_with_dir(&dest).unwrap();
    assert_eq!(rating_of(&browser, "photo.jpg"), 5);
}

#[test]
fn copying_a_rated_file_rates_the_copy_too() {
    let temp = TempDir::new().unwrap();
    let dest = temp.path().join("elsewhere");
    fs::create_dir(&dest).unwrap();
    let photo = image(temp.path(), "photo.jpg");
    ratings::set_rating(&photo, 3, SidecarNaming::default()).unwrap();

    let copied = transfer::perform(TransferMode::Copy, &photo, &dest).unwrap();

    assert_eq!(ratings::read_rating(&copied), Some(3));
    assert_eq!(
        ratings::read_rating(&photo),
        Some(3),
        "the original keeps its rating"
    );
}

#[test]
fn a_folder_using_the_adobe_convention_keeps_using_it() {
    // Matching the folder rather than imposing a convention is what lets ptui round-trip
    // with a library another program has already been rating.
    let temp = TempDir::new().unwrap();
    image(temp.path(), "a.png");
    image(temp.path(), "b.png");
    fs::write(
        temp.path().join("a.XMP"),
        concat!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#,
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="5">"#,
            r#"</rdf:Description></rdf:RDF></x:xmpmeta>"#,
        ),
    )
    .unwrap();

    let browser = FileBrowser::new_with_dir(temp.path()).unwrap();
    let naming = browser.sidecar_naming();
    assert!(!naming.appended);
    assert!(naming.uppercase);

    ratings::set_rating(&temp.path().join("b.png"), 2, naming).unwrap();
    assert!(
        temp.path().join("b.XMP").exists(),
        "matched the folder's convention"
    );
}

#[test]
fn declining_sidecars_keeps_ratings_working_privately() {
    let temp = TempDir::new().unwrap();
    let photo = image(temp.path(), "photo.jpg");
    let stars = ptui::config::StarsConfig::default();

    let mut state = PTuiState::default();
    assert_eq!(
        rating_destination(&stars, state.sidecar_consent(temp.path())),
        RatingDestination::Ask
    );

    state.set_sidecar_consent(temp.path(), SidecarConsent::Deny);
    assert_eq!(
        rating_destination(&stars, state.sidecar_consent(temp.path())),
        RatingDestination::Fallback
    );

    state.set_fallback_rating(&photo, 3);

    // No file appears in the user's folder...
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);

    // ...but the rating still reaches the listing.
    let mut browser = FileBrowser::new_with_dir(temp.path()).unwrap();
    browser.apply_fallback_ratings(&state.fallback_ratings_in(temp.path()));
    assert_eq!(rating_of(&browser, "photo.jpg"), 3);
}

#[test]
fn ratings_from_another_program_are_read_without_being_rewritten() {
    let temp = TempDir::new().unwrap();
    let photo = image(temp.path(), "photo.raw");
    let original = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 4.4.0">"#,
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
        r#"<rdf:Description rdf:about="" xmlns:darktable="http://darktable.sf.net/""#,
        r#" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2""#,
        r#" darktable:history_end="14"></rdf:Description></rdf:RDF></x:xmpmeta>"#,
    );
    fs::write(temp.path().join("photo.raw.xmp"), original).unwrap();

    let browser = FileBrowser::new_with_dir(temp.path()).unwrap();
    assert_eq!(rating_of(&browser, "photo.raw"), 2);

    // Reading must not touch the file another program owns.
    assert_eq!(
        fs::read_to_string(temp.path().join("photo.raw.xmp")).unwrap(),
        original
    );

    // Writing changes the rating and nothing else.
    ratings::set_rating(&photo, 5, browser.sidecar_naming()).unwrap();
    let after = fs::read_to_string(temp.path().join("photo.raw.xmp")).unwrap();
    assert!(after.contains(r#"darktable:history_end="14""#));
    assert!(after.contains(r#"x:xmptk="XMP Core 4.4.0""#));
    assert_eq!(ratings::read_rating(&photo), Some(5));
}

#[test]
fn the_file_pane_is_widened_to_pay_for_the_rating_column() {
    use ptui::ui::UILayout;
    use ratatui::layout::Rect;

    // Ratings must not be paid for out of the space file names already had. Above the
    // narrow-screen floor the pane is exactly its percentage share plus the indicator.
    for width in [150u16, 160, 200, 240, 300] {
        let mut layout = UILayout::new();
        let area = Rect::new(0, 0, width, 40);
        let (file_area, preview_area, _) = layout.calculate_layout(area);

        let percent_only = (area.width * layout.preview_size) / 100;
        assert_eq!(
            file_area.width,
            percent_only + 3,
            "file pane at width {} should gain exactly the rating column",
            width
        );

        assert_eq!(file_area.width + preview_area.width, width);
        assert!(preview_area.width > 0);
    }
}

#[test]
fn narrow_terminals_get_a_readable_pane_rather_than_a_percentage() {
    use ptui::ui::UILayout;
    use ratatui::layout::Rect;

    // Below roughly 150 columns a flat percentage leaves too little for names once the
    // borders, icon and rating column are paid for, so a column floor takes over.
    for width in [60u16, 80, 100, 120, 140] {
        let mut layout = UILayout::new();
        let (file_area, preview_area, _) = layout.calculate_layout(Rect::new(0, 0, width, 40));

        assert!(
            file_area.width >= 21,
            "pane was only {} columns at width {}",
            file_area.width,
            width
        );
        assert!(
            preview_area.width > 0,
            "preview vanished at width {}",
            width
        );
    }
}

#[test]
fn widening_survives_the_resize_keys() {
    use ptui::ui::UILayout;
    use ratatui::layout::Rect;

    let mut layout = UILayout::new();
    let area = Rect::new(0, 0, 200, 40);
    let (start, _, _) = layout.calculate_layout(area);
    let start_percent = layout.preview_size;

    layout.increase_size(2);
    let widened_percent = layout.preview_size;
    let (wider, _, _) = layout.calculate_layout(area);

    layout.decrease_size(2);
    let (back, _, _) = layout.calculate_layout(area);

    // The offset is constant, so [ and ] move the divider without eroding it.
    assert_eq!(wider.width, (area.width * widened_percent) / 100 + 3);
    assert_eq!(back.width, (area.width * start_percent) / 100 + 3);
    assert_eq!(back.width, start.width, "resizing round-trips");
    assert!(wider.width > start.width);
}
