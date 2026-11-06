// SPDX-License-Identifier: MIT
//! Updating a kind from a newer version of its template, keeping what a person changed: a three-way merge of the
//! template's earlier kind (`base`), the space's kind (`mine`) and the template's newer kind (`theirs`), item by item
//! – the name, each name in another language, each field by its key, each word, the title. What only one side
//! changed is taken from that side; what both changed alike is taken; where both changed an item differently, the
//! space's stays and the item is a [`Conflict`], with the template's version beside it.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value as Json;

use crate::{FieldDef, Kind};

/// An item both sides changed differently: the space's version stayed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conflict {
    /// What it is: `name`, `names.de`, `field.total`, `word.invoice`, `title`.
    pub item: String,
    /// The space's version – kept; `null` where the space removed it.
    pub mine: Json,
    /// The template's newer version; `null` where the template removed it.
    pub theirs: Json,
}

/// The outcome of a merge.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    /// The kind as it would be.
    pub kind: Kind,
    /// The items both sides changed differently.
    pub conflicts: Vec<Conflict>,
}

fn json<T: Serialize>(value: Option<&T>) -> Json {
    value.map_or(Json::Null, |v| serde_json::to_value(v).expect("a kind is JSON"))
}

/// One item: absent on a side is `None`.
fn three<T: PartialEq + Clone + Serialize>(
    item: String,
    base: Option<&T>,
    mine: Option<&T>,
    theirs: Option<&T>,
    conflicts: &mut Vec<Conflict>,
) -> Option<T> {
    if mine == theirs || theirs == base {
        mine.cloned()
    } else if mine == base {
        theirs.cloned()
    } else {
        conflicts.push(Conflict { item, mine: json(mine), theirs: json(theirs) });
        mine.cloned()
    }
}

