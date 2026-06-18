import { ArrowRight, Bot, Smartphone, Wifi } from "lucide-react";
import { motion } from "motion/react";
import { Button } from "@/components/ui/button";
import { Terminal } from "@/components/ui/terminal";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidAgentHandoff
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Homepage bridge to the coding agents use case:
 * OpenLid keeps the Mac awake while Claude Code, Codex, builds, and tests keep
 * running; the user's phone connects through their own trusted remote access
 * path. Keeps the claim precise and links to the full /coding-agents page.
 */
export function AgentHandoff() {
  return (
    <section className="border-t border-white/[0.06] bg-ploy-background-primary">
      <div className="mx-auto grid max-w-6xl gap-12 px-5 py-24 sm:px-8 lg:grid-cols-[0.95fr_1.05fr]">
        <div>
          <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
            Coding agents on the go
          </p>
          <h2 className="font-heading typography-heading mt-3 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Let Claude Code and Codex keep working while your laptop is closed.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            Start a coding harness on your Mac, arm OpenLid, close the lid, and
            keep an eye on the session from your phone through SSH, Tailscale,
            Screen Sharing, or your remote terminal of choice.
          </p>

          <div className="mt-7 grid gap-4 sm:grid-cols-3">
            {[
              { icon: Bot, label: "Agent runs on the Mac" },
              { icon: Wifi, label: "Remote link stays reachable" },
              { icon: Smartphone, label: "Phone becomes the control surface" },
            ].map((item) => (
              <div key={item.label} className="border-l border-white/[0.08] pl-4">
                <item.icon className="size-5 text-ploy-accent-primary" strokeWidth={1.6} />
                <p className="mt-3 text-sm leading-relaxed text-ploy-text-secondary">
                  {item.label}
                </p>
              </div>
            ))}
          </div>

          <Button asChild variant="secondary" size="lg" className="mt-8">
            <a href={sitePath("/coding-agents")}>
              See the phone workflow
              <ArrowRight className="size-[1.1em]" />
            </a>
          </Button>
        </div>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.5 }}
          className="grid gap-4"
        >
          <Terminal
            title="macbook — agent session"
            lines={[
              { kind: "prompt", text: "openlid on" },
              { kind: "ok", text: "Active — preventing sleep now" },
              { kind: "comment", text: "" },
              { kind: "prompt", text: "tmux new -s codex" },
              { kind: "prompt", text: "codex" },
              { kind: "out", text: "Working tree ready · waiting for instructions" },
              { kind: "comment", text: "  // lid closed, session still alive" },
            ]}
          />
          <Terminal
            title="phone — remote terminal"
            lines={[
              { kind: "prompt", text: "ssh macbook.local" },
              { kind: "prompt", text: "tmux attach -t codex" },
              { kind: "ok", text: "Back in the same Codex session" },
            ]}
          />
        </motion.div>
      </div>
    </section>
  );
}
