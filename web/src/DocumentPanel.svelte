<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, fileUrl, shown, upload, type DocumentView, type Kind, type Value } from "./lib/api";
  import { byName, fieldName, kindName, nameOf, t } from "./lib/i18n.svelte";

  let {
    space,
    id,
    kinds,
    readOnly = false,
    onclose,
    onchange,
  }: {
    space: string;
    id: string;
    kinds: Kind[];
    readOnly?: boolean;
    onclose: () => void;
    onchange: () => void;
  } = $props();

  let document = $state<DocumentView | null>(null);
  let pages = $state<{ method: string; text: string }[] | null>(null);
  let error = $state("");
  let editingTitle = $state<string | null>(null);
  let label = $state("");
  let editing = $state<{ key: string; text: string; currency: string } | null>(null);
  let picker = $state<HTMLInputElement>();

  const path = $derived({ space, document: id });
  const kind = $derived(kinds.find((k) => k.id === document?.kind) ?? null);
  // The fields of the kind first, in its order, then any other the document has.
  const fields = $derived.by(() => {
    const own = document?.fields ?? [];
    const keys = [...(kind?.fields.map((f) => f.key) ?? []), ...own.map((f) => f.key)];
    return [...new Set(keys)].map((key) => ({
      key,
      type: kind?.fields.find((f) => f.key === key)?.type ?? "text",
      field: own.find((f) => f.key === key) ?? null,
    }));
  });

  async function load() {
    const { data, error: problem } = await api.GET("/api/v1/spaces/{space}/documents/{document}", {
      params: { path },
    });
    error = problem?.error ?? "";
    document = data ?? null;
    if (document?.reading === "done") {
      const { data: text } = await api.GET("/api/v1/spaces/{space}/documents/{document}/text", { params: { path } });
      pages = text ?? null;
    }
  }

  /** Takes what a change answered: the document as it is now. */
  function took({ data, error: problem }: { data?: DocumentView; error?: { error: string } }) {
    error = problem?.error ?? "";
    if (data) document = data;
    onchange();
  }

  async function setTitle(title: string) {
    took(await api.PUT("/api/v1/spaces/{space}/documents/{document}/title", { params: { path }, body: { title } }));
    editingTitle = null;
  }

  async function setKind(value: string | null) {
    took(await api.PUT("/api/v1/spaces/{space}/documents/{document}/kind", { params: { path }, body: { kind: value } }));
  }

  async function setField(key: string, value: Value) {
    const params = { path: { ...path, key } };
    took(await api.PUT("/api/v1/spaces/{space}/documents/{document}/fields/{key}", { params, body: { value } }));
  }

  async function clearField(key: string) {
    const params = { path: { ...path, key } };
    took(await api.DELETE("/api/v1/spaces/{space}/documents/{document}/fields/{key}", { params }));
  }

  function edit(key: string, value: Value | null | undefined) {
    if (value?.type === "money") editing = { key, text: value.value.amount, currency: value.value.currency };
    else editing = { key, text: value && value.type !== "flag" ? value.value : "", currency: "EUR" };
  }

  async function saveField(event: SubmitEvent, type: string) {
    event.preventDefault();
    if (!editing) return;
    const { key, text, currency } = editing;
    const value: Value =
      type === "money"
        ? { type: "money", value: { amount: text.trim().replace(",", "."), currency: currency.trim().toUpperCase() } }
        : type === "date"
          ? { type: "date", value: text }
          : type === "number"
            ? { type: "number", value: text.trim().replace(",", ".") }
            : { type: "text", value: text };
    await setField(key, value);
    if (!error) editing = null;
  }

  async function addLabel(event: SubmitEvent) {
    event.preventDefault();
    took(await api.POST("/api/v1/spaces/{space}/documents/{document}/labels", { params: { path }, body: { label } }));
    if (!error) label = "";
  }

  async function removeLabel(text: string) {
    const params = { path: { ...path, label: text } };
    took(await api.DELETE("/api/v1/spaces/{space}/documents/{document}/labels/{label}", { params }));
  }

  async function bin() {
    const route = document?.trashed
      ? "/api/v1/spaces/{space}/documents/{document}/restore"
      : "/api/v1/spaces/{space}/documents/{document}/trash";
    took(await api.POST(route, { params: { path } }));
  }

  async function addVersion(files: FileList) {
    const file = files[0];
    if (!file) return;
    const answer = await upload(`/api/v1/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(id)}/versions`, file);
    if (answer.ok) took({ data: answer.body as DocumentView });
    else error = (answer.body as { error?: string } | null)?.error ?? "";
  }

  function size(bytes: number): string {
    return bytes < 1024 * 1024 ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  $effect(() => {
    void [space, id];
    load();
  });

  // While it waits to be read, look again.
  $effect(() => {
    if (document?.reading !== "waiting" && document?.reading !== "reading") return;
    const timer = setInterval(load, 2000);
    return () => clearInterval(timer);
  });
</script>

<article>
  <button class="link" onclick={onclose}>← {t("doc.back")}</button>
  {#if error}<p class="error">{error}</p>{/if}
  <!-- On a device that may only read, every control that changes something is off. -->
  <fieldset class="plain" disabled={readOnly}>
  {#if document}
    {#if editingTitle !== null}
      <form
        class="row"
        onsubmit={(e) => {
          e.preventDefault();
          setTitle(editingTitle ?? "");
        }}
      >
        <input bind:value={editingTitle} aria-label={t("doc.title")} />
        <button type="submit">{t("doc.save")}</button>
        <button type="button" onclick={() => (editingTitle = null)}>{t("doc.cancel")}</button>
      </form>
    {:else}
      <h1>
        {document.title}
        <button class="link small" onclick={() => (editingTitle = document?.title ?? "")}>{t("doc.edit")}</button>
      </h1>
    {/if}
    {#if document.title_suggestion}
      <p class="suggestion">
        {t("doc.titleSuggestion", { title: document.title_suggestion })}
        <button onclick={() => setTitle(document?.title_suggestion ?? "")}>{t("doc.take")}</button>
      </p>
    {/if}
    {#if document.trashed}<p class="note">{t("doc.inBin")}</p>{/if}

    <dl>
      <dt>{t("doc.date")}</dt>
      <dd>{document.date}</dd>
      <dt>{t("doc.kind")}</dt>
      <dd>
        <select value={document.kind_confirmed ? document.kind : ""} onchange={(e) => setKind(e.currentTarget.value || null)}>
          <option value="">{document.kind_suggestion ? `${kindName(kinds, document.kind_suggestion)} (${t("doc.suggested")})` : t("doc.noKind")}</option>
          {#each byName(kinds) as k (k.id)}
            <option value={k.id}>{nameOf(k)}</option>
          {/each}
        </select>
        {#if document.kind_suggestion && !document.kind_confirmed}
          <button onclick={() => setKind(document?.kind_suggestion ?? null)}>{t("doc.confirm")}</button>
        {/if}
      </dd>
      <dt>{t("doc.reading")}</dt>
      <dd>
        <span class="reading {document.reading}">{t(`reading.${document.reading}`)}</span>
        {#each document.versions.slice(-1) as v (v.number)}
          {#if v.read?.ocr}· {t("reading.ocr")}{/if}
          {#if v.read?.unread}· {t("reading.unread", { unread: v.read.unread, pages: v.read.pages })}{/if}
          {#if v.read?.error}<span class="error"> · {v.read.error}</span>{/if}
        {/each}
      </dd>
    </dl>

    <h2>{t("doc.fields")}</h2>
    <dl class="fields">
      {#each fields as { key, type, field } (key)}
        <dt>{fieldName(kinds, document.kind, key)}</dt>
        <dd>
          {#if editing?.key === key}
            <form class="row" onsubmit={(e) => saveField(e, type)}>
              {#if type === "date"}
                <input type="date" bind:value={editing.text} aria-label={fieldName(kinds, document.kind, key)} />
              {:else}
                <input bind:value={editing.text} aria-label={fieldName(kinds, document.kind, key)} />
              {/if}
              {#if type === "money"}<input class="currency" bind:value={editing.currency} maxlength="3" aria-label="Currency" />{/if}
              <button type="submit">{t("doc.save")}</button>
              <button type="button" onclick={() => (editing = null)}>{t("doc.cancel")}</button>
            </form>
          {:else if type === "flag"}
            {@const value = field?.confirmed ?? field?.suggestion}
            <label class="check">
              <input
                type="checkbox"
                checked={value?.type === "flag" && value.value}
                onchange={(e) => setField(key, { type: "flag", value: e.currentTarget.checked })}
              />
              {#if field?.suggestion}<span class="suggested">({t("doc.suggested")})</span>{/if}
            </label>
          {:else}
            {#if field?.confirmed}{shown(field.confirmed, t("value.yes"), t("value.no"))}{/if}
            {#if field?.suggestion}
              <span class="suggested">{shown(field.suggestion, t("value.yes"), t("value.no"))} ({t("doc.suggested")})</span>
              <button class="small" onclick={() => field?.suggestion && setField(key, field.suggestion)}>{t("doc.confirm")}</button>
            {/if}
            <button class="link small" onclick={() => edit(key, field?.suggestion ?? field?.confirmed)}>{t("doc.edit")}</button>
            {#if field}<button class="link small" onclick={() => clearField(key)}>{t("doc.clear")}</button>{/if}
          {/if}
        </dd>
      {/each}
    </dl>

    <h2>{t("doc.labels")}</h2>
    <p>
      {#each document.labels as l (l)}
        <span class="label">{l} <button class="link small" aria-label={t("doc.removeLabel", { label: l })} onclick={() => removeLabel(l)}>×</button></span>
      {/each}
    </p>
    <form class="row" onsubmit={addLabel}>
      <input bind:value={label} placeholder={t("doc.addLabel")} aria-label={t("doc.addLabel")} />
      <button type="submit" disabled={!label.trim()}>+</button>
    </form>

    <h2>{t("doc.versions")}</h2>
    <ul class="versions">
      {#each [...document.versions].reverse() as v (v.number)}
        <li>
          <span>{t("doc.version", { n: v.number })}: {v.name} · {size(v.size)} · {new Date(v.added).toLocaleDateString()}</span>
          <span>
            <a href={fileUrl(space, id, v.number)} target="_blank" rel="noopener noreferrer">{t("doc.open")}</a>
            <a href={fileUrl(space, id, v.number, true)}>{t("doc.download")}</a>
          </span>
        </li>
      {/each}
    </ul>
    <button onclick={() => picker?.click()}>{t("doc.addVersion")}</button>
    <input
      class="hidden"
      type="file"
      bind:this={picker}
      onchange={(e) => {
        if (e.currentTarget.files) addVersion(e.currentTarget.files);
        e.currentTarget.value = "";
      }}
    />

    {#if pages?.length}
      <details>
        <summary>{t("doc.text")}</summary>
        {#each pages as page, i (i)}<pre>{page.text}</pre>{/each}
      </details>
    {/if}

    <p><button class="danger" onclick={bin}>{document.trashed ? t("doc.restore") : t("doc.trash")}</button></p>
  {/if}
  </fieldset>
</article>
