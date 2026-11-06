// SPDX-License-Identifier: MIT
//! The texts of aktujo's web app and terminal interface, as data: one JSON object per language in `locales/`, a key
//! for each text. The web app imports the files when it is built; the terminal interface takes them from here.
//! English is complete; another language may lack a text, which then shows in English.
#![no_std]

/// The languages, English first: their code and their texts as JSON.
pub const LANGUAGES: [(&str, &str); 2] = [("en", ENGLISH), ("de", include_str!("../locales/de.json"))];

/// The English texts – every key.
pub const ENGLISH: &str = include_str!("../locales/en.json");

#[cfg(test)]
mod tests {
    extern crate std;
    use std::collections::BTreeMap;

    #[test]
    fn each_language_has_the_keys_of_english_and_no_others() {
        let keys = |json: &str| serde_json::from_str::<BTreeMap<std::string::String, serde_json::Value>>(json).unwrap();
        let english = keys(super::ENGLISH);
        for (code, json) in super::LANGUAGES {
            let texts = keys(json);
            let unknown: std::vec::Vec<_> = texts.keys().filter(|k| !english.contains_key(*k)).collect();
            assert!(unknown.is_empty(), "{code}: {unknown:?} not in English");
            assert!(texts.values().all(|v| v.is_string()), "{code}: every text is a string");
        }
    }
}
