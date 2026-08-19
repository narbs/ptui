//! Star ratings stored as XMP sidecar files.
//!
//! A sidecar sits next to the image it describes and carries `xmp:Rating` (0-5), the same
//! property darktable, digiKam, RawTherapee, Bridge and Lightroom read. That interoperability
//! is the whole reason for choosing this format over an extended attribute or a private
//! database: a rating written here survives a sync, a backup and a move to another machine,
//! and other photo tools can see it.
//!
//! Two naming conventions exist in the wild, and both are read:
//!
//! ```text
//! photo.jpg -> photo.jpg.xmp   (appended; darktable, digiKam)
//! photo.jpg -> photo.xmp       (replaced; Adobe tools)
//! ```
//!
//! Appended is the safer convention because it cannot collide, while under the replaced
//! convention `photo.jpg` and `photo.png` in one folder both map to `photo.xmp`. ptui writes
//! whichever convention a folder already uses and appended otherwise, so it round-trips with
//! an existing library instead of fragmenting it.
//!
//! Writes are merge-only. A darktable sidecar holds an entire edit history, so rewriting one
//! from scratch would destroy work ptui knows nothing about. Every write parses what is
//! already there, changes only `xmp:Rating` and `xmp:MetadataDate`, and preserves the rest.

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Highest rating XMP defines. `xmp:Rating` also allows -1 ("rejected"), which ptui neither
/// writes nor offers, but reads through untouched if another tool set it.
pub const MAX_RATING: u8 = 5;

const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const SIDECAR_EXT: &str = "xmp";

/// Which of the two sidecar naming conventions to use, and in which case. Case matters:
/// `photo.XMP` and `photo.xmp` are different files on a case-sensitive filesystem, so ptui
/// matches the case a folder already uses rather than creating a second sidecar beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarNaming {
    pub appended: bool,
    pub uppercase: bool,
}

impl Default for SidecarNaming {
    /// Appended and lowercase: the collision-free convention, used when a folder gives no
    /// indication of its own.
    fn default() -> Self {
        Self {
            appended: true,
            uppercase: false,
        }
    }
}

impl SidecarNaming {
    fn extension(&self) -> String {
        if self.uppercase {
            SIDECAR_EXT.to_uppercase()
        } else {
            SIDECAR_EXT.to_string()
        }
    }

    /// The sidecar path this convention would use for `image`.
    pub fn path_for(&self, image: &Path) -> PathBuf {
        let ext = self.extension();
        if self.appended {
            let mut name = image.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".{}", ext));
            image.with_file_name(name)
        } else {
            image.with_extension(ext)
        }
    }
}

/// True when `name` looks like an XMP sidecar, in any case.
pub fn is_sidecar_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(SIDECAR_EXT))
}

/// The image a sidecar belongs to under each convention, as bare file names.
///
/// `photo.jpg.xmp` yields `photo.jpg` (appended) and `photo` (replaced). The second is only
/// a stem, so a caller matches it against real names in the folder.
fn sidecar_partners(sidecar_name: &str) -> (String, String) {
    let path = Path::new(sidecar_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bare = Path::new(&stem)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| stem.clone());
    (stem, bare)
}

/// True when `sidecar_name` describes a file present in `names`.
///
/// Used to hide sidecars from the browser without hiding orphans: a sidecar whose image is
/// gone stays visible so it can be noticed and cleaned up, rather than lingering invisibly
/// the way 26 of the 440 sidecars in a real library had.
pub fn sidecar_has_partner(sidecar_name: &str, names: &HashSet<String>) -> bool {
    let (appended_partner, replaced_stem) = sidecar_partners(sidecar_name);
    if names.contains(&appended_partner) {
        return true;
    }
    // Replaced convention: any file sharing the stem, whatever its extension.
    names.iter().any(|n| {
        !is_sidecar_name(n)
            && Path::new(n)
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == replaced_stem)
    })
}

