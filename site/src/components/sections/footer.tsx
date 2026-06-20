import { GitHubMark } from "@/components/ui/github-mark";
import { Wordmark } from "@/components/ui/wordmark";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidFooter
 * @ployComponentType section
 * @ployComponentPattern footer
 * @ployComponentStatus stable
 * @ployComponentDescription Quiet footer for OpenLid: wordmark + one-line
 * description on the left, grouped link columns (Product / Open source) on the
 * right, and a bottom meta row with license + platform note. Dark hairline
 * dividers, no closing CTA (the final CTA section owns that).
 */
interface FooterColumn {
  title: string;
  links: { label: string; href: string }[];
}

const DEFAULT_COLUMNS: FooterColumn[] = [
  {
    title: "Product",
    links: [
      { label: "Features", href: sitePath("/#features") },
      { label: "CLI", href: sitePath("/#cli") },
      { label: "Coding agents", href: sitePath("/coding-agents") },
      { label: "Install", href: sitePath("/install") },
      { label: "Story", href: sitePath("/story") },
      { label: "Privacy", href: sitePath("/#privacy") },
      { label: "Roadmap", href: sitePath("/#roadmap") },
    ],
  },
  {
    title: "Open source",
    links: [
      { label: "GitHub", href: "https://github.com/openlid/openlid" },
      {
        label: "Releases",
        href: "https://github.com/openlid/openlid/releases",
      },
      {
        label: "Report an issue",
        href: "https://github.com/openlid/openlid/issues",
      },
      {
        label: "License (Apache 2.0)",
        href: "https://github.com/openlid/openlid/blob/main/LICENSE",
      },
    ],
  },
];

export function Footer({
  columns = DEFAULT_COLUMNS,
  repoUrl = "https://github.com/openlid/openlid",
}: {
  columns?: FooterColumn[];
  repoUrl?: string;
}) {
  const isExternalHref = (href: string) => /^[a-z][a-z\d+.-]*:/i.test(href);

  return (
    <footer className="border-t border-white/[0.06] bg-ploy-background-primary">
      <div className="mx-auto max-w-6xl px-5 py-14 sm:px-8">
        <div className="grid gap-10 md:grid-cols-[1.4fr_1fr_1fr]">
          <div className="max-w-xs">
            <Wordmark />
            <p className="mt-4 text-sm leading-relaxed text-ploy-text-secondary">
              A tiny macOS menu bar utility that keeps your laptop awake — even
              with the lid closed. No telemetry, ever.
            </p>
            <a
              href={repoUrl}
              target="_blank"
              rel="noreferrer"
              className="mt-5 inline-flex items-center gap-2 text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
            >
              <GitHubMark className="size-4" />
              github.com/openlid
            </a>
          </div>

          {columns.map((col) => (
            <div key={col.title}>
              <h3 className="font-eyebrow text-xs uppercase tracking-[0.16em] text-ploy-text-secondary/80">
                {col.title}
              </h3>
              <ul className="mt-4 space-y-3">
                {col.links.map((l) => (
                  <li key={l.label}>
                    <a
                      href={l.href}
                      target={isExternalHref(l.href) ? "_blank" : undefined}
                      rel={isExternalHref(l.href) ? "noreferrer" : undefined}
                      className="text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
                    >
                      {l.label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-12 flex flex-col gap-2 border-t border-white/[0.06] pt-6 text-xs text-ploy-text-secondary/80 sm:flex-row sm:items-center sm:justify-between">
          <p>© {new Date().getFullYear()} OpenLid. Apache License 2.0.</p>
          <p className="font-mono">macOS 13+ · Apple Silicon · Linux planned</p>
        </div>
      </div>
    </footer>
  );
}
