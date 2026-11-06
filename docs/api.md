---
title: API
nav_order: 4
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
| GET | `/spaces/{space}/kinds` | the kinds of documents the space defines – none until someone adds or imports them |
| POST | `/spaces/{space}/kinds` | adds a kind: `{"name": "…"}`; its identity is made from the name |
| PUT, DELETE | `/spaces/{space}/kinds/{kind}` | defines a kind anew – name, fields, words, title, as JSON ([RFC 0005](rfc/0005-pure-core.md)) – or removes it |
| POST | `/spaces/{space}/kind-import` | takes over a file of kinds as it is – a template (it remembers its SHA-256), an export, a list |
| GET, POST, PUT, DELETE | `/sources` | the sources of templates on this device: `{"url"}` to add, `{"url", "on"}` to switch, `?url=…` to remove (not the official catalog) |
| GET | `/spaces/{space}/catalog` | the templates of the sources that are on, read now, each `new`, `taken` or with an `update` |
| POST | `/spaces/{space}/catalog/plan` | what taking a template over would do: `{"source", "template"}` → each kind `new`, `same`, `replace`, `merge` (with conflicts) or `gone` |
| POST | `/spaces/{space}/catalog/take` | takes it over: `{"source", "template", "choices": {"<kind>": "take" \| "theirs" \| "keep" \| "remove"}}` |
| GET | `/spaces/{space}/export` | the space as a ZIP: `space.json`, every version, what was read, `events.jsonl` – not encrypted |
| POST | `/space-import?name=…` | an exported ZIP (the body as it is) read back in as a new space; what could not be taken over is listed |
| GET, POST, PUT, DELETE | `/watches` | the watched folders of this device: `{"space", "folder", "after": "leave" \| "move", "subfolders"}` to add, with `"on"` to switch, `?space=…&folder=…` to stop; each with how the last look went |
| GET | `D/versions/{number}/pages` | how many pages a version has – full build only, like the next four |
| GET | `D/versions/{number}/pages/{page}?width=…` | a page as PNG |
| POST | `D/improve/preview?page=…&width=…` | a page of what improving would make, as PNG: `{"pages", "turns", "deskew", "crop", "colour"}` |
| POST | `D/improve` | improves the newest version into a new one (the same body) |
| POST | `/spaces/{space}/combine` | `{"documents": […], "improve": {…}}` – their pages as the newest version of the first, the others into the bin |
| POST | `/search` | finds documents: `{"words", "spaces", "kind", "labels", "from", "to", "bin", "limit"}`, every part optional |
| POST | `/spaces/{space}/documents?name=…` | adds a file – its bytes as the body, its media type as `Content-Type` |
| GET | `D` | a document with its fields, versions and suggestions |
| GET | `D/text` | the text read, page by page |
| POST | `D/versions?name=…` | adds a file as the newest version |
| GET | `D/versions/{number}/file` | a version's file; `?download=true` to download it |
| PUT | `D/title` | `{"title": "…"}` |
| PUT | `D/kind` | confirms the kind: `{"kind": "letter"}`, or `null` for none; the space must have it |
| PUT, DELETE | `D/fields/{key}` | confirms a value – `{"value": {"type": "money", "value": {"amount": "12.50", "currency": "EUR"}}}` – or empties the field |
| POST | `D/labels` | adds a label: `{"label": "…"}` |
| DELETE | `D/labels/{label}` | takes a label off |
| POST | `D/trash`, `D/restore` | puts the document in the bin, takes it out |
| PUT | `/device` | names this device for the other members: `{"name": "…"}` |
| POST | `/spaces/{space}/sync/start` | starts to synchronise through a relay: `{"relay": "http://…:8072"}`; answers with the recovery code, shown only now |
| POST | `/spaces/{space}/sync` | synchronises now |
| POST | `/spaces/{space}/invitations` | an invitation for another device: `{"rights": "read" \| "write" \| "manage"}` |
| POST | `/join` | joins with an invitation: `{"invitation": "…"}` |
| POST | `/recover` | brings a space back: `{"relay", "space", "code"}` |
| GET | `/spaces/{space}/members` | the members, as the relay lists them |
| DELETE | `/spaces/{space}/members/{member}` | removes a member by its identity |
