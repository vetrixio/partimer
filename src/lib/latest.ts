// Saves only the latest value of a control that can change faster than a save completes
// (e.g. a slider moved with the keyboard), and says when no newer input is waiting.

export class LatestSaver<T> {
  private inputs = 0;
  private savedInputs = 0;
  private pending: { value: T } | null = null;
  private sending = false;

  /**
   * `save` stores one value; `settled` runs when every input so far is saved (the control
   * can follow the stored value again).
   */
  private readonly save: (value: T) => Promise<void>;
  private readonly settled: () => void;

  // Plain fields, not parameter properties: Node's type stripping (npm test) rejects those.
  constructor(save: (value: T) => Promise<void>, settled: () => void) {
    this.save = save;
    this.settled = settled;
  }

  /** The user moved the control (not saved yet). */
  input(): void {
    this.inputs += 1;
  }

  /** The user committed `value`; values arriving during a save are coalesced to the latest. */
  async change(value: T): Promise<void> {
    this.pending = { value };
    if (this.sending) return;
    this.sending = true;
    try {
      while (this.pending !== null) {
        const next = this.pending.value;
        const upTo = this.inputs;
        this.pending = null;
        await this.save(next);
        this.savedInputs = upTo;
      }
    } finally {
      this.sending = false;
      if (this.savedInputs === this.inputs) this.settled();
    }
  }
}
