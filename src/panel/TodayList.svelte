<!--
  Today's entries, newest first, plus the running one (spec §4 item 8). Click opens the entry
  in the records window. Only the list scrolls when the panel reaches the screen height.
-->
<script lang="ts">
  import type { EntryView, Id } from "../lib/api";
  import { colorOf } from "../lib/colors";
  import ContentText from "../lib/ContentText.svelte";
  import { formatHms } from "../lib/time";

  interface Props {
    entries: EntryView[];
    totalSeconds: number;
    /** Seconds to add to a running entry (time since the state was fetched). */
    runningDelta: number;
    onopen: (id: Id) => void;
  }

  let { entries, totalSeconds, runningDelta, onopen }: Props = $props();

  const live = (e: EntryView) => e.seconds + (e.status === "running" ? runningDelta : 0);

  function activate(ev: KeyboardEvent, id: Id) {
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      onopen(id);
    }
  }

  function endLabel(e: EntryView): string {
    if (e.status === "running") return "計時中";
    if (e.status === "paused") return "暫停中";
    return e.endHhmm ?? "";
  }
</script>

<section class="today" aria-label="今日紀錄">
  <h2>今日紀錄</h2>
  <div class="scroll" data-autosize-scroll>
    <div>
      {#if entries.length === 0}
        <p class="empty">今天還沒有紀錄</p>
      {:else}
        <ul class="entries">
          {#each entries as e (e.id)}
            <li>
              <!-- A div, not a <button>: the content may hold lists, which buttons cannot. -->
              <div
                class="entry"
                role="button"
                tabindex="0"
                title="在紀錄視窗編輯"
                onclick={() => onopen(e.id)}
                onkeydown={(ev) => activate(ev, e.id)}
              >
                <span class="head">
                  <span class="dot" style:background={colorOf(e.color)} aria-hidden="true"></span>
                  <span class="span" class:live={e.status !== "idle"}>
                    {e.startHhmm}–{endLabel(e)}
                  </span>
                  <span class="duration">{formatHms(live(e))}</span>
                </span>
                <span class="title">{e.title || "（無標題）"}</span>
                <span class="project">{e.clientName}・{e.projectName}</span>
                <ContentText content={e.content} />
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
  <p class="total">今日總計 <strong>{formatHms(totalSeconds)}</strong></p>
</section>

<style>
  .today {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  h2 {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--muted);
    margin: 0 0 4px;
  }
  .scroll {
    min-height: 0;
    overflow-y: auto;
  }
  .empty {
    margin: 4px 0;
    color: var(--muted);
  }
  .entries {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .entry {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    width: 100%;
    padding: 6px 8px;
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
    align-items: center;
    gap: 6px;
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .span {
    flex: 1;
    font-variant-numeric: tabular-nums;
  }
  .span.live {
    color: var(--accent-strong);
  }
  .duration {
    font-variant-numeric: tabular-nums;
  }
  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .project {
    font-size: 0.85em;
    color: var(--muted);
  }
  .total {
    margin: 6px 0 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
