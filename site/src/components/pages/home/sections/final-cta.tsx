import { motion } from "motion/react";
import { Apple } from "lucide-react";
import { Button } from "@/components/ui/button";
import { GitHubMark } from "@/components/ui/github-mark";
import { Terminal } from "../components/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidFinalCta
 * @ployComponentType section
 * @ployComponentPattern cta
 * @ployComponentStatus stable
 * @ployComponentDescription Closing CTA. Centered oversized headline on a smoky
 * black field, white primary Download + secondary GitHub action, and the brew
 * install one-liner. The footer carries links only — this section owns the
 * page's final conversion moment. Reuses Button + Terminal primitives.
 */
export function FinalCta({
  downloadUrl = "https://github.com/openlid/openlid/releases/latest",
  repoUrl = "https://github.com/openlid/openlid",
}: {
  downloadUrl?: string;
  repoUrl?: string;
}) {
  return (
    <section className="relative overflow-hidden border-t border-white/[0.06] bg-ploy-background-primary">
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[120%] -translate-x-1/2 -translate-y-1/2 bg-[radial-gradient(ellipse_at_center,rgba(143,179,217,0.10),transparent_64%)]"
      />
      <div className="relative mx-auto max-w-2xl px-5 py-28 text-center sm:px-8">
        <motion.h2
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.5 }}
          className="font-heading typography-heading text-balance text-4xl tracking-[-0.03em] text-ploy-text-primary sm:text-6xl"
        >
          Start the build. Close the lid anyway.
        </motion.h2>
        <p className="mx-auto mt-5 max-w-lg text-base leading-relaxed text-ploy-text-secondary">
          Free and open source. Install in seconds, arm it with one click, and
          stop fighting your laptop's sleep settings.
        </p>

        <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <Button asChild size="lg" className="w-full sm:w-auto">
            <a href={downloadUrl} target="_blank" rel="noreferrer">
              <Apple className="size-[1.15em]" />
              Download for macOS
            </a>
          </Button>
          <Button asChild size="lg" variant="secondary" className="w-full sm:w-auto">
            <a href={repoUrl} target="_blank" rel="noreferrer">
              <GitHubMark className="size-[1.15em]" />
              Star on GitHub
            </a>
          </Button>
        </div>

        <div className="mx-auto mt-6 max-w-md">
          <Terminal
            lines={[
              { kind: "prompt", text: "brew install --cask openlid/tap/openlid" },
            ]}
          />
        </div>
      </div>
    </section>
  );
}
