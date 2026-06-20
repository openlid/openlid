import { Check, CircleDashed } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * @ployComponent
 * @ployComponentId OpenLidRoadmap
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Roadmap timeline. Left-aligned heading + a vertical
 * list of milestones, each with a status (shipped = icy-blue check, planned =
 * dashed circle), version tag, title, and note. Mirrors the README roadmap
 * (macOS shipped, Linux v3.0.0 planned, Windows on demand). Items prop-driven.
 * Static (always visible).
 */
interface Milestone {
  status: "shipped" | "planned";
  version: string;
  title: string;
  body: string;
}

const DEFAULT_MILESTONES: Milestone[] = [
  {
    status: "shipped",
    version: "v2.3.2",
    title: "macOS — stable",
    body: "Menu bar app, first-class CLI, recurring schedules, battery & transit safeguards. Signed, notarized, Homebrew tap. Locked API under semver.",
  },
  {
    status: "planned",
    version: "v3.0.0",
    title: "Linux support",
    body: "A logind backend over D-Bus, wired into the cross-platform openlid-core traits. Tray icon or headless daemon driven by the CLI.",
  },
  {
    status: "planned",
    version: "Future",
    title: "Windows — on demand",
    body: "SetThreadExecutionState + power-broadcast backend. The core traits are already cross-platform-shaped, so this is an addition, not a rewrite.",
  },
];

export function Roadmap({
  milestones = DEFAULT_MILESTONES,
}: {
  milestones?: Milestone[];
}) {
  return (
    <section
      id="roadmap"
      className="border-t border-white/[0.06] bg-ploy-background-secondary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <h2 className="font-heading typography-heading text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            macOS today. More platforms next.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            Built in Rust with a platform-abstraction core, so new operating
            systems are a backend addition — not a rebuild.
          </p>
        </div>

        <ol className="mt-12 space-y-px overflow-hidden rounded-2xl border border-white/[0.07]">
          {milestones.map((m, i) => (
            <li key={m.title} className="flex gap-5 bg-white/[0.015] p-6 sm:p-7">
              <div className="flex flex-col items-center">
                <span
                  className={cn(
                    "grid size-9 shrink-0 place-items-center rounded-full border",
                    m.status === "shipped"
                      ? "border-ploy-accent-primary/40 bg-ploy-accent-primary/10 text-ploy-accent-primary"
                      : "border-white/10 text-ploy-text-secondary",
                  )}
                >
                  {m.status === "shipped" ? (
                    <Check className="size-4" />
                  ) : (
                    <CircleDashed className="size-4" />
                  )}
                </span>
                {i < milestones.length - 1 && (
                  <span className="mt-2 w-px flex-1 bg-white/[0.08]" />
                )}
              </div>
              <div className="pb-1">
                <div className="flex flex-wrap items-center gap-3">
                  <h3 className="text-lg font-semibold text-ploy-text-primary">
                    {m.title}
                  </h3>
                  <span
                    className={cn(
                      "rounded-full border px-2 py-0.5 font-mono text-[0.68rem]",
                      m.status === "shipped"
                        ? "border-ploy-accent-primary/30 text-ploy-accent-primary"
                        : "border-white/10 text-ploy-text-secondary",
                    )}
                  >
                    {m.version}
                  </span>
                  <span className="font-mono text-[0.68rem] uppercase tracking-wide text-ploy-text-secondary/80">
                    {m.status}
                  </span>
                </div>
                <p className="mt-2 max-w-2xl text-sm leading-relaxed text-ploy-text-secondary">
                  {m.body}
                </p>
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