/// Which convention a folder already uses, inferred from the sidecars in it.
///
/// A folder that already carries `IMG_1.NEF` + `IMG_1.XMP` gets more of the same, so ptui's
/// ratings land in the files the user's other tools are already reading.
pub fn detect_naming(names: &HashSet<String>) -> SidecarNaming {
    let mut appended = 0usize;
    let mut replaced = 0usize;
    let mut uppercase = 0usize;
    let mut lowercase = 0usize;

    for name in names.iter().filter(|n| is_sidecar_name(n)) {
        let (appended_partner, _) = sidecar_partners(name);
        if names.contains(&appended_partner) {
            appended += 1;
        } else {
            replaced += 1;
        }
        if name.ends_with(&SIDECAR_EXT.to_uppercase()) {
            uppercase += 1;
        } else {
            lowercase += 1;
        }
    }

    if appended == 0 && replaced == 0 {
        return SidecarNaming::default();
    }

    SidecarNaming {
        appended: appended >= replaced,
        uppercase: uppercase > lowercase,
    }
}

/// The existing sidecar for `image`, under either convention and either case.
pub fn existing_sidecar(image: &Path) -> Option<PathBuf> {
    for appended in [true, false] {
        for uppercase in [false, true] {
            let candidate = SidecarNaming {
                appended,
                uppercase,
            }
            .path_for(image);
            if candidate != image && candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Read the rating for `image` from its sidecar, if it has one.
pub fn read_rating(image: &Path) -> Option<u8> {
    let sidecar = existing_sidecar(image)?;
    let contents = fs::read_to_string(&sidecar).ok()?;
    parse_rating(&contents)
}

/// Ratings for every file in `dir`, batched.
///
/// One pass over the folder rather than a lookup per rendered frame: parsing is cheap but not
/// free, and a library where every image carries a sidecar would otherwise re-read hundreds of
/// small files on every keypress.
pub fn scan_directory(dir: &Path, names: &HashSet<String>) -> HashMap<String, u8> {
    let mut ratings = HashMap::new();

    for name in names.iter().filter(|n| is_sidecar_name(n)) {
        let sidecar = dir.join(name);
        let Ok(contents) = fs::read_to_string(&sidecar) else {
            continue;
        };
        let Some(rating) = parse_rating(&contents) else {
            continue;
        };

        let (appended_partner, replaced_stem) = sidecar_partners(name);
        if names.contains(&appended_partner) {
            ratings.insert(appended_partner, rating);
            continue;
        }
        for candidate in names.iter().filter(|n| !is_sidecar_name(n)) {
            if Path::new(candidate)
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == replaced_stem)
            {
                ratings.insert(candidate.clone(), rating);
            }
        }
    }

    ratings
}

/// Set the rating for `image`, creating or updating its sidecar.
///
/// A rating of 0 clears the property instead of storing a zero, and deletes the sidecar when
/// nothing but ptui's own bookkeeping would be left in it.
pub fn set_rating(
    image: &Path,
    rating: u8,
    naming: SidecarNaming,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let existing = existing_sidecar(image);
    let target = existing.clone().unwrap_or_else(|| naming.path_for(image));

    let updated = match &existing {
        Some(path) => {
            let contents = fs::read_to_string(path)?;
            let merged = merge_rating(&contents, rating)?;
            match merged {
                // Nothing of anyone else's left in it, so the file itself goes.
                None => {
                    fs::remove_file(path)?;
                    return Ok(None);
                }
                Some(xml) => xml,
            }
        }
        None => {
            if rating == 0 {
                return Ok(None);
            }
            new_sidecar(rating)
        }
    };

    write_atomically(&target, &updated)?;
    Ok(Some(target))
}

/// Delete the sidecar belonging to `image`, if there is one.
///
/// Called when the image is deleted, so ptui does not leave behind the orphans that a tool
/// which writes sidecars but does not manage them accumulates.
pub fn remove_sidecar(image: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(sidecar) = existing_sidecar(image) {
        fs::remove_file(sidecar)?;
    }
    Ok(())
}

/// Carry the sidecar along when an image is copied or moved.
///
/// Without this a rated image loses its rating the moment it is filed away, which is exactly
/// when a user is most likely to be relying on it.
pub fn transfer_sidecar(source: &Path, target: &Path, move_it: bool) -> Result<(), Box<dyn Error>> {
    let Some(sidecar) = existing_sidecar(source) else {
        return Ok(());
    };

    // Reproduce the convention the source used, so the pair stays recognisable.
    let appended = sidecar
        .file_name()
        .map(|n| {
            let n = n.to_string_lossy();
            let (partner, _) = sidecar_partners(&n);
            source
                .file_name()
                .is_some_and(|s| s.to_string_lossy() == partner)
        })
        .unwrap_or(true);
    let uppercase = sidecar
        .extension()
        .is_some_and(|e| e.to_string_lossy().chars().all(|c| c.is_uppercase()));

    let dest = SidecarNaming {
        appended,
        uppercase,
    }
    .path_for(target);

    fs::copy(&sidecar, &dest)?;
    if move_it {
        fs::remove_file(&sidecar)?;
    }
    Ok(())
}

/// Write via a temporary file in the same directory, then rename over the target.
///
/// A half-written sidecar is worse than no sidecar, and rename within a directory is atomic.
fn write_atomically(path: &Path, contents: &str) -> Result<(), Box<dyn Error>> {
    let temp = path.with_extension("ptui-tmp");
    fs::write(&temp, contents)?;
    fs::rename(&temp, path)?;
    Ok(())
}

/// Every namespace prefix bound to the XMP namespace in this document.
///
/// `xmp:` is near-universal but the prefix is arbitrary in XML, so it is resolved rather than
/// assumed. `xmp` itself is included because sidecars in the wild bind it on an ancestor
/// element that a streaming reader may have already passed.
fn xmp_prefixes(xml: &str) -> HashSet<String> {
    let mut prefixes: HashSet<String> = HashSet::new();
    prefixes.insert("xmp".to_string());

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let value = String::from_utf8_lossy(&attr.value).into_owned();
                    if value == XMP_NS
                        && let Some(prefix) = key.strip_prefix("xmlns:")
                    {
                        prefixes.insert(prefix.to_string());
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    prefixes
}

/// True when `name` is `<some xmp prefix>:<local>`.
fn is_xmp_property(name: &str, local: &str, prefixes: &HashSet<String>) -> bool {
    match name.split_once(':') {
        Some((prefix, rest)) => rest == local && prefixes.contains(prefix),
        None => false,
    }
}

fn is_rdf_description(name: &str) -> bool {
    name == "rdf:Description" || name.ends_with(":Description")
}

/// The `xmp:Rating` in a sidecar, whether written as an attribute or a child element.
///
/// Both forms are valid XMP and both occur in real files, so a reader that handles only one
/// silently reports "unrated" for half the libraries it meets.
pub fn parse_rating(xml: &str) -> Option<u8> {
    let prefixes = xmp_prefixes(xml);
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut in_rating_element = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();

                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    if is_xmp_property(&key, "Rating", &prefixes) {
                        let value = String::from_utf8_lossy(&attr.value).into_owned();
                        return clamp_rating(value.trim());
                    }
                }

                if is_xmp_property(&name, "Rating", &prefixes) {
                    in_rating_element = true;
                }
            }
            Ok(Event::Text(t)) if in_rating_element => {
                let value = t.decode().ok()?.into_owned();
                return clamp_rating(value.trim());
            }
            Ok(Event::End(_)) => in_rating_element = false,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

/// XMP allows -1 for "rejected"; ptui shows that as unrated rather than inventing a glyph.
fn clamp_rating(value: &str) -> Option<u8> {
    let parsed: i32 = value.parse().ok()?;
    if parsed <= 0 {
        Some(0)
    } else {
        Some((parsed as u8).min(MAX_RATING))
    }
}

/// Rebuild an element, substituting the two properties ptui owns and copying the rest.
///
/// `wrote_rating` tracks whether the rating has already been placed, so a document with
/// several `rdf:Description` blocks gains exactly one.
fn rebuild_element(
    source: &BytesStart,
    name: &str,
    rating: u8,
    prefixes: &HashSet<String>,
    has_element_form: bool,
    wrote_rating: &mut bool,
) -> Result<BytesStart<'static>, Box<dyn Error>> {
    let mut elem = BytesStart::new(name.to_string());
    let mut binds_xmp_here = false;

    for attr in source.attributes() {
        let attr = attr?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = String::from_utf8_lossy(&attr.value).into_owned();

        if key == "xmlns:xmp" || (key.starts_with("xmlns:") && value == XMP_NS) {
            binds_xmp_here = true;
        }

        if is_xmp_property(&key, "Rating", prefixes) {
            // Clearing the rating drops the attribute rather than storing a zero, so a
            // sidecar ptui emptied looks the same as one that never had a rating.
            if rating > 0 {
                elem.push_attribute((key.as_str(), rating.to_string().as_str()));
            }
            *wrote_rating = true;
            continue;
        }
        if is_xmp_property(&key, "MetadataDate", prefixes) {
            elem.push_attribute((key.as_str(), xmp_timestamp().as_str()));
            continue;
        }
        elem.push_attribute((key.as_str(), value.as_str()));
    }

    // No rating anywhere in the document yet: attach one to the first rdf:Description.
    if !*wrote_rating && !has_element_form && rating > 0 && is_rdf_description(name) {
        if !binds_xmp_here {
            // A second binding of the same URI on a descendant element is legal, and this
            // guarantees the prefix resolves even if the document bound XMP elsewhere.
            elem.push_attribute(("xmlns:xmp", XMP_NS));
        }
        elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
        elem.push_attribute(("xmp:MetadataDate", xmp_timestamp().as_str()));
        *wrote_rating = true;
    }

    Ok(elem)
}

/// Rewrite `xml` with a new rating, preserving every other property.
///
/// Returns `None` when clearing the rating leaves a sidecar holding nothing but ptui's own
/// bookkeeping, signalling the caller to delete it. A sidecar carrying anyone else's data is
/// always kept, and everything outside the two properties ptui owns is copied through
/// verbatim -- a darktable sidecar holds an entire edit history, and losing it would cost
/// the user work ptui cannot see.
fn merge_rating(xml: &str, rating: u8) -> Result<Option<String>, Box<dyn Error>> {
    let prefixes = xmp_prefixes(xml);
    let has_element_form = has_rating_element(xml, &prefixes);

    if rating == 0 && !holds_foreign_data(xml, &prefixes) {
        return Ok(None);
    }

    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut wrote_rating = false;
    let mut in_rating_element = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();

                if is_xmp_property(&name, "Rating", &prefixes) {
                    // Element form: emit the new value and skip the old text and end tag.
                    in_rating_element = true;
                    wrote_rating = true;
                    if rating > 0 {
                        writer.write_event(Event::Start(BytesStart::new(name.clone())))?;
                        writer.write_event(Event::Text(BytesText::new(&rating.to_string())))?;
                        writer.write_event(Event::End(BytesEnd::new(name)))?;
                    }
                    buf.clear();
                    continue;
                }

                let elem = rebuild_element(
                    &e,
                    &name,
                    rating,
                    &prefixes,
                    has_element_form,
                    &mut wrote_rating,
                )?;
                writer.write_event(Event::Start(elem))?;
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();

                if is_xmp_property(&name, "Rating", &prefixes) {
                    wrote_rating = true;
                    if rating > 0 {
                        writer.write_event(Event::Start(BytesStart::new(name.clone())))?;
                        writer.write_event(Event::Text(BytesText::new(&rating.to_string())))?;
                        writer.write_event(Event::End(BytesEnd::new(name)))?;
                    }
                    buf.clear();
                    continue;
                }

                let elem = rebuild_element(
                    &e,
                    &name,
                    rating,
                    &prefixes,
                    has_element_form,
                    &mut wrote_rating,
                )?;
                // Must stay empty: re-emitting as a start tag would leave it unclosed.
                writer.write_event(Event::Empty(elem))?;
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if in_rating_element && is_xmp_property(&name, "Rating", &prefixes) {
                    in_rating_element = false;
                    buf.clear();
                    continue;
                }
                writer.write_event(Event::End(BytesEnd::new(name)))?;
            }
            Ok(Event::Text(t)) => {
                if in_rating_element {
                    buf.clear();
                    continue;
                }
                // Written back escaped exactly as it arrived; decoding and re-escaping
                // would turn an existing `&amp;` into `&amp;amp;`.
                let text = String::from_utf8_lossy(t.as_ref()).into_owned();
                writer.write_event(Event::Text(BytesText::from_escaped(text)))?;
            }
            Ok(Event::Eof) => break,
            Ok(other) => writer.write_event(other)?,
            Err(e) => return Err(Box::new(e)),
        }
        buf.clear();
    }

    let bytes = writer.into_inner().into_inner();
    Ok(Some(String::from_utf8(bytes)?))
}

