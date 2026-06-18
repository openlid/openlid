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

// /proxied-sitemap-<i>.xml: the i-th upstream urlset, host-rewritten. `i` only
// indexes the upstream's own entries or the operator-supplied array, never a
// caller-supplied URL.
export const prerender = false;

export const GET: APIRoute = async ({ params, site }) => {
  if (!site) return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });

  const urls = normalizeSourceUrls(SITE_CONFIG.sourceSitemapUrl);
  if (urls.length === 0 || !/^\d+$/.test(params.i ?? "")) {
    return new Response("Not found", { status: 404 });
  }
  const idx = Number(params.i);

  // Array config: index directly into the array. Single URL: fetch the upstream
  // <sitemapindex> and take its i-th <loc>.
  let entryUrl: string | undefined;
  if (urls.length > 1) {
    entryUrl = urls[idx];
  } else {
    const indexXml = await fetchUpstream(urls[0]);
    const entries = indexXml ? parseSitemapIndex(indexXml) : null;
    entryUrl = entries?.[idx];
  }
  if (!entryUrl) {
    return new Response("Not found", { status: 404 });
  }

  const xml = await fetchUpstream(entryUrl);
  if (xml === null) {
    return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });
  }
  if (parseSitemapIndex(xml)) {
    // Nested index: fail safe rather than emit URLs we don't serve.
    console.warn(`[sitemap-proxy] nested sitemap index not supported: ${entryUrl}`);
    return new Response(EMPTY_URLSET, { headers: EMPTY_SITEMAP_HEADERS });
  }

  return new Response(rewriteSitemapDomain(xml, site.origin), {
    headers: SITEMAP_HEADERS,
  });
};
