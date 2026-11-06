<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, type Kind, type Space } from "./lib/api";
  import CatalogPanel from "./CatalogPanel.svelte";
  import { byName, i18n, nameOf, t } from "./lib/i18n.svelte";

  type Field = Kind["fields"][number];
  type FieldType = Field["type"];
  type Named = { name: string; names?: Record<string, string> };

  let {
    space,
    kinds,
    readOnly = false,
    onclose,
    onchange,
  }: { space: Space; kinds: Kind[]; readOnly?: boolean; onclose: () => void; onchange: () => void } = $props();

  const TYPES: FieldType[] = ["text", "date", "money", "number", "flag"];
  /** The weight of a word without one; a kind is suggested from a sum of 6. */
  const WEIGHT = 2;

  let draft = $state<Kind | null>(null);
  let browsing = $state(false);
  let words = $state("");
  let json = $state("");
  let asJson = $state(false);
  let newName = $state("");
  let newField = $state("");
  let error = $state("");
  let message = $state("");
  let picker = $state<HTMLInputElement>();

  const path = $derived({ space: space.id });

  /** The name in the chosen language if the kind has one there, else its name – what a person sees elsewhere. */
  function named(item: Named): string {
    return nameOf(item);
  }

  function rename(item: Named, value: string) {
    if (item.names?.[i18n.language] !== undefined) item.names[i18n.language] = value;
    else item.name = value;
  }

  /** An identity from a name: letters and digits in lower case, joined by `-`. */
  function slug(name: string): string {
    const plain = name
      .toLowerCase()
      .replaceAll("ß", "ss")
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "");
    return plain.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 60) || "field";
  }

  function wordsText(kind: Kind): string {
    return (kind.words ?? []).map((w) => (w.weight === WEIGHT ? w.word : `${w.word} = ${w.weight}`)).join("\n");
  }

  function parseWords(text: string): Kind["words"] {
    return text
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const [word, weight] = line.split("=").map((part) => part.trim());
        const number = Number.parseInt(weight ?? "", 10);
        return { word: word ?? line, weight: Number.isFinite(number) && number > 0 ? number : WEIGHT };
      });
  }

  function pick(kind: Kind) {
    draft = structuredClone($state.snapshot(kind)) as Kind;
    words = wordsText(draft);
    json = JSON.stringify(draft, null, 2);
    asJson = false;
    error = "";
    message = "";
  }

  /** The labels a field is found after, from its first way with labels. */
  function labelsOf(field: Field): string {
    for (const way of field.find ?? []) {
      if (way.way === "label" || way.way === "largest") return way.labels.join(", ");
    }
    return "";
  }

  function setLabels(field: Field, text: string) {
    const labels = text
      .split(",")
      .map((l) => l.trim())
      .filter(Boolean);
    const find = field.find ?? [];
    const at = find.findIndex((w) => w.way === "label" || w.way === "largest");
    if (at >= 0) {
      const way = find[at];
      if (way && (way.way === "label" || way.way === "largest")) {
        if (labels.length) way.labels = labels;
        else find.splice(at, 1);
      }
    } else if (labels.length) {
      find.unshift({ way: field.type === "money" ? "largest" : "label", labels });
    }
    field.find = find;
  }

  function inTitle(kind: Kind, key: string): boolean {
    return (kind.title ?? []).some((group) => group.includes(key));
  }

  function toggleTitle(kind: Kind, key: string, on: boolean) {
    const title = (kind.title ?? []).map((group) => group.filter((k) => k !== key)).filter((g) => g.length);
    if (on) title.push([key]);
    kind.title = title;
  }

  function addField() {
    if (!draft || !newField.trim()) return;
    const base = slug(newField);
    let key = base;
    for (let n = 2; draft.fields.some((f) => f.key === key); n++) key = `${base}-${n}`;
    draft.fields.push({ key, name: newField.trim(), type: "text" });
    newField = "";
  }

  function removeField(key: string) {
    if (!draft) return;
    draft.fields = draft.fields.filter((f) => f.key !== key);
    toggleTitle(draft, key, false);
  }

  function move(at: number, by: number) {
    if (!draft) return;
    const to = at + by;
    if (to < 0 || to >= draft.fields.length) return;
    const fields = [...draft.fields];
    [fields[at], fields[to]] = [fields[to]!, fields[at]!];
    draft.fields = fields;
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/kinds", {
      params: { path },
      body: { name: newName },
    });
    error = problem?.error ?? "";
    if (data) {
      newName = "";
      pick(data);
      onchange();
    }
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!draft) return;
    let body: Kind;
    try {
      body = asJson ? (JSON.parse(json) as Kind) : { ...draft, words: parseWords(words) };
    } catch (problem) {
      error = String(problem);
      return;
    }
    const { data, error: problem } = await api.PUT("/api/v1/spaces/{space}/kinds/{kind}", {
      params: { path: { space: space.id, kind: draft.id } },
      body,
    });
    error = problem?.error ?? "";
    if (data) {
      pick(data);
      message = t("kinds.saved");
      onchange();
    }
  }

  async function remove() {
    if (!draft || !confirm(t("kinds.removeConfirm", { name: named(draft) }))) return;
    const { error: problem } = await api.DELETE("/api/v1/spaces/{space}/kinds/{kind}", {
      params: { path: { space: space.id, kind: draft.id } },
    });
    error = problem?.error ?? "";
    if (!problem) {
      draft = null;
      onchange();
    }
  }

  async function importFile(files: FileList) {
    const file = files[0];
    if (!file) return;
    // The file as it is: a template remembers its SHA-256, so that it is found in a catalog later.
    const response = await fetch(`/api/v1/spaces/${encodeURIComponent(space.id)}/kind-import`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: file,
    });
    const body = (await response.json().catch(() => null)) as { changed?: number; error?: string } | null;
    error = response.ok ? "" : `${file.name}: ${body?.error ?? response.statusText}`;
    if (response.ok) {
      message = t("kinds.imported", { n: body?.changed ?? 0 });
      onchange();
    }
  }

  function exportKinds() {
    const blob = new Blob([JSON.stringify({ name: space.name ?? "", kinds }, null, 2) + "\n"], {
      type: "application/json",
    });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = `${slug(space.name ?? "space")}-kinds.json`;
    link.click();
    URL.revokeObjectURL(link.href);
  }
