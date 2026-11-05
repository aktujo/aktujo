---
title: API
nav_order: 2
---

# The API

aktujo serves its API on the device itself: `aktujo serve` listens on `http://127.0.0.1:8071`, the web
app at `/`, the API under `/api/v1`. Requests and answers are JSON; an error is `{"error": "…"}` with a fitting
status. It answers only requests addressed to `localhost`, `127.0.0.1` or `[::1]`, and takes changes only from its
own pages – not from other sites open in the same browser.

The full description in OpenAPI is [`api/openapi.json`](https://github.com/aktujo/aktujo/blob/main/api/openapi.json)
– generated from the code by `aktujo openapi`, and checked by CI to match it. A running server gives the same at
`/api/v1/openapi.json`.

Paths below are under `/api/v1`; `{space}` and `{document}` are identities, `D` stands for
`/spaces/{space}/documents/{document}`.

| Method | Path | What |
|---|---|---|
| GET | `/status` | the version, the device, whether this build reads with OCR |
| GET | `/spaces` | every space on this device |
| POST | `/spaces` | founds a space: `{"name": "…"}` |
| PUT | `/spaces/{space}` | renames a space: `{"name": "…"}` |
| GET | `/spaces/{space}/labels` | the labels used, with how many documents have each |
| GET | `/kinds` | the kinds of document with their fields |
| POST | `/search` | finds documents: `{"words", "spaces", "kind", "labels", "from", "to", "bin", "limit"}`, every part optional |
| POST | `/spaces/{space}/documents?name=…` | adds a file – its bytes as the body, its media type as `Content-Type` |
| GET | `D` | a document with its fields, versions and suggestions |
| GET | `D/text` | the text read, page by page |
| POST | `D/versions?name=…` | adds a file as the newest version |
| GET | `D/versions/{number}/file` | a version's file; `?download=true` to download it |
| PUT | `D/title` | `{"title": "…"}` |
| PUT | `D/kind` | confirms the kind: `{"kind": "invoice"}`, or `null` for none |
| PUT, DELETE | `D/fields/{key}` | confirms a value – `{"value": {"type": "money", "value": {"amount": "12.50", "currency": "EUR"}}}` – or empties the field |
| POST | `D/labels` | adds a label: `{"label": "…"}` |
| DELETE | `D/labels/{label}` | takes a label off |
| POST | `D/trash`, `D/restore` | puts the document in the bin, takes it out |
