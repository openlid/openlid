# AGENTS.md

Conventions for the OpenLid marketing site. The repository root is still the
Rust application; this directory is the isolated Astro site published to GitHub
Pages.

## Commands

```bash
npm run dev             # local dev server at localhost:3000
npm run check           # Astro typecheck
npm run lint            # ESLint
npm run build           # static build for local/root hosting
npm run build:pages     # static build with GitHub Pages /openlid base path
npm run verify          # check + build
```

`npm run verify` is the gate for site changes.

## Stack

| Layer      | Tool                          |
| ---------- | ----------------------------- |
| Framework  | Astro 6 static output         |
| UI         | React 19                      |
| Types      | TypeScript                    |
| Styling    | Tailwind CSS v4               |
| Components | CVA                           |
| Animations | motion/react                  |
| Icons      | lucide-react                  |
| Deploy     | GitHub Pages from `site/dist` |

## Routing

Astro routes live in `src/pages/`. `.astro` files should stay thin and mount
React page/section components from `src/components/`.

GitHub Pages is static hosting. Do not add request-time SSR routes, middleware,
Cloudflare adapter config, or Wrangler deploy scripts. Dynamic content routes
must use `getStaticPaths()` so every published page is generated at build time.

The Pages workflow builds with `GITHUB_PAGES=true`, which sets Astro's base path
to `/openlid`. Any manually authored root asset links must use
`import.meta.env.BASE_URL` or a local helper that applies it.

## Organization

```text
src/pages/              # Astro routes
src/components/pages/   # page-local React composition and sections
src/components/sections # shared page sections
src/components/ui/      # shared primitives
src/lib/                # helpers
src/styles/globals.css  # Tailwind v4 config + theme tokens
```

Keep components local until a second consumer needs them. Promote only when
there is a real reuse point.

## Style

- Match the existing dark OpenLid design language.
- Use the `ploy-*` theme tokens already defined in `globals.css`.
- Use `lucide-react` for icons.
- Respect reduced motion through the existing `MotionConfig` pattern.
- Keep public assets in `public/`; avoid external runtime dependencies for core
  visuals.
