import { motion } from "motion/react";
import { Terminal, type TermLine } from "@/components/ui/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidUpdateUninstall
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Update & uninstall reference. Two framed blocks (text
 * + Terminal) sharing the home Cli/Scenarios anatomy: hairline-bordered cards,
 * eyebrow + heading + icy-blue bullet list, with rendered command panels. Covers
 * updating via Homebrew / built-in openlid update / re-download, and a clean
 * uninstall via brew or manual removal of the app, state, and login item.
 */
interface InfoBlock {
  eyebrow: string;
  heading: string;
  body: string;
  points: string[];
  terminalTitle: string;
  lines: TermLine[];
}

const UPDATE_BLOCK: InfoBlock = {
  eyebrow: "Stay current",
  heading: "Updating",
  body: "However you installed it, getting the latest release is one command. OpenLid never updates silently — it only touches the network when you ask.",
  points: [
    "Homebrew: brew upgrade --cask openlid/tap/openlid",
    "Built-in: openlid update checks GitHub releases",
    "From source: git pull && cargo build --release",
  ],
  terminalTitle: "openlid — update",
  lines: [
    { kind: "prompt", text: "brew upgrade --cask openlid/tap/openlid" },
    { kind: "ok", text: "Upgraded OpenLid 2.3.1 → 2.3.2" },
    { kind: "comment", text: "" },
    { kind: "prompt", text: "openlid update" },
    { kind: "out", text: "Checking github.com/openlid/openlid…" },
    { kind: "ok", text: "You're on the latest release (2.3.2)" },
  ],
};

const UNINSTALL_BLOCK: InfoBlock = {
  eyebrow: "Clean removal",
  heading: "Uninstalling",
  body: "OpenLid leaves a tiny footprint — one app and one small state folder. Remove both and it's gone without a trace.",
  points: [
    "Homebrew: brew uninstall --cask openlid/tap/openlid",
    "Manual: delete OpenLid.app from /Applications",
    "Clear state in ~/Library/Application Support/OpenLid",
  ],
  terminalTitle: "openlid — uninstall",
  lines: [
    { kind: "prompt", text: "brew uninstall --cask openlid/tap/openlid" },
    { kind: "ok", text: "Uninstalled OpenLid" },
    { kind: "comment", text: "// or, if you installed manually:" },
    { kind: "prompt", text: "rm -rf /Applications/OpenLid.app" },
    { kind: "prompt", text: 'rm -rf "$HOME/Library/Application Support/OpenLid"' },
    { kind: "ok", text: "Removed — toggle it off in Login Items too" },
  ],
};

export function UpdateUninstall({
  blocks = [UPDATE_BLOCK, UNINSTALL_BLOCK],
}: {
  blocks?: InfoBlock[];
}) {
  return (
    <section
      id="update-uninstall"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <h2 className="font-heading typography-heading text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Update or remove it anytime.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            No account, no background updater, no leftovers. You stay in control
            of when OpenLid changes and how completely it goes away.
          </p>
        </div>

        <div className="mt-12 grid gap-5 lg:grid-cols-2">
          {blocks.map((b) => (
            <div
              key={b.heading}
              className="flex flex-col rounded-2xl border border-white/[0.07] bg-white/[0.015] p-6 sm:p-7"
            >
              <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
                {b.eyebrow}
              </p>
              <h3 className="mt-3 text-xl font-semibold text-ploy-text-primary">
                {b.heading}
              </h3>
              <p className="mt-3 text-sm leading-relaxed text-ploy-text-secondary">
                {b.body}
              </p>
              <ul className="mt-5 space-y-3 text-sm text-ploy-text-secondary">
                {b.points.map((p) => (
                  <li key={p} className="flex items-start gap-3">
                    <span className="mt-2 size-1 shrink-0 rounded-full bg-ploy-accent-primary" />
                    {p}
                  </li>
                ))}
              </ul>
              <motion.div
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true, margin: "-80px" }}
                transition={{ duration: 0.5 }}
                className="mt-6"
              >
                <Terminal title={b.terminalTitle} lines={b.lines} />
              </motion.div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
