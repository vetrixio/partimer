// Effective pin counting (src/lib/pins.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { pinnedCount } from "../../src/lib/pins.ts";

const p = (id: string, pinned: boolean, archived = false) => ({
  id,
  name: id,
  color: 0,
  archived,
  pinned,
  entryCount: 0,
});

test("pinnedCount counts only effective pins", () => {
  const catalog = {
    pinLimit: 5,
    clients: [
      { id: "c1", name: "c1", archived: false, projects: [p("a", true), p("b", false), p("c", true, true)] },
      { id: "c2", name: "c2", archived: true, projects: [p("d", true)] },
    ],
  };
  assert.equal(pinnedCount(catalog), 1);
  assert.equal(pinnedCount({ pinLimit: 5, clients: [] }), 0);
});