fn has_rating_element(xml: &str, prefixes: &HashSet<String>) -> bool {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if is_xmp_property(&name, "Rating", prefixes) {
                    return true;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    false
}

/// True when the sidecar carries anything beyond the two properties ptui manages.
///
/// This is the guard on deletion. darktable stores an entire develop history in a sidecar, so
/// clearing a rating must never be allowed to take that with it.
fn holds_foreign_data(xml: &str, prefixes: &HashSet<String>) -> bool {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();

                // Structural elements of an empty sidecar carry no data of their own.
                let structural = name == "x:xmpmeta"
                    || name == "rdf:RDF"
                    || is_rdf_description(&name)
                    || is_xmp_property(&name, "Rating", prefixes)
                    || is_xmp_property(&name, "MetadataDate", prefixes);
                if !structural {
                    return true;
                }

                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    if key.starts_with("xmlns")
                        || key == "x:xmptk"
                        || key == "rdf:about"
                        || is_xmp_property(&key, "Rating", prefixes)
                        || is_xmp_property(&key, "MetadataDate", prefixes)
                    {
                        continue;
                    }
                    return true;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    false
}

/// A minimal sidecar, shaped like the ones the surrounding ecosystem writes.
fn new_sidecar(rating: u8) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="ptui">"#,
            r#"<rdf:RDF xmlns:rdf="{rdf}">"#,
            r#"<rdf:Description xmlns:xmp="{xmp}" xmp:Rating="{rating}" xmp:MetadataDate="{date}">"#,
            r#"</rdf:Description></rdf:RDF></x:xmpmeta>"#,
        ),
        rdf = RDF_NS,
        xmp = XMP_NS,
        rating = rating,
        date = xmp_timestamp(),
    )
}

