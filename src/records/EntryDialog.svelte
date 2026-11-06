<!--
  Modal editor for one entry (spec §7.1): edit an existing one, or add a missed one (id = null).
  Backend errors are shown inside the dialog and keep it open.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, type CatalogView, type EntryForm, type Id } from "../lib/api";
  import { toLocalInput } from "../lib/time";

  interface Props {
    /** null = a new entry. */
    id: Id | null;
    initial: EntryForm;
    catalog: CatalogView;
    /** Called after a successful save or delete. */
    ondone: () => void;
    oncancel: () => void;
  }

  let { id, initial, catalog, ondone, oncancel }: Props = $props();

  // The dialog edits its own copy; later list refreshes never touch it (state is read once).
  // svelte-ignore state_referenced_locally
  let form = $state<EntryForm>({ ...initial });
  // svelte-ignore state_referenced_locally
  const canStayRunning = initial.stillRunning;
  let error = $state<string | null>(null);
  let busy = $state(false);
  let first: HTMLSelectElement | undefined = $state();

  onMount(() => first?.focus());

  function toggleRunning() {
    // Turning "still running" off needs an end time to start from.
    if (!form.stillRunning && !form.endLocal) form.endLocal = toLocalInput(new Date());
  }

  async function save() {
    busy = true;
    error = null;
    try {
      await api.saveEntry(id, { ...form, endLocal: form.stillRunning ? null : form.endLocal });
      ondone();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (id === null) return;
    if (!(await ask("確定刪除這筆紀錄？", { title: "刪除紀錄", kind: "warning" }))) return;
    busy = true;
    error = null;
    try {
      await api.deleteEntry(id);
      ondone();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function onkeydown(ev: KeyboardEvent) {
    // Esc while an IME is composing only cancels the composition.
    if (ev.key === "Escape" && !ev.isComposing) {
      ev.preventDefault();
      oncancel();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={id === null ? "新增紀錄" : "編輯紀錄"}>
  <form
    onsubmit={(ev) => {
      ev.preventDefault();
      void save();
    }}
  >
    <h2>{id === null ? "新增紀錄" : "編輯紀錄"}</h2>

    <label>
      <span>專案</span>
      <select bind:value={form.projectId} bind:this={first}>
        {#each catalog.clients as c (c.id)}
          <optgroup label={c.archived ? `${c.name}（已封存）` : c.name}>
            {#each c.projects as p (p.id)}
              <option value={p.id}>{p.archived ? `${p.name}（已封存）` : p.name}</option>
            {/each}
          </optgroup>
        {/each}
      </select>
    </label>

    <div class="row">
      <label>
        <span>開始</span>
        <input type="datetime-local" bind:value={form.startLocal} required />
      </label>
      <label>
        <span>結束</span>
        <input
          type="datetime-local"
          value={form.endLocal ?? ""}
          oninput={(e) => (form.endLocal = e.currentTarget.value || null)}
          disabled={form.stillRunning}
          required={!form.stillRunning}
        />
      </label>
    </div>

    {#if canStayRunning}
      <label class="check">
        <input type="checkbox" bind:checked={form.stillRunning} onchange={toggleRunning} />
        <span>仍在計時</span>
      </label>
    {/if}

    <label>
      <span>暫停分鐘數</span>
      <input type="number" min="0" step="1" bind:value={form.pausedMinutes} />
    </label>

    <label>
      <span>標題</span>
      <input type="text" bind:value={form.title} />
    </label>

    <label>
      <span>內容</span>
      <textarea rows="6" bind:value={form.content}></textarea>
    </label>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      {#if id !== null}
        <button type="button" class="danger" onclick={remove} disabled={busy}>刪除</button>
      {/if}
      <span class="spacer"></span>
      <button type="button" onclick={oncancel} disabled={busy}>取消</button>
      <button type="submit" class="primary" disabled={busy}>儲存</button>
    </div>
  </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    background: rgb(0 0 0 / 0.4);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .dialog {
    width: min(520px, 100%);
    max-height: 100%;
    overflow-y: auto;
    padding: 16px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  label > span {
    font-size: 0.85em;
    color: var(--muted);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .check input {
    width: auto;
  }
  .check span {
    font-size: 1em;
    color: var(--fg);
  }
  textarea {
    resize: vertical;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .danger {
    color: var(--danger);
    border-color: var(--danger);
  }
  .error {
    margin: 0;
  }
</style>
