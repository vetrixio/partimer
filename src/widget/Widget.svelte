<!--
  The floating clock (spec §3.2). The backend creates this window while it should be on
  screen and destroys it otherwise (windows::ensure_widget); here we only draw it.
  - Time is computed locally each second from PanelState, refetched on `state-changed`.
  - Press + move more than 4 px drags the window; press + release without that toggles the panel.
  - Moves are reported (debounced) in physical pixels, the unit ensure_widget restores in.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, onStateChanged, type PanelState } from "../lib/api";
  import { formatHm, liveSeconds } from "../lib/time";
  import { plantAwake, plantRest } from "./plant";

  const DRAG_THRESHOLD_PX = 4;
  const MOVE_DEBOUNCE_MS = 500;

  let panel = $state<PanelState | null>(null);
  let opacity = $state(0.6);
  /** `Date.now()` when `panel` arrived: the base for the per-second display. */
  let fetchedAt = $state(Date.now());
  let now = $state(Date.now());

  const status = $derived(panel?.status ?? "idle");
  const elapsed = $derived(
    panel ? liveSeconds(panel.elapsedSeconds, status === "running", fetchedAt, now) : 0,
  );
  const time = $derived(
    status === "running" ? formatHm(elapsed) : status === "paused" ? `⏸ ${formatHm(elapsed)}` : "",
  );

  // Newest request wins: an older answer arriving late is dropped.
  let panelSeq = 0;
  let panelApplied = 0;
  let settingsSeq = 0;
  let settingsApplied = 0;

  async function refreshPanel() {
    const seq = ++panelSeq;
    try {
      const next = await api.getPanelState();
      if (seq < panelApplied) return;
      panelApplied = seq;
      panel = next;
      fetchedAt = Date.now();
      now = fetchedAt;
    } catch (e) {
      console.warn("partimer: get_panel_state", e);
    }
  }

  async function refreshOpacity() {
    const seq = ++settingsSeq;
    try {
      const settings = await api.getSettings();
      if (seq < settingsApplied) return;
      settingsApplied = seq;
      opacity = settings.widget.opacity;
    } catch (e) {
      console.warn("partimer: get_settings", e);
    }
  }

  function refresh() {
    void refreshPanel();
    void refreshOpacity();
  }

  // ---- click vs drag ----

  /** Where the left button went down; null once a drag started or nothing is pressed. */
  let press: { x: number; y: number } | null = null;

  // Mouse events, not pointer events: the release that ends a window drag goes to the window
  // manager, so the page never sees it, and the next press would then not fire `pointerdown`
  // (the page still thinks the button is held). `mousedown` fires for every press. While the
  // button is held the window keeps getting moves even outside its 130x40.
  function mousedown(e: MouseEvent) {
    press = e.button === 0 ? { x: e.screenX, y: e.screenY } : null;
  }

  function mousemove(e: MouseEvent) {
    if (!press) return;
    if ((e.buttons & 1) === 0) {
      press = null;
      return;
    }
    if (Math.hypot(e.screenX - press.x, e.screenY - press.y) <= DRAG_THRESHOLD_PX) return;
    press = null;
    getCurrentWindow()
      .startDragging()
      .catch((err) => console.warn("partimer: start dragging", err));
  }

  function mouseup(e: MouseEvent) {
    if (e.button !== 0 || !press) return;
    press = null;
    togglePanel();
  }

  function togglePanel() {
    api.togglePanel("widget").catch((err) => console.warn("partimer: toggle panel", err));
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      togglePanel();
    }
  }

  onMount(() => {
    refresh();
    const tick = setInterval(() => (now = Date.now()), 1000);
    const unlisten = onStateChanged(refresh);

    let moveTimer: ReturnType<typeof setTimeout> | undefined;
    const unmoved = getCurrentWindow().onMoved(({ payload }) => {
      clearTimeout(moveTimer);
      moveTimer = setTimeout(() => {
        api
          .widgetMoved(Math.round(payload.x), Math.round(payload.y))
          .catch((err) => console.warn("partimer: widget_moved", err));
      }, MOVE_DEBOUNCE_MS);
    });

    return () => {
      clearInterval(tick);
      clearTimeout(moveTimer);
      void unlisten.then((f) => f());
      void unmoved.then((f) => f());
    };
  });
</script>

<div
  class="capsule"
  class:idle={status === "idle"}
  style:--widget-opacity={opacity}
  role="button"
  tabindex="-1"
  aria-label="打工人計時器：開關打卡面板"
  onmousedown={mousedown}
  onmousemove={mousemove}
  onmouseup={mouseup}
  onkeydown={keydown}
  oncontextmenu={(e) => e.preventDefault()}
>
  <span class="plant" class:awake={status === "running"} aria-hidden="true">
    {@html status === "running" ? plantAwake : plantRest}
  </span>
  {#if time}
    <span class="time" class:paused={status === "paused"}>{time}</span>
  {/if}
</div>

<style>
  .capsule {
    width: 100vw;
    height: 100vh;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 12px 0 6px;
    border-radius: 20px;
    background: rgba(25, 25, 30, 0.55);
    color: #f6efe8;
    opacity: var(--widget-opacity);
    transition: opacity 0.15s ease;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
  }

  .capsule:hover {
    opacity: 1;
  }

  .capsule.idle {
    justify-content: center;
    padding: 0;
  }

  .plant {
    flex: none;
    width: 32px;
    height: 32px;
  }

  .plant :global(svg) {
    display: block;
    width: 32px;
    height: 32px;
  }

  .plant.awake :global(.sway) {
    transform-box: view-box;
    /* Where the stem meets the soil. */
    transform-origin: 20px 24px;
    animation: partimer-sway 1.2s ease-in-out infinite alternate;
  }

  @keyframes -global-partimer-sway {
    from {
      transform: rotate(-7deg);
    }
    to {
      transform: rotate(7deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .plant.awake :global(.sway) {
      animation: none;
    }
  }

  .time {
    flex: 1;
    text-align: center;
    font-size: 16px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  .time.paused {
    color: #c9c2bb;
  }
</style>
