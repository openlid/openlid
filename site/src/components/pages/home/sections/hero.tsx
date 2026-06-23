import {
  ArrowRight,
  Laptop,
  ShieldCheck,
  Smartphone,
  TerminalSquare,
  Wifi,
  type LucideIcon,
} from "lucide-react";
import { motion } from "motion/react";
import { AppleMark } from "@/components/ui/apple-mark";
import { Button } from "@/components/ui/button";
import { GitHubMark } from "@/components/ui/github-mark";
import { MenuBarScene } from "../components/menu-bar-scene";
import { Terminal } from "../components/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidHero
 * @ployComponentType section
 * @ployComponentPattern hero
 * @ployComponentStatus stable
 * @ployComponentDescription OpenLid homepage hero. Hallmark Map / Diagram
 * layout: left-biased technical pitch with install CTAs, paired with a system map
 * showing Mac -> OpenLid -> remote harness -> phone control. Content (headline,
 * sub, CTAs, proof) is prop-overridable.
 */
const DEFAULT_PROOF = [
  "macOS 13+",
  "Apple Silicon",
  "Signed & notarized",
  "No telemetry",
  "Apache-2.0",
];

const TRUST_SIGNALS = [
  "Signed + notarized",
  "No telemetry",
  "No sudo",
  "Open source",
];

export function Hero({
  headline = "Close the lid. Keep the run alive.",
  subcopy = "OpenLid is a tiny menu bar utility that stops macOS from sleeping when you close the lid — so builds, coding agents, downloads, and remote sessions keep running while you walk away.",
  downloadUrl = "https://github.com/openlid/openlid/releases/latest",
  repoUrl = "https://github.com/openlid/openlid",
  proof = DEFAULT_PROOF,
}: {
  headline?: string;
  subcopy?: string;
  downloadUrl?: string;
  repoUrl?: string;
  proof?: string[];
}) {
  return (
    <section
      id="top"
      className="relative overflow-hidden border-b border-white/[0.06]"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-[-28rem] mx-auto h-[48rem] max-w-5xl rounded-full bg-ploy-accent-primary/[0.08] blur-[140px]"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-y-0 right-[-18rem] hidden w-[38rem] bg-ploy-accent-primary/[0.05] blur-[110px] lg:block"
      />

      <div className="relative mx-auto grid max-w-6xl gap-12 px-5 pb-20 pt-18 sm:px-8 sm:pb-24 sm:pt-24 lg:grid-cols-[0.9fr_1.1fr] lg:items-center lg:gap-14">
        <div className="min-w-0">
          <motion.div
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.5 }}
            className="inline-flex items-center gap-2 rounded-md border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 font-mono text-[0.72rem] uppercase tracking-[0.12em] text-ploy-text-secondary"
          >
            <span className="relative flex size-1.5">
              <span className="absolute inline-flex size-full animate-ping rounded-full bg-ploy-accent-primary/70" />
              <span className="relative inline-flex size-1.5 rounded-full bg-ploy-accent-primary" />
            </span>
            Active · lid closed
          </motion.div>

          <motion.h1
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, delay: 0.05 }}
            className="font-heading typography-heading mt-7 max-w-[10ch] text-[clamp(3.3rem,8vw,6.25rem)] leading-[0.93] tracking-[-0.045em] text-ploy-text-primary [overflow-wrap:anywhere]"
          >
            {headline}
          </motion.h1>

          <motion.p
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, delay: 0.12 }}
            className="mt-6 max-w-xl text-pretty text-base leading-relaxed text-ploy-text-secondary sm:text-lg"
          >
            {subcopy}
          </motion.p>

          <motion.div
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, delay: 0.18 }}
            className="mt-8 flex flex-col gap-3 sm:flex-row"
          >
            <Button asChild size="lg" className="w-full sm:w-auto">
              <a href={downloadUrl} target="_blank" rel="noreferrer">
                <AppleMark className="size-[1.15em]" />
                Download for macOS
              </a>
            </Button>
            <Button
              asChild
              size="lg"
              variant="secondary"
              className="w-full sm:w-auto"
            >
              <a href={repoUrl} target="_blank" rel="noreferrer">
                <GitHubMark className="size-[1.15em]" />
                View source
              </a>
            </Button>
          </motion.div>

          <ul
            aria-label="Install trust signals"
            className="mt-4 grid max-w-lg grid-cols-2 gap-x-3 gap-y-2 font-mono text-[0.72rem] text-ploy-text-secondary sm:flex sm:flex-wrap"
          >
            {TRUST_SIGNALS.map((signal) => (
              <li
                key={signal}
                className="inline-flex min-w-0 items-center gap-2 whitespace-nowrap"
              >
                <span className="size-1.5 rounded-full bg-ploy-accent-primary" />
                {signal}
              </li>
            ))}
          </ul>

          <motion.div
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, delay: 0.24 }}
            className="mt-6 max-w-lg"
          >
            <Terminal
              lines={[
                {
                  kind: "prompt",
                  text: "brew install --cask openlid/tap/openlid",
                },
                { kind: "prompt", text: "openlid on" },
                { kind: "ok", text: "Preventing sleep until you turn it off" },
              ]}
            />
          </motion.div>
        </div>

        <SystemMap />
      </div>

      <div className="relative mx-auto max-w-6xl px-5 pb-16 sm:px-8 sm:pb-20">
        <motion.ul
          initial={false}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.6, delay: 0.32 }}
          className="grid gap-px overflow-hidden rounded-xl border border-white/[0.07] bg-white/[0.06] font-mono text-[0.72rem] text-ploy-text-secondary/85 sm:grid-cols-5"
        >
          {proof.map((p) => (
            <li key={p} className="bg-ploy-background-primary px-4 py-3">
              {p}
            </li>
          ))}
        </motion.ul>
      </div>
    </section>
  );
}

