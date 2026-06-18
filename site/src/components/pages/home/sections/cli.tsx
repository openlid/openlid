import { motion } from "motion/react";
import { Terminal } from "../components/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidCli
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription CLI showcase. Two-column module: left concise text
 * block (scriptable, --json, scheduling), right a Terminal panel demonstrating
 * real openlid subcommands. Reuses the OpenLidTerminal primitive. Keeps the
 * icy-blue ✓ accent for success output.
 */
export function Cli() {
  return (
    <section
      id="cli"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto grid max-w-6xl items-center gap-12 px-5 py-24 sm:px-8 lg:grid-cols-[0.9fr_1.1fr]">
        <div>
          <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
            Built for the terminal
          </p>
          <h2 className="font-heading typography-heading mt-3 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Drive everything from the command line.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            The Homebrew install puts <code className="font-mono text-ploy-text-primary">openlid</code> on
            your PATH. Arm it, schedule it, or check status from scripts and
            CI — with machine-readable <code className="font-mono text-ploy-text-primary">--json</code> output
            on every command.
          </p>
          <ul className="mt-6 space-y-3 text-sm text-ploy-text-secondary">
            {[
              "Single binary, no daemon to babysit",
              "Locked subcommands & config schema under semver",
              "Same state whether you use the menu bar or the CLI",
            ].map((item) => (
              <li key={item} className="flex items-start gap-3">
                <span className="mt-2 size-1 shrink-0 rounded-full bg-ploy-accent-primary" />
                {item}
              </li>
            ))}
          </ul>
        </div>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.5 }}
        >
          <Terminal
            title="openlid — zsh"
            lines={[
              { kind: "prompt", text: "openlid on" },
              { kind: "ok", text: "Active — preventing sleep now" },
              { kind: "comment", text: "" },
              {
                kind: "prompt",
                text: "openlid schedule set --from 08:00 --to 18:00 \\",
              },
              { kind: "prompt", text: "  --days Mon,Tue,Wed,Thu,Fri" },
              { kind: "ok", text: "Schedule saved · active 08:00–18:00" },
              { kind: "comment", text: "" },
              { kind: "prompt", text: "openlid status --json" },
              { kind: "out", text: '{ "state": "active", "lid": "closed",' },
              { kind: "out", text: '  "power": "ac", "schedule": true }' },
            ]}
          />
        </motion.div>
      </div>
    </section>
  );
}
