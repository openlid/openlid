import { cn } from "@/lib/utils";

/**
 * @ployComponent
 * @ployComponentId OpenLidWordmark
 * @ployComponentType component
 * @ployComponentPattern logo
 * @ployComponentStatus stable
 * @ployComponentDescription OpenLid wordmark: a small open-laptop glyph with a
 * faint icy-blue "awake" glow plus the OpenLid name set in heading font. Used in
 * the navbar and footer. `compact` hides the text for tight spaces.
 */
export function Wordmark({
  className,
  compact = false,
}: {
  className?: string;
  compact?: boolean;
}) {
  return (
    <span className={cn("inline-flex items-center gap-2.5", className)}>
      <span className="relative grid size-7 place-items-center">
        <span className="absolute inset-0 rounded-full bg-ploy-accent-primary/20 blur-md" />
        <svg
          viewBox="0 0 24 24"
          fill="none"
          className="relative size-6 text-ploy-text-primary"
          aria-hidden="true"
        >
          {/* open laptop */}
          <path
            d="M5 5.5h14v8H5z"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinejoin="round"
          />
          <path
            d="M2.5 17.5 4 13.5h16l1.5 4a1 1 0 0 1-.94 1.35H3.44A1 1 0 0 1 2.5 17.5Z"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinejoin="round"
          />
          {/* awake indicator */}
          <circle cx="12" cy="9.5" r="1.6" className="fill-ploy-accent-primary" />
        </svg>
      </span>
      {!compact && (
        <span className="font-heading text-[1.15rem] font-bold tracking-tight text-ploy-text-primary">
          OpenLid
        </span>
      )}
    </span>
  );
}
