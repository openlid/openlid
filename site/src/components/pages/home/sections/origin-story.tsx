import { ArrowRight } from "lucide-react";
import { sitePath } from "@/lib/site-path";

/**
 * @ployComponent
 * @ployComponentId OpenLidOriginStory
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Homepage bridge to the origin story. Connects the
 * product premise to the meme that inspired it, then sends readers to the full
 * /story page without competing with the primary download CTA.
 */
export function OriginStory() {
  return (
    <section className="border-t border-white/[0.06] bg-ploy-background-primary">
      <div className="mx-auto grid max-w-6xl items-center gap-10 px-5 py-20 sm:px-8 lg:grid-cols-[0.8fr_1.2fr]">
        <div className="font-mono text-sm text-ploy-accent-primary">
          before agents → after agents
        </div>
        <div>
          <h2 className="font-heading typography-heading text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            OpenLid started with a meme about the half-open laptop era.
          </h2>
          <p className="mt-4 max-w-2xl text-base leading-relaxed text-ploy-text-secondary">
            The joke was simple: after coding agents, engineers suddenly needed
            their laptops to keep working even when the lid was almost closed.
            OpenLid turned that joke into a small utility with real guardrails.
          </p>
          <a
            href={sitePath("/story")}
            className="mt-6 inline-flex items-center gap-2 text-sm font-medium text-ploy-text-primary transition-colors hover:text-ploy-accent-primary"
          >
            Read how it started
            <ArrowRight className="size-4" />
          </a>
        </div>
      </div>
    </section>
  );
}
