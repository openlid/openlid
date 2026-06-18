import { defineCollection } from "astro:content";
import { z } from "astro/zod";
import { glob } from "astro/loaders";

// Build-time collection: Markdown is schema-validated and bundled into the
// server build. Use z.coerce.date() (not z.date()) since frontmatter dates are
// serialized. For external CMS/API/DB content, use a live collection
// (src/live.config.ts) instead — not for repo Markdown.
//
// To add a page (rendered on demand by src/pages/[...slug].astro at /<filename>):
// 1. Create `src/content/pages/<slug>.md` (or `.mdx`).
// 2. Frontmatter:
//    ---
//    title: Page title
//    pubDate: 2026-01-01
//    description: For SEO (optional)
//    draft: true            # optional; drafts render in dev only
//    ---
// 3. Body is Markdown, rendered inside a `.prose` container. Sitemap is handled
//    automatically by src/lib/sitemap/get-sitemap-paths.ts.
const pages = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/pages" }),
  schema: z.object({
    title: z.string(),
    pubDate: z.coerce.date(),
    slug: z.string().optional(),
    description: z.string().optional(),
    draft: z.boolean().default(false),
  }),
});

// To add a blog:
// 1. Uncomment the `posts` collection below.
// 2. Create `src/content/posts/` and add markdown files matching the schema.
// 3. Add `posts` to the exported `collections` object.
// 4. Create a route (e.g. `src/pages/blog/[slug].astro`) that calls
//    `getEntry("posts", slug)` / `getCollection("posts")`.
//
// const posts = defineCollection({
//   loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/posts" }),
//   schema: z.object({
//     title: z.string(),
//     pubDate: z.coerce.date(),
//     slug: z.string().optional(),
//     description: z.string().optional(),
//     author: z.string().optional(),
//     tags: z.array(z.string()).default([]),
//     draft: z.boolean().default(false),
//   }),
// });

export const collections = { pages };