/// The keys of both sides: the newer template's order, then what only the space has, in its order.
fn order<'a>(theirs: impl Iterator<Item = &'a str>, mine: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut keys: Vec<&str> = theirs.collect();
    for key in mine {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Merges the newer template's kind `theirs` into the space's kind `mine`, both descended from `base`.
pub fn merge(base: &Kind, mine: &Kind, theirs: &Kind) -> Merged {
    let mut conflicts = Vec::new();
    let name = three("name".into(), Some(&base.name), Some(&mine.name), Some(&theirs.name), &mut conflicts)
        .unwrap_or_else(|| mine.name.clone());

    let mut names = BTreeMap::new();
    for language in order(theirs.names.keys().map(String::as_str), mine.names.keys().map(String::as_str)) {
        let item = format!("names.{language}");
        let merged =
            three(item, base.names.get(language), mine.names.get(language), theirs.names.get(language), &mut conflicts);
        if let Some(merged) = merged {
            names.insert(language.to_owned(), merged);
        }
    }

    let field = |kind: &Kind, key: &str| -> Option<FieldDef> { kind.field(key).cloned() };
    let mut fields = Vec::new();
    let keys = order(theirs.fields.iter().map(|f| f.key.as_str()), mine.fields.iter().map(|f| f.key.as_str()));
    // Removed on both sides, or added and removed: gone; the rest as `three` says.
    for key in keys {
        let item = format!("field.{key}");
        let (b, m, t) = (field(base, key), field(mine, key), field(theirs, key));
        if let Some(merged) = three(item, b.as_ref(), m.as_ref(), t.as_ref(), &mut conflicts) {
            fields.push(merged);
        }
    }

    let weight = |kind: &Kind, word: &str| kind.words.iter().find(|w| w.word == word).map(|w| w.weight);
    let mut words = Vec::new();
    let all = order(theirs.words.iter().map(|w| w.word.as_str()), mine.words.iter().map(|w| w.word.as_str()));
    for word in all {
        let item = format!("word.{word}");
        let (b, m, t) = (weight(base, word), weight(mine, word), weight(theirs, word));
        if let Some(weight) = three(item, b.as_ref(), m.as_ref(), t.as_ref(), &mut conflicts) {
            words.push(crate::Weighted { word: word.to_owned(), weight });
        }
    }

    let title = three("title".into(), Some(&base.title), Some(&mine.title), Some(&theirs.title), &mut conflicts)
        .unwrap_or_default();
    // A title of a field that is gone no longer names it.
    let title: Vec<Vec<String>> = title
        .into_iter()
        .map(|group| group.into_iter().filter(|key| fields.iter().any(|f: &FieldDef| &f.key == key)).collect())
        .filter(|group: &Vec<String>| !group.is_empty())
        .collect();

    Merged { kind: Kind { id: mine.id.clone(), name, names, fields, words, title }, conflicts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldType, Weighted};

    fn base() -> Kind {
        let mut kind = Kind::new(
            "letter",
            "Letter",
            vec![
                FieldDef::new("sender", "Sender", FieldType::Text),
                FieldDef::new("date", "Date", FieldType::Date),
                FieldDef::new("subject", "Subject", FieldType::Text),
            ],
        );
        kind.words = vec![Weighted { word: "dear".into(), weight: 2 }, Weighted { word: "regards".into(), weight: 2 }];
        kind.title = vec![vec!["sender".into()], vec!["date".into()]];
        kind
    }

    #[test]
    fn what_only_one_side_changed_is_taken() {
        let base = base();
        // The person renamed the kind, added a field and a word.
        let mut mine = base.clone();
        mine.name = "Mail".into();
        mine.fields.push(FieldDef::new("reply", "Answered", FieldType::Flag));
        mine.words.push(Weighted { word: "sincerely".into(), weight: 3 });
        // The template added a German name, changed a field's type, removed one and weighed a word anew.
        let mut theirs = base.clone();
        theirs.names.insert("de".into(), "Brief".into());
        theirs.fields[2].field_type = FieldType::Number;
        theirs.fields.remove(1);
        theirs.words[0].weight = 3;

        let merged = merge(&base, &mine, &theirs);
        assert!(merged.conflicts.is_empty(), "{:?}", merged.conflicts);
        let kind = merged.kind;
        assert_eq!((kind.name.as_str(), kind.names.get("de").map(String::as_str)), ("Mail", Some("Brief")));
        let keys: Vec<&str> = kind.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["sender", "subject", "reply"]);
        assert_eq!(kind.field("subject").unwrap().field_type, FieldType::Number);
        let words: Vec<(&str, u32)> = kind.words.iter().map(|w| (w.word.as_str(), w.weight)).collect();
        assert_eq!(words, [("dear", 3), ("regards", 2), ("sincerely", 3)]);
        // The date is gone, and so is it from the title.
        assert_eq!(kind.title, vec![vec!["sender".to_owned()]]);
        assert_eq!(kind.check(), Ok(()));
    }

    #[test]
    fn what_both_changed_differently_keeps_the_space_and_is_shown() {
        let base = base();
        let mut mine = base.clone();
        mine.name = "Mail".into();
        mine.fields[0].name = "From".into();
        mine.fields.remove(2);
        let mut theirs = base.clone();
        theirs.name = "Letter or email".into();
        theirs.fields[0].name = "Author".into();
        theirs.fields[2].name = "Re".into();

        let merged = merge(&base, &mine, &theirs);
        let items: Vec<&str> = merged.conflicts.iter().map(|c| c.item.as_str()).collect();
        assert_eq!(items, ["name", "field.sender", "field.subject"]);
        assert_eq!(merged.kind.name, "Mail");
        assert_eq!(merged.kind.field("sender").unwrap().name, "From");
        assert!(merged.kind.field("subject").is_none(), "the person removed it");
        assert_eq!(merged.conflicts[2].mine, Json::Null);
        assert_eq!(merged.conflicts[2].theirs["name"], "Re");
    }

    #[test]
    fn unchanged_on_one_side_is_the_other() {
        let base = base();
        let mut theirs = base.clone();
        theirs.fields.push(FieldDef::new("reference", "Reference", FieldType::Text));
        assert_eq!(merge(&base, &base, &theirs).kind, theirs);
        assert_eq!(merge(&base, &theirs, &base).kind, theirs);
        assert!(merge(&base, &base, &theirs).conflicts.is_empty());
    }
}
