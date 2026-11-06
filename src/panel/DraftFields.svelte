<!--
  Title and content inputs. Deliberately uncontrolled (spec §5.1): the values are set once on
  mount and never written from outside again, so incoming state can never disturb typing or an
  IME composition. To show other text, the parent rebuilds this component ({#key generation}).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";

  interface Props {
    /** Read once on mount. */
    title: string;
    /** Read once on mount. */
    content: string;
    /** Called on every edit with the current values (for the saved/unsaved label only). */
    onedit?: (title: string, content: string) => void;
  }

  let { title, content, onedit }: Props = $props();

  let titleEl: HTMLInputElement;
  let contentEl: HTMLTextAreaElement;

  let composing = false;
  let compositionWaiters: (() => void)[] = [];
  // The last draft sync; awaited before handing values to a command, so a late `set_draft`
  // can never land after (and overwrite the draft of) the entry that command creates.
  let lastSync: Promise<unknown> = Promise.resolve();

  /** The field that had focus when `getValues` blurred it, with its caret. */
  let blurred: { el: HTMLInputElement | HTMLTextAreaElement; start: number; end: number } | null =
    null;

  /** Give up waiting for `compositionend` after this long (some IMEs never send it). */
  const COMPOSITION_TIMEOUT_MS = 2000;

  onMount(() => {
    titleEl.value = title;
    contentEl.value = content;
  });

  function sync() {
    const t = titleEl.value;
    const c = contentEl.value;
    lastSync = api.setDraft(t, c).catch((e) => console.warn("partimer: set_draft", e));
    onedit?.(t, c);
  }

  function compositionStart() {
    composing = true;
  }

  function compositionEnd() {
    composing = false;
    const waiters = compositionWaiters;
    compositionWaiters = [];
    waiters.forEach((w) => w());
    // Some engines send no `input` after the composition is committed.
    sync();
  }

  /** True while an IME composition is in progress in either field. */
  export function isComposing(): boolean {
    return composing;
  }

  /**
   * End editing and return what the fields hold right now (spec §5.2): wait for an IME
   * composition to finish, blur, then read the DOM.
   */
  export async function getValues(): Promise<{ title: string; content: string }> {
    if (composing) {
      await new Promise<void>((resolve) => {
        compositionWaiters.push(resolve);
        setTimeout(resolve, COMPOSITION_TIMEOUT_MS);
      });
    }
    const active = [titleEl, contentEl].find((el) => el === document.activeElement);
    blurred = active
      ? { el: active, start: active.selectionStart ?? 0, end: active.selectionEnd ?? 0 }
      : null;
    titleEl.blur();
    contentEl.blur();
    await lastSync;
    return { title: titleEl.value, content: contentEl.value };
  }

  /** After a save (same fields, nothing rebuilt): put the caret back where the user was typing. */
  export function refocus() {
    if (!blurred || !blurred.el.isConnected) return;
    blurred.el.focus();
    blurred.el.setSelectionRange(blurred.start, blurred.end);
    blurred = null;
  }
</script>

<div class="draft">
  <input
    bind:this={titleEl}
    class="title"
    type="text"
    placeholder="標題"
    aria-label="標題"
    autocomplete="off"
    spellcheck="false"
    oninput={sync}
    oncompositionstart={compositionStart}
    oncompositionend={compositionEnd}
  />
  <textarea
    bind:this={contentEl}
    class="content"
    rows="5"
    placeholder="內容（行首打「- 」會顯示成項目符號）"
    aria-label="內容"
    spellcheck="false"
    oninput={sync}
    oncompositionstart={compositionStart}
    oncompositionend={compositionEnd}
  ></textarea>
</div>

<style>
  .draft {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .content {
    resize: none;
  }
</style>
