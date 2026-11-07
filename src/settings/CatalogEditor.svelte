<!--
  Client / project management (spec §8): rename (Save / Revert / Enter), colour, archive, delete.
  Name drafts are kept per id, so a refetch never overwrites what the user is typing.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, onStateChanged, type CatalogView, type Id, type ItemKind } from "../lib/api";
  import { COLORS } from "../lib/colors";
  import { formatMinutes } from "../lib/time";

  let catalog = $state<CatalogView | null>(null);
  /** Pending name edits by id; an id is present only while its text differs from the stored one. */
  let drafts = $state<Record<Id, string>>({});
  let errors = $state<Record<Id, string>>({});

  // Only the newest response wins.
  let seq = 0;
  async function load() {
    const mine = ++seq;
    try {
      const c = await api.getCatalog();
      if (mine !== seq) return;
      catalog = c;
      globalError = null;
      // Drop drafts for rows that no longer exist or already equal the stored name.
      const names = new Map<Id, string>();
      for (const cl of c.clients) {
        names.set(cl.id, cl.name);
        for (const p of cl.projects) names.set(p.id, p.name);
      }
      for (const id of Object.keys(drafts)) {
        if (!names.has(id) || drafts[id].trim() === names.get(id)) delete drafts[id];
      }
    } catch (e) {
      if (mine === seq) globalError = String(e);
    }
  }
  let globalError = $state<string | null>(null);

  onMount(() => {
    void load();
    const unlisten = onStateChanged(() => void load());
    return () => void unlisten.then((f) => f());
  });

  function edit(id: Id, original: string, value: string) {
    if (value === original) delete drafts[id];
    else drafts[id] = value;
    delete errors[id];
  }

  function revert(id: Id) {
    delete drafts[id];
    delete errors[id];
  }

  async function guarded(id: Id, task: () => Promise<void>) {
    delete errors[id];
    try {
      await task();
    } catch (e) {
      errors[id] = String(e);
    }
    await load();
  }

  async function save(kind: ItemKind, id: Id, original: string) {
    const value = drafts[id];
    if (value === undefined || value.trim() === original) return;
    const name = value.trim();
    await guarded(id, async () => {
      if (kind === "client") await api.renameClient(id, name);
      else await api.renameProject(id, name);
      delete drafts[id];
    });
  }

  function onKey(e: KeyboardEvent, kind: ItemKind, id: Id, original: string) {
    if (e.key !== "Enter" || e.isComposing || e.keyCode === 229) return;
    e.preventDefault();
    void save(kind, id, original);
  }

  async function remove(kind: ItemKind, id: Id) {
    await guarded(id, async () => {
      const impact = await api.deletionImpact(kind, id);
      let text = "確定刪除？";
      if (impact.entryCount > 0) {
        text = `會刪掉 ${impact.entryCount} 筆紀錄、共 ${formatMinutes(impact.totalMinutes)}，確定刪除？`;
        if (impact.includesRunning) text += "\n其中包含正在計時的紀錄，計時會一併停止。";
      }
      if (!(await ask(text, { title: "刪除", kind: "warning" }))) return;
      await api.deleteItem(kind, id);
    });
  }
</script>

{#snippet nameRow(kind: ItemKind, id: Id, name: string, archived: boolean, label: string)}
  {@const draft = drafts[id]}
  {@const dirty = draft !== undefined && draft.trim() !== name}
  <div class="row">
    <input
      type="text"
      aria-label={label}
      value={draft ?? name}
      oninput={(e) => edit(id, name, e.currentTarget.value)}
      onkeydown={(e) => onKey(e, kind, id, name)}
    />
    {#if dirty}
      <button type="button" class="primary" onclick={() => save(kind, id, name)}>儲存</button>
      <button type="button" onclick={() => revert(id)}>還原</button>
    {/if}
    <button
      type="button"
      onclick={() => guarded(id, () => api.setArchived(kind, id, !archived))}
    >
      {archived ? "取消封存" : "封存"}
    </button>
    <button type="button" class="danger" onclick={() => remove(kind, id)}>刪除</button>
  </div>
  {#if errors[id]}
    <p class="error" role="alert">{errors[id]}</p>
  {/if}
{/snippet}

{#if globalError}
  <p class="error" role="alert">{globalError}</p>
{/if}
{#if catalog}
  {#if catalog.clients.length === 0}
    <p class="empty">還沒有客戶或專案；在打卡面板建立第一個專案。</p>
  {/if}
  {#each catalog.clients as client (client.id)}
    <div class="client" class:archived={client.archived}>
      {@render nameRow("client", client.id, client.name, client.archived, `客戶名稱：${client.name}`)}
      <ul>
        {#each client.projects as p (p.id)}
          <li class:archived={p.archived}>
            {@render nameRow("project", p.id, p.name, p.archived, `專案名稱：${p.name}`)}
            <div class="meta">
              <span class="swatches" role="group" aria-label="專案顏色">
                {#each COLORS as c, i (i)}
                  <button
                    type="button"
                    class="swatch"
                    class:current={p.color === i}
                    style:background={c}
                    aria-label={`顏色 ${i + 1}`}
                    aria-pressed={p.color === i}
                    onclick={() => {
                      if (p.color !== i) void guarded(p.id, () => api.setProjectColor(p.id, i));
                    }}
                  ></button>
                {/each}
              </span>
              <small>{p.entryCount} 筆紀錄</small>
            </div>
          </li>
        {/each}
        {#if client.projects.length === 0}
          <li class="empty">沒有專案</li>
        {/if}
      </ul>
    </div>
  {/each}
{/if}

<style>
  .client {
    margin-bottom: 14px;
  }
  .client > :global(.row input) {
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0 0 0 20px;
  }
  li {
    margin-bottom: 8px;
  }
  .archived :global(input[type="text"]) {
    color: var(--muted);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .row button {
    padding: 4px 10px;
    white-space: nowrap;
  }
  button.danger {
    color: var(--danger);
  }
  .meta {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-top: 4px;
  }
  .swatches {
    display: flex;
    gap: 6px;
    padding: 4px;
  }
  .swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid transparent;
  }
  .swatch.current {
    outline: 2px solid var(--fg);
    outline-offset: 2px;
  }
  small {
    color: var(--muted);
  }
  .error {
    margin: 4px 0 0;
  }
  .empty {
    color: var(--muted);
  }
</style>
