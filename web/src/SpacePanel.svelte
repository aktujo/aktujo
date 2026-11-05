<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api, type Space } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import type { components } from "./lib/api-types";

  type Member = components["schemas"]["MemberView"];
  type Rights = components["schemas"]["RightsView"];

  let { space, device, onclose, onchange }: { space: Space; device: string; onclose: () => void; onchange: () => void } =
    $props();

  let name = $state("");
  let relay = $state("");
  let code = $state<string | null>(null);
  let rights = $state<Rights>("write");
  let invitation = $state<string | null>(null);
  let copied = $state(false);
  let members = $state<Member[]>([]);
  let report = $state("");
  let error = $state("");
  let busy = $state(false);

  const manages = $derived(space.rights === "manage" || space.relay == null);
  const path = $derived({ space: space.id });

  async function loadMembers() {
    if (!space.relay || space.waiting) {
      members = [];
      return;
    }
    const { data, error: problem } = await api.GET("/api/v1/spaces/{space}/members", { params: { path } });
    members = data ?? [];
    if (problem) error = problem.error;
  }

  async function rename(event: SubmitEvent) {
    event.preventDefault();
    const { error: problem } = await api.PUT("/api/v1/spaces/{space}", { params: { path }, body: { name } });
    error = problem?.error ?? "";
    onchange();
  }

  async function start(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/sync/start", {
      params: { path },
      body: { relay },
    });
    busy = false;
    error = problem?.error ?? "";
    if (data) {
      code = data.recovery_code;
      onchange();
    }
  }

  async function syncNow() {
    busy = true;
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/sync", { params: { path } });
    busy = false;
    error = problem?.error ?? "";
    if (data) {
      report = t("sync.report", { received: data.received, sent: data.sent });
      if (data.added.length) report += ` · ${t("sync.added", { names: data.added.join(", ") })}`;
    }
    onchange();
    loadMembers();
  }

  async function invite(event: SubmitEvent) {
    event.preventDefault();
    const { data, error: problem } = await api.POST("/api/v1/spaces/{space}/invitations", {
      params: { path },
      body: { rights },
    });
    error = problem?.error ?? "";
    invitation = data?.invitation ?? null;
    copied = false;
  }

  async function copy() {
    if (!invitation) return;
    try {
      await navigator.clipboard.writeText(invitation);
      copied = true;
    } catch {
      copied = false;
    }
  }

  function label(member: Member): string {
    if (member.role === "recovery") return t("sync.code");
    return member.name ?? member.device ?? "…";
  }

  async function remove(member: Member) {
    if (!confirm(t("sync.removeConfirm", { name: label(member) }))) return;
    busy = true;
    const { error: problem } = await api.DELETE("/api/v1/spaces/{space}/members/{member}", {
      params: { path: { space: space.id, member: member.id } },
    });
    busy = false;
    error = problem?.error ?? "";
    loadMembers();
  }

  function time(ms: number): string {
    return new Date(ms).toLocaleString();
  }

  $effect(() => {
    name = space.name ?? "";
  });
  $effect(() => {
    void [space.id, space.relay, space.waiting];
    loadMembers();
  });
</script>

<article>
  <button class="link" onclick={onclose}>← {t("doc.back")}</button>
  <h1>{t("sync.title")}</h1>
  {#if error}<p class="error">{error}</p>{/if}

  {#if manages}
    <form class="row" onsubmit={rename}>
      <input bind:value={name} aria-label={t("space.name")} />
      <button type="submit" disabled={!name.trim() || name === space.name}>{t("space.rename")}</button>
    </form>
  {:else}
    <h2>{space.name ?? "…"}</h2>
  {/if}

  {#if code}
    <section class="code" aria-live="polite">
      <h2>{t("sync.code")}</h2>
      <p>{t("sync.codeHint")}</p>
      <pre>{code}</pre>
      <button onclick={() => (code = null)}>{t("sync.codeDone")}</button>
    </section>
  {:else if !space.relay}
    <p>{t("sync.notYet")}</p>
    <form class="row" onsubmit={start}>
      <input bind:value={relay} placeholder="http://192.168.1.10:8072" aria-label={t("sync.relay")} required />
      <button type="submit" disabled={busy || !relay.trim()}>{t("sync.start")}</button>
    </form>
    <p class="note">{t("sync.relayHint")}</p>
  {:else}
    <p>{t("sync.via", { relay: space.relay })}</p>
    {#if space.rights}<p class="note">{t(`rights.${space.rights}`)}</p>{/if}
    {#if space.waiting}
      <p class="note">{t("sync.waiting")}</p>
    {/if}
    <p class="note">
      {space.synced_at ? t("sync.last", { time: time(space.synced_at) }) : t("sync.never")}
    </p>
    {#if space.sync_problem}<p class="error">{t("sync.problem", { problem: space.sync_problem })}</p>{/if}
    <p class="row">
      <button class="primary" onclick={syncNow} disabled={busy}>{t("sync.now")}</button>
      {#if report}<span class="note">{report}</span>{/if}
    </p>

    {#if space.rights === "manage"}
      <h2>{t("sync.invite")}</h2>
      <p class="note">{t("sync.inviteHint")}</p>
      <form class="row" onsubmit={invite}>
        <select bind:value={rights} aria-label={t("sync.invite")}>
          {#each ["read", "write", "manage"] as const as r (r)}
            <option value={r}>{t(`rights.${r}`)}</option>
          {/each}
        </select>
        <button type="submit">{t("sync.invite")}</button>
      </form>
      {#if invitation}
        <label class="invitation">
          {t("sync.invitation")}
          <textarea readonly rows="3">{invitation}</textarea>
        </label>
        <button onclick={copy}>{copied ? t("sync.copied") : t("sync.copy")}</button>
      {/if}
    {/if}

    {#if members.length}
      <h2>{t("sync.members")}</h2>
      <ul class="versions">
        {#each members as member (member.id)}
          {@const mine = member.device === device}
          <li>
            <span>
              {label(member)}{#if mine}&nbsp;({t("sync.thisDevice")}){/if}
              <span class="note"> · {t(`rights.${member.rights}`)}</span>
            </span>
            {#if space.rights === "manage" && !mine && member.role !== "recovery"}
              <button class="danger small" onclick={() => remove(member)} disabled={busy}>{t("sync.remove")}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</article>
