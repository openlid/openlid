/**
 * TODO: Customize this 404 page to match workspace brand styling.
 * Update the layout, background pattern, messaging, and button styles.
 */
export function NotFoundPage() {
  const homeHref = import.meta.env.BASE_URL;

  return (
    <div className="min-h-screen bg-ploy-background-primary text-ploy-text-primary flex items-center justify-center">
      <div className="text-center space-y-6 px-6">
        <h1 className="font-heading typography-heading text-4xl md:text-6xl lg:text-7xl">
          404
        </h1>
        <p className="text-lg md:text-xl lg:text-2xl text-ploy-text-secondary max-w-md mx-auto">
          This page doesn't exist.
        </p>
        <a
          href={homeHref}
          className="mt-4 inline-flex items-center justify-center rounded-md border border-ploy-border-primary px-4 py-2 text-sm md:text-base lg:text-lg transition-colors hover:bg-ploy-neutral-primary-s2"
        >
          Go home
        </a>
      </div>
    </div>
  );
}
