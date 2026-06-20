import { SeoJson, createFaqPageSchema, type FaqItem } from "@/components/seo-json";

/**
 * @ployComponent
 * @ployComponentId OpenLidFaq
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription FAQ section for OpenLid, built for answer engines
 * (AEO). Left-aligned section header + a hairline-divided list of always-visible
 * Q&A rows (question semibold, answer muted) matching the home section anatomy.
 * Co-locates FAQPage JSON-LD via createFaqPageSchema so the visible Q&A and the
 * structured data stay in sync. Answers are sourced from the openlid/openlid
 * README. Items are prop-overridable.
 */
const DEFAULT_FAQS: FaqItem[] = [
  {
    question:
      "Does OpenLid keep my Mac awake with the lid closed and no external display?",
    answer:
      "Yes — that's the whole point. While OpenLid is active it prevents macOS from sleeping when you close the lid, even without an external monitor, so builds, coding agents, downloads, and remote sessions keep running. It also turns the built-in display off on lid close to protect battery and thermals.",
  },
  {
    question: "Will macOS show an \u201cunidentified developer\u201d warning?",
    answer:
      "No. The Homebrew cask and the direct DMG are both Apple-signed and notarized, so OpenLid opens without any Gatekeeper right-click workaround. (Only a build you compile from source is ad-hoc-signed and would warn.)",
  },
  {
    question: "Does OpenLid need my admin password or a kernel extension?",
    answer:
      "No kernel extension and no sudo. The privileged helper installs automatically through Apple's SMAppService — you just approve OpenLid once under System Settings \u2192 General \u2192 Login Items \u2192 Allow in the Background.",
  },
  {
    question: "Does OpenLid collect any data?",
    answer:
      "No telemetry, no analytics, nothing leaves your machine. OpenLid only touches the network when you explicitly run \u201copenlid update\u201d or click \u201cCheck for updates\u2026\u201d. All state stays local in ~/Library/Application Support/io.openlid.app.",
  },
  {
    question: "Can I keep it awake only during certain hours?",
    answer:
      "Yes. Set a recurring schedule (for example 08:00\u201318:00 on weekdays) from the menu bar or the CLI. OpenLid can also auto-deactivate below a battery threshold or when it detects the laptop is being carried in transit.",
  },
  {
    question: "What about heat and battery if it's running in a bag?",
    answer:
      "A closed lid traps heat, so keep an eye on the machine under heavy load. OpenLid turns the internal display off on lid close to reduce heat and drain, and its battery-threshold safeguard can automatically return the Mac to normal sleep before the charge gets low.",
  },
  {
    question: "How do I update or uninstall OpenLid?",
    answer:
      "Update with \u201cbrew upgrade --cask openlid/tap/openlid\u201d or the built-in \u201copenlid update\u201d. Uninstall with \u201cbrew uninstall --cask openlid/tap/openlid\u201d (add --zap to also clear preferences), or remove /Applications/OpenLid.app manually.",
  },
  {
    question: "Is OpenLid free and open source?",
    answer:
      "Yes — it's Apache-2.0 licensed and the full source is on GitHub. It runs on macOS 13+ (Apple Silicon) today, with Linux support planned for v3.0.0.",
  },
];

export function Faq({ faqs = DEFAULT_FAQS }: { faqs?: FaqItem[] }) {
  return (
    <section
      id="faq"
      className="border-t border-white/[0.06] bg-ploy-background-secondary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
            FAQ
          </p>
          <h2 className="font-heading typography-heading mt-3 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Questions, answered.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            The practical details — signing, privacy, scheduling, heat, and
            cleanup — straight from the project's docs.
          </p>
        </div>

        <dl className="mt-12 overflow-hidden rounded-2xl border border-white/[0.07]">
          {faqs.map((f) => (
            <div
              key={f.question}
              className="border-b border-white/[0.06] bg-white/[0.015] p-6 last:border-b-0 sm:p-7"
            >
              <dt className="text-base font-semibold text-ploy-text-primary">
                {f.question}
              </dt>
              <dd className="mt-2 max-w-3xl text-sm leading-relaxed text-ploy-text-secondary">
                {f.answer}
              </dd>
            </div>
          ))}
        </dl>
      </div>

      <SeoJson schema={createFaqPageSchema(faqs, { name: "OpenLid FAQ" })} />
    </section>
  );
}
