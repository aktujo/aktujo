// SPDX-License-Identifier: AGPL-3.0-or-later
import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { apply } from "./lib/theme.svelte";

apply();

export default mount(App, { target: document.getElementById("app")! });
