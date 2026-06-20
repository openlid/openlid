import { motion } from "motion/react";
import { AppleMark } from "@/components/ui/apple-mark";
import { Button } from "@/components/ui/button";
import { Terminal } from "@/components/ui/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidInstallHero
 * @ployComponentType section
 * @ployComponentPattern hero
 * @ployComponentStatus stable
 * @ployComponentDescription Compact intro for the /install page. Eyebrow +
 * Manrope headline + muted subcopy, the brew one-liner terminal, and a primary
 * Download CTA alongside a secondary anchor to the full method list below.
 * Reuses the smoky radial bloom from the home hero so the page opens on brand.
 * Content is prop-overridable.
 */
export function InstallHero({
  downloadUrl = "https://github.com/openlid/openlid/releases/latest",
}: {
  downloadUrl?: string;
}) {
  return (
    <section className="relative overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-[-12%] mx-auto h-[560px] max-w-4xl rounded-full bg-ploy-accent-primary/[0.07] blur-[120px]"
      />

      <div className="relative mx-auto max-w-3xl px-5 pb-16 pt-20 text-center sm:px-8 lg:pt-28">
        <motion.p
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
          className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary"
        >
          Install &amp; setup
        </motion.p>

        <motion.h1
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.05 }}
          className="font-heading typography-heading mt-4 text-[2.6rem] leading-[1.02] tracking-[-0.03em] text-ploy-text-primary sm:text-6xl"
        >
          Up and running in
          <span className="block text-ploy-accent-primary">under a minute.</span>
        </motion.h1>

        <motion.p
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.12 }}
          className="mx-auto mt-6 max-w-xl text-base leading-relaxed text-ploy-text-secondary sm:text-lg"
        >
          Every path ships the same signed, notarized app and puts the{" "}
          <code className="font-mono text-ploy-text-primary">openlid</code> CLI
          on your PATH. Use Homebrew, grab the DMG, or build it from source.
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
          <Button asChild size="lg" variant="secondary" className="w-full sm:w-auto">
            <a href="#methods">See all install paths</a>
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
      </div>
    </section>
  );
}
