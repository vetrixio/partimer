// Entry content blocks (src/lib/content.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { contentBlocks } from "../../src/lib/content.ts";

test("consecutive '- ' lines form one list, other lines are paragraphs", () => {
  assert.deepEqual(contentBlocks("- 改 banner\n- 修 RWD\n備註\n- 再一項"), [
    { kind: "list", items: ["改 banner", "修 RWD"] },
    { kind: "para", text: "備註" },
    { kind: "list", items: ["再一項"] },
  ]);
});

test("blank lines are skipped, CRLF is handled, '-' without space is text", () => {
  assert.deepEqual(contentBlocks("\r\n- a\r\n\r\n-b\r\n   \n"), [
    { kind: "list", items: ["a"] },
    { kind: "para", text: "-b" },
  ]);
  assert.deepEqual(contentBlocks(""), []);
});

test("a blank line between bullets does not split the list", () => {
  assert.deepEqual(contentBlocks("- a\n\n- b"), [{ kind: "list", items: ["a", "b"] }]);
});
