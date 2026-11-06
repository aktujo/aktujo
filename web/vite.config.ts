// SPDX-License-Identifier: AGPL-3.0-or-later
// The web app is built into crates/aktujo-web/web/, which aktujo-web embeds into the binary. In development, `pnpm dev` serves it and passes
// /api on to a `aktujo serve` running on the usual port.
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  build: { outDir: "../crates/aktujo-web/web", emptyOutDir: true, assetsInlineLimit: 0 },
  // The texts are in ../crates/aktujo-locales/locales, shared with the terminal interface.
  server: { proxy: { "/api": "http://127.0.0.1:8071" }, fs: { allow: [".."] } },
});
