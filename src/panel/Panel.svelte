<!--
  The punch panel (spec §4, §5). Rules that keep typing safe (old bugs #1–#3):
  - The title/content inputs are uncontrolled (DraftFields) and only rebuilt when
    `draft.generation` changes; refetched state with the same generation never touches them.
  - Every command first takes the fields' values (`getValues`: wait for IME, blur, read DOM)
    and sends them with the command in one call; the returned state replaces ours.
  - One command at a time; refetches that would race a command are dropped and redone after.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, onStateChanged, type Id, type Notice, type PanelState } from "../lib/api";
  import { autosize } from "../lib/autosize";
  import { addHoursLocal, formatHms, liveSeconds, localDateKey } from "../lib/time";
  import DraftFields from "./DraftFields.svelte";
  import ProjectPicker from "./ProjectPicker.svelte";
  import TodayList from "./TodayList.svelte";

  const SWITCH_QUESTION = "結束目前這筆，改記另一個專案？";
  const isMac = navigator.userAgent.includes("Mac");

  let root: HTMLElement;
  let fields: ReturnType<typeof DraftFields> | undefined = $state();
  let endEl: HTMLInputElement | undefined = $state();

  let panel = $state<PanelState | null>(null);
  /** `Date.now()` when `panel` arrived: the base for the per-second display. */
  let fetchedAt = $state(Date.now());
  let now = $state(Date.now());
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** What the fields hold after the user edited them (null = untouched since the rebuild). */
  let edited = $state<{ title: string; content: string } | null>(null);

  // Ordering of state updates: every request takes a number; older answers are dropped.
  let requestSeq = 0;
  let appliedSeq = 0;
  let refreshAfterCommand = false;
  let fetchedDay = localDateKey(new Date());

  const running = $derived(panel?.status === "running");
  const timing = $derived(panel !== null && panel.status !== "idle");
  const delta = $derived(liveSeconds(0, true, fetchedAt, now));
  const elapsed = $derived(panel ? liveSeconds(panel.elapsedSeconds, running, fetchedAt, now) : 0);
  const todayTotal = $derived(
    panel ? liveSeconds(panel.todayTotalSeconds, running, fetchedAt, now) : 0,
  );
  const saved = $derived.by(() => {
    if (!panel) return true;
    const r = panel.running;
    if (r && edited) return edited.title === r.title && edited.content === r.content;
    return panel.draftSaved;
  });
  const promptStart = $derived(
    panel?.running ? `${panel.running.date}T${panel.running.startHhmm}` : null,
  );

  /** Returns whether `next` was taken. */
  function apply(next: PanelState, seq: number): boolean {
    if (seq < appliedSeq) return false;
    // Generations only grow; an older one would bring back text of a finished entry.
    if (panel && next.draft.generation < panel.draft.generation) return false;
    appliedSeq = seq;
    if (!panel || next.draft.generation !== panel.draft.generation) edited = null;
    panel = next;
    fetchedAt = Date.now();
    now = fetchedAt;
    return true;
  }

  async function refresh() {
    if (busy) {
      refreshAfterCommand = true;
      return;
    }
    const seq = ++requestSeq;
    const day = localDateKey(new Date());
    try {
      const next = await api.getPanelState();
      if (busy) {
        refreshAfterCommand = true;
        return;
      }
      // Only a successful fetch counts for the day: after a failure the next second retries.
      if (apply(next, seq)) fetchedDay = day;
    } catch (e) {
      console.warn("partimer: get_panel_state", e);
    }
  }

  /**
   * Run one command with the fields' current values. `confirm` runs first (e.g. a dialog);
   * returning false cancels without touching anything. `draft: false` for commands that
   * neither take nor affect the text (they leave the fields alone).
   */
  async function command(
    run: (v: { title: string; content: string }) => Promise<PanelState>,
    { confirm, draft = true }: { confirm?: () => Promise<boolean>; draft?: boolean } = {},
  ): Promise<boolean> {
    if (busy || !panel) return false;
    const f = draft ? fields : undefined;
    if (draft && !f) {
      error = "輸入框還沒準備好，請再按一次";
      return false;
    }
    const generation = panel.draft.generation;
    /** The backend's answer, even if `apply` dropped it as out of order. */
    let result: PanelState | null = null;
    busy = true;
    error = null;
    try {
      if (confirm && !(await confirm())) return false;
      const v = f ? await f.getValues() : { title: "", content: "" };
      const seq = ++requestSeq;
      result = await run(v);
      apply(result, seq);
      return true;
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = false;
      // Same draft → the same fields stay: let them sync again. A new generation rebuilds
      // them, and the old ones stay closed (their text belongs to the finished command) —
      // also when the new state was not shown yet, so the backend's answer counts too.
      const sameDraft =
        panel?.draft.generation === generation &&
        (result === null || result.draft.generation === generation);
      if (f && sameDraft) f.reopen();
      if (refreshAfterCommand) {
        refreshAfterCommand = false;
        void refresh();
      }
    }
  }

  const confirmSwitch = () =>
    ask(SWITCH_QUESTION, {
      title: "打工人計時器",
      kind: "warning",
      okLabel: "結束並切換",
      cancelLabel: "取消",
    });

  function startStop() {
    if (!panel) return;
    if (panel.status === "idle") {
      const id = panel.selectedProjectId;
      if (id) void command((v) => api.start(id, v.title, v.content));
    } else {
      void command((v) => api.stop(v.title, v.content));
    }
  }

  function pauseResume() {
    if (!panel) return;
    if (panel.status === "running") void command(() => api.pause());
    else if (panel.status === "paused") void command(() => api.resume());
  }

  async function save() {
    const before = fields;
    const generation = panel?.draft.generation;
    const ok = await command((v) => api.saveDraft(v.title, v.content));
    // Saving keeps the same fields (no new generation): let the user keep typing.
    if (ok && before && panel?.draft.generation === generation) before.refocus();
  }

  async function pick(id: Id) {
    await command((v) => api.selectProject(id, v.title, v.content), {
      confirm: timing ? confirmSwitch : undefined,
    });
  }

  async function addProject(client: string, project: string): Promise<true | string | null> {
    // Creating a project while timing switches to it, so it needs the same confirmation.
    let cancelled = false;
    const ok = await command((v) => api.addProject(client, project, v.title, v.content), {
      confirm: timing
        ? async () => {
            cancelled = !(await confirmSwitch());
            return !cancelled;
          }
        : undefined,
    });
    if (ok) return true;
    if (cancelled) return null;
    // Show the error under the form instead of under the buttons.
    const message = error;
    error = null;
    return message ?? null;
  }

  function stopAtChosen() {
    const end = endEl?.value ?? "";
    if (!end) {
      error = "請選擇結束時間";
      return;
    }
    void command((v) => api.stopAt(v.title, v.content, end));
  }

  function snooze() {
    void command(() => api.snoozeLongRunning(), { draft: false });
  }

  function dismiss(n: Notice) {
    void command(() => api.dismissNotice(n.id), { draft: false });
  }

  function openRecords(entryId: Id | null = null) {
    api.openWindow("records", entryId).catch((e) => (error = String(e)));
  }

  function openSettings() {
    api.openWindow("settings").catch((e) => (error = String(e)));
  }

  async function quit() {
    if (busy || !panel) return;
    busy = true;
    const f = fields;
    try {
      // Hand the latest text to the backend; it saves an unsaved draft on exit.
      if (f) {
        const v = await f.getValues();
        await api.setDraft(v.title, v.content, panel.draft.generation);
      }
      await api.quitApp();
    } catch (e) {
      error = String(e);
      busy = false;
      f?.reopen();
    }
  }

  /** Keys that belong to an IME composition (keyCode 229 covers WebKit's commit key). */
  const imeKey = (e: KeyboardEvent) => e.isComposing || e.keyCode === 229 || !!fields?.isComposing();

  function keydown(e: KeyboardEvent) {
    if (imeKey(e)) return;
    const mod = isMac ? e.metaKey : e.ctrlKey;
    // `code` too: some IME modes report another `key` for the S key.
    const sKey = e.code === "KeyS" || e.key.toLowerCase() === "s";
    if (mod && !e.altKey && !e.shiftKey && sKey) {
      e.preventDefault();
      if (timing && !busy) void save();
      return;
    }
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      getCurrentWindow()
        .hide()
        .catch((err) => console.warn("partimer: hide", err));
    }
  }

  onMount(() => {
    void refresh();
    const stopAutosize = autosize(root);
    const tick = setInterval(() => {
      now = Date.now();
      // The ticker does not broadcast at midnight while idle, so notice the new day here.
      if (localDateKey(new Date(now)) !== fetchedDay) void refresh();
    }, 1000);
    const visible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    const unlisteners = [
      onStateChanged(() => void refresh()),
      getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused) void refresh();
      }),
    ];
    return () => {
      stopAutosize();
      clearInterval(tick);
      document.removeEventListener("visibilitychange", visible);
      unlisteners.forEach((p) => p.then((off) => off()).catch(() => {}));
    };
  });
