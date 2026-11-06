<!--
  Records window (spec §7): month / client / project filters, totals on the left, days of
  entries on the right, CSV export, and the entry editor as a modal.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    onOpenEntry,
    onStateChanged,
    type CatalogView,
    type EntryForm,
    type EntryView,
    type Id,
    type RecordsQuery,
    type RecordsView,
  } from "../lib/api";
  import { colorOf } from "../lib/colors";
  import ContentText from "../lib/ContentText.svelte";
  import { formatHms, formatMinutes, liveSeconds } from "../lib/time";
  import EntryDialog from "./EntryDialog.svelte";

  const now0 = new Date();
  let query = $state<RecordsQuery>({
    year: now0.getFullYear(),
    month: now0.getMonth() + 1,
    clientId: null,
    projectId: null,
  });
  let view = $state<RecordsView | null>(null);
  let fetchedAt = $state(Date.now());
  let nowMs = $state(Date.now());
  let message = $state<string | null>(null);
  let exporting = $state(false);
  let dialog = $state<{
    key: number;
    id: Id | null;
    form: EntryForm;
    catalog: CatalogView;
  } | null>(null);
  let dialogKey = 0;

  // Only the newest response wins; a slow older one must not overwrite it.
  let seq = 0;
  async function load() {
    const mine = ++seq;
    try {
      const v = await api.getRecords($state.snapshot(query));
      if (mine !== seq) return;
      view = v;
      query = v.query;
      fetchedAt = Date.now();
    } catch (e) {
      if (mine === seq) message = String(e);
    }
  }

  async function openEntry(id: Id | null) {
    message = null;
    try {
      const [form, catalog] = await Promise.all([api.getEntryForm(id), api.getCatalog()]);
      dialog = { key: ++dialogKey, id, form, catalog };
    } catch (e) {
      message = String(e);
    }
  }

  /** From the panel or tray: show the month the entry is in, with no filter hiding it. */
  async function jumpToEntry(id: Id) {
    if (dialog && !(await ask("放棄目前的編輯，改開另一筆紀錄？", { title: "紀錄", kind: "warning" })))
      return;
    try {
      const form = await api.getEntryForm(id);
      const m = /^(\d{4})-(\d{2})/.exec(form.startLocal);
      if (m) {
        query = { year: Number(m[1]), month: Number(m[2]), clientId: null, projectId: null };
        await load();
      }
      const catalog = await api.getCatalog();
      dialog = { key: ++dialogKey, id, form, catalog };
    } catch (e) {
      message = String(e);
    }
  }

  function shiftMonth(delta: number) {
    const d = new Date(query.year, query.month - 1 + delta, 1);
    query = { ...query, year: d.getFullYear(), month: d.getMonth() + 1 };
    void load();
  }

  function thisMonth() {
    const d = new Date();
    query = { ...query, year: d.getFullYear(), month: d.getMonth() + 1 };
    void load();
  }

  function pickClient(id: string) {
    query = { ...query, clientId: id || null, projectId: null };
    void load();
  }

  function pickProject(id: string) {
    query = { ...query, projectId: id || null };
    void load();
  }

  async function exportCsv() {
    exporting = true;
    message = null;
    try {
      if (await api.exportCsv($state.snapshot(query))) message = "已匯出";
    } catch (e) {
      message = String(e);
    } finally {
      exporting = false;
    }
  }

  function afterDialog() {
    dialog = null;
    void load();
  }

  function seconds(e: EntryView): number {
    return liveSeconds(e.seconds, e.status === "running", fetchedAt, nowMs);
  }

  function endLabel(e: EntryView): string {
    if (e.status === "running") return "計時中";
    if (e.status === "paused") return "暫停中";
    return e.endHhmm ?? "";
  }

  function activate(ev: KeyboardEvent, id: Id) {
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      void openEntry(id);
    }
  }

  onMount(() => {
    let alive = true;
    const unlisten: (() => void)[] = [];
    const tick = setInterval(() => (nowMs = Date.now()), 1000);
    void load();
    (async () => {
      const a = await onStateChanged(() => void load());
      const b = await onOpenEntry((id) => {
        // The event reached us, so the parked id is not needed any more.
        void api.takePendingEntry();
        if (id) void jumpToEntry(id);
      });
      if (!alive) {
        a();
        b();
        return;
      }
      unlisten.push(a, b);
      // After the listener exists: the window may have been opened for an entry.
      const pending = await api.takePendingEntry();
      if (pending && alive) void jumpToEntry(pending);
    })();
    return () => {
      alive = false;
      clearInterval(tick);
      unlisten.forEach((f) => f());
    };
  });
</script>

