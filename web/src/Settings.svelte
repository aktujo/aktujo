<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, type Schemas } from "./lib/api";
  import { choose, i18n, languages, t } from "./lib/i18n.svelte";

  let { onclose, onjoined }: { onclose: () => void; onjoined: (space: string) => void } = $props();

  let status = $state<{ version: string; device: string; ocr: boolean; device_name?: string | null } | null>(null);
  let deviceName = $state("");
  let saved = $state(false);
  let invitation = $state("");
  let relay = $state("");
  let space = $state("");
  let code = $state("");
  let error = $state("");
  let busy = $state(false);
  let sources = $state<{ url: string; on: boolean; default: boolean }[]>([]);
  let watches = $state<Schemas["WatchView"][]>([]);
  let spaces = $state<Schemas["SpaceView"][]>([]);
  let watchSpace = $state("");
  let watchFolder = $state("");
  let watchMove = $state(false);
  let watchDeep = $state(false);

  async function loadWatches() {
    const [w, s] = await Promise.all([api.GET("/api/v1/watches"), api.GET("/api/v1/spaces")]);
    watches = w.data ?? [];
    spaces = s.data ?? [];
    if (!spaces.some((sp) => sp.id === watchSpace)) watchSpace = spaces[0]?.id ?? "";
  }

  async function addWatch(event: SubmitEvent) {
    event.preventDefault();
    const after = watchMove ? ("move" as const) : ("leave" as const);
    const body = { space: watchSpace, folder: watchFolder, after, subfolders: watchDeep };
    const { data, error: problem } = await api.POST("/api/v1/watches", { body });
    error = problem?.error ?? "";
    if (data) {
      watches = data;
      watchFolder = "";
    }
  }

  async function switchWatch(
    watch: Schemas["WatchView"],
    on: boolean,
    after: Schemas["AfterView"],
    subfolders: boolean = watch.subfolders,
  ) {
    const body = { space: watch.space, folder: watch.folder, on, after, subfolders };
    const { data, error: problem } = await api.PUT("/api/v1/watches", { body });
    error = problem?.error ?? "";
    if (data) watches = data;
  }

  async function removeWatch(watch: Schemas["WatchView"]) {
    if (!confirm(t("watch.removeConfirm", { folder: watch.folder }))) return;
    const { data, error: problem } = await api.DELETE("/api/v1/watches", {
      params: { query: { space: watch.space, folder: watch.folder } },
    });
    error = problem?.error ?? "";
    if (data) watches = data;
  }

  function spaceName(id: string): string {
    return spaces.find((sp) => sp.id === id)?.name ?? "…";
  }
  let newSource = $state("");

  async function loadSources() {
    const { data } = await api.GET("/api/v1/sources");
    sources = data ?? [];
  }

  async function addSource(event: SubmitEvent) {
    event.preventDefault();
    const { data, error: problem } = await api.POST("/api/v1/sources", { body: { url: newSource } });
    error = problem?.error ?? "";
    if (data) {
      sources = data;
      newSource = "";
    }
  }

  async function switchSource(url: string, on: boolean) {
    const { data, error: problem } = await api.PUT("/api/v1/sources", { body: { url, on } });
    error = problem?.error ?? "";
    if (data) sources = data;
  }

  async function removeSource(url: string) {
    if (!confirm(t("sources.removeConfirm", { source: url }))) return;
    const { data, error: problem } = await api.DELETE("/api/v1/sources", { params: { query: { url } } });
    error = problem?.error ?? "";
    if (data) sources = data;
  }

  async function load() {
    const { data } = await api.GET("/api/v1/status");
    status = data ?? null;
    deviceName = data?.device_name ?? "";
  }

  async function saveName(event: SubmitEvent) {
    event.preventDefault();
    const { error: problem } = await api.PUT("/api/v1/device", { body: { name: deviceName } });
    error = problem?.error ?? "";
    saved = !problem;
  }

  async function join(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/join", { body: { invitation } });
    busy = false;
    error = problem?.error ?? "";
    if (data) onjoined(data.id);
  }

  async function recover(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/recover", { body: { relay, space, code } });
    busy = false;
    error = problem?.error ?? "";
    if (data) onjoined(data.id);
  }

  let importFile = $state<File | null>(null);
  let importName = $state("");
  let importNote = $state("");

  async function importExport(event: SubmitEvent) {
    event.preventDefault();
    if (!importFile) return;
    busy = true;
    importNote = "";
    const query = importName.trim() ? `?name=${encodeURIComponent(importName.trim())}` : "";
    // Fetch, not openapi-fetch: the body is the ZIP as it is.
    const response = await fetch(`/api/v1/space-import${query}`, {
      method: "POST",
      headers: { "Content-Type": "application/zip" },
      body: importFile,
    });
    busy = false;
    const answer = (await response.json().catch(() => null)) as
      | (Schemas["SpaceImported"] & { error?: string })
      | null;
    if (!response.ok || !answer) {
      error = answer?.error ?? response.statusText;
      return;
    }
    error = "";
    importNote = t("import.done", { documents: answer.documents, kinds: answer.kinds });
    if (answer.missing) importNote += " " + t("import.missing", { n: answer.missing });
    if (answer.skipped.length) importNote += " " + t("import.skipped", { what: answer.skipped.join("; ") });
    importFile = null;
    importName = "";
    onjoined(answer.space);
  }

  $effect(() => {
    load();
    loadSources();
    loadWatches();
  });

  // How the watched folders were looked at shows as it happens.
  $effect(() => {
    if (!watches.some((w) => w.on)) return;
    const timer = setInterval(loadWatches, 5000);
    return () => clearInterval(timer);
  });
