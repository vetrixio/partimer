// Keep the window as tall as its content (spec §4.1).
//
// `root` is the content's outer element. It may be capped at the window height
// (`max-height: 100vh`); the parts allowed to scroll then are marked `data-autosize-scroll`,
// and whatever they hide is added back, so the measured height is always the content's
// natural height. Height is clamped to [220, available screen height − 40]; a 0 measurement
// (hidden / not laid out yet) never resizes the window.

import {
  LogicalPosition,
  LogicalSize,
  currentMonitor,
  getCurrentWindow,
} from "@tauri-apps/api/window";

const MIN_HEIGHT = 220;
const SCREEN_MARGIN = 40;

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

/** Start following `root`; returns a function that stops. */
export function autosize(root: HTMLElement, width = 360): () => void {
  const win = getCurrentWindow();
  let last = 0;
  let scheduled = false;
  let running: Promise<void> = Promise.resolve();

  const apply = async () => {
    const natural = naturalHeight(root);
    if (natural <= 0) return;
    const area = await workArea();
    const h = windowHeight(natural, area.height);
    // Sub-pixel rounding of the new size can make the measurement flip by 1px; ignore that.
    if (Math.abs(h - last) < 2) return;
    last = h;
    await win.setSize(new LogicalSize(width, h));
    // Opened above a bottom taskbar, growing would push the buttons under it: move up.
    const scale = await win.scaleFactor();
    const pos = (await win.outerPosition()).toLogical(scale);
    const outer = (await win.outerSize()).toLogical(scale);
    const bottom = area.top + area.height;
    if (pos.y + outer.height > bottom) {
      const y = Math.max(area.top, bottom - outer.height);
      await win.setPosition(new LogicalPosition(pos.x, y));
    }
  };

  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      // One resize at a time, in order.
      running = running.then(apply).catch((e) => console.warn("partimer: autosize", e));
    });
  };

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
    resize.disconnect();
    mutations.disconnect();
  };
}
