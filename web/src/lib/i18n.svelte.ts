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

/** A text, with its `{name}`s filled in. */
export function t(key: string, values: Record<string, string | number> = {}): string {
  const text = (languages[i18n.language] as Texts | undefined)?.[key] ?? (en as Texts)[key] ?? key;
  return Object.entries(values).reduce((s, [name, value]) => s.replaceAll(`{${name}}`, String(value)), text);
}

/** Something with a name and names in other languages – a kind or a field, as the space defines it. */
type Named = { name: string; names?: Record<string, string> };

/** The name in the chosen language, if it has one there. */
export function nameOf(named: Named): string {
  return named.names?.[i18n.language] ?? named.name;
}

type KindLike = Named & { id: string; fields: (Named & { key: string })[] };

/** Kinds in the order of their names in the chosen language. */
export function byName<T extends Named>(items: T[]): T[] {
  return [...items].sort((a, b) => nameOf(a).localeCompare(nameOf(b), i18n.language));
}

/** The name of a kind of the space; its identity if the space has no such kind. */
export function kindName(kinds: KindLike[], id: string): string {
  const kind = kinds.find((k) => k.id === id);
  return kind ? nameOf(kind) : id;
}

/** The name of a field of a kind; its key if the kind does not have it. */
export function fieldName(kinds: KindLike[], kind: string | null | undefined, key: string): string {
  const field = kinds.find((k) => k.id === kind)?.fields.find((f) => f.key === key);
  return field ? nameOf(field) : key;
}
