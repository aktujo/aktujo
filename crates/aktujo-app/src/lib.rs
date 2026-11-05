// SPDX-License-Identifier: AGPL-3.0-or-later
//! The workspace of aktujo: every command and query. The terminal interface, the API and the command line work
//! through it only – never with the store or the domain on their own.

use aktujo_core::{DomainError, Item, title};
mod items;

use items::Store;
pub use items::StoreError;

/// Why a command did not happen.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// What was asked is not valid.
    #[error(transparent)]
    Invalid(#[from] DomainError),
    /// The store.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// One workspace: the data and what can be done with it.
#[derive(Debug, Default)]
pub struct Workspace {
    store: Store,
}

impl Workspace {
    /// A new, empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an item.
    pub fn add_item(&mut self, text: &str) -> Result<Item, AppError> {
        Ok(self.store.insert(title(text)?))
    }

    /// Removes an item.
    pub fn remove_item(&mut self, id: u64) -> Result<Item, AppError> {
        Ok(self.store.remove(id)?)
    }

    /// Every item.
    pub fn items(&self) -> &[Item] {
        self.store.items()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_added_and_removed() {
        let mut workspace = Workspace::new();
        let item = workspace.add_item(" bread ").unwrap();
        assert_eq!(item.title, "bread");
        assert!(matches!(workspace.add_item(""), Err(AppError::Invalid(DomainError::EmptyTitle))));
        assert_eq!(workspace.remove_item(item.id).unwrap(), item);
        assert!(matches!(workspace.remove_item(item.id), Err(AppError::Store(StoreError::NotFound(_)))));
        assert!(workspace.items().is_empty());
    }
}
