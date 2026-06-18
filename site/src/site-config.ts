interface SiteConfig {
  name: string;
  description: string;
  /** Default OG/Twitter share image (path resolved against the site origin). */
  ogImage: string;
  /** Canonical source repository — used for JSON-LD sameAs / download links. */
  repoUrl: string;
  // URL(s) of an existing sitemap to mirror at /sitemap.xml with hosts
  // rewritten to this site's domain. Empty to disable.
  sourceSitemapUrl: string | string[];
}

export const SITE_CONFIG: SiteConfig = {
  name: "OpenLid",
  description:
    "Keep your laptop awake — even with the lid closed. OpenLid is a tiny, privacy-first macOS menu bar utility for builds, coding agents, downloads, and remote access. Signed, notarized, open source.",
  ogImage: "/og.png",
  repoUrl: "https://github.com/openlid/openlid",
  sourceSitemapUrl: "",
};
