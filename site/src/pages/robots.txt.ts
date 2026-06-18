import type { APIRoute } from "astro";

import { SITE_CONFIG } from "@/site-config";
import { normalizeSourceUrls } from "@/lib/sitemap/shared";

export const prerender = true;

// AI / answer-engine crawlers we explicitly welcome (GEO/AEO). Being listed by
// name makes the allow-intent unambiguous for each bot's parser.
const AI_CRAWLERS = [
  "GPTBot",
  "OAI-SearchBot",
  "ChatGPT-User",
  "ClaudeBot",
  "Claude-Web",
  "anthropic-ai",
  "PerplexityBot",
  "Perplexity-User",
  "Google-Extended",
  "Applebot-Extended",
  "Amazonbot",
  "Bytespider",
  "CCBot",
  "cohere-ai",
  "Meta-ExternalAgent",
  "DuckAssistBot",
];

const basePath = import.meta.env.BASE_URL.replace(/\/$/, "");

const withBase = (path: string) => {
  const normalized = path.startsWith("/") ? path : `/${path}`;

  if (!basePath || normalized === basePath || normalized.startsWith(`${basePath}/`)) {
    return normalized;
  }

  return `${basePath}${normalized}`;
};

export const GET: APIRoute = ({ site }) => {
  const sitemap = site
    ? [
        `Sitemap: ${new URL(withBase("/sitemap-index.xml"), site).href}`,
        // Mirrored sitemap, only when sourceSitemapUrl is configured.
        ...(normalizeSourceUrls(SITE_CONFIG.sourceSitemapUrl).length > 0
          ? [`Sitemap: ${new URL(withBase("/sitemap.xml"), site).href}`]
          : []),
      ].join("\n") + "\n"
    : "";

  const searchBots = ["Googlebot", "Bingbot", "Twitterbot", "facebookexternalhit"]
    .map((ua) => `User-agent: ${ua}\nAllow: /\n`)
    .join("\n");

  const aiBots = AI_CRAWLERS.map((ua) => `User-agent: ${ua}\nAllow: /\n`).join(
    "\n",
  );

  const body = `${searchBots}
${aiBots}
User-agent: *
Allow: /

${sitemap}`;

  return new Response(body, {
    headers: { "Content-Type": "text/plain" },
  });
};
