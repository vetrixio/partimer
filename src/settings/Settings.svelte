<!--
  Settings window (spec §8): autostart, floating clock, hourly reminder, shortcut status,
  client/project management and the data folder button.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, onStateChanged, type SettingsView, type WidgetSettings } from "../lib/api";
  import CatalogEditor from "./CatalogEditor.svelte";

  const isMac = navigator.userAgent.includes("Mac");

  let view = $state<SettingsView | null>(null);
  let message = $state<string | null>(null);
  let busy = $state(false);
  /** Slider value while dragging; null when it follows the stored value. */
  let dragOpacity = $state<number | null>(null);

  async function refresh() {
    try {
      view = await api.getSettings();
    } catch (e) {
      message = String(e);
    }
  }

  onMount(() => {
    void refresh();
    const unlisten = onStateChanged(() => void refresh());
    return () => void unlisten.then((f) => f());
  });

  async function run(task: () => Promise<SettingsView>) {
    busy = true;
    message = null;
    try {
      view = await task();
    } catch (e) {
      message = String(e);
      await refresh();
    } finally {
      busy = false;
    }
  }

  function setWidget(patch: Partial<WidgetSettings>) {
    if (!view) return;
    const widget = { ...view.widget, ...patch };
    return run(() => api.setWidgetSettings(widget));
  }

  async function openFolder() {
    message = null;
    try {
      await api.openDataFolder();
    } catch (e) {
      message = String(e);
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
          onchange={(e) => run(() => api.setAutostart(e.currentTarget.checked))}
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
          value={view.widget.opacity}
          disabled={busy}
          oninput={(e) => (dragOpacity = Number(e.currentTarget.value))}
          onchange={async (e) => {
            await setWidget({ opacity: Number(e.currentTarget.value) });
            dragOpacity = null;
          }}
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
          onchange={(e) => run(() => api.setHourlyReminder(e.currentTarget.checked))}
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
  .path {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
