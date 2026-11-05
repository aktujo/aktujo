<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  import { api } from "./lib/api";
  import type { components } from "./lib/api-types";

  type Item = components["schemas"]["ItemView"];

  let items = $state<Item[]>([]);
  let title = $state("");
  let error = $state("");

  async function load() {
    const { data } = await api.GET("/api/v1/items");
    items = data ?? [];
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const { error: problem } = await api.POST("/api/v1/items", { body: { title } });
    error = problem?.error ?? "";
    if (!problem) title = "";
    await load();
  }

  async function remove(id: number) {
    await api.DELETE("/api/v1/items/{id}", { params: { path: { id } } });
    await load();
  }

  load();
</script>

<main>
  <h1>aktujo</h1>
  <form onsubmit={add}>
    <input bind:value={title} placeholder="A new item" aria-label="Title" />
    <button type="submit">Add</button>
  </form>
  {#if error}<p class="error">{error}</p>{/if}
  <ul>
    {#each items as item (item.id)}
      <li>{item.title} <button onclick={() => remove(item.id)}>Delete</button></li>
    {/each}
  </ul>
</main>