const MAP_NODES: {
  icon: LucideIcon;
  label: string;
  detail: string;
}[] = [
  {
    icon: Laptop,
    label: "Mac stays awake",
    detail: "Display can be off. Sleep stays blocked.",
  },
  {
    icon: TerminalSquare,
    label: "Local work continues",
    detail: "Builds, downloads, agents, and shells keep running.",
  },
  {
    icon: Wifi,
    label: "Remote path remains open",
    detail: "Use your trusted SSH, VNC, Tailscale, or app bridge.",
  },
  {
    icon: Smartphone,
    label: "Phone controls the session",
    detail: "Check in without turning the laptop into a server.",
  },
];

function SystemMap() {
  return (
    <motion.div
      initial={false}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.7, delay: 0.28 }}
      aria-labelledby="openlid-system-map-title"
      className="relative min-w-0 rounded-3xl border border-white/[0.08] bg-white/[0.025] p-3 shadow-2xl shadow-black/30 sm:p-4"
    >
      <div className="rounded-2xl border border-white/[0.07] bg-ploy-background-primary/70 p-3 sm:p-4">
        <div className="flex items-center justify-between gap-4 border-b border-white/[0.06] pb-3 font-mono text-[0.72rem] text-ploy-text-secondary">
          <span id="openlid-system-map-title">system map</span>
          <span className="inline-flex items-center gap-1.5 text-ploy-accent-primary">
            <ShieldCheck className="size-3.5" strokeWidth={1.7} />
            local first
          </span>
        </div>

        <div className="grid gap-3 py-4 sm:grid-cols-2">
          {MAP_NODES.map((node, index) => (
            <MapNode key={node.label} node={node} index={index} />
          ))}
        </div>

        <div className="grid gap-3 lg:grid-cols-[1fr_auto] lg:items-end">
          <MenuBarScene className="max-w-none rounded-xl" />
          <a
            href="#features"
            className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-white/[0.12] px-4 py-3 text-sm font-medium text-ploy-text-primary transition-colors hover:bg-white/[0.05] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ploy-accent-primary/60 lg:w-36"
          >
            Trace flow
            <ArrowRight className="size-4" strokeWidth={1.8} />
          </a>
        </div>
      </div>
    </motion.div>
  );
}

function MapNode({
  node,
  index,
}: {
  node: (typeof MAP_NODES)[number];
  index: number;
}) {
  return (
    <div className="group relative rounded-xl border border-white/[0.07] bg-ploy-background-secondary/70 p-4 transition-colors hover:border-ploy-accent-primary/35">
      <div className="flex items-start gap-3">
        <div className="grid size-9 shrink-0 place-items-center rounded-md border border-white/[0.08] bg-white/[0.03] text-ploy-accent-primary">
          <node.icon className="size-4.5" strokeWidth={1.6} />
        </div>
        <div className="min-w-0">
          <p className="font-mono text-[0.68rem] uppercase tracking-[0.14em] text-ploy-text-secondary/80">
            {String(index + 1).padStart(2, "0")}
          </p>
          <p className="mt-1 text-sm font-semibold text-ploy-text-primary">
            {node.label}
          </p>
          <p className="mt-1 text-sm leading-relaxed text-ploy-text-secondary">
            {node.detail}
          </p>
        </div>
      </div>
    </div>
  );
}
