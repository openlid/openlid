import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

/**
 * @ployComponent
 * @ployComponentId OpenLidButton
 * @ployComponentType component
 * @ployComponentPattern button
 * @ployComponentStatus stable
 * @ployComponentDescription Brand button for OpenLid. Variants: primary (white
 * fill, near-black text), secondary (transparent dark surface, hairline gray
 * border), ghost (text only). Rounded-md, medium-weight Inter. Use `asChild` to
 * render as an anchor. Reserve `primary` for the single brightest CTA per view.
 */
const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md font-button text-[0.95rem] font-medium transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ploy-accent-primary/60 focus-visible:ring-offset-2 focus-visible:ring-offset-ploy-background-primary disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-[1.1em] [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary:
          "bg-ploy-neutral-inverse text-ploy-text-on-accent-primary hover:bg-ploy-neutral-inverse/90",
        secondary:
          "border border-ploy-button-secondary-border bg-white/[0.02] text-ploy-text-primary hover:bg-white/[0.06]",
        ghost: "text-ploy-text-secondary hover:text-ploy-text-primary",
      },
      size: {
        sm: "h-9 px-4",
        md: "h-11 px-5",
        lg: "h-12 px-6 text-base",
      },
    },
    defaultVariants: { variant: "primary", size: "md" },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        ref={ref}
        className={cn(buttonVariants({ variant, size, className }))}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { buttonVariants };
