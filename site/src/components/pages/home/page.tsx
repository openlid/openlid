import { MotionConfig } from "motion/react";
import { Navbar } from "@/components/sections/navbar";
import { Footer } from "@/components/sections/footer";
import { Hero } from "./sections/hero";
import { Scenarios } from "./sections/scenarios";
import { Features } from "./sections/features";
import { AgentHandoff } from "./sections/agent-handoff";
import { OriginStory } from "./sections/origin-story";
import { Cli } from "./sections/cli";
import { Privacy } from "./sections/privacy";
import { Roadmap } from "./sections/roadmap";
import { Faq } from "@/components/sections/faq";
import { FinalCta } from "./sections/final-cta";

/**
 * @ployComponent
 * @ployComponentId OpenLidHomePage
 * @ployComponentType page
 * @ployComponentPattern landing
 * @ployComponentStatus stable
 * @ployComponentDescription OpenLid launch homepage shell. Composes the shared
 * Navbar + Footer with the home sections in order: Hero, Scenarios, Features,
 * AgentHandoff, Cli, Privacy, Roadmap, OriginStory, Faq, FinalCta. The
 * install/setup/upkeep sections live on the dedicated /install page so the home
 * scroll stays a focused pitch. Wrapped in MotionConfig (reducedMotion="user")
 * so scroll/mount animations respect accessibility. Dark monochrome theme with a
 * single icy-blue accent; sections alternate between the primary and secondary
 * surface to give the scroll rhythm.
 */
export function HomePage() {
  return (
    <MotionConfig reducedMotion="user">
      <div className="min-h-screen bg-ploy-background-primary text-ploy-text-primary">
        <Navbar />
        <main>
          <Hero />
          <Scenarios />
          <Features />
          <AgentHandoff />
          <Cli />
          <Privacy />
          <Roadmap />
          <OriginStory />
          <Faq />
          <FinalCta />
        </main>
        <Footer />
      </div>
    </MotionConfig>
  );
}
