// Pure parts of the panel's auto height (src/lib/autosize.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { snappedTop, windowHeight } from "../../src/lib/autosize.ts";

test("windowHeight rounds up and clamps to [220, available - 40]", () => {
  assert.equal(windowHeight(400.2, 1000), 401);
  assert.equal(windowHeight(100, 1000), 220);
  assert.equal(windowHeight(2000, 1000), 960);
});

const area = { y: 0, height: 1000 };

test("a window sticking out below the work area moves up", () => {
  assert.equal(snappedTop(area, { y: 500, height: 400 }, { y: 500, height: 600 }), 400);
  // Never above the work area's top.
  assert.equal(snappedTop(area, { y: 100, height: 400 }, { y: 100, height: 1200 }), 0);
});

test("a window on the bottom edge stays there when it shrinks", () => {
  assert.equal(snappedTop(area, { y: 400, height: 600 }, { y: 400, height: 500 }), 500);
});

test("a window elsewhere, or at the top, stays put", () => {
  assert.equal(snappedTop(area, { y: 100, height: 400 }, { y: 100, height: 300 }), null);
  // Full-height window at the top: shrinking keeps the top, not the bottom.
  assert.equal(snappedTop(area, { y: 0, height: 1000 }, { y: 0, height: 600 }), null);
  // Already where it should be.
  assert.equal(snappedTop(area, { y: 400, height: 600 }, { y: 400, height: 600 }), null);
});
