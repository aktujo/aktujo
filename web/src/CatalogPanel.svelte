<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, type Schemas, type Space } from "./lib/api";
  import { i18n, nameOf, t } from "./lib/i18n.svelte";

  type Listing = Schemas["TemplateListing"];
  type Plan = Schemas["PlanView"];
  type Planned = Schemas["PlannedView"];
  type Choice = Schemas["ChoiceView"];

  let { space, onclose, onchange }: { space: Space; onclose: () => void; onchange: () => void } = $props();

  let sources = $state<Schemas["ReadSource"][]>([]);
  let templates = $state<Listing[]>([]);
  let tag = $state("");
  let loading = $state(false);
  let error = $state("");
  let message = $state("");
  let chosen = $state<Listing | null>(null);
  let plan = $state<Plan | null>(null);
  let choices = $state<Record<string, Choice>>({});
  let busy = $state(false);

  const path = $derived({ space: space.id });
  const tags = $derived([...new Set(templates.flatMap((tl) => tl.tags))].sort());
  const shown = $derived(
    [...templates]
      .filter((tl) => !tag || tl.tags.includes(tag))
      .sort((a, b) => Number(b.standing === "update") - Number(a.standing === "update") || Number(b.official) - Number(a.official)),
  );
  const updates = $derived(templates.filter((tl) => tl.standing === "update").length);

  function textIn(text: string, texts: Record<string, string> | undefined): string {
    return texts?.[i18n.language] ?? text;
  }

  async function load() {
    loading = true;
    const { data, error: problem } = await api.GET("/api/v1/spaces/{space}/catalog", { params: { path } });
    loading = false;
    error = problem?.error ?? "";
    sources = data?.sources ?? [];
    templates = data?.templates ?? [];
  }

  async function open(listing: Listing) {
    chosen = listing;
    plan = null;
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/catalog/plan", {
      params: { path },
      body: { source: listing.source, template: listing.id },
    });
    busy = false;
    error = problem?.error ?? "";
    plan = data ?? null;
    choices = Object.fromEntries((data?.kinds ?? []).map((k) => [k.id, "take" as Choice]));
  }

  /** The choices that make sense for a step; the first is what happens without one. */
  function optionsOf(kind: Planned): Choice[] {
    switch (kind.step) {
      case "new":
        return ["take", "keep"];
      case "same":
        return [];
      case "replace":
        return ["take", "keep"];
      case "merge":
        return ["take", "theirs", "keep"];
      case "gone":
        return ["keep", "remove"];
    }
  }

  function choiceText(kind: Planned, choice: Choice): string {
    if (kind.step === "new") return t(choice === "take" ? "catalog.add" : "catalog.skip");
    if (kind.step === "gone") return t(choice === "remove" ? "catalog.remove" : "catalog.keepMine");
    if (choice === "take") return t(kind.step === "merge" ? "catalog.merge" : "catalog.replace");
    return t(choice === "theirs" ? "catalog.theirs" : "catalog.keepMine");
  }

  function described(value: unknown): string {
    if (value === null || value === undefined) return t("catalog.absent");
    if (typeof value === "string" || typeof value === "number") return String(value);
    if (Array.isArray(value)) return value.map((v) => (Array.isArray(v) ? v.join(" / ") : String(v))).join(", ");
    const named = value as { name?: string; type?: string };
    return named.name ? `${named.name}${named.type ? ` (${t(`type.${named.type}`)})` : ""}` : JSON.stringify(value);
  }

  async function take() {
    if (!chosen || !plan) return;
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/catalog/take", {
      params: { path },
      body: { source: chosen.source, template: chosen.id, choices },
    });
    busy = false;
    error = problem?.error ?? "";
    if (data) {
      message = t("kinds.imported", { n: data.changed });
      chosen = null;
      plan = null;
      onchange();
      await load();
    }
  }

  // Read when this view is opened – never otherwise.
  $effect(() => {
    void space.id;
    load();
  });
