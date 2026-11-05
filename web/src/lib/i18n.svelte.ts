// SPDX-License-Identifier: AGPL-3.0-or-later
// The texts of the web app, as data: locales/*.json in the repository, shared with the terminal interface. A text
// missing in a language is taken from English; a key missing there shows as itself.
import en from "../../../locales/en.json";
import de from "../../../locales/de.json";

type Texts = Record<string, string>;

export const languages: Record<string, Texts> = { en, de };

const STORED = "aktujo.language";

function first(): string {
  try {
    const stored = localStorage.getItem(STORED);
    if (stored && stored in languages) return stored;
  } catch {
    // Storage may be blocked; the browser's language then decides.
  }
  for (const language of navigator.languages ?? [navigator.language]) {
    const code = language.split("-")[0]?.toLowerCase() ?? "";
    if (code in languages) return code;
  }
  return "en";
}

export const i18n = $state({ language: first() });

/** Chooses the language and remembers it in this browser. */
export function choose(language: string) {
  i18n.language = language;
  document.documentElement.lang = language;
  try {
    localStorage.setItem(STORED, language);
  } catch {
    // Remembering is a convenience only.
  }
}

function has(key: string): boolean {
  return key in (languages[i18n.language] ?? {}) || key in en;
}

/** A text, with its `{name}`s filled in. */
export function t(key: string, values: Record<string, string | number> = {}): string {
  const text = (languages[i18n.language] as Texts | undefined)?.[key] ?? (en as Texts)[key] ?? key;
  return Object.entries(values).reduce((s, [name, value]) => s.replaceAll(`{${name}}`, String(value)), text);
}

/** The name of a kind. */
export function kindName(id: string): string {
  return has(`kind.${id}`) ? t(`kind.${id}`) : id;
}

/** The name of a field of a kind. */
export function fieldName(kind: string | null | undefined, key: string): string {
  if (kind && has(`field.${kind}.${key}`)) return t(`field.${kind}.${key}`);
  return has(`field.${key}`) ? t(`field.${key}`) : key;
}
