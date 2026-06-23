import { sitePath } from "@/lib/site-path";
import { cn } from "@/lib/utils";

const SCREENSHOTS = {
  contextMenu: {
    src: "/screenshots/openlid-context-menu.png",
    width: 552,
    height: 392,
    alt: "OpenLid menu-bar context menu showing active status, Turn Off, Preferences, Check for updates, and Quit OpenLid.",
  },
  preferencesGeneral: {
    src: "/screenshots/openlid-preferences-general.png",
    width: 1180,
    height: 1013,
    alt: "OpenLid Preferences General tab with login, activate at launch, and keep display awake toggles enabled.",
  },
  preferencesSafeguards: {
    src: "/screenshots/openlid-preferences-safeguards.png",
    width: 1180,
    height: 1013,
    alt: "OpenLid Preferences Safeguards tab showing low battery and in-transit automatic turn-off rules.",
  },
  preferencesSchedule: {
    src: "/screenshots/openlid-preferences-schedule.png",
    width: 1180,
    height: 1013,
    alt: "OpenLid Preferences Schedule tab showing scheduled active hours and selected weekdays.",
  },
} as const;

export type ProductScreenshotName = keyof typeof SCREENSHOTS;

export function ProductScreenshot({
  name,
  caption,
  className,
  imageClassName,
  priority = false,
}: {
  name: ProductScreenshotName;
  caption?: string;
  className?: string;
  imageClassName?: string;
  priority?: boolean;
}) {
  const screenshot = SCREENSHOTS[name];

  return (
    <figure
      className={cn(
        "overflow-hidden rounded-xl border border-white/[0.09] bg-[#08090a] shadow-2xl shadow-black/45",
        className,
      )}
    >
      <img
        src={sitePath(screenshot.src)}
        width={screenshot.width}
        height={screenshot.height}
        alt={screenshot.alt}
        loading={priority ? "eager" : "lazy"}
        decoding="async"
        className={cn("block h-auto w-full select-none", imageClassName)}
      />
      {caption && (
        <figcaption className="border-t border-white/[0.06] px-4 py-3 font-mono text-[0.72rem] leading-relaxed text-ploy-text-secondary">
          {caption}
        </figcaption>
      )}
    </figure>
  );
}
