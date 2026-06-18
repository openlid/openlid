import { cn } from "@/lib/utils";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidWordmark
 * @ployComponentType component
 * @ployComponentPattern logo
 * @ployComponentStatus stable
 * @ployComponentDescription OpenLid wordmark: the app icon plus the OpenLid name
 * set in heading font. Used in the navbar and footer. `compact` hides the text
 * for tight spaces.
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
      <img
        src={sitePath("/icon-192.png")}
        alt=""
        aria-hidden="true"
        width={28}
        height={28}
        className="size-7 shrink-0"
      />
      {!compact && (
        <span className="font-heading text-[1.15rem] font-bold tracking-tight text-ploy-text-primary">
          OpenLid
        </span>
      )}
    </span>
  );
}
