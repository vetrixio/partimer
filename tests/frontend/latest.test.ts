// Slider save coalescing (src/lib/latest.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { LatestSaver } from "../../src/lib/latest.ts";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

test("changes during a save are coalesced to the latest value", async () => {
  const saved: number[] = [];
  const gates: Array<() => void> = [];
  let settled = 0;
  const s = new LatestSaver<number>(
    (v) => {
      saved.push(v);
      const d = deferred();
      gates.push(d.resolve);
      return d.promise;
    },
    () => settled++,
  );
  s.input();
  const first = s.change(0.5);
  s.input();
  void s.change(0.55);
  s.input();
  void s.change(0.6);
  gates.shift()!();
  await new Promise((r) => setImmediate(r));
  gates.shift()!();
  await first;
  assert.deepEqual(saved, [0.5, 0.6]);
  assert.equal(settled, 1);
});

test("a drag that starts during a save keeps its value until its own save", async () => {
  let settled = 0;
  const d = deferred();
  const s = new LatestSaver<number>(() => d.promise, () => settled++);
  s.input();
  const saving = s.change(0.5);
  s.input(); // new drag, not committed yet
  d.resolve();
  await saving;
  assert.equal(settled, 0, "drag value must not jump back");
  await s.change(0.7);
  assert.equal(settled, 1);
});
