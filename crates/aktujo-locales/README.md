# aktujo-locales

The texts of [aktujo](https://aktujo.dev)'s web app and terminal interface, as data: one JSON object per language in
`locales/` (English, German), a key for each text, `{name}` for what is filled in.

```rust
for (code, json) in aktujo_locales::LANGUAGES {
    // "en", "de" – and the texts as JSON
}
```

A new text gets a key in every file; English has every key, another language may lack one (it then shows in
English). The apps of aktujo keep their own texts.

MIT. Part of [aktujo](https://github.com/aktujo/aktujo).
