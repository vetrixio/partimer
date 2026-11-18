import type { CatalogView } from "./api";

/** Effective pins: pinned, project not archived, and its client not archived. */
export function pinnedCount(catalog: CatalogView): number {
  let n = 0;
  for (const c of catalog.clients) {
    if (c.archived) continue;
    for (const p of c.projects) if (p.pinned && !p.archived) n++;
  }
  return n;
}
