// @ts-check
import { defineConfig } from "astro/config";
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
