import {
  SeoJson,
  type SeoJsonGraph,
  type SeoJsonNode,
  type SeoJsonSchema,
} from "@/components/seo-json";

const SCHEMA_CONTEXT = "https://schema.org" as const;

interface SEOProps {
  title: string;
  description?: string;
  canonicalUrl?: string;
  imageUrl?: string;
  imageAlt?: string;
  type?: "website" | "article";
  noindex?: boolean;
  siteName?: string;
  jsonLd?: SeoJsonSchema | SeoJsonSchema[];
}

export function SEO({
  title,
  description,
  canonicalUrl,
  imageUrl,
  imageAlt,
  type = "website",
  noindex = false,
  siteName,
  jsonLd,
}: SEOProps) {
  const resolvedSiteName = siteName ?? title;
  const fullTitle =
    title === resolvedSiteName ? title : `${title} | ${resolvedSiteName}`;
  const robots = noindex ? "noindex, nofollow" : "index, follow";
  const twitterCard = imageUrl ? "summary_large_image" : "summary";

  const graph = noindex
    ? []
    : buildJsonLdGraph({
        fullTitle,
        description,
        canonicalUrl,
        imageUrl,
        type,
        siteName: resolvedSiteName,
        extras: jsonLd,
      });

  return (
    <>
      <title>{fullTitle}</title>
      {description && <meta name="description" content={description} />}
      <meta name="robots" content={robots} />
      {canonicalUrl && <link rel="canonical" href={canonicalUrl} />}

      <meta property="og:type" content={type} />
      <meta property="og:title" content={fullTitle} />
      <meta property="og:site_name" content={resolvedSiteName} />
      {description && <meta property="og:description" content={description} />}
      {canonicalUrl && <meta property="og:url" content={canonicalUrl} />}
      {imageUrl && <meta property="og:image" content={imageUrl} />}
      {imageAlt && <meta property="og:image:alt" content={imageAlt} />}

      <meta name="twitter:card" content={twitterCard} />
      <meta name="twitter:title" content={fullTitle} />
      {description && <meta name="twitter:description" content={description} />}
      {imageUrl && <meta name="twitter:image" content={imageUrl} />}

      <SeoJson schemas={graph} />
    </>
  );
}

interface BuildGraphArgs {
  fullTitle: string;
  description?: string;
  canonicalUrl?: string;
  imageUrl?: string;
  type: "website" | "article";
  siteName: string;
  extras?: SeoJsonSchema | SeoJsonSchema[];
}

function buildJsonLdGraph({
  fullTitle,
  description,
  canonicalUrl,
  imageUrl,
  type,
  siteName,
  extras,
}: BuildGraphArgs): SeoJsonSchema[] {
  const siteUrl = canonicalUrl ? getSiteRoot(canonicalUrl) : undefined;

  const defaults = [
    {
      "@context": SCHEMA_CONTEXT,
      "@type": "Organization",
      name: siteName,
      ...(siteUrl && { url: siteUrl }),
    },
    {
      "@context": SCHEMA_CONTEXT,
      "@type": "WebSite",
      name: siteName,
      ...(siteUrl && { url: siteUrl }),
    },
    {
      "@context": SCHEMA_CONTEXT,
      "@type": type === "article" ? "Article" : "WebPage",
      name: fullTitle,
      ...(description && { description }),
      ...(canonicalUrl && { url: canonicalUrl }),
      ...(imageUrl && { image: imageUrl }),
    },
  ];

  const extraNodes = extras ? (Array.isArray(extras) ? extras : [extras]) : [];
  const overridden = new Set(
    extraNodes.flatMap((node) =>
      isGraph(node) ? node["@graph"].map(getType) : [getType(node)],
    ),
  );

  return [
    ...defaults.filter((n) => !overridden.has(getType(n))),
    ...extraNodes,
  ];
}

function getSiteRoot(canonicalUrl: string): string {
  const url = new URL(canonicalUrl);
  const basePath = import.meta.env.BASE_URL.replace(/\/$/, "");

  url.pathname = basePath || "/";
  url.search = "";
  url.hash = "";

  return url.href;
}

function isGraph(node: SeoJsonSchema): node is SeoJsonGraph {
  return "@graph" in node;
}

function getType(node: SeoJsonNode): string | undefined {
  const type = node["@type"];
  return typeof type === "string" ? type : type?.[0];
}
