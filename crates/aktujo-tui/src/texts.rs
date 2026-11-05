// SPDX-License-Identifier: AGPL-3.0-or-later
//! The texts of the interface, as data: `locales/*.json` in the repository, shared with the web app. A text missing
//! in a language is taken from English, a key missing there shows as itself.

use std::collections::HashMap;

/// The languages built in: their code and their texts.
const LANGUAGES: [(&str, &str); 2] =
    [("en", include_str!("../../../locales/en.json")), ("de", include_str!("../../../locales/de.json"))];

/// The texts of one language.
#[derive(Debug, Clone)]
pub struct Texts {
    texts: HashMap<String, String>,
    english: HashMap<String, String>,
}

fn parse(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).expect("the texts are JSON")
}

impl Texts {
    /// The texts of a language by its code (`de`, `en-GB` …); English for one not built in.
    pub fn of(language: &str) -> Self {
        let code = language.split(['-', '_', '.']).next().unwrap_or_default().to_ascii_lowercase();
        let json = LANGUAGES.iter().find(|(c, _)| *c == code).map_or(LANGUAGES[0].1, |(_, json)| json);
        Texts { texts: parse(json), english: parse(LANGUAGES[0].1) }
    }

    /// The texts of the language the environment asks for (`LC_ALL`, `LC_MESSAGES`, `LANG`).
    pub fn from_environment() -> Self {
        let language = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX");
        Self::of(language.as_deref().unwrap_or("en"))
    }

    /// A text.
    pub fn get(&self, key: &str) -> String {
        self.texts.get(key).or_else(|| self.english.get(key)).cloned().unwrap_or_else(|| key.to_owned())
    }

    /// A text with its `{name}`s filled in.
    pub fn fill(&self, key: &str, values: &[(&str, &str)]) -> String {
        values.iter().fold(self.get(key), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
    }

    /// The name of a kind.
    pub fn kind(&self, id: &str) -> String {
        let key = format!("kind.{id}");
        if self.texts.contains_key(&key) || self.english.contains_key(&key) { self.get(&key) } else { id.to_owned() }
    }

    /// The name of a field of a kind.
    pub fn field(&self, kind: Option<&str>, key: &str) -> String {
        let special = kind.map(|kind| format!("field.{kind}.{key}"));
        for candidate in special.into_iter().chain([format!("field.{key}")]) {
            if self.texts.contains_key(&candidate) || self.english.contains_key(&candidate) {
                return self.get(&candidate);
            }
        }
        key.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_text() {
        let english = parse(LANGUAGES[0].1);
        for (code, json) in LANGUAGES {
            let texts = parse(json);
            let mut missing: Vec<_> = english.keys().filter(|k| !texts.contains_key(*k)).collect();
            missing.sort();
            assert!(missing.is_empty(), "{code} misses {missing:?}");
        }
    }

    #[test]
    fn texts_are_found_by_language_and_filled_in() {
        let german = Texts::of("de_DE.UTF-8");
        assert_eq!(german.kind("invoice"), "Rechnung");
        assert_eq!(german.field(Some("policy"), "number"), "Versicherungsnummer");
        assert_eq!(german.field(Some("invoice"), "number"), "Nummer");
        assert_eq!(german.field(None, "colour"), "colour");
        assert_eq!(german.fill("doc.added", &[("n", "3")]), "3 hinzugefügt");
        assert_eq!(Texts::of("fr").get("doc.trash"), "Put in the bin");
    }
}
