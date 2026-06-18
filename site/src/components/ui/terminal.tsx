import { cn } from "@/lib/utils";

/**
 * @ployComponent
 * @ployComponentId OpenLidTerminal
 * @ployComponentType component
 * @ployComponentPattern card
 * @ployComponentStatus stable
 * @ployComponentDescription Rendered terminal/CLI panel (real DOM). Traffic-light
 * header with a title, then a typed list of lines. Each line is { kind } where
 * kind controls color: `prompt` (white with $ caret), `comment` (muted), `ok`
 * (icy-blue ✓), `out` (gray output). Used for install commands and CLI demos.
 * Shared primitive — promoted to ui/ so the home and install pages both consume it.
 */
export type TermLineKind = "prompt" | "comment" | "ok" | "out";

export interface TermLine {
  kind: TermLineKind;
  text: string;
}

export function Terminal({
  title = "openlid — zsh",
  lines,
  className,
}: {
  title?: string;
  lines: TermLine[];
  className?: string;
}) {
  return (
    <div
      className={cn(
        "overflow-hidden rounded-xl border border-white/[0.09] bg-[#0f1011] shadow-xl shadow-black/40",
        className,
      )}
    >
      <div className="flex items-center gap-2 border-b border-white/[0.06] px-4 py-2.5">
        <span className="flex gap-1.5">
          <span className="size-3 rounded-full bg-white/15" />
          <span className="size-3 rounded-full bg-white/15" />
          <span className="size-3 rounded-full bg-white/15" />
        </span>
        <span className="ml-2 font-mono text-[0.72rem] text-ploy-text-secondary">
          {title}
        </span>
      </div>
      <div className="space-y-1.5 px-4 py-4 font-mono text-[0.8rem] leading-relaxed">
        {lines.map((line, i) => (
          <p
            key={i}
            className={cn(
              "whitespace-pre-wrap break-words",
              line.kind === "prompt" && "text-ploy-text-primary",
              line.kind === "comment" && "text-ploy-text-secondary/70",
              line.kind === "ok" && "text-ploy-accent-primary",
              line.kind === "out" && "text-ploy-text-secondary",
            )}
          >
            {line.kind === "prompt" && (
              <span className="mr-2 select-none text-ploy-accent-primary">$</span>
            )}
            {line.kind === "ok" && <span className="mr-2 select-none">✓</span>}
            {line.text}
          </p>
        ))}
      </div>
    </div>
  );
}
