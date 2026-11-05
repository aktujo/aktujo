<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api } from "./lib/api";
  import { choose, i18n, languages, t } from "./lib/i18n.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let status = $state<{ version: string; device: string; ocr: boolean } | null>(null);

  $effect(() => {
    api.GET("/api/v1/status").then(({ data }) => (status = data ?? null));
  });
</script>

<article class="settings">
  <button class="link" onclick={onclose}>← {t("doc.back")}</button>
  <h1>{t("settings")}</h1>

  <h2>{t("language.choose")}</h2>
  <div class="choices" role="radiogroup" aria-label={t("language.choose")}>
    {#each Object.entries(languages) as [code, texts] (code)}
      <label class="check">
        <input type="radio" name="language" value={code} checked={i18n.language === code} onchange={() => choose(code)} />
        {texts.language}
      </label>
    {/each}
  </div>

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