</script>

<svelte:window onkeydown={keydown} />

<main class="panel" bind:this={root}>
  {#if panel}
    {#if panel.notices.length > 0 || (panel.longRunningPrompt && panel.running)}
      <div class="notices">
        {#if panel.longRunningPrompt && panel.running && promptStart}
          <div class="notice long" role="alert">
            <p>已經計時超過 12 小時，是不是忘了停？</p>
            {#key panel.running.id}
              <input
                bind:this={endEl}
                type="datetime-local"
                aria-label="實際結束時間"
                value={addHoursLocal(promptStart, 12) ?? ""}
                min={promptStart}
              />
            {/key}
            <div class="row">
              <button type="button" class="primary" disabled={busy} onclick={stopAtChosen}>
                用這個時間結束
              </button>
              <button type="button" class="secondary" disabled={busy} onclick={snooze}>
                沒忘，繼續計時
              </button>
            </div>
          </div>
        {/if}
        {#each panel.notices as n (n.id)}
          <div
            class="notice"
            class:danger={n.kind === "saveError" ||
              n.kind === "unreadable" ||
              n.kind === "newerVersion" ||
              n.kind === "corruptNoBackup"}
            role="status"
          >
            <p>{n.message}</p>
            {#if n.dismissable}
              <button
                type="button"
                class="close"
                aria-label="關閉"
                title="關閉"
                disabled={busy}
                onclick={() => dismiss(n)}>×</button
              >
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <ProjectPicker
      groups={panel.groups}
      selectedId={panel.selectedProjectId}
      disabled={busy}
      onpick={pick}
      onadd={addProject}
    />

    <div class="clock" class:paused={panel.status === "paused"} aria-label="已計時">
      {#if panel.status === "paused"}<span class="pause-mark">⏸</span>{/if}{formatHms(elapsed)}
    </div>

    {#key panel.draft.generation}
      <DraftFields
        bind:this={fields}
        title={panel.draft.title}
        content={panel.draft.content}
        generation={panel.draft.generation}
        onedit={(title, content) => (edited = { title, content })}
      />
    {/key}

    {#if timing}
      <div class="save-row">
        <button type="button" class="secondary" disabled={busy} onclick={save}>儲存</button>
        <span class="save-status" class:unsaved={!saved}>{saved ? "已儲存" : "尚未儲存"}</span>
        <span class="shortcut">{isMac ? "⌘S" : "Ctrl+S"}</span>
      </div>
    {/if}

    <div class="actions">
      {#if timing}
        <button type="button" class="secondary big" disabled={busy} onclick={pauseResume}>
          {panel.status === "paused" ? "RESUME" : "PAUSE"}
        </button>
      {/if}
      <button
        type="button"
        class="big"
        class:primary={!timing}
        class:stop={timing}
        disabled={busy || (!timing && !panel.selectedProjectId)}
        onclick={startStop}
      >
        {timing ? "STOP" : "START"}
      </button>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <TodayList
      entries={panel.today}
      totalSeconds={todayTotal}
      runningDelta={running ? delta : 0}
      onopen={(id) => openRecords(id)}
    />

    <footer>
      <button type="button" class="link" onclick={() => openRecords()}>紀錄</button>
      <button type="button" class="link" onclick={openSettings}>設定</button>
      <button type="button" class="link quit" disabled={busy} onclick={quit}>結束 app</button>
    </footer>
  {/if}
</main>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 100vh;
    /* Only when even the shrunk today list cannot make room (many notices): scroll as a
       whole instead of clipping the buttons. */
    overflow-y: auto;
    padding: 12px 14px;
  }
  /* Everything keeps its height; only the today list gives way (and scrolls). */
  .panel > :global(*) {
    flex-shrink: 0;
  }
  .panel > :global(.today) {
    flex-shrink: 1;
  }
  .notices {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--notice-bg);
    border: 1px solid var(--notice-line);
  }
  .notice p {
    flex: 1;
    margin: 0;
    overflow-wrap: anywhere;
    /* Keeps line breaks of unsaved text quoted in the "ended elsewhere" notice. */
    white-space: pre-wrap;
  }
  .notice.danger {
    background: var(--danger-bg);
    border-color: var(--danger);
    color: var(--danger-fg);
  }
  .notice.long {
    flex-direction: column;
    align-items: stretch;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .close {
    flex: none;
    border: none;
    background: none;
    color: inherit;
    font-size: 1.1rem;
    line-height: 1;
    padding: 0 2px;
    cursor: pointer;
  }
  .clock {
    text-align: center;
    font-size: 2.6rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
    line-height: 1.1;
  }
  .clock.paused {
    color: var(--muted);
  }
  .pause-mark {
    font-size: 0.6em;
    margin-right: 0.3em;
    vertical-align: middle;
  }
  .save-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .save-status {
    flex: 1;
    color: var(--muted);
  }
  .save-status.unsaved {
    color: var(--accent-strong);
    font-weight: 600;
  }
  .shortcut {
    font-size: 0.8em;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .actions .big {
    flex: 1;
    min-height: 44px;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: 0.05em;
  }
  .error {
    margin: 0;
  }
  footer {
    display: flex;
    justify-content: space-between;
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
</style>
