import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

// `e.currentTarget` is only set while the event is being dispatched. Settings' `run()`
// queues its callback, so reading `e.currentTarget` inside that callback gets the wrong
// element (`checked` became undefined and the command lost its `enabled` key).
// Read the value in the handler and pass it in instead.

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return svelteFiles(path);
    return path.endsWith(".svelte") ? [path] : [];
  });
}

test("no queued callback reads e.currentTarget", () => {
  const offenders: string[] = [];
  for (const file of svelteFiles("src")) {
    readFileSync(file, "utf8")
      .split("\n")
      .forEach((line, i) => {
        if (/\brun\(\s*(async\s*)?\(\)\s*=>[^\n]*currentTarget/.test(line)) {
          offenders.push(`${file}:${i + 1}: ${line.trim()}`);
        }
      });
  }
  assert.deepEqual(offenders, []);
});
