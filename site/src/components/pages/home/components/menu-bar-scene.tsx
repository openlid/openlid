import { Battery, Search, Wifi } from "lucide-react";
import { cn } from "@/lib/utils";
import { MenuBarCard } from "./menu-bar-card";

/**
 * @ployComponent
 * @ployComponentId OpenLidMenuBarScene
 * @ployComponentType component
 * @ployComponentPattern card
 * @ployComponentStatus stable
 * @ployComponentDescription Grounds the MenuBarCard dropdown in its real
 * environment: a slice of the macOS menu bar with the OpenLid lid icon shown
 * active (accent-tinted, live ping) in the status tray, and the dropdown opened
 * beneath it over a quiet desktop with a faint icy-blue corner bloom. All
 * DOM/SVG — no screenshot. Used as the hero product object.
 */
function LidGlyph({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden>
      <path
        d="M5 5.5h14v8H5z"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      <path
        d="M2.5 17.5 4 13.5h16l1.5 4a1 1 0 0 1-.94 1.35H3.44A1 1 0 0 1 2.5 17.5Z"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function MenuBarScene({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "relative w-full overflow-hidden rounded-2xl border border-white/[0.09] bg-[#0a0b0c] shadow-2xl shadow-black/60",
        className,
      )}
    >
      {/* desktop wallpaper hint — faint accent bloom in the status-tray corner */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-[radial-gradient(120%_90%_at_85%_-15%,rgba(143,179,217,0.12),transparent_55%)]"
      />

      {/* menu bar */}
      <div className="relative flex h-7 items-center justify-between border-b border-white/[0.08] bg-black/40 px-3 backdrop-blur-md">
        <div className="flex items-center gap-3 font-mono text-[0.7rem]">
          <span className="font-semibold text-ploy-text-primary">Finder</span>
          <span className="hidden text-ploy-text-secondary sm:inline">File</span>
          <span className="hidden text-ploy-text-secondary sm:inline">Edit</span>
          <span className="hidden text-ploy-text-secondary sm:inline">View</span>
        </div>
        <div className="flex items-center gap-2.5 text-ploy-text-secondary">
          <Search className="size-3.5" strokeWidth={1.6} />
          <Wifi className="size-3.5" strokeWidth={1.6} />
          <Battery className="size-4" strokeWidth={1.6} />
          {/* active OpenLid status item — the open menu below sits under it */}
          <span className="inline-flex items-center gap-1 rounded-[5px] bg-ploy-accent-primary/15 px-1.5 py-0.5 text-ploy-accent-primary ring-1 ring-inset ring-ploy-accent-primary/30">
            <LidGlyph className="size-3.5" />
            <span className="relative flex size-1">
              <span className="absolute inline-flex size-full animate-ping rounded-full bg-ploy-accent-primary/70" />
              <span className="relative inline-flex size-1 rounded-full bg-ploy-accent-primary" />
            </span>
          </span>
          <span className="font-mono text-[0.7rem]">Fri 9:41</span>
        </div>
      </div>

      {/* desktop with the open menu anchored to the top-right status area */}
      <div className="relative flex min-h-[300px] items-start justify-end p-3 sm:min-h-[340px] sm:p-4">
        <div className="w-[300px] max-w-full sm:w-[340px]">
          <MenuBarCard className="w-full" />
        </div>
      </div>
    </div>
  );
}
