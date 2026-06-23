import { cn } from "@/lib/utils";
import { ProductScreenshot } from "./product-screenshot";

/**
 * @ployComponent
 * @ployComponentId OpenLidMenuBarScene
 * @ployComponentType component
 * @ployComponentPattern card
 * @ployComponentStatus stable
 * @ployComponentDescription Real screenshot of the OpenLid menu-bar context
 * menu, captured from the running macOS app. Used as the hero product object.
 */
export function MenuBarScene({ className }: { className?: string }) {
  return (
    <ProductScreenshot
      name="contextMenu"
      priority
      className={cn("max-w-none rounded-xl", className)}
    />
  );
}
