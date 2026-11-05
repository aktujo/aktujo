// SPDX-License-Identifier: AGPL-3.0-or-later
// The web app is built into dist/, which the aktujo binary embeds. In development, `pnpm dev` serves it and passes
// /api on to a `aktujo serve` running on the usual port.
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  build: { outDir: "dist", emptyOutDir: true, assetsInlineLimit: 0 },
  server: { proxy: { "/api": "http://127.0.0.1:8071" } },
});
