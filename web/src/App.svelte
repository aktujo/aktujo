<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, upload, type Kind, type Space, type Summary } from "./lib/api";
  import { i18n, kindName, t } from "./lib/i18n.svelte";
  import { setTheme, theme, type Theme } from "./lib/theme.svelte";
  import DocumentPanel from "./DocumentPanel.svelte";
  import Settings from "./Settings.svelte";
  import SpacePanel from "./SpacePanel.svelte";

  const THEMES: Theme[] = ["auto", "light", "dark"];

  type Bin = "outside" | "inside" | "both";

  let spaces = $state<Space[]>([]);
  let space = $state<string | null>(null);
  let kinds = $state<Kind[]>([]);
  let labels = $state<{ label: string; documents: number }[]>([]);
  let documents = $state<Summary[]>([]);
  let chosen = $state<string | null>(null);
  let loaded = $state(false);

  let words = $state("");
  let kind = $state("");
  let wanted = $state<string[]>([]);
  let from = $state("");
  let to = $state("");
  let bin = $state<Bin>("outside");

  let newSpace = $state("");
  let message = $state("");
  let error = $state("");
  let dragging = $state(false);
  let ocr = $state(true);
  let readingProblem = $state<string | null>(null);
  let picker = $state<HTMLInputElement>();
  let settings = $state(false);
  let sharing = $state(false);
  let device = $state("");

  const current = $derived(spaces.find((s) => s.id === space) ?? null);
  const readOnly = $derived(current?.rights === "read" || current?.waiting === true);
  const synchronised = $derived(spaces.some((s) => s.relay));
  const busy = $derived(documents.some((d) => d.reading === "waiting" || d.reading === "reading"));

  async function loadSpaces() {
    const { data } = await api.GET("/api/v1/spaces");
    spaces = data ?? [];
    if (!spaces.some((s) => s.id === space)) space = spaces[0]?.id ?? null;
    loaded = true;
  }

  async function loadLabels() {
    if (!space) return;
    const { data } = await api.GET("/api/v1/spaces/{space}/labels", { params: { path: { space } } });
    labels = data ?? [];
    wanted = wanted.filter((w) => labels.some((l) => l.label === w));
  }

  async function search() {
    if (!space) {
      documents = [];
      return;
    }
    const { data, error: problem } = await api.POST("/api/v1/search", {
      body: {
        words,
        spaces: [space],
        kind: kind || null,
        labels: wanted,
        from: from || null,
        to: to || null,
        bin,
        limit: null,
      },
    });
    error = problem?.error ?? "";
    documents = data ?? [];
  }

  async function refresh() {
    await loadSpaces();
    await Promise.all([loadLabels(), search()]);
  }

  async function foundSpace(event: SubmitEvent) {
    event.preventDefault();
    const { data, error: problem } = await api.POST("/api/v1/spaces", { body: { name: newSpace } });
    error = problem?.error ?? "";
    if (data) {
      newSpace = "";
      space = data.id;
      await refresh();
    }
  }

  async function add(files: FileList | File[]) {
    if (!space) return;
    let added = 0;
    let known = 0;
    for (const file of files) {
      const { ok, body } = await upload(`/api/v1/spaces/${encodeURIComponent(space)}/documents`, file);
      if (!ok) {
        error = `${file.name}: ${(body as { error?: string } | null)?.error ?? ""}`;
        continue;
      }
      if ((body as { new: boolean }).new) added++;
      else known++;
    }
    message = [added ? t("doc.added", { n: added }) : "", known ? t("doc.known", { n: known }) : ""]
      .filter(Boolean)
      .join(", ");
    await refresh();
  }

  function dropped(event: DragEvent) {
    event.preventDefault();
    dragging = false;
    if (readOnly) return;
    if (event.dataTransfer?.files.length) add(event.dataTransfer.files);
  }

  function resetFilters() {
    kind = "";
    wanted = [];
    from = "";
    to = "";
  }

  async function loadStatus() {
    const { data } = await api.GET("/api/v1/status");
    ocr = data?.ocr ?? true;
    device = data?.device ?? "";
    readingProblem = data?.reading_problem ?? null;
  }

  // The first loading, and again while documents wait to be read.
  $effect(() => {
    api.GET("/api/v1/kinds").then(({ data }) => (kinds = data ?? []));
    loadStatus();
    refresh();
  });

  $effect(() => {
    if (!busy) return;
    const timer = setInterval(() => {
      search();
      loadStatus();
    }, 2000);
    return () => clearInterval(timer);
  });

  // What other devices sent arrives through the sync in the background: look again now and then.
  $effect(() => {
    if (!synchronised) return;
    const timer = setInterval(refresh, 15000);
    return () => clearInterval(timer);
  });

  // A change of the space or a filter searches again; words are searched as they are typed, after a pause.
  $effect(() => {
    void [space, kind, wanted.length, from, to, bin];
    search();
  });
  $effect(() => {
    void words;
    const timer = setTimeout(search, 250);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    void space;
    chosen = null;
    loadLabels();
  });
  $effect(() => {
    document.documentElement.lang = i18n.language;
  });
