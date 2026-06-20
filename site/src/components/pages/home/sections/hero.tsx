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
 * @ployComponentDescription OpenLid homepage hero. Centered poster layout: live
 * menu-bar status chip, oversized Manrope headline, muted subcopy, primary
 * Download + secondary GitHub CTAs, a brew-install terminal one-liner, a proof
 * row of platform/trust chips, and the MenuBarCard product object resting on a
 * smoky radial bloom. Reserve the single white primary CTA as the brightest
 * element. Content (headline, sub, CTAs, proof) is prop-overridable.
 */
const DEFAULT_PROOF = [
  "macOS 13+",
  "Apple Silicon",
  "Signed & notarized",
  "No telemetry",
  "Apache-2.0",
];

export function Hero({
  headline = "Keep your laptop awake.",
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
    <section id="top" className="relative overflow-hidden">
      {/* atmospheric bloom */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-[-10%] mx-auto h-[640px] max-w-4xl rounded-full bg-ploy-accent-primary/[0.07] blur-[120px]"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute bottom-0 left-1/2 h-[420px] w-[140%] -translate-x-1/2 bg-[radial-gradient(ellipse_at_center,rgba(143,179,217,0.10),transparent_62%)]"
      />

      <div className="relative mx-auto max-w-3xl px-5 pb-24 pt-20 text-center sm:px-8 sm:pt-28">
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
          className="inline-flex items-center gap-2 rounded-full border border-white/[0.08] bg-white/[0.03] px-3.5 py-1.5 font-mono text-[0.72rem] text-ploy-text-secondary"
        >
          <span className="relative flex size-1.5">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-ploy-accent-primary/70" />
            <span className="relative inline-flex size-1.5 rounded-full bg-ploy-accent-primary" />
          </span>
          Active — preventing sleep, lid closed
        </motion.div>

        <motion.h1
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.05 }}
          className="font-heading typography-heading mt-7 text-[2.6rem] leading-[1.02] tracking-[-0.03em] text-ploy-text-primary sm:text-6xl lg:text-7xl"
        >
          {headline}
          <span className="block">
            Even with the{" "}
            <span className="text-ploy-accent-primary">lid closed.</span>
          </span>
        </motion.h1>

        <motion.p
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.12 }}
          className="mx-auto mt-6 max-w-xl text-base leading-relaxed text-ploy-text-secondary sm:text-lg"
        >
          {subcopy}
        </motion.p>

        <motion.div
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.18 }}
          className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row"
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
              View on GitHub
            </a>
          </Button>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.24 }}
          className="mx-auto mt-6 max-w-md"
        >
          <Terminal
            lines={[
              {
                kind: "prompt",
                text: "brew install --cask openlid/tap/openlid",
              },
            ]}
          />
        </motion.div>

        <motion.ul
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.6, delay: 0.32 }}
          className="mt-9 flex flex-wrap items-center justify-center gap-x-5 gap-y-2 font-mono text-[0.72rem] text-ploy-text-secondary/80"
        >
          {proof.map((p, i) => (
            <li key={p} className="flex items-center gap-3">
              {i > 0 && <span className="text-white/15">·</span>}
              {p}
            </li>
          ))}
        </motion.ul>
      </div>

      {/* product object on smoky platform */}
      <div className="relative mx-auto max-w-5xl px-5 pb-24 sm:px-8">
        <motion.div
          initial={{ opacity: 0, y: 28 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.7, delay: 0.36 }}
          className="relative flex justify-center"
        >
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 bottom-[-20%] mx-auto h-72 max-w-2xl rounded-[50%] bg-[radial-gradient(ellipse_at_center,rgba(143,179,217,0.12),transparent_70%)] blur-2xl"
          />
          <MenuBarScene className="relative max-w-3xl" />
        </motion.div>
      </div>
    </section>
  );
}
