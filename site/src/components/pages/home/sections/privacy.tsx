import { motion } from "motion/react";
import { ShieldCheck } from "lucide-react";

/**
 * @ployComponent
 * @ployComponentId OpenLidPrivacy
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Privacy statement section. Centered oversized claim
 * ("No telemetry. No data leaves your machine. Ever.") with three supporting
 * one-liners. Quiet dark surface, single icy-blue glyph. Reinforces the
 * open-source, local-only promise.
 */
const POINTS = [
  {
    title: "Zero telemetry",
    body: "No analytics, no tracking, no phone-home. The app only touches the network when you run openlid update.",
  },
  {
    title: "Everything stays local",
    body: "All state lives in ~/Library/Application Support on your machine. Nothing is collected or transmitted.",
  },
  {
    title: "Open source & auditable",
    body: "Apache-2.0, signed and notarized. Read every line on GitHub and build it yourself if you want.",
  },
];

export function Privacy() {
  return (
    <section
      id="privacy"
      className="relative overflow-hidden border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-0 h-80 w-[120%] -translate-x-1/2 bg-[radial-gradient(ellipse_at_center,rgba(143,179,217,0.08),transparent_65%)]"
      />
      <div className="relative mx-auto max-w-3xl px-5 py-28 text-center sm:px-8">
        <ShieldCheck
          className="mx-auto size-8 text-ploy-accent-primary"
          strokeWidth={1.5}
        />
        <motion.h2
          initial={false}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.5 }}
          className="font-heading typography-heading mt-6 text-balance text-4xl tracking-[-0.025em] text-ploy-text-primary sm:text-5xl"
        >
          No telemetry. No data leaves your machine. Ever.
        </motion.h2>
        <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-ploy-text-secondary">
          OpenLid does one job and respects your machine while doing it. There's
          nothing to sign in to and nothing to opt out of.
        </p>

        <div className="mt-14 grid gap-8 text-left sm:grid-cols-3">
          {POINTS.map((p) => (
            <div key={p.title}>
              <h3 className="text-sm font-semibold text-ploy-text-primary">
                {p.title}
              </h3>
              <p className="mt-2 text-sm leading-relaxed text-ploy-text-secondary">
                {p.body}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
