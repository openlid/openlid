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
 * @ployComponentDescription Sticky top navigation for OpenLid. Wordmark left,
 * centered anchor links, and right-side actions (GitHub star action + Download
 * button). Collapses to a simple drawer on mobile. Links and the GitHub repo URL
 * are props with sensible defaults.
 */
export interface NavLink {
  label: string;
  href: string;
}

const DEFAULT_LINKS: NavLink[] = [
  { label: "Features", href: sitePath("/#features") },
  { label: "CLI", href: sitePath("/#cli") },
  { label: "Coding agents", href: sitePath("/coding-agents") },
  { label: "Story", href: sitePath("/story") },
  { label: "Privacy", href: sitePath("/#privacy") },
  { label: "Roadmap", href: sitePath("/#roadmap") },
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
    <header className="sticky top-0 z-50 border-b border-white/[0.06] bg-ploy-background-primary/80 backdrop-blur-xl">
      <nav className="mx-auto flex h-16 max-w-6xl items-center justify-between gap-6 px-5 sm:px-8">
        <a href={sitePath("/")} className="shrink-0" aria-label="OpenLid home">
          <Wordmark />
        </a>

        <div className="hidden items-center gap-6 md:flex">
          {links.map((l) => (
            <a
              key={l.href}
              href={l.href}
              className="text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
            >
              {l.label}
            </a>
          ))}
        </div>

        <div className="hidden items-center gap-3 md:flex">
          <a
            href={repoUrl}
            target="_blank"
            rel="noreferrer"
            aria-label="Star OpenLid on GitHub"
            className="inline-flex items-center gap-2 rounded-md border border-white/[0.08] bg-white/[0.02] px-3 py-2 text-sm text-ploy-text-secondary transition-colors hover:text-ploy-text-primary"
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
          className="grid size-10 place-items-center rounded-md border border-white/[0.08] text-ploy-text-primary md:hidden"
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
          className="border-t border-white/[0.06] md:hidden"
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
