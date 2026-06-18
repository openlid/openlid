// Shared helpers for the sitemap-proxy routes (./sitemap.ts and ./proxy-sitemap.ts):
// fetch the upstream, validate it's XML, parse <sitemapindex> entries, rewrite every
// URL's host to this site's domain. Plus the response headers and empty-urlset body
// both routes return.

// Successful responses cache for 1h; the empty fallback caches for 5m so a
// transient bad upstream self-heals on the next crawl.
const CACHE_SECONDS = 3600;
const EMPTY_CACHE_SECONDS = 300;

export const SITEMAP_HEADERS = {
  "Content-Type": "application/xml; charset=utf-8",
  "Cache-Control": `public, max-age=${CACHE_SECONDS}`,
};

export const EMPTY_SITEMAP_HEADERS = {
  "Content-Type": "application/xml; charset=utf-8",
  "Cache-Control": `public, max-age=${EMPTY_CACHE_SECONDS}`,
};

export const EMPTY_URLSET =
  '<?xml version="1.0" encoding="UTF-8"?>\n' +
  '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"></urlset>\n';

// Trim a string-or-array config to a clean, non-empty array.
export function normalizeSourceUrls(raw: string | string[]): string[] {
  const arr = Array.isArray(raw) ? raw : [raw];
  return arr.map((s) => s.trim()).filter((s) => s.length > 0);
}

// Only rewrite hosts in <loc>, <image:loc>, and <xhtml:link href>.
const LOC_RE = /(<(?:loc|image:loc)>)([^<]+)(<\/(?:loc|image:loc)>)/g;
const XHTML_HREF_RE = /(<xhtml:link\b[^>]*?\bhref=")([^"]+)(")/g;

export function rewriteSitemapDomain(xml: string, targetOrigin: string): string {
  const target = new URL(targetOrigin);

  const rewriteUrl = (raw: string): string => {
    try {
      const url = new URL(raw.trim());
      url.protocol = target.protocol;
      url.host = target.host;
      return url.toString();
    } catch {
      return raw;
    }
  };

  let out = xml.replace(
    LOC_RE,
    (_m, open, url, close) => `${open}${rewriteUrl(url)}${close}`,
  );
  if (out.includes("xhtml:link")) {
    out = out.replace(
      XHTML_HREF_RE,
      (_m, pre, url, post) => `${pre}${rewriteUrl(url)}${post}`,
    );
  }
  return out;
}

// Returns the entries inside `xml` if it's a <sitemapindex>, else null.
export function parseSitemapIndex(xml: string): string[] | null {
  if (!/<sitemapindex[\s>]/.test(xml)) return null;
  const urls: string[] = [];
  const re = /<loc>([^<]+)<\/loc>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(xml)) !== null) urls.push(m[1].trim());
  return urls;
}

// Fetch a sitemap from `url`. Returns the body only if it looks like XML, so an
// HTML error / parked page can't get served back as a sitemap. null on any
// failure (logged).
export async function fetchUpstream(url: string): Promise<string | null> {
  try {
    const res = await fetch(url, {
      headers: { Accept: "application/xml, text/xml" },
      cf: { cacheTtl: CACHE_SECONDS, cacheEverything: true },
    } as RequestInit);
    if (!res.ok) {
      console.warn(`[sitemap-proxy] upstream ${url} returned ${res.status}`);
      return null;
    }
    const text = await res.text();
    if (/^\s*<\??(xml|urlset|sitemapindex)/i.test(text)) return text;
    console.warn(`[sitemap-proxy] upstream ${url} is not XML; ignoring`);
  } catch (err) {
    console.warn(`[sitemap-proxy] failed to fetch ${url}`, err);
  }
  return null;
}
