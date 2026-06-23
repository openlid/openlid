import { motion } from "motion/react";
import {
  ProductScreenshot,
  type ProductScreenshotName,
} from "../components/product-screenshot";

const PREFERENCE_SHOTS: {
  name: ProductScreenshotName;
  caption: string;
}[] = [
  {
    name: "preferencesGeneral",
    caption: "General: launch behavior, activation, and display-awake control.",
  },
  {
    name: "preferencesSafeguards",
    caption: "Safeguards: automatic turn-off rules for battery and travel.",
  },
  {
    name: "preferencesSchedule",
    caption: "Schedule: recurring active hours, stored locally.",
  },
];

/**
 * @ployComponent
 * @ployComponentId OpenLidProductScreenshots
 * @ployComponentType section
 * @ployComponentPattern showcase
 * @ployComponentStatus stable
 * @ployComponentDescription Real OpenLid screenshots section. Uses cropped
 * captures from the running macOS app to prove native menu-bar and Preferences
 * surfaces without relying on synthetic website mocks.
 */
export function ProductScreenshots() {
  return (
    <section
      id="screenshots"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="grid gap-10 lg:grid-cols-[0.72fr_1.28fr] lg:items-start">
          <motion.div
            initial={false}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.55 }}
            className="max-w-xl lg:sticky lg:top-24"
          >
            <p className="font-mono text-[0.74rem] text-ploy-accent-primary">
              captured from the running app
            </p>
            <h2 className="font-heading typography-heading mt-4 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
              Native Mac controls, not a fake dashboard.
            </h2>
            <p className="mt-4 max-w-lg text-base leading-relaxed text-ploy-text-secondary">
              OpenLid lives where a sleep-prevention utility should: the menu
              bar for quick action, and a native Preferences window for startup,
              safeguards, and schedule rules.
            </p>

            <div className="mt-8 max-w-sm">
              <ProductScreenshot
                name="contextMenu"
                caption="Menu-bar context menu: active state and quick actions."
              />
            </div>
          </motion.div>

          <div className="grid gap-5">
            {PREFERENCE_SHOTS.map((shot, index) => (
              <motion.div
                key={shot.name}
                initial={false}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.55, delay: 0.08 * (index + 1) }}
              >
                <ProductScreenshot name={shot.name} caption={shot.caption} />
              </motion.div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