<main>
  <header>
    <div class="month">
      <button type="button" onclick={() => shiftMonth(-1)} aria-label="上個月">‹</button>
      <strong>{query.year} 年 {query.month} 月</strong>
      <button type="button" onclick={() => shiftMonth(1)} aria-label="下個月">›</button>
      <button type="button" class="link" onclick={thisMonth}>本月</button>
    </div>
    <select
      aria-label="客戶"
      value={query.clientId ?? ""}
      onchange={(e) => pickClient(e.currentTarget.value)}
    >
      <option value="">全部客戶</option>
      {#each view?.clients ?? [] as [id, name] (id)}
        <option value={id}>{name}</option>
      {/each}
    </select>
    <select
      aria-label="專案"
      value={query.projectId ?? ""}
      onchange={(e) => pickProject(e.currentTarget.value)}
    >
      <option value="">全部專案</option>
      {#each view?.projects ?? [] as p (p.id)}
        <option value={p.id}>{p.name}</option>
      {/each}
    </select>
    <span class="spacer"></span>
    <button type="button" onclick={() => openEntry(null)}>新增紀錄</button>
    <button type="button" onclick={exportCsv} disabled={exporting}>匯出 CSV</button>
  </header>

  {#if message}
    <p class="message" role="status">{message}</p>
  {/if}

  {#if view}
    <div class="body">
      <aside aria-label="合計">
        <p class="label">本月合計</p>
        <p class="total">{view.totalHm}</p>
        <p class="hours">{view.totalHours} 小時</p>
        <ul class="projects">
          {#each view.perProject as p (p.projectId)}
            <li>
              <span class="dot" style:background={colorOf(p.color)} aria-hidden="true"></span>
              <span class="plabel">{p.label}</span>
              <span class="pnum">{p.hm}<br /><small>{p.hours} 小時</small></span>
            </li>
          {/each}
        </ul>
      </aside>

      <section class="days" aria-label="紀錄">
        {#each view.days as day (day.date)}
          <h2>{day.date} {day.weekday}</h2>
          <ul class="entries">
            {#each day.entries as e (e.id)}
              <li>
                <!-- A div, not a <button>: the content may hold lists, which buttons cannot. -->
                <div
                  class="entry"
                  role="button"
                  tabindex="0"
                  title="編輯"
                  onclick={() => openEntry(e.id)}
                  onkeydown={(ev) => activate(ev, e.id)}
                >
                  <span class="head">
                    <span class="dot" style:background={colorOf(e.color)} aria-hidden="true"></span>
                    <span class="span" class:live={e.status !== "idle"}>
                      {e.startHhmm}–{endLabel(e)}
                    </span>
                    <span class="duration" class:live={e.status !== "idle"}>
                      {formatHms(seconds(e))}
                    </span>
                  </span>
                  {#if e.pausedMinutes > 0}
                    <small class="paused">另暫停 {formatMinutes(e.pausedMinutes)}</small>
                  {/if}
                  <span class="title">{e.title || "（無標題）"}</span>
                  <span class="project">{e.clientName}・{e.projectName}</span>
                  <ContentText content={e.content} />
                </div>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty">這個月沒有紀錄</p>
        {/each}
      </section>
    </div>
  {/if}

  {#if dialog}
    {#key dialog.key}
    <EntryDialog
      id={dialog.id}
      initial={dialog.form}
      catalog={dialog.catalog}
      ondone={afterDialog}
      oncancel={() => (dialog = null)}
    />
    {/key}
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--line);
  }
  header select {
    width: auto;
  }
  .month {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .message {
    margin: 0;
    padding: 6px 16px;
    background: var(--notice-bg);
    border-bottom: 1px solid var(--notice-line);
    overflow-wrap: anywhere;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 240px 1fr;
  }
  aside {
    padding: 12px 16px;
    border-right: 1px solid var(--line);
    overflow-y: auto;
  }
  aside p {
    margin: 0;
  }
  .label {
    color: var(--muted);
    font-size: 0.85em;
  }
  .total {
    font-size: 1.8rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .hours {
    color: var(--muted);
    margin-bottom: 12px;
  }
  .projects {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .projects li {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .plabel {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .pnum {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  small {
    color: var(--muted);
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    margin-top: 0.4em;
    border-radius: 50%;
  }
  .days {
    padding: 8px 16px 16px;
    overflow-y: auto;
  }
  h2 {
    margin: 12px 0 4px;
    font-size: 0.9rem;
    color: var(--muted);
  }
  .empty {
    color: var(--muted);
  }
  .entries {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .entry {
    display: flex;
    flex-direction: column;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    cursor: pointer;
  }
  .entry:hover,
  .entry:focus-visible {
    border-color: var(--accent);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .head .dot {
    align-self: center;
    margin-top: 0;
  }
  .span {
    flex: 1;
    font-variant-numeric: tabular-nums;
  }
  .duration {
    font-variant-numeric: tabular-nums;
  }
  .live {
    color: var(--accent-strong);
  }
  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .project {
    font-size: 0.85em;
    color: var(--muted);
  }
</style>
