import { fileURLToPath } from "node:url";
import sitemap, { type SitemapOptions } from "@astrojs/sitemap";
import type { AstroIntegration } from "astro";
import { SITE_CONFIG } from "../../site-config";
import { getSitemapPaths } from "./get-sitemap-paths";
import { normalizeSourceUrls } from "./shared";

// Ploy patches the `site:` literal at deploy time, so we emit URLs against a
// placeholder and swap it for the resolved `config.site` at sitemap emit time.
const PLACEHOLDER = "https://ploy.invalid";
const configuredBase = (
  process.env.SITE_BASE ??
  (process.env.GITHUB_PAGES === "true" ? "/openlid" : "/")
).replace(/\/$/, "");

const withBase = (path: string) => {
  const normalized = path.startsWith("/") ? path : `/${path}`;

  if (
    !configuredBase ||
    normalized === configuredBase ||
    normalized.startsWith(`${configuredBase}/`)
  ) {
    return normalized;
  }

  return `${configuredBase}${normalized}`;
};

export function sitemapWithCustomPages(
  options: SitemapOptions = {},
): AstroIntegration[] {
  let resolvedSite = "";
  const userSerialize = options.serialize;
  const paths = getSitemapPaths();

  return [
    {
      name: "capture-site-for-sitemap",
      hooks: {
        // Inject the proxy routes only when an upstream is configured.
        "astro:config:setup": ({ injectRoute }) => {
          if (normalizeSourceUrls(SITE_CONFIG.sourceSitemapUrl).length > 0) {
            injectRoute({
              pattern: "/sitemap.xml",
              entrypoint: fileURLToPath(
                new URL("./sitemap.ts", import.meta.url),
              ),
              prerender: false,
            });
            injectRoute({
              pattern: "/proxied-sitemap-[i].xml",
              entrypoint: fileURLToPath(
                new URL("./proxy-sitemap.ts", import.meta.url),
              ),
              prerender: false,
            });
          }
        },
        "astro:config:done": ({ config }) => {
          if (config.site) {
            resolvedSite = String(config.site).replace(/\/$/, "");
          }
        },
      },
    },
    sitemap({
      ...options,
      customPages: [
        ...paths.map((p) => `${PLACEHOLDER}${withBase(p)}`),
        ...(options.customPages ?? []),
      ],
      serialize(item) {
        if (resolvedSite && item.url.startsWith(PLACEHOLDER)) {
          item.url = resolvedSite + item.url.slice(PLACEHOLDER.length);
        }
        return userSerialize ? userSerialize(item) : item;
      },
    }),
  ];
}
