// SPDX-License-Identifier: AGPL-3.0-or-later
// Light, dark or like the system – as on aktujo.org: `data-theme` on <html>, remembered in this browser.

export type Theme = "auto" | "light" | "dark";

const STORED = "aktujo.theme";

function stored(): Theme {
  try {
    const value = localStorage.getItem(STORED);
    if (value === "light" || value === "dark") return value;
  } catch {
    // Storage may be blocked; then the system decides.
  }
  return "auto";
}

export const theme = $state({ value: stored() });

/** Shows the theme on the page. */
export function apply() {
  if (theme.value === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme.value;
}

/** Chooses a theme and remembers it in this browser. */
export function setTheme(value: Theme) {
  theme.value = value;
  apply();
  try {
    if (value === "auto") localStorage.removeItem(STORED);
    else localStorage.setItem(STORED, value);
  } catch {
    // Remembering is a convenience only.
  }
}