</script>

<section class="catalog">
  <p class="row">
    <button class="link" onclick={onclose}>← {t("kinds.title")}</button>
    <button class="small" onclick={load} disabled={loading}>{t("catalog.reload")}</button>
  </p>
  <h2>{t("catalog.title")}</h2>
  <p class="note">{t("catalog.hint")}</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if message}<p class="note" aria-live="polite">{message}</p>{/if}
  {#each sources.filter((s) => s.error) as source (source.url)}
    <p class="error">{t("catalog.unreadable", { source: source.url, problem: source.error ?? "" })}</p>
  {/each}

  {#if plan && chosen}
    <h3>{textIn(chosen.name, chosen.names)}</h3>
    <p class="note">{chosen.official ? t("catalog.official") : t("catalog.notReviewed", { source: chosen.source })}</p>
    <ul class="plan">
      {#each plan.kinds as kind (kind.id)}
        {@const named = kind.theirs ?? kind.mine}
        <li>
          <div class="row spread">
            <strong>{named ? nameOf(named) : kind.id}</strong>
            <span class="step {kind.step}">{t(`catalog.step.${kind.step}`)}</span>
          </div>
          {#if optionsOf(kind).length}
            <div class="choices" role="radiogroup" aria-label={named ? nameOf(named) : kind.id}>
              {#each optionsOf(kind) as option (option)}
                <label class="check">
                  <input type="radio" name="choice-{kind.id}" value={option} bind:group={choices[kind.id]} />
                  {choiceText(kind, option)}
                </label>
              {/each}
            </div>
          {/if}
          {#if kind.conflicts.length}
            <p class="note">{t("catalog.conflicts")}</p>
            <table class="conflicts">
              <thead><tr><th>{t("catalog.item")}</th><th>{t("catalog.mine")}</th><th>{t("catalog.theirs.column")}</th></tr></thead>
              <tbody>
                {#each kind.conflicts as conflict (conflict.item)}
                  <tr><td>{conflict.item}</td><td>{described(conflict.mine)}</td><td>{described(conflict.theirs)}</td></tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </li>
      {/each}
    </ul>
    <p class="row">
      <button class="primary" onclick={take} disabled={busy}>{t("catalog.take")}</button>
      <button onclick={() => ((plan = null), (chosen = null))}>{t("doc.cancel")}</button>
    </p>
  {:else}
    {#if loading}<p class="note">{t("catalog.reading")}</p>{/if}
    {#if updates}<p>{t("catalog.updates", { n: updates })}</p>{/if}
    {#if tags.length > 1}
      <label>
        {t("catalog.tag")}
        <select bind:value={tag}>
          <option value="">{t("catalog.anyTag")}</option>
          {#each tags as tg (tg)}<option value={tg}>{tg}</option>{/each}
        </select>
      </label>
    {/if}
    {#if !loading && !shown.length && !sources.some((s) => s.error)}<p>{t("catalog.empty")}</p>{/if}
    <ul class="templates">
      {#each shown as listing (`${listing.source} ${listing.id}`)}
        <li>
          <div class="row spread">
            <strong>{textIn(listing.name, listing.names)}</strong>
            <span class="row">
              {#if listing.standing !== "new"}<span class="step {listing.standing}">{t(`catalog.standing.${listing.standing}`)}</span>{/if}
              <button onclick={() => open(listing)} disabled={busy}>
                {listing.standing === "update" ? t("catalog.showUpdate") : t("catalog.open")}
              </button>
            </span>
          </div>
          <p>{textIn(listing.description, listing.descriptions)}</p>
          <p class="note">
            {listing.kinds.map((k) => nameOf(k)).join(", ")}
            · {listing.official ? t("catalog.official") : listing.author}
            {#if listing.license}· {listing.license}{/if}
          </p>
        </li>
      {/each}
    </ul>
  {/if}
</section>
