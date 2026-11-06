<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, type DocumentView, type Schemas } from "./lib/api";
  import { t } from "./lib/i18n.svelte";

  type Improve = Schemas["ImproveView"];
  type Colour = Schemas["ColourView"];

  let {
    space,
    document,
    onclose,
    ondone,
  }: { space: string; document: DocumentView; onclose: () => void; ondone: (view: DocumentView) => void } = $props();

  const COLOURS: Colour[] = ["keep", "grey", "black_white"];
  const number = $derived(document.versions.length);
  const base = $derived(
    `/api/v1/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(document.id)}`,
  );

  let count = $state(0);
  /** The pages of the result: which page of the version, in their order. */
  let order = $state<number[]>([]);
  let turns = $state<number[]>([]);
  let deskew = $state(false);
  let colour = $state<Colour>("keep");
  let crop = $state({ left: 0, top: 0, right: 0, bottom: 0 });
  let shown = $state(0);
  let preview = $state<string | null>(null);
  let error = $state("");
  let busy = $state(false);

  const steps = $derived<Improve>({
    pages: order,
    turns,
    deskew,
    colour,
    crop: crop.left || crop.top || crop.right || crop.bottom
      ? { left: crop.left / 100, top: crop.top / 100, right: crop.right / 100, bottom: crop.bottom / 100 }
      : null,
  });

  async function load() {
    const { data, error: problem } = await api.GET(
      "/api/v1/spaces/{space}/documents/{document}/versions/{number}/pages",
      { params: { path: { space, document: document.id, number } } },
    );
    error = problem?.error ?? "";
    count = data?.pages ?? 0;
    order = Array.from({ length: count }, (_, i) => i);
    turns = Array.from({ length: count }, () => 0);
  }

  async function show() {
    if (!order.length) {
      preview = null;
      return;
    }
    const page = Math.min(shown, order.length - 1);
    // Fetch, not openapi-fetch: the answer is a picture.
    const response = await fetch(`${base}/improve/preview?page=${page}&width=900`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(steps),
    });
    if (!response.ok) {
      error = ((await response.json().catch(() => null)) as { error?: string } | null)?.error ?? response.statusText;
      return;
    }
    error = "";
    // As a data address: the web app's policy loads pictures only from itself and as data.
    const blob = await response.blob();
    preview = await new Promise<string>((done) => {
      const reader = new FileReader();
      reader.onload = () => done(String(reader.result));
      reader.readAsDataURL(blob);
    });
  }

  function turn(page: number) {
    turns[page] = ((turns[page] ?? 0) + 1) % 4;
  }

  function move(at: number, by: number) {
    const to = at + by;
    if (to < 0 || to >= order.length) return;
    const next = [...order];
    [next[at], next[to]] = [next[to]!, next[at]!];
    order = next;
  }

  function leaveOut(at: number) {
    order = order.filter((_, i) => i !== at);
    shown = Math.min(shown, Math.max(order.length - 1, 0));
  }

  async function save() {
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/documents/{document}/improve", {
      params: { path: { space, document: document.id } },
      body: steps,
    });
    busy = false;
    error = problem?.error ?? "";
    if (data) ondone(data);
  }

  $effect(() => {
    void [document.id, number];
    load();
  });
  // The preview follows every change, after a pause.
  $effect(() => {
    void [JSON.stringify(steps), shown, count];
    const timer = setTimeout(show, 300);
    return () => clearTimeout(timer);
  });
</script>

<section class="improve">
  <h2>{t("improve.title")}: {document.title}</h2>
  <p class="note">{t("improve.hint")}</p>
  {#if error}<p class="error">{error}</p>{/if}

  <div class="improve-grid">
    <div>
      <h3>{t("improve.pages")}</h3>
      <ol class="pages">
        {#each order as page, at (page)}
          <li class:active={at === shown}>
            <button class="thumb" onclick={() => (shown = at)} aria-label={t("improve.show", { n: page + 1 })}>
              <img
                src={`${base}/versions/${number}/pages/${page}?width=160`}
                alt={t("improve.page", { n: page + 1 })}
                style:transform={`rotate(${(turns[page] ?? 0) * 90}deg)`}
              />
            </button>
            <span class="row">
              <span class="note">{t("improve.page", { n: page + 1 })}</span>
              <button class="small" onclick={() => turn(page)} aria-label={t("improve.turn")}>↻</button>
              <button class="small" onclick={() => move(at, -1)} disabled={at === 0} aria-label={t("kinds.up")}>↑</button>
              <button class="small" onclick={() => move(at, 1)} disabled={at === order.length - 1} aria-label={t("kinds.down")}>↓</button>
              <button class="small danger" onclick={() => leaveOut(at)} disabled={order.length === 1} aria-label={t("improve.leaveOut")}>×</button>
            </span>
          </li>
        {/each}
      </ol>
      {#if order.length < count}
        <button class="link small" onclick={load}>{t("improve.allPages")}</button>
      {/if}

      <h3>{t("improve.steps")}</h3>
      <label class="check"><input type="checkbox" bind:checked={deskew} /> {t("improve.deskew")}</label>
      <label>
        {t("improve.colour")}
        <select bind:value={colour}>
          {#each COLOURS as c (c)}<option value={c}>{t(`improve.colour.${c}`)}</option>{/each}
        </select>
      </label>
      <fieldset class="crop">
        <legend>{t("improve.crop")}</legend>
        {#each ["left", "top", "right", "bottom"] as const as side (side)}
          <label>
            {t(`improve.crop.${side}`)}
            <input type="number" min="0" max="45" step="1" bind:value={crop[side]} />&nbsp;%
          </label>
        {/each}
      </fieldset>
      <p class="row">
        <button class="primary" onclick={save} disabled={busy || !order.length}>{t("improve.save")}</button>
        <button onclick={onclose}>{t("doc.cancel")}</button>
      </p>
    </div>
    <figure class="preview">
      {#if preview}<img src={preview} alt={t("improve.preview")} />{:else}<p class="note">…</p>{/if}
      <figcaption class="note">{t("improve.preview")} · {t("improve.page", { n: shown + 1 })}</figcaption>
    </figure>
  </div>
</section>
