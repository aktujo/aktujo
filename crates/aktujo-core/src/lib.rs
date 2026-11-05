// SPDX-License-Identifier: AGPL-3.0-or-later
//! The domain of aktujo: its types and rules, without I/O – what a title, a label and the name of a space may be,
//! and the events of the program that are not about documents (`aktujo-doc` holds those).

use serde::{Deserialize, Serialize};

/// The longest title, in characters.
pub const TITLE_MAX: usize = 200;
/// The longest label, in characters.
pub const LABEL_MAX: usize = 100;
/// The longest name of a space, in characters.
pub const SPACE_NAME_MAX: usize = 100;

/// Why something is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// A title with nothing in it.
    #[error("the title is empty")]
    EmptyTitle,
    /// A title longer than [`TITLE_MAX`].
    #[error("the title is longer than {TITLE_MAX} characters")]
    LongTitle,
    /// A label with nothing in it.
    #[error("the label is empty")]
    EmptyLabel,
    /// A label longer than [`LABEL_MAX`].
    #[error("the label is longer than {LABEL_MAX} characters")]
    LongLabel,
    /// A name of a space with nothing in it.
    #[error("the name of the space is empty")]
    EmptySpaceName,
    /// A name of a space longer than [`SPACE_NAME_MAX`].
    #[error("the name of the space is longer than {SPACE_NAME_MAX} characters")]
    LongSpaceName,
}

/// Text trimmed, its runs of white space one space, and checked for length.
fn checked(text: &str, max: usize, empty: DomainError, long: DomainError) -> Result<String, DomainError> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.chars().count() {
        0 => Err(empty),
        n if n > max => Err(long),
        _ => Ok(text),
    }
}

/// A title as it is kept: trimmed, one space between words, and checked.
pub fn title(text: &str) -> Result<String, DomainError> {
    checked(text, TITLE_MAX, DomainError::EmptyTitle, DomainError::LongTitle)
}

/// A label as it is kept: trimmed, one space between words, and checked. Labels differ in case: `Tax` is not `tax`.
pub fn label(text: &str) -> Result<String, DomainError> {
    checked(text, LABEL_MAX, DomainError::EmptyLabel, DomainError::LongLabel)
}

/// The name of a space as it is kept: trimmed, one space between words, and checked.
pub fn space_name(text: &str) -> Result<String, DomainError> {
    checked(text, SPACE_NAME_MAX, DomainError::EmptySpaceName, DomainError::LongSpaceName)
}

/// The title a file brings: its name without the folder and the extension.
pub fn title_of_file(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    };
    title(stem).or_else(|_| title(name)).unwrap_or_else(|_| "Document".to_owned())
}

/// What happens to a space itself – the payload of an event in its log, beside the events about documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpaceEvent {
    /// The space got a name; the latest in the agreed order holds.
    SpaceNamed {
        /// The name.
        name: String,
    },
}

/// The kinds of event of [`SpaceEvent`].
pub const SPACE_KINDS: [&str; 1] = ["space_named"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_checked() {
        assert_eq!(title("  milk "), Ok("milk".to_owned()));
        assert_eq!(title(" tax \n 2026 "), Ok("tax 2026".to_owned()));
        assert_eq!(title(" \t"), Err(DomainError::EmptyTitle));
        assert_eq!(title(&"x".repeat(TITLE_MAX + 1)), Err(DomainError::LongTitle));
    }

    #[test]
    fn labels_and_names_are_checked() {
        assert_eq!(label(" Tax "), Ok("Tax".to_owned()));
        assert_eq!(label(""), Err(DomainError::EmptyLabel));
        assert_eq!(space_name(&"y".repeat(SPACE_NAME_MAX + 1)), Err(DomainError::LongSpaceName));
    }

    #[test]
    fn a_file_brings_its_title() {
        assert_eq!(title_of_file("scans/Lease 2026.pdf"), "Lease 2026");
        assert_eq!(title_of_file("C:\\Scans\\notice.tiff"), "notice");
        assert_eq!(title_of_file(".profile"), ".profile");
        assert_eq!(title_of_file("   .pdf"), ".pdf");
        assert_eq!(title_of_file(""), "Document");
    }

    #[test]
    fn a_space_event_reads_as_written() {
        let event = SpaceEvent::SpaceNamed { name: "Home".into() };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "space_named", "name": "Home"}));
        assert_eq!(json["kind"], SPACE_KINDS[0]);
    }
}
