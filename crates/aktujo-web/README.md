# aktujo-web

The web app of [aktujo](https://aktujo.dev), built in: served from the binary around the API, with a
Content-Security-Policy that allows nothing from elsewhere. `aktujo serve` puts it around the API:

```rust
let app = aktujo_web::around(aktujo_api::routes(workspace, background));
aktujo_api::serve(app, 8071, |address| println!("aktujo at http://{address}")).await?;
```

The app itself (Svelte, TypeScript) lives in `web/` of the repository; `pnpm build` there builds it into this crate,
and the crate on crates.io carries the build.

AGPL-3.0-or-later. Part of [aktujo](https://github.com/aktujo/aktujo).
