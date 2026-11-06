# aktujo-api

The open API of [aktujo](https://aktujo.dev): HTTP and JSON under `/api/v1`, described in OpenAPI
([`api/openapi.json`](https://github.com/aktujo/aktujo/blob/main/api/openapi.json)). It works through the workspace
of `aktujo-sdk` only, listens on the device itself and answers only requests addressed to it there.

```rust
let app = aktujo_web::around(aktujo_api::routes(workspace, background));
aktujo_api::serve(app, 8071, |address| println!("aktujo at http://{address}")).await?;
```

`router` is the API alone, behind the same guard. The API is described on [aktujo.dev](https://aktujo.dev).

AGPL-3.0-or-later. Part of [aktujo](https://github.com/aktujo/aktujo).