/// Current UTC time as XMP writes it, e.g. `2026-08-18T15:30:00+0000`.
///
/// Hand-rolled rather than pulling in a date crate: this is the only place ptui needs a
/// calendar, and a sidecar timestamp does not justify a dependency in a terminal viewer.
fn xmp_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+0000",
        year,
        month,
        day,
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    )
}

/// Days since the Unix epoch to a civil date (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// The shape ptui found in a real library: rating and date as attributes, nothing else.
    const MINIMAL: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 5.4.0">"#,
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
        r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="5""#,
        r#" xmp:MetadataDate="2026-07-24T06:37:03+0000"></rdf:Description>"#,
        r#"</rdf:RDF></x:xmpmeta>"#,
    );

    /// A sidecar with an edit history, standing in for darktable. Losing any of this would
    /// cost a user real work, so it is the case the merge logic exists to protect.
    const WITH_HISTORY: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 4.4.0">"#,
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
        r#"<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/""#,
        r#" xmlns:darktable="http://darktable.sf.net/" xmp:Rating="1""#,
        r#" darktable:import_timestamp="1699999999">"#,
        r#"<darktable:history><rdf:Seq><rdf:li darktable:operation="exposure""#,
        r#" darktable:params="gz09eJxjYGBgYAFiCQYYOOHEgAZY0QVwAAAxgwK/"/></rdf:Seq>"#,
        r#"</darktable:history></rdf:Description></rdf:RDF></x:xmpmeta>"#,
    );

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn parses_attribute_form() {
        assert_eq!(parse_rating(MINIMAL), Some(5));
    }

    #[test]
    fn parses_element_form() {
        let xml = concat!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#,
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/">"#,
            r#"<xmp:Rating>3</xmp:Rating></rdf:Description></rdf:RDF></x:xmpmeta>"#,
        );
        assert_eq!(parse_rating(xml), Some(3));
    }

    #[test]
    fn parses_non_standard_xmp_prefix() {
        let xml = concat!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#,
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
            r#"<rdf:Description xmlns:xap="http://ns.adobe.com/xap/1.0/" xap:Rating="4">"#,
            r#"</rdf:Description></rdf:RDF></x:xmpmeta>"#,
        );
        assert_eq!(parse_rating(xml), Some(4));
    }

    #[test]
    fn treats_rejected_as_unrated() {
        let xml = MINIMAL.replace(r#"xmp:Rating="5""#, r#"xmp:Rating="-1""#);
        assert_eq!(parse_rating(&xml), Some(0));
    }

    #[test]
    fn merge_updates_existing_rating() {
        let merged = merge_rating(MINIMAL, 3).unwrap().unwrap();
        assert_eq!(parse_rating(&merged), Some(3));
        assert!(merged.contains("XMP Core 5.4.0"), "toolkit tag preserved");
    }

    #[test]
    fn merge_preserves_foreign_data() {
        let merged = merge_rating(WITH_HISTORY, 5).unwrap().unwrap();

        assert_eq!(parse_rating(&merged), Some(5));
        assert!(merged.contains("darktable:history"), "history survives");
        assert!(
            merged.contains("gz09eJxjYGBgYAFiCQYYOOHEgAZY0QVwAAAxgwK/"),
            "params survive"
        );
        assert!(merged.contains(r#"darktable:import_timestamp="1699999999""#));
        assert!(merged.contains(r#"rdf:about="""#), "rdf:about survives");
    }

    #[test]
    fn clearing_rating_deletes_a_sidecar_ptui_owns() {
        // Nothing but ptui's own two properties, so the file has no reason to remain.
        assert!(merge_rating(MINIMAL, 0).unwrap().is_none());
    }

    #[test]
    fn clearing_rating_keeps_a_sidecar_holding_other_data() {
        let merged = merge_rating(WITH_HISTORY, 0)
            .unwrap()
            .expect("must not delete a sidecar with an edit history");
        assert_eq!(parse_rating(&merged), None);
        assert!(merged.contains("darktable:history"));
    }

    #[test]
    fn merge_adds_a_rating_where_none_existed() {
        let xml = concat!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">"#,
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#,
            r#"<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/""#,
            r#" dc:creator="Someone"></rdf:Description></rdf:RDF></x:xmpmeta>"#,
        );
        let merged = merge_rating(xml, 4).unwrap().unwrap();
        assert_eq!(parse_rating(&merged), Some(4));
        assert!(merged.contains(r#"dc:creator="Someone""#));
    }

    #[test]
    fn self_closing_elements_stay_self_closing() {
        // Re-emitting an empty element as a start tag would leave it unclosed and the
        // document malformed for every other reader.
        let merged = merge_rating(WITH_HISTORY, 2).unwrap().unwrap();
        assert!(merged.contains(r#"/>"#), "empty elements kept empty");
        // Re-parsing proves the output is still well formed.
        assert_eq!(parse_rating(&merged), Some(2));
    }

    #[test]
    fn merge_is_idempotent_apart_from_the_timestamp() {
        let once = merge_rating(WITH_HISTORY, 3).unwrap().unwrap();
        let twice = merge_rating(&once, 3).unwrap().unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn detects_sidecar_names_in_any_case() {
        assert!(is_sidecar_name("photo.xmp"));
        assert!(is_sidecar_name("photo.XMP"));
        assert!(is_sidecar_name("photo.jpg.Xmp"));
        assert!(!is_sidecar_name("photo.jpg"));
    }

    #[test]
    fn naming_follows_the_folder() {
        let replaced: HashSet<String> = ["a.png", "a.XMP", "b.png", "b.XMP"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let naming = detect_naming(&replaced);
        assert!(!naming.appended, "folder uses the replaced convention");
        assert!(naming.uppercase, "folder uses uppercase");

        let appended: HashSet<String> = ["a.png", "a.png.xmp"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let naming = detect_naming(&appended);
        assert!(naming.appended);
        assert!(!naming.uppercase);
    }

    #[test]
    fn empty_folder_defaults_to_the_collision_free_convention() {
        let naming = detect_naming(&HashSet::new());
        assert!(
            naming.appended,
            "appended cannot collide, so it is the default"
        );
        assert!(!naming.uppercase);
    }

    #[test]
    fn appended_naming_avoids_the_collision_replaced_naming_creates() {
        // photo.jpeg and photo.ascii coexist in a real ptui test folder, and the replaced
        // convention maps both to photo.xmp.
        let appended = SidecarNaming {
            appended: true,
            uppercase: false,
        };
        let replaced = SidecarNaming {
            appended: false,
            uppercase: false,
        };

        let jpeg = Path::new("/tmp/IMG_1588.jpeg");
        let ascii = Path::new("/tmp/IMG_1588.ascii");

        assert_ne!(appended.path_for(jpeg), appended.path_for(ascii));
        assert_eq!(replaced.path_for(jpeg), replaced.path_for(ascii));
    }

    #[test]
    fn finds_a_sidecar_under_either_convention() {
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "not really a png");

        write(temp.path(), "a.png.xmp", MINIMAL);
        assert_eq!(
            existing_sidecar(&image).unwrap().file_name().unwrap(),
            "a.png.xmp"
        );
        fs::remove_file(temp.path().join("a.png.xmp")).unwrap();

        write(temp.path(), "a.XMP", MINIMAL);
        assert_eq!(
            existing_sidecar(&image).unwrap().file_name().unwrap(),
            "a.XMP"
        );
    }

    #[test]
    fn hides_paired_sidecars_but_not_orphans() {
        let names: HashSet<String> = ["a.png", "a.png.xmp", "gone.xmp"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert!(
            sidecar_has_partner("a.png.xmp", &names),
            "paired, so hidden"
        );
        assert!(
            !sidecar_has_partner("gone.xmp", &names),
            "orphan stays visible so it can be cleaned up"
        );
    }

    #[test]
    fn scans_a_folder_under_both_conventions() {
        let temp = TempDir::new().unwrap();
        write(temp.path(), "a.png", "x");
        write(temp.path(), "a.png.xmp", MINIMAL);
        write(temp.path(), "b.png", "x");
        write(
            temp.path(),
            "b.XMP",
            &MINIMAL.replace(r#"Rating="5""#, r#"Rating="2""#),
        );
        write(temp.path(), "c.png", "x");

        let names: HashSet<String> = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        let ratings = scan_directory(temp.path(), &names);
        assert_eq!(ratings.get("a.png"), Some(&5));
        assert_eq!(ratings.get("b.png"), Some(&2));
        assert_eq!(ratings.get("c.png"), None, "unrated files are absent");
    }

    #[test]
    fn set_rating_round_trips_through_the_filesystem() {
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "x");

        set_rating(&image, 4, SidecarNaming::default()).unwrap();
        assert_eq!(read_rating(&image), Some(4));
        assert!(temp.path().join("a.png.xmp").exists());

        set_rating(&image, 2, SidecarNaming::default()).unwrap();
        assert_eq!(read_rating(&image), Some(2));

        set_rating(&image, 0, SidecarNaming::default()).unwrap();
        assert_eq!(read_rating(&image), None);
        assert!(
            !temp.path().join("a.png.xmp").exists(),
            "emptied sidecar removed"
        );
    }

    #[test]
    fn set_rating_zero_on_an_unrated_file_writes_nothing() {
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "x");

        assert!(
            set_rating(&image, 0, SidecarNaming::default())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fs::read_dir(temp.path()).unwrap().count(),
            1,
            "no sidecar created"
        );
    }

    #[test]
    fn set_rating_updates_the_sidecar_that_already_exists() {
        // An existing sidecar is updated in place even when it uses the other convention,
        // so ptui never leaves a folder with two sidecars for one image.
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "x");
        write(temp.path(), "a.XMP", MINIMAL);

        set_rating(&image, 1, SidecarNaming::default()).unwrap();

        assert!(temp.path().join("a.XMP").exists());
        assert!(!temp.path().join("a.png.xmp").exists(), "no second sidecar");
        assert_eq!(read_rating(&image), Some(1));
    }

    #[test]
    fn removing_an_image_removes_its_sidecar() {
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "x");
        write(temp.path(), "a.png.xmp", MINIMAL);

        remove_sidecar(&image).unwrap();
        assert!(!temp.path().join("a.png.xmp").exists());
    }

    #[test]
    fn transfer_carries_the_sidecar() {
        let temp = TempDir::new().unwrap();
        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let image = write(temp.path(), "a.png", "x");
        write(temp.path(), "a.png.xmp", MINIMAL);

        transfer_sidecar(&image, &dest.join("a.png"), false).unwrap();
        assert!(dest.join("a.png.xmp").exists(), "copy leaves the original");
        assert!(temp.path().join("a.png.xmp").exists());

        transfer_sidecar(&image, &dest.join("b.png"), true).unwrap();
        assert!(dest.join("b.png.xmp").exists());
        assert!(
            !temp.path().join("a.png.xmp").exists(),
            "move takes the original"
        );
    }

    #[test]
    fn transfer_without_a_sidecar_is_not_an_error() {
        let temp = TempDir::new().unwrap();
        let image = write(temp.path(), "a.png", "x");
        assert!(transfer_sidecar(&image, &temp.path().join("b.png"), false).is_ok());
    }

    #[test]
    fn timestamp_has_the_xmp_shape() {
        let stamp = xmp_timestamp();
        assert_eq!(stamp.len(), 24, "{}", stamp);
        assert!(stamp.ends_with("+0000"));
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        // A leap day, the case an off-by-one in the algorithm would miss.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }
}
