import type { APIRoute } from "astro";

import { getPublishedPages } from "@/lib/content";
import { SITE_CONFIG } from "@/site-config";

export const prerender = true;

// Generates /llms.txt at dev/build time with no crawler or AI step.
// Today this combines a few manually curated discovery links with published
// content collection entries. Later, this can grow to include maintained
// nav/footer links, product/service pages, or other route metadata.

const normalizeText = (value: string) => value.trim().replace(/\s+/g, " ");

const escapeLinkText = (value: string) =>
  normalizeText(value).replaceAll("[", "\\[").replaceAll("]", "\\]");

const pagePathForEntryId = (id: string) => {
  const slug = id.replace(/\/index$/, "");
  return slug ? `/${slug}` : "/";
};

const basePath = import.meta.env.BASE_URL.replace(/\/$/, "");

const withBase = (path: string) => {
  const normalized = path.startsWith("/") ? path : `/${path}`;

  if (!basePath || normalized === basePath || normalized.startsWith(`${basePath}/`)) {
    return normalized;
  }

  return `${basePath}${normalized}`;
};

export const GET: APIRoute = async ({ site }) => {
  const origin = site?.origin ?? "https://example.com";
  const pages = await getPublishedPages();

  const contentLinks = pages
    .toSorted((a, b) => {
      const dateComparison =
        b.data.pubDate.getTime() - a.data.pubDate.getTime();
      return dateComparison || a.data.title.localeCompare(b.data.title);
    })
    .map((entry) => {
      const title = escapeLinkText(entry.data.title);
      const url = new URL(withBase(pagePathForEntryId(entry.id)), origin).href;
      const description = entry.data.description
        ? `: ${normalizeText(entry.data.description)}`
        : "";

      return `- [${title}](${url})${description}`;
    });

  const lines = [
    ...(SITE_CONFIG.name ? [`# ${SITE_CONFIG.name}`, ""] : []),
    ...(SITE_CONFIG.description ? [`> ${SITE_CONFIG.description}`, ""] : []),
    "This file is generated from the site's source content. It is a curated index for AI assistants, not a crawler permissions file. For crawler permissions, see `/robots.txt`.",
    "",
    "## Core Pages",
    "",
    `- [Home](${new URL(withBase("/"), origin).href}): Primary overview of the site.`,
    "",
    ...(contentLinks.length > 0
      ? ["## Public Content", "", ...contentLinks, ""]
      : []),
    "## Discovery",
    "",
    `- [Sitemap](${new URL(withBase("/sitemap-index.xml"), origin).href}): Complete search-engine sitemap, if available.`,
    `- [Robots policy](${new URL(withBase("/robots.txt"), origin).href}): Crawler permissions for automated agents.`,
    "",
  ];

  return new Response(lines.join("\n"), {
    headers: {
      "content-type": "text/plain; charset=utf-8",
    },
  });
};