</script>

<article class="settings">
  <button class="link" onclick={onclose}>← {t("doc.back")}</button>
  <h1>{t("settings")}</h1>
  {#if error}<p class="error">{error}</p>{/if}

  <h2>{t("language.choose")}</h2>
  <div class="choices" role="radiogroup" aria-label={t("language.choose")}>
    {#each Object.entries(languages) as [code, texts] (code)}
      <label class="check">
        <input type="radio" name="language" value={code} checked={i18n.language === code} onchange={() => choose(code)} />
        {texts.language}
      </label>
    {/each}
  </div>

  <h2>{t("settings.deviceName")}</h2>
  <form class="row" onsubmit={saveName}>
    <input bind:value={deviceName} oninput={() => (saved = false)} aria-label={t("settings.deviceName")} />
    <button type="submit" disabled={!deviceName.trim()}>{saved ? "✓" : t("doc.save")}</button>
  </form>
  <p class="note">{t("settings.deviceNameHint")}</p>

  <h2>{t("settings.join")}</h2>
  <p class="note">{t("settings.joinHint")}</p>
  <form class="row" onsubmit={join}>
    <input class="wide" bind:value={invitation} aria-label={t("sync.invitationField")} required />
    <button type="submit" disabled={busy || !invitation.trim()}>{t("sync.join")}</button>
  </form>

  <h2>{t("settings.recover")}</h2>
  <p class="note">{t("settings.recoverHint")}</p>
  <form class="stack" onsubmit={recover}>
    <input bind:value={relay} placeholder="http://192.168.1.10:8072" aria-label={t("sync.relay")} required />
    <input bind:value={space} placeholder={t("settings.space")} aria-label={t("settings.space")} required />
    <input bind:value={code} placeholder={t("sync.code")} aria-label={t("sync.code")} required />
    <button type="submit" disabled={busy}>{t("sync.recover")}</button>
  </form>

  <h2>{t("import.title")}</h2>
  <p class="note">{t("import.hint")}</p>
  <form class="stack" onsubmit={importExport}>
    <input
      type="file"
      accept=".zip,application/zip"
      aria-label={t("import.file")}
      onchange={(e) => (importFile = e.currentTarget.files?.[0] ?? null)}
      required
    />
    <input bind:value={importName} placeholder={t("import.name")} aria-label={t("import.name")} />
    <button type="submit" disabled={busy || !importFile}>{t("import.start")}</button>
  </form>
  {#if importNote}<p class="note">{importNote}</p>{/if}

  <h2>{t("watch.title")}</h2>
  <p class="note">{t("watch.hint")}</p>
  {#if watches.length}
    <ul class="versions">
      {#each watches as watch (`${watch.space} ${watch.folder}`)}
        <li>
          <span class="watch">
            <label class="check">
              <input type="checkbox" checked={watch.on} onchange={(e) => switchWatch(watch, e.currentTarget.checked, watch.after)} />
              <code>{watch.folder}</code> → {spaceName(watch.space)}
            </label>
            <span class="note">
              {watch.after === "move" ? t("watch.move") : t("watch.leave")}
              · {watch.subfolders ? t("watch.deep") : t("watch.flat")}
              {#if watch.looked_at}· {t("watch.taken", { n: watch.taken, time: new Date(watch.looked_at).toLocaleTimeString() })}{/if}
            </span>
            {#if watch.problem}<span class="error">{watch.problem}</span>{/if}
          </span>
          <span class="row">
            <button class="small" onclick={() => switchWatch(watch, watch.on, watch.after === "move" ? "leave" : "move")}>
              {watch.after === "move" ? t("watch.toLeave") : t("watch.toMove")}
            </button>
            <button class="small" onclick={() => switchWatch(watch, watch.on, watch.after, !watch.subfolders)}>
              {watch.subfolders ? t("watch.toFlat") : t("watch.toDeep")}
            </button>
            <button class="danger small" onclick={() => removeWatch(watch)}>{t("watch.remove")}</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if spaces.length}
    <form class="stack" onsubmit={addWatch}>
      <input bind:value={watchFolder} placeholder={t("watch.folderPlaceholder")} aria-label={t("watch.folder")} required />
      <select bind:value={watchSpace} aria-label={t("watch.space")}>
        {#each spaces as sp (sp.id)}<option value={sp.id}>{sp.name ?? "…"}</option>{/each}
      </select>
      <label class="check"><input type="checkbox" bind:checked={watchDeep} /> {t("watch.deepOption")}</label>
      <label class="check"><input type="checkbox" bind:checked={watchMove} /> {t("watch.moveOption")}</label>
      <button type="submit" disabled={!watchFolder.trim() || !watchSpace}>{t("watch.add")}</button>
    </form>
  {/if}

  <h2>{t("sources.title")}</h2>
  <p class="note">{t("sources.hint")}</p>
  <ul class="versions">
    {#each sources as source (source.url)}
      <li>
        <label class="check">
          <input type="checkbox" checked={source.on} onchange={(e) => switchSource(source.url, e.currentTarget.checked)} />
          {source.url}{#if source.default}&nbsp;<span class="note">({t("sources.official")})</span>{/if}
        </label>
        {#if !source.default}
          <button class="danger small" onclick={() => removeSource(source.url)}>{t("sources.remove")}</button>
        {/if}
      </li>
    {/each}
  </ul>
  <form class="row" onsubmit={addSource}>
    <input class="wide" bind:value={newSource} placeholder="https://github.com/…/catalog" aria-label={t("sources.add")} />
    <button type="submit" disabled={!newSource.trim()}>{t("sources.add")}</button>
  </form>

  {#if status}
    <h2>{t("settings.about")}</h2>
    <dl>
      <dt>{t("settings.version")}</dt>
      <dd>aktujo {status.version}</dd>
      <dt>{t("settings.device")}</dt>
      <dd>{status.device}</dd>
      <dt>{t("settings.ocr")}</dt>
      <dd>{status.ocr ? t("value.yes") : t("value.no")}</dd>
    </dl>
  {/if}
</article>
