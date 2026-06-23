import { useState } from "react";
import { Menu, Star, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { GitHubMark } from "@/components/ui/github-mark";
import { Wordmark } from "@/components/ui/wordmark";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidNavbar
 * @ployComponentType section
 * @ployComponentPattern navbar
 * @ployComponentStatus stable
 * @ployComponentDescription Sticky top navigation for OpenLid. Hallmark N8
 * terminal-command voice: wordmark, command-flag link strip, and right-side
 * GitHub + Download actions. Collapses to a simple drawer on mobile. Links and
 * the GitHub repo URL are props with sensible defaults.
 */
export interface NavLink {
  label: string;
  href: string;
}

const DEFAULT_LINKS: NavLink[] = [
  { label: "Features", href: sitePath("/#features") },
  { label: "CLI", href: sitePath("/#cli") },
  { label: "Coding agents", href: sitePath("/coding-agents") },
  { label: "Install", href: sitePath("/install") },
  { label: "Story", href: sitePath("/story") },
];

export function Navbar({
  links = DEFAULT_LINKS,
  repoUrl = "https://github.com/openlid/openlid",
  downloadUrl = "https://github.com/openlid/openlid/releases/latest",
  githubLabel = "Star",
}: {
  links?: NavLink[];
  repoUrl?: string;
  downloadUrl?: string;
  githubLabel?: string;
}) {
  const [open, setOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 border-b border-white/[0.08] bg-ploy-background-primary/88 backdrop-blur-xl">
      <nav className="mx-auto flex min-h-16 max-w-6xl items-center justify-between gap-5 px-5 py-2 sm:px-8">
        <a href={sitePath("/")} className="shrink-0" aria-label="OpenLid home">
          <Wordmark />
        </a>

        <div className="hidden min-w-0 flex-1 justify-center lg:flex">
          <div className="flex max-w-full items-center gap-3 overflow-hidden rounded-md border border-white/[0.08] bg-white/[0.025] px-3 py-2 font-mono text-[0.76rem] text-ploy-text-secondary">
            <span className="shrink-0 text-ploy-accent-primary">$</span>
            <span className="shrink-0 text-ploy-text-primary">openlid</span>
            <span className="shrink-0 text-ploy-text-secondary/70">--</span>
            {links.map((l) => (
              <a
                key={l.href}
                href={l.href}
                className="shrink-0 whitespace-nowrap transition-colors hover:text-ploy-text-primary"
              >
                {l.label.toLowerCase().replace(/\s+/g, "-")}
              </a>
            ))}
          </div>
        </div>

        <div className="hidden items-center gap-3 lg:flex">
          <a
            href={repoUrl}
            target="_blank"
            rel="noreferrer"
            aria-label="Star OpenLid on GitHub"
            className="inline-flex min-h-10 items-center gap-2 whitespace-nowrap rounded-md border border-white/[0.08] bg-white/[0.02] px-3 py-2 text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ploy-accent-primary/60"
          >
            <GitHubMark className="size-4" />
            <span className="inline-flex items-center gap-1">
              <Star className="size-3.5 fill-current text-ploy-accent-primary" />
              {githubLabel}
            </span>
          </a>
          <Button asChild size="sm">
            <a href={downloadUrl} target="_blank" rel="noreferrer">
              Download
            </a>
          </Button>
        </div>

        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="grid size-11 place-items-center rounded-md border border-white/[0.08] text-ploy-text-primary lg:hidden"
          aria-label="Toggle menu"
          aria-controls="mobile-navigation"
          aria-expanded={open}
        >
          {open ? <X className="size-5" /> : <Menu className="size-5" />}
        </button>
      </nav>

      {open && (
        <div
          id="mobile-navigation"
          className="border-t border-white/[0.06] lg:hidden"
        >
          <div className="flex flex-col gap-1 px-5 py-4">
            {links.map((l) => (
              <a
                key={l.href}
                href={l.href}
                onClick={() => setOpen(false)}
                className="rounded-md px-2 py-2.5 text-sm text-ploy-text-secondary hover:bg-white/[0.04] hover:text-ploy-text-primary"
              >
                {l.label}
              </a>
            ))}
            <Button asChild size="sm" className="mt-2">
              <a href={downloadUrl} target="_blank" rel="noreferrer">
                Download for macOS
              </a>
            </Button>
          </div>
        </div>
      )}
    </header>
  );
}
