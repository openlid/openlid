// SSR routes are invisible to @astrojs/sitemap (it only sees prerendered
// pages), so list them here. This runs synchronously in Astro's config chain
// (plain Node) — no `astro:content`/`getCollection`, no `@/` alias — so we read
// the `pages` collection from disk and reproduce the glob loader's id
// (per-segment github slug, `/index` stripped, `slug` override) to match what
// getEntry() resolves. Add more sources with relative imports.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { slug as githubSlug } from "github-slugger";

const PAGES_DIR = fileURLToPath(new URL("../../content/pages", import.meta.url));

function deriveSlug(relPath: string): string {
  return relPath
    .replace(/\.(md|mdx)$/i, "")
    .split(/[/\\]/)
    .map((segment) => githubSlug(segment))
    .join("/")
    .replace(/\/index$/, "");
}

function readFrontmatter(content: string): { draft: boolean; slug?: string } {
  const block = /^---\r?\n([\s\S]*?)\r?\n---/.exec(content)?.[1] ?? "";
  return {
    draft: /^draft:\s*true\s*$/m.test(block),
    slug: /^slug:\s*["']?(.+?)["']?\s*$/m.exec(block)?.[1],
  };
}

function getPagePaths(): string[] {
  let entries: string[];
  try {
    entries = readdirSync(PAGES_DIR, { recursive: true }) as string[];
  } catch {
    return []; // no src/content/pages yet
  }
  return entries
    .filter((rel) => /\.(md|mdx)$/i.test(rel))
    .map((rel) => {
      const { draft, slug } = readFrontmatter(
        readFileSync(join(PAGES_DIR, rel), "utf8"),
      );
      return draft ? null : `/${slug ?? deriveSlug(rel)}`; // drafts 404 in prod
    })
    .filter((path): path is string => path !== null);
}

export function getSitemapPaths(): string[] {
  return [...getPagePaths()];
}
