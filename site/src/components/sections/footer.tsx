import { GitHubMark } from "@/components/ui/github-mark";
import { Wordmark } from "@/components/ui/wordmark";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidFooter
 * @ployComponentType section
 * @ployComponentPattern footer
 * @ployComponentStatus stable
 * @ployComponentDescription Hallmark Ft5 statement footer for OpenLid: one
 * closing display sentence, a short product note, minimal inline links, and a
 * compact meta row. Dark hairline dividers, no duplicate CTA (the final CTA
 * section owns that).
 */
const DEFAULT_LINKS = [
  { label: "Features", href: sitePath("/#features") },
  { label: "CLI", href: sitePath("/#cli") },
  { label: "Coding agents", href: sitePath("/coding-agents") },
  { label: "Install", href: sitePath("/install") },
  { label: "Story", href: sitePath("/story") },
  { label: "GitHub", href: "https://github.com/openlid/openlid" },
];

export function Footer({
  links = DEFAULT_LINKS,
  repoUrl = "https://github.com/openlid/openlid",
}: {
  links?: { label: string; href: string }[];
  repoUrl?: string;
}) {
  const isExternalHref = (href: string) => /^[a-z][a-z\d+.-]*:/i.test(href);

  return (
    <footer className="border-t border-white/[0.06] bg-ploy-background-primary">
      <div className="mx-auto max-w-6xl px-5 py-14 sm:px-8 sm:py-18">
        <div className="grid gap-8 lg:grid-cols-[1.3fr_0.7fr] lg:items-end">
          <div>
            <Wordmark />
            <p className="mt-7 max-w-2xl font-heading text-4xl font-semibold leading-[1.04] tracking-[-0.035em] text-ploy-text-primary sm:text-5xl">
              Close the lid. Keep the work local.
            </p>
          </div>

          <div className="lg:justify-self-end">
            <p className="max-w-sm text-sm leading-relaxed text-ploy-text-secondary">
              A tiny macOS menu bar utility for closed-lid work, remote
              sessions, and coding agents. No telemetry, ever.
            </p>
            <a
              href={repoUrl}
              target="_blank"
              rel="noreferrer"
              className="mt-5 inline-flex items-center gap-2 whitespace-nowrap text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
            >
              <GitHubMark className="size-4" />
              github.com/openlid
            </a>
          </div>
        </div>

        <div className="mt-10 flex flex-wrap gap-x-5 gap-y-3 border-t border-white/[0.06] pt-6">
          {links.map((l) => (
            <a
              key={l.href}
              href={l.href}
              target={isExternalHref(l.href) ? "_blank" : undefined}
              rel={isExternalHref(l.href) ? "noreferrer" : undefined}
              className="whitespace-nowrap text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
            >
              {l.label}
            </a>
          ))}
        </div>

        <div className="mt-8 flex flex-col gap-2 text-xs text-ploy-text-secondary/80 sm:flex-row sm:items-center sm:justify-between">
          <p>© {new Date().getFullYear()} OpenLid. Apache License 2.0.</p>
          <p className="font-mono">macOS 13+ · Apple Silicon · Linux planned</p>
        </div>
      </div>
    </footer>
  );
}
