import type { APIRoute } from "astro";

import { SITE_CONFIG } from "../../site-config";
import {
  EMPTY_SITEMAP_HEADERS,
  EMPTY_URLSET,
  fetchUpstream,
  normalizeSourceUrls,
  parseSitemapIndex,
  rewriteSitemapDomain,
  SITEMAP_HEADERS,
} from "./shared";

// /sitemap.xml: serves a sitemap proxied from SITE_CONFIG.sourceSitemapUrl with every
// URL's host rewritten to this site's domain. A <sitemapindex> upstream (or array
// config) is served as an index whose entries are /proxied-sitemap-[i].xml
// (./proxy-sitemap.ts).
export const prerender = false;

export const GET: APIRoute = async ({ site }) => {
  if (!site) return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });

  const urls = normalizeSourceUrls(SITE_CONFIG.sourceSitemapUrl);
  if (urls.length === 0) {
    return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });
  }

  // Array config: one /proxied-sitemap-N.xml slot per entry. The entry route
  // fetches each upstream URL on demand.
  if (urls.length > 1) {
    return new Response(buildProxyIndex(urls.length, site.origin), {
      headers: SITEMAP_HEADERS,
    });
  }

  // Single URL: mirror the upstream's shape (urlset stays urlset; index becomes
  // a proxied index).
  const xml = await fetchUpstream(urls[0]);
  if (xml === null) {
    return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });
  }
  const entries = parseSitemapIndex(xml);
  const body = entries
    ? buildProxyIndex(entries.length, site.origin)
    : rewriteSitemapDomain(xml, site.origin);
  return new Response(body, { headers: SITEMAP_HEADERS });
};

// A <sitemapindex> whose entries are our own /proxied-sitemap-<i>.xml URLs.
function buildProxyIndex(count: number, origin: string): string {
  const entries = Array.from(
    { length: count },
    (_, i) => `  <sitemap><loc>${origin}/proxied-sitemap-${i}.xml</loc></sitemap>`,
  ).join("\n");
  return (
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
    `${entries}\n` +
    "</sitemapindex>\n"
  );
}
