// Project palette (src/lib/colors.ts) and plant artwork (src/widget/plant.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { COLORS, colorOf } from "../../src/lib/colors.ts";
import { plantAwake, plantRest } from "../../src/widget/plant.ts";

test("eight palette colours, unknown indexes fall back to the first", () => {
  assert.equal(COLORS.length, 8);
  assert.equal(colorOf(3), COLORS[3]);
  assert.equal(colorOf(8), COLORS[0]);
  assert.equal(colorOf(-1), COLORS[0]);
});

test("plant artwork is a 40x40 SVG; only the awake one has swaying leaves", () => {
  for (const svg of [plantAwake, plantRest]) {
    assert.match(svg, /^<svg [^>]*viewBox="0 0 40 40"/);
    assert.match(svg, /<\/svg>$/);
  }
  assert.match(plantAwake, /<g class="sway">/);
  assert.doesNotMatch(plantRest, /class="sway"/);
});