</script>

<article>
  <button class="link" onclick={onclose}>← {t("doc.back")}</button>
  <h1>{t("kinds.title")}</h1>
  <p class="note">{t("kinds.hint")}</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if message}<p class="note" aria-live="polite">{message}</p>{/if}

  <fieldset class="plain" disabled={readOnly}>
    {#if browsing}
      <CatalogPanel {space} onclose={() => (browsing = false)} {onchange} />
    {:else}
    {#if kinds.length}
      <ul class="versions">
        {#each byName(kinds) as kind (kind.id)}
          <li>
            <button class="link" class:active-link={draft?.id === kind.id} onclick={() => pick(kind)}>{named(kind)}</button>
            <span class="note">{t("kinds.fieldCount", { n: kind.fields.length })}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p>{t("kinds.none")}</p>
    {/if}

    <form class="row" onsubmit={add}>
      <input bind:value={newName} placeholder={t("kinds.newName")} aria-label={t("kinds.newName")} />
      <button type="submit" disabled={!newName.trim()}>{t("kinds.add")}</button>
    </form>
    <p class="row">
      <button class="primary" onclick={() => ((browsing = true), (draft = null))}>{t("catalog.browse")}</button>
      <button onclick={() => picker?.click()}>{t("kinds.import")}</button>
      <button onclick={exportKinds} disabled={!kinds.length}>{t("kinds.export")}</button>
      <input
        class="hidden"
        type="file"
        accept="application/json,.json"
        bind:this={picker}
        onchange={(e) => {
          if (e.currentTarget.files) importFile(e.currentTarget.files);
          e.currentTarget.value = "";
        }}
      />
    </p>
    <p class="note">{t("kinds.importHint")}</p>

    {#if draft}
      {@const kind = draft}
      <form class="kind" onsubmit={save}>
        <h2>{named(kind)}</h2>
        <label class="check">
          <input type="checkbox" bind:checked={asJson} onchange={() => (json = JSON.stringify({ ...kind, words: parseWords(words) }, null, 2))} />
          {t("kinds.asJson")}
        </label>
        {#if asJson}
          <textarea class="json" rows="20" bind:value={json} aria-label={t("kinds.asJson")} spellcheck="false"></textarea>
        {:else}
          <label>
            {t("kinds.name")}
            <input value={named(kind)} oninput={(e) => rename(kind, e.currentTarget.value)} required />
          </label>

          <h3>{t("doc.fields")}</h3>
          {#if kind.fields.length}
            <table class="fields-table">
              <thead>
                <tr>
                  <th>{t("kinds.fieldName")}</th>
                  <th>{t("kinds.fieldType")}</th>
                  <th>{t("kinds.labels")}</th>
                  <th>{t("kinds.inTitle")}</th>
                  <th><span class="hidden">{t("kinds.order")}</span></th>
                </tr>
              </thead>
              <tbody>
                {#each kind.fields as field, at (field.key)}
                  <tr>
                    <td>
                      <input
                        value={named(field)}
                        oninput={(e) => rename(field, e.currentTarget.value)}
                        aria-label={t("kinds.fieldName")}
                        required
                      />
                      <span class="note">{field.key}</span>
                    </td>
                    <td>
                      <select bind:value={field.type} aria-label={t("kinds.fieldType")}>
                        {#each TYPES as type (type)}<option value={type}>{t(`type.${type}`)}</option>{/each}
                      </select>
                    </td>
                    <td>
                      <input
                        value={labelsOf(field)}
                        onchange={(e) => setLabels(field, e.currentTarget.value)}
                        placeholder={t("kinds.labelsPlaceholder")}
                        aria-label={t("kinds.labels")}
                      />
                    </td>
                    <td>
                      <label class="check">
                        <input
                          type="checkbox"
                          checked={inTitle(kind, field.key)}
                          onchange={(e) => toggleTitle(kind, field.key, e.currentTarget.checked)}
                          aria-label={t("kinds.inTitle")}
                        />
                        <span class="narrow-only" aria-hidden="true">{t("kinds.inTitle")}</span>
                      </label>
                    </td>
                    <td class="row">
                      <button type="button" class="small" onclick={() => move(at, -1)} disabled={at === 0} aria-label={t("kinds.up")}>↑</button>
                      <button type="button" class="small" onclick={() => move(at, 1)} disabled={at === kind.fields.length - 1} aria-label={t("kinds.down")}>↓</button>
                      <button type="button" class="small danger" onclick={() => removeField(field.key)} aria-label={t("kinds.removeField", { name: named(field) })}>×</button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
          <div class="row">
            <input
              bind:value={newField}
              placeholder={t("kinds.newField")}
              aria-label={t("kinds.newField")}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addField();
                }
              }}
            />
            <button type="button" disabled={!newField.trim()} onclick={addField}>+</button>
          </div>

          <h3>{t("kinds.words")}</h3>
          <p class="note">{t("kinds.wordsHint")}</p>
          <textarea rows="6" bind:value={words} aria-label={t("kinds.words")}></textarea>
        {/if}
        <p class="row">
          <button type="submit" class="primary">{t("doc.save")}</button>
          <button type="button" onclick={() => (draft = null)}>{t("doc.cancel")}</button>
          <button type="button" class="danger" onclick={remove}>{t("kinds.remove")}</button>
        </p>
      </form>
    {/if}
    {/if}
  </fieldset>
</article>
