<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api } from "./lib/api";
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

  $effect(() => {
    load();
    loadSources();
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
