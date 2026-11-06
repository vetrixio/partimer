// Entry content as shown in lists: lines starting with "- " become bullet items
// (consecutive ones form one list); other non-blank lines are paragraphs.

export type ContentBlock = { kind: "list"; items: string[] } | { kind: "para"; text: string };

export function contentBlocks(content: string): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  for (const raw of content.split("\n")) {
    const line = raw.replace(/\r$/, "");
    if (line.startsWith("- ")) {
      const last = blocks[blocks.length - 1];
      const item = line.slice(2);
      if (last?.kind === "list") last.items.push(item);
      else blocks.push({ kind: "list", items: [item] });
    } else if (line.trim() !== "") {
      blocks.push({ kind: "para", text: line });
    }
  }
  return blocks;
}
