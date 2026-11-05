// SPDX-License-Identifier: AGPL-3.0-or-later
//! The domain of aktujo: its types and rules, without I/O. Provisional – an item with a title stands in for the real
//! domain until there is one.

use serde::{Deserialize, Serialize};

/// The longest title, in characters.
pub const TITLE_MAX: usize = 200;

/// An item: what aktujo keeps, for now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Unique within a store, given by it.
    pub id: u64,
    /// What the item is called: not empty, at most [`TITLE_MAX`] characters, without surrounding spaces.
    pub title: String,
}

/// Why something is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// A title with nothing in it.
    #[error("the title is empty")]
    EmptyTitle,
    /// A title longer than [`TITLE_MAX`].
    #[error("the title is longer than {TITLE_MAX} characters")]
    LongTitle,
}

/// A title as it is kept: trimmed and checked.
pub fn title(text: &str) -> Result<String, DomainError> {
    let text = text.trim();
    match text.chars().count() {
        0 => Err(DomainError::EmptyTitle),
        n if n > TITLE_MAX => Err(DomainError::LongTitle),
        _ => Ok(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_checked() {
        assert_eq!(title("  milk "), Ok("milk".to_owned()));
        assert_eq!(title(" \t"), Err(DomainError::EmptyTitle));
        assert_eq!(title(&"x".repeat(TITLE_MAX + 1)), Err(DomainError::LongTitle));
    }
}
