<!--
  Project menu (spec §4 item 2): a native <select> grouped by client, ending with
  「＋ 新增專案…」. The colour dot is a separate element left of the select, never inside an
  option (old bug #6). With no projects at all, only the add form is shown.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { Id, ProjectGroup } from "../lib/api";
  import { colorOf } from "../lib/colors";

  interface Props {
    groups: ProjectGroup[];
    selectedId: Id | null;
    disabled: boolean;
    /** Switch to a project. Resolves when done (or cancelled); the menu then shows `selectedId`. */
    onpick: (id: Id) => Promise<void>;
    /**
     * Create a project. Resolves to true when created; otherwise to an error message to show,
     * or null when the user cancelled (the form stays as it is).
     */
    onadd: (clientName: string, projectName: string) => Promise<true | string | null>;
  }

  let { groups, selectedId, disabled, onpick, onadd }: Props = $props();

  const NEW = "__new__";

  let selectEl: HTMLSelectElement | undefined = $state();
  let clientEl: HTMLInputElement | undefined = $state();
  let projectEl: HTMLInputElement | undefined = $state();
  let adding = $state(false);
  let addError = $state<string | null>(null);

  const noProjects = $derived(groups.length === 0);
  const showForm = $derived(noProjects || adding);
  const selectedColor = $derived.by(() => {
    for (const g of groups) {
      const p = g.projects.find((p) => p.id === selectedId);
      if (p) return colorOf(p.color);
    }
    return null;
  });

  /** Put the menu back on the selected project (after cancel / error / success). */
  async function resetMenu() {
    await tick();
    if (selectEl) selectEl.value = selectedId ?? "";
  }

  async function changed() {
    const value = selectEl?.value ?? "";
    if (value === NEW) {
      await resetMenu();
      adding = true;
      addError = null;
      await tick();
      clientEl?.focus();
      return;
    }
    if (value && value !== selectedId) {
      await onpick(value);
    }
    await resetMenu();
  }

  async function create() {
    if (!clientEl || !projectEl) return;
    addError = null;
    const result = await onadd(clientEl.value, projectEl.value);
    if (result === true) {
      // The form may already be gone (it was the first project).
      if (clientEl) clientEl.value = "";
      if (projectEl) projectEl.value = "";
      adding = false;
      await resetMenu();
    } else {
      addError = result;
    }
  }

  function cancel() {
    adding = false;
    addError = null;
  }

  function keydown(e: KeyboardEvent) {
    // Enter that commits an IME composition must not submit (keyCode 229 covers WebKit).
    if (e.key !== "Enter" || e.isComposing || e.keyCode === 229) return;
    e.preventDefault();
    if (!disabled) void create();
  }
</script>

<div class="picker">
  {#if !noProjects}
    <div class="menu">
      <span
        class="dot"
        style:background={selectedColor ?? "transparent"}
        style:border-color={selectedColor ?? "var(--muted)"}
        aria-hidden="true"
      ></span>
      <select
        bind:this={selectEl}
        value={selectedId ?? ""}
        {disabled}
        aria-label="專案"
        onchange={changed}
      >
        {#if selectedId === null}
          <option value="" disabled>選擇專案</option>
        {/if}
        {#each groups as g (g.clientId)}
          <optgroup label={g.clientName}>
            {#each g.projects as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </optgroup>
        {/each}
        <option value={NEW}>＋ 新增專案…</option>
      </select>
    </div>
  {/if}

  {#if showForm}
    <div class="add" role="group" aria-label="新增專案">
      {#if noProjects}
        <p class="hint">還沒有專案，先新增一個：</p>
      {/if}
      <input
        bind:this={clientEl}
        type="text"
        placeholder="客戶名"
        aria-label="客戶名"
        autocomplete="off"
        list="partimer-clients"
        onkeydown={keydown}
      />
      <datalist id="partimer-clients">
        {#each groups as g (g.clientId)}
          <option value={g.clientName}></option>
        {/each}
      </datalist>
      <input
        bind:this={projectEl}
        type="text"
        placeholder="專案名"
        aria-label="專案名"
        autocomplete="off"
        onkeydown={keydown}
      />
      <div class="add-buttons">
        {#if !noProjects}
          <button type="button" class="secondary" {disabled} onclick={cancel}>取消</button>
        {/if}
        <button type="button" class="primary" {disabled} onclick={create}>建立</button>
      </div>
      {#if addError}
        <p class="error" role="alert">{addError}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .menu {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid;
  }
  select {
    flex: 1;
    min-width: 0;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
  }
  .hint {
    margin: 0;
    font-size: 0.9em;
  }
  .add-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .error {
    margin: 0;
  }
</style>
