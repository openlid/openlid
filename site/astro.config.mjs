// @ts-check
import { defineConfig, fontProviders } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import { sitemapWithCustomPages } from "./src/lib/sitemap/sitemap-with-custom-pages-plugin.ts";

const isGithubPages = process.env.GITHUB_PAGES === "true";
const site = process.env.SITE_URL ?? (isGithubPages ? "https://openlid.github.io" : "http://localhost:3000");
const base = process.env.SITE_BASE ?? (isGithubPages ? "/openlid" : "/");

// Separate vite cache dirs so `astro dev` and `astro build`/`check` don't conflict.
const astroCommand = process.argv.slice(2).find((arg) => !arg.startsWith("-"));
const viteCacheDir =
  astroCommand === "dev" || astroCommand === "preview"
    ? "node_modules/.vite-dev"
    : "node_modules/.vite-build";

// https://astro.build/config
export default defineConfig({
  site,
  base,
  output: "static",
  trailingSlash: "never",
  build: {
    assets: "_astro",
  },
  // Self-hosted fonts: Astro downloads these at build time and serves them from
  // this origin, so a visitor's browser never calls Google Fonts (keeps the
  // "no telemetry / nothing leaves your machine" promise true for the site
  // itself) and there's no render-blocking third-party request. Only the
  // weights actually used in the design are fetched.
  fonts: [
    {
      provider: fontProviders.google(),
      name: "Inter",
      cssVariable: "--font-inter",
      weights: [400, 500, 600],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["system-ui", "sans-serif"],
    },
    {
      provider: fontProviders.google(),
      name: "Manrope",
      cssVariable: "--font-manrope",
      weights: [500, 600, 700, 800],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["system-ui", "sans-serif"],
    },
    {
      provider: fontProviders.google(),
      name: "IBM Plex Mono",
      cssVariable: "--font-ibm-plex-mono",
      weights: [400, 500],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["ui-monospace", "monospace"],
    },
  ],
  integrations: [
    mdx(),
    react(),
    // For SSR-only dynamic routes, edit src/lib/sitemap/get-sitemap-paths.ts.
    ...sitemapWithCustomPages(),
  ],
  vite: {
    cacheDir: viteCacheDir,
    plugins: [tailwindcss()],
    server: {
      strictPort: true,
    },
  },
  devToolbar: {
    enabled: false,
  },
});
