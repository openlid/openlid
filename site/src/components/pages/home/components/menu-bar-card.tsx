import { Check, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * @ployComponent
 * @ployComponentId OpenLidMenuBarCard
 * @ployComponentType section
 * @ployComponentPattern card
 * @ployComponentStatus stable
 * @ployComponentDescription Rendered mock of the OpenLid macOS menu-bar dropdown
 * (real DOM, not a screenshot). Shows the live status header with an icy-blue
 * "awake" dot, a status detail line, and the menu rows (Turn Off, Preferences,
 * Schedule). Used as the hero product artifact and reusable elsewhere. Acts as a
 * brand "product object" — keep the frosted dark surface and hairline borders.
 */
export function MenuBarCard({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "w-full max-w-[340px] overflow-hidden rounded-2xl border border-white/[0.09] bg-[#161719]/90 shadow-2xl shadow-black/60 backdrop-blur-xl",
        className,
      )}
    >
      {/* status header */}
      <div className="flex items-center justify-between border-b border-white/[0.06] px-4 py-3">
        <div className="flex items-center gap-2.5">
          <svg viewBox="0 0 24 24" fill="none" className="size-4 text-ploy-text-primary" aria-hidden>
            <path d="M5 5.5h14v8H5z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
            <path d="M2.5 17.5 4 13.5h16l1.5 4a1 1 0 0 1-.94 1.35H3.44A1 1 0 0 1 2.5 17.5Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
          </svg>
          <span className="font-mono text-[0.78rem] text-ploy-text-primary">OpenLid</span>
        </div>
        <span className="inline-flex items-center gap-1.5 font-mono text-[0.72rem] text-ploy-accent-primary">
          <span className="relative flex size-1.5">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-ploy-accent-primary/70" />
            <span className="relative inline-flex size-1.5 rounded-full bg-ploy-accent-primary" />
          </span>
          Active
        </span>
      </div>

      {/* status detail */}
      <div className="px-4 py-3">
        <p className="text-[0.82rem] font-medium text-ploy-text-primary">
          Active — indefinite
        </p>
        <p className="mt-0.5 font-mono text-[0.72rem] text-ploy-text-secondary">
          lid closed · AC · display off
        </p>
      </div>

      <div className="h-px bg-white/[0.06]" />

      {/* menu rows */}
      <div className="px-1.5 py-1.5 text-[0.82rem]">
        <Row label="Turn Off" />
        <Row label="Preferences…" trailing="⌘," />
      </div>

      <div className="h-px bg-white/[0.06]" />

      {/* schedule */}
      <div className="flex items-center justify-between px-4 py-3">
        <div>
          <p className="flex items-center gap-1.5 text-[0.8rem] text-ploy-text-primary">
            <Check className="size-3.5 text-ploy-accent-primary" />
            Schedule
          </p>
          <p className="mt-0.5 font-mono text-[0.72rem] text-ploy-text-secondary">
            08:00–18:00 · Mon–Fri
          </p>
        </div>
        <ChevronRight className="size-4 text-ploy-text-secondary/60" />
      </div>
    </div>
  );
}

function Row({ label, trailing }: { label: string; trailing?: string }) {
  return (
    <div className="flex items-center justify-between rounded-lg px-2.5 py-2 text-ploy-text-primary transition-colors hover:bg-white/[0.05]">
      <span>{label}</span>
      {trailing && (
        <span className="font-mono text-[0.72rem] text-ploy-text-secondary">
          {trailing}
        </span>
      )}
    </div>
  );
}
