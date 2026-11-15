<!--
  Date + 24-hour time field. Replaces `<input type="datetime-local">`, whose time part follows
  the OS locale (上午/下午 with 12-hour clocks on zh-TW Windows/macOS).
  `value` is local `YYYY-MM-DDTHH:MM`, or "" while the date is empty or the time is not a valid
  24-hour time.
-->
<script lang="ts">
  import { parseHm } from "./time";

  interface Props {
    value: string | null;
    /** Accessible name, e.g. 「開始」; the two fields become 「開始日期」 and 「開始時間」. */
    label: string;
    disabled?: boolean;
    /** Earliest selectable date, `YYYY-MM-DD`. */
    minDate?: string;
  }

  let { value = $bindable(), label, disabled = false, minDate }: Props = $props();

  let date = $state("");
  let time = $state("");
  // The last value this field produced or took in; a different incoming value resets both parts.
  let seen: string | null = null;

  $effect.pre(() => {
    const v = value ?? "";
    if (v === seen) return;
    seen = v;
    const m = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})$/.exec(v);
    date = m?.[1] ?? "";
    time = m?.[2] ?? "";
  });

  let typing = $state(false);
  // Flagged once the user leaves the field, not while a time is half typed.
  const invalid = $derived(!typing && time.trim() !== "" && parseHm(time) === null);

  function emit() {
    const hm = parseHm(time);
    const v = date && hm ? `${date}T${hm}` : "";
    seen = v;
    value = v;
  }

  function tidy() {
    typing = false;
    const hm = parseHm(time);
    if (hm) time = hm;
  }
</script>

<span class="datetime">
  <input
    type="date"
    aria-label={`${label}日期`}
    min={minDate}
    {disabled}
    value={date}
    oninput={(e) => {
      date = e.currentTarget.value;
      emit();
    }}
  />
  <input
    type="text"
    class="time"
    inputmode="numeric"
    autocomplete="off"
    maxlength="5"
    placeholder="HH:MM"
    title="24 小時制，例如 09:30、17:45"
    aria-label={`${label}時間（24 小時制）`}
    aria-invalid={invalid}
    {disabled}
    value={time}
    oninput={(e) => {
      time = e.currentTarget.value;
      emit();
    }}
    onfocus={() => (typing = true)}
    onblur={tidy}
  />
</span>

<style>
  .datetime {
    display: flex;
    gap: 6px;
  }
  .datetime input[type="date"] {
    flex: 1;
    min-width: 0;
  }
  .time {
    flex: none;
    width: 5.5em;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .time[aria-invalid="true"] {
    border-color: var(--danger);
  }
</style>
