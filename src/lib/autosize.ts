// Keep the window as tall as its content (spec §4.1).
//
// `root` is the content's outer element. It may be capped at the window height
// (`max-height: 100vh`); the parts allowed to scroll then are marked `data-autosize-scroll`,
// and whatever they hide is added back, so the measured height is always the content's
// natural height. Height is clamped to [220, available screen height − 40]; an empty root or
// a 0 measurement (hidden / not laid out yet) never resizes the window.

import {
  LogicalPosition,
  LogicalSize,
  currentMonitor,
  getCurrentWindow,
} from "@tauri-apps/api/window";

const MIN_HEIGHT = 220;
const SCREEN_MARGIN = 40;
/** A failed resize is tried again after RETRY_MS × attempt, up to MAX_RETRIES times. */
const RETRY_MS = 500;
const MAX_RETRIES = 5;

/** Natural content height in CSS pixels. */
function naturalHeight(root: HTMLElement): number {
  let h = root.scrollHeight;
  root.querySelectorAll<HTMLElement>("[data-autosize-scroll]").forEach((el) => {
    h += Math.max(0, el.scrollHeight - el.clientHeight);
  });
  return h;
}

/** Work area of the window's monitor in logical pixels, or the browser's idea of it. */
async function workArea(): Promise<{ top: number; height: number }> {
  try {
    const m = await currentMonitor();
    if (m && m.workArea.size.height > 0) {
      return {
        top: m.workArea.position.y / m.scaleFactor,
        height: m.workArea.size.height / m.scaleFactor,
      };
    }
  } catch (e) {
    console.warn("partimer: currentMonitor", e);
  }
  return { top: 0, height: window.screen.availHeight };
}

/** Pure part: the window height to use for a measured content height. */
export function windowHeight(natural: number, available: number): number {
  const max = available - SCREEN_MARGIN;
  return Math.max(MIN_HEIGHT, Math.min(Math.ceil(natural), max));
}

interface Span {
  y: number;
  height: number;
}

/**
 * Pure part: where the window's top should go after a resize, or null to stay. A window that
 * would stick out below the work area moves up; one whose bottom sat on the work area's bottom
 * (opened above a bottom taskbar) stays snapped there when it shrinks. A window at the top
 * never follows the bottom.
 */
export function snappedTop(area: Span, before: Span, after: Span): number | null {
  const bottom = area.y + area.height;
  const overflows = after.y + after.height > bottom;
  const wasOnBottom =
    Math.abs(before.y + before.height - bottom) < 2 && before.y > area.y + 2;
  if (!overflows && !wasOnBottom) return null;
  const y = Math.max(area.y, bottom - after.height);
  return Math.abs(y - after.y) < 1 ? null : y;
}

/** Start following `root`; returns a function that stops. */
export function autosize(root: HTMLElement, width = 360): () => void {
  const win = getCurrentWindow();
  let last = 0;
  let scheduled = false;
  let failures = 0;
  let stopped = false;
  let running: Promise<void> = Promise.resolve();

  const apply = async () => {
    // Nothing rendered yet (state still loading): keep the window as it is, no flash.
    if (root.childElementCount === 0) return;
    const natural = naturalHeight(root);
    if (natural <= 0) return;
    const area = await workArea();
    const h = windowHeight(natural, area.height);
    // Sub-pixel rounding of the new size can make the measurement flip by 1px; ignore that.
    if (Math.abs(h - last) < 2) return;
    const scale = await win.scaleFactor();
    const outerSpan = async () => {
      const pos = (await win.outerPosition()).toLogical(scale);
      const outer = (await win.outerSize()).toLogical(scale);
      return { x: pos.x, y: pos.y, height: outer.height };
    };
    const before = await outerSpan();
    try {
      await win.setSize(new LogicalSize(width, h));
    } catch (e) {
      // Not remembered as applied, so the same height is tried again.
      last = 0;
      if (!stopped && failures < MAX_RETRIES) {
        failures += 1;
        setTimeout(schedule, RETRY_MS * failures);
      }
      throw e;
    }
    failures = 0;
    last = h;
    // Opened above a bottom taskbar: growing would push the buttons under it (move up), and
    // after shrinking the panel stays on the taskbar instead of floating above it.
    const after = await outerSpan();
    const y = snappedTop({ y: area.top, height: area.height }, before, after);
    if (y !== null) await win.setPosition(new LogicalPosition(after.x, y));
  };

  function schedule() {
    if (scheduled || stopped) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      // One resize at a time, in order.
      running = running.then(apply).catch((e) => console.warn("partimer: autosize", e));
    });
  }

  const resize = new ResizeObserver(schedule);
  const observeScrollContent = () => {
    root.querySelectorAll<HTMLElement>("[data-autosize-scroll] > *").forEach((el) => {
      resize.observe(el);
    });
  };
  resize.observe(root);
  observeScrollContent();
  // Content inside a capped scroll area can grow without `root` changing size.
  const mutations = new MutationObserver(() => {
    observeScrollContent();
    schedule();
  });
  mutations.observe(root, { childList: true, subtree: true });
  schedule();

  return () => {
    stopped = true;
    resize.disconnect();
    mutations.disconnect();
  };
}