</script>

<svelte:window
  ondragover={(e) => {
    e.preventDefault();
    if (space) dragging = true;
  }}
  ondragleave={(e) => {
    if (!e.relatedTarget) dragging = false;
  }}
  ondrop={dropped}
/>

<header>
  <span class="brand">
    <img class="on-dark" src="/aktujo-lockup-on-dark.svg" alt="aktujo" />
    <img class="on-light" src="/aktujo-lockup-on-light.svg" alt="aktujo" />
  </span>
  <span class="tagline">{t("tagline")}</span>
  <div class="theme" role="group" aria-label={t("theme")}>
    {#each THEMES as value (value)}
      <button
        type="button"
        aria-pressed={theme.value === value}
        title={t(`theme.${value}`)}
        aria-label={t(`theme.${value}`)}
        onclick={() => setTheme(value)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          {#if value === "auto"}
            <rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" />
          {:else if value === "light"}
            <circle cx="12" cy="12" r="4" /><path
              d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"
            />
          {:else}
            <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" />
          {/if}
        </svg>
      </button>
    {/each}
  </div>
  <button
    type="button"
    class="settings-button"
    class:active={settings}
    title={t("settings")}
    aria-label={t("settings")}
    onclick={() => (settings = !settings)}
  >
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="3" /><path
        d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"
      />
    </svg>
  </button>
</header>

{#if settings}
  <main class="first">
    <Settings
      onclose={() => (settings = false)}
      onjoined={async (id) => {
        settings = false;
        space = id;
        sharing = true;
        await refresh();
      }}
    />
  </main>
{:else if loaded && spaces.length === 0}
  <main class="first">
    <p>{t("space.first")}</p>
    <form onsubmit={foundSpace}>
      <input bind:value={newSpace} placeholder={t("space.name")} aria-label={t("space.name")} required />
      <button type="submit">{t("space.found")}</button>
    </form>
    {#if error}<p class="error">{error}</p>{/if}
    <p><button class="link" onclick={() => (settings = true)}>{t("settings.join")}</button></p>
  </main>
{:else if loaded}
  <div class="layout">
    <nav>
      <h2>{t("spaces")}</h2>
      <ul class="spaces">
        {#each spaces as s (s.id)}
          <li>
            <button class:active={s.id === space} onclick={() => (space = s.id)}>
              <span>
                {s.name ?? "…"}
                {#if s.sync_problem}<span class="sync-mark" title={t("sync.problem", { problem: s.sync_problem })}>!</span>
                {:else if s.relay}<span class="sync-mark" title={t("sync.via", { relay: s.relay })}>⇅</span>{/if}
              </span>
              <span class="count">{s.documents}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if current}
        <button
          class="link"
          class:active-link={sharing}
          onclick={() => {
            sharing = !sharing;
            chosen = null;
          }}>{t("sync.title")}</button
        >
      {/if}
      <form class="new-space" onsubmit={foundSpace}>
        <input bind:value={newSpace} placeholder={t("space.new")} aria-label={t("space.new")} />
        <button type="submit" disabled={!newSpace.trim()}>+</button>
      </form>

      <div class="bin" role="group">
        {#each ["outside", "inside", "both"] as const as which (which)}
          <button class:active={bin === which} onclick={() => (bin = which)}>{t(`bin.${which}`)}</button>
        {/each}
      </div>

      <label>
        {t("filter.kind")}
        <select bind:value={kind}>
          <option value="">{t("filter.anyKind")}</option>
          {#each kinds as k (k.id)}
            <option value={k.id}>{kindName(k.id)}</option>
          {/each}
        </select>
      </label>
      <div class="dates">
        <label>{t("filter.from")} <input type="date" bind:value={from} /></label>
        <label>{t("filter.to")} <input type="date" bind:value={to} /></label>
      </div>
      {#if labels.length}
        <fieldset>
          <legend>{t("filter.labels")}</legend>
          {#each labels as l (l.label)}
            <label class="check">
              <input type="checkbox" value={l.label} bind:group={wanted} />
              {l.label} <span class="count">{l.documents}</span>
            </label>
          {/each}
        </fieldset>
      {/if}
      {#if kind || wanted.length || from || to}
        <button class="link" onclick={resetFilters}>{t("filter.reset")}</button>
      {/if}
    </nav>

    <main>
      <div class="bar">
        <input
          type="search"
          class="words"
          bind:value={words}
          placeholder={t("search.placeholder")}
          aria-label={t("search")}
        />
        <button class="primary" onclick={() => picker?.click()} disabled={!space || readOnly}>{t("doc.add")}</button>
        <input
          class="hidden"
          type="file"
          multiple
          bind:this={picker}
          onchange={(e) => {
            if (e.currentTarget.files) add(e.currentTarget.files);
            e.currentTarget.value = "";
          }}
        />
      </div>
      {#if !ocr}<p class="note">{t("reading.noOcr")}</p>{/if}
      {#if current?.waiting}<p class="note">{t("sync.waiting")}</p>
      {:else if readOnly}<p class="note">{t("sync.readOnly")}</p>{/if}
      {#if readingProblem}<p class="error">{t("reading.problem", { problem: readingProblem })}</p>{/if}
      {#if message}<p class="note">{message}</p>{/if}
      {#if error}<p class="error">{error}</p>{/if}

      {#if sharing && current}
        <SpacePanel space={current} {device} onclose={() => (sharing = false)} onchange={refresh} />
      {:else if chosen && current}
        <DocumentPanel
          space={current.id}
          id={chosen}
          {kinds}
          {readOnly}
          onclose={() => (chosen = null)}
          onchange={refresh}
        />
      {:else}
        <p class="count">{t("doc.found", { n: documents.length })}</p>
        {#if documents.length === 0}
          <p class="empty">{t("doc.none")}</p>
        {/if}
        <ul class="documents">
          {#each documents as d (d.id)}
            <li>
              <button class="document" onclick={() => (chosen = d.id)}>
                <span class="title">{d.title}</span>
                <span class="meta">
                  {d.date}
                  {#if d.kind}· {kindName(d.kind)}{#if !d.kind_confirmed}&nbsp;({t("doc.suggested")}){/if}{/if}
                  {#if d.reading !== "done"}· <span class="reading {d.reading}">{t(`reading.${d.reading}`)}</span>{/if}
                  {#each d.labels as label (label)}<span class="label">{label}</span>{/each}
                </span>
                {#if d.excerpt.length}
                  <span class="excerpt">
                    {#each d.excerpt as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
                  </span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </main>
  </div>
  {#if dragging}
    <div class="drop">{t("doc.drop")}</div>
  {/if}
{/if}
