<!--
  Settings window (spec §8): autostart, floating clock, hourly reminder, shortcut status,
  client/project management and the data folder button.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, onStateChanged, type SettingsView, type WidgetSettings } from "../lib/api";
  import { LatestSaver } from "../lib/latest";
  import CatalogEditor from "./CatalogEditor.svelte";

  const isMac = navigator.userAgent.includes("Mac");

  let view = $state<SettingsView | null>(null);
  let message = $state<string | null>(null);
  let busy = $state(false);
  let info = $state<string | null>(null);
  let transferring = $state(false);
  /** Slider value while dragging; null when it follows the stored value. */
  let dragOpacity = $state<number | null>(null);

  // Only the newest answer is shown: every refresh and every command takes a number.
  let viewSeq = 0;
  async function refresh() {
    const mine = ++viewSeq;
    try {
      const next = await api.getSettings();
      if (mine === viewSeq) view = next;
    } catch (e) {
      if (mine === viewSeq) message = String(e);
    }
  }

  onMount(() => {
    void refresh();
    const unlisten = onStateChanged(() => void refresh());
    return () => void unlisten.then((f) => f());
  });

  // Commands run one after another, each built from the view the previous one left, so a
  // slider step never sends back a checkbox value that was just changed.
  let queue: Promise<void> = Promise.resolve();
  function run(task: () => Promise<SettingsView>): Promise<void> {
    const next = queue.then(async () => {
      busy = true;
      message = null;
      const mine = ++viewSeq;
      try {
        const result = await task();
        if (mine === viewSeq) view = result;
      } catch (e) {
        message = String(e);
        await refresh();
      } finally {
        busy = false;
      }
    });
    queue = next;
    return next;
  }

  function setWidget(patch: Partial<WidgetSettings>) {
    return run(() => {
      if (!view) return api.getSettings();
      return api.setWidgetSettings({ ...view.widget, ...patch });
    });
  }

  // The slider stays usable while a change is saved: keyboard steps that arrive meanwhile are
  // coalesced into one save of the latest value, and the shown value only snaps back to the
  // stored one when no newer input is waiting.
  const opacitySaver = new LatestSaver<number>(
    (opacity) => setWidget({ opacity }),
    () => (dragOpacity = null),
  );
  function dragTo(value: number) {
    opacitySaver.input();
    dragOpacity = value;
  }

  async function openFolder() {
    message = null;
    try {
      await api.openDataFolder();
    } catch (e) {
      message = String(e);
    }
  }

  async function transfer(task: () => Promise<string | null>) {
    transferring = true;
    message = null;
    info = null;
    try {
      info = await task();
    } catch (e) {
      message = String(e);
    } finally {
      transferring = false;
    }
  }

  const accelerator = $derived(
    view ? (isMac ? view.shortcut.accelerator.replace("Ctrl+Alt+", "⌃⌥") : view.shortcut.accelerator) : "",
  );
  const shortcutProblem = $derived.by(() => {
    if (!view) return null;
    const { registered, wayland } = view.shortcut;
    if (!registered || wayland) {
      return wayland ? (view.shortcutHelp ?? "快捷鍵被其他程式占用") : "快捷鍵被其他程式占用";
    }
    return null;
  });
</script>

<main>
  <h1>設定</h1>
  {#if message}
    <p class="error pad" role="alert">{message}</p>
  {/if}

  {#if view}
    <section>
      <h2>開機自動啟動</h2>
      <label class="check">
        <input
          type="checkbox"
          checked={view.autostartEnabled}
          disabled={busy}
          onchange={(e) => {
            const on = e.currentTarget.checked;
            void run(() => api.setAutostart(on));
          }}
        />
        登入電腦時自動開啟
      </label>
      {#if view.autostartError}
        <p class="error" role="alert">{view.autostartError}</p>
        <p class="error">{view.autostartHelp}</p>
      {/if}
    </section>

    <section>
      <h2>浮動小時鐘</h2>
      <label class="check">
        <input
          type="checkbox"
          checked={view.widget.enabled ?? true}
          disabled={busy}
          onchange={(e) => setWidget({ enabled: e.currentTarget.checked })}
        />
        顯示浮動小時鐘
      </label>
      <label class="slider">
        透明度
        <input
          type="range"
          min="0.4"
          max="1"
          step="0.05"
          value={dragOpacity ?? view.widget.opacity}
          oninput={(e) => dragTo(Number(e.currentTarget.value))}
          onchange={(e) => void opacitySaver.change(Number(e.currentTarget.value))}
        />
        <span class="pct">{Math.round((dragOpacity ?? view.widget.opacity) * 100)}%</span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={view.widget.showWhenIdle}
          disabled={busy}
          onchange={(e) => setWidget({ showWhenIdle: e.currentTarget.checked })}
        />
        沒在計時時也顯示
      </label>
    </section>

    <section>
      <h2>整點提醒</h2>
      <label class="check">
        <input
          type="checkbox"
          checked={view.hourlyReminder}
          disabled={busy}
          onchange={(e) => {
            const on = e.currentTarget.checked;
            void run(() => api.setHourlyReminder(on));
          }}
        />
        計時中每到整點通知一次
      </label>
    </section>

    <section>
      <h2>全域快捷鍵</h2>
      <p>叫出打卡面板：<kbd>{accelerator}</kbd></p>
      {#if shortcutProblem}
        <p class="error" role="alert">{shortcutProblem}</p>
      {/if}
    </section>

    <section>
      <h2>客戶與專案</h2>
      <CatalogEditor />
    </section>

    <section>
      <h2>資料</h2>
      <p class="path">{view.dataDir}</p>
      <button type="button" onclick={openFolder}>打開資料夾</button>
      <h3>換電腦</h3>
      <div class="actions">
        <button type="button" onclick={() => transfer(api.transferOut)} disabled={transferring}>
          轉出資料
        </button>
        <button type="button" onclick={() => transfer(api.transferIn)} disabled={transferring}>
          轉入資料
        </button>
      </div>
      {#if info}
        <p class="info" role="status">{info}</p>
      {/if}
    </section>
  {/if}
</main>

<style>
  main {
    padding-bottom: 24px;
  }
  .pad,
  section {
    padding: 0 16px;
  }
  section {
    margin: 0 0 18px;
  }
  h2 {
    font-size: 1rem;
    margin: 0 0 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }
  p {
    margin: 4px 0;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 4px 0;
  }
  .check input,
  .slider input[type="range"] {
    width: auto;
  }
  .slider {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 4px 0;
  }
  .slider input[type="range"] {
    flex: 1;
    max-width: 240px;
    padding: 0;
  }
  .pct {
    min-width: 3.5em;
    color: var(--muted);
  }
  kbd {
    font-family: inherit;
    padding: 1px 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
  }
  h3 {
    font-size: 0.9rem;
    margin: 12px 0 6px;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .info {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .path {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
