// SPDX-License-Identifier: AGPL-3.0-or-later
//! The items of the starter, kept in memory until the archive's domain replaces them (on the core: `aktujo-store`,
//! `aktujo-log`, `aktujo-blob`).

use aktujo_core::Item;

/// Why the store could not do what was asked.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No item with this id.
    #[error("no item {0}")]
    NotFound(u64),
}

/// The items, in the order they were added.
#[derive(Debug, Default)]
pub struct Store {
    items: Vec<Item>,
    next: u64,
}

impl Store {
    /// Keeps a new item with a checked title and gives it the next id.
    pub fn insert(&mut self, title: String) -> Item {
        self.next += 1;
        let item = Item { id: self.next, title };
        self.items.push(item.clone());
        item
    }

    /// Every item.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Takes an item out.
    pub fn remove(&mut self, id: u64) -> Result<Item, StoreError> {
        let at = self.items.iter().position(|item| item.id == id).ok_or(StoreError::NotFound(id))?;
        Ok(self.items.remove(at))
    }
}
