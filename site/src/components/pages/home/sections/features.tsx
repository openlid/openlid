import {
  BatteryCharging,
  CalendarClock,
  MonitorOff,
  MousePointerClick,
  Sun,
  TerminalSquare,
  type LucideIcon,
} from "lucide-react";

/**
 * @ployComponent
 * @ployComponentId OpenLidFeatures
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Feature spec sheet for OpenLid. One large control
 * promise plus a tabular set of smaller capabilities, avoiding the equal 3-card
 * feature-grid pattern. Items are prop-overridable via the features array.
 * Static (always visible).
 */
interface Feature {
  icon: LucideIcon;
  title: string;
  body: string;
}

const DEFAULT_FEATURES: Feature[] = [
  {
    icon: MousePointerClick,
    title: "One-click toggle",
    body: "Left-click the menu bar icon to arm or disarm. Right-click for the full menu and status.",
  },
  {
    icon: CalendarClock,
    title: "Recurring schedule",
    body: "Keep sleep prevention active only during set hours — e.g. 08:00–18:00 on weekdays. UI or CLI.",
  },
  {
    icon: Sun,
    title: "Display stays awake",
    body: "No idle dim, no screen lock while active — including closed-lid VNC. Opt out anytime.",
  },
  {
    icon: MonitorOff,
    title: "Display off on lid close",
    body: "Turns the built-in panel off to save battery and thermals — skipped when an external display is attached.",
  },
  {
    icon: BatteryCharging,
    title: "Battery & transit safeguards",
    body: "Auto-deactivate below a battery threshold, or when OpenLid detects the laptop is packed away.",
  },
  {
    icon: TerminalSquare,
    title: "First-class CLI",
    body: "Script everything: openlid on / off / status / schedule. Machine-readable --json output.",
  },
];

export function Features({
  features = DEFAULT_FEATURES,
}: {
  features?: Feature[];
}) {
  const [primaryFeature, ...secondaryFeatures] = features;

  return (
    <section
      id="features"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <h2 className="font-heading typography-heading text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Small utility. Serious control.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            Everything you'd want from a sleep-prevention tool, and nothing you
            wouldn't — native, precise, and out of your way.
          </p>
        </div>

        <div className="mt-12 grid gap-5 lg:grid-cols-[1.05fr_1.4fr]">
          {primaryFeature && (
            <div className="rounded-2xl border border-white/[0.08] bg-ploy-background-secondary p-7 sm:p-8">
              <primaryFeature.icon
                className="size-6 text-ploy-accent-primary"
                strokeWidth={1.6}
              />
              <p className="mt-8 font-mono text-[0.72rem] uppercase tracking-[0.16em] text-ploy-text-secondary">
                Primary control
              </p>
              <h3 className="mt-3 max-w-sm text-2xl font-semibold tracking-[-0.02em] text-ploy-text-primary">
                {primaryFeature.title}
              </h3>
              <p className="mt-4 text-sm leading-relaxed text-ploy-text-secondary">
                {primaryFeature.body}
              </p>
            </div>
          )}

          <div className="overflow-hidden rounded-2xl border border-white/[0.08]">
            {secondaryFeatures.map((f) => (
              <div
                key={f.title}
                className="grid gap-4 border-b border-white/[0.06] bg-ploy-background-primary p-5 last:border-b-0 sm:grid-cols-[2.4rem_0.8fr_1.2fr] sm:items-start sm:p-6"
              >
                <div className="grid size-10 place-items-center rounded-md border border-white/[0.08] bg-white/[0.02] text-ploy-accent-primary">
                  <f.icon className="size-5" strokeWidth={1.6} />
                </div>
                <h3 className="text-base font-semibold text-ploy-text-primary">
                  {f.title}
                </h3>
                <p className="text-sm leading-relaxed text-ploy-text-secondary">
                  {f.body}
                </p>
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
