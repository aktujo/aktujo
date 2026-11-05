---
title: API
nav_order: 2
---

# The API

aktujo serves its API on the device itself: `aktujo serve` listens on `http://127.0.0.1:8071`, the web
app at `/`, the API under `/api/v1`. Requests and answers are JSON; an error is `{"error": "…"}` with a fitting
status.

The full description in OpenAPI is [`api/openapi.json`](https://github.com/aktujo/aktujo/blob/main/api/openapi.json)
– generated from the code by `aktujo openapi`, and checked by CI to match it. A running server gives the same at
`/api/v1/openapi.json`.

| Method | Path | What |
|---|---|---|
| GET | `/api/v1/items` | every item |
| POST | `/api/v1/items` | adds an item: `{"title": "…"}` |
| DELETE | `/api/v1/items/{id}` | removes an item |
