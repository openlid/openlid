/**
 * @ployComponent
 * @ployComponentId OpenLidLoginItemsApproval
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription One-time Login Items approval walkthrough for macOS 13+.
 * Numbered vertical step list reusing the home Roadmap timeline anatomy (size-9
 * rounded marker, connector rule, hairline-bordered rows) — but markers carry step
 * numbers instead of status glyphs. Ends with a quiet note that OpenLid needs no
 * kernel extension or admin password. Steps are prop-overridable.
 */
interface ApprovalStep {
  title: string;
  body: string;
}

const DEFAULT_STEPS: ApprovalStep[] = [
  {
    title: "Launch OpenLid",
    body: "Open it from Applications. The lid icon appears in your menu bar right away — it's signed and notarized, so there's no “unidentified developer” prompt to click through.",
  },
  {
    title: "macOS shows “Background Items Added”",
    body: "The first time OpenLid registers to stay running, macOS posts a notification that a background item was added. That's expected — it's how the app keeps working after you log back in.",
  },
  {
    title: "Open Login Items & Extensions",
    body: "Go to System Settings → General → Login Items & Extensions. You'll find OpenLid listed under “Allow in the Background.”",
  },
  {
    title: "Toggle OpenLid on",
    body: "Flip the switch so OpenLid is allowed in the background and opens at login. Now it relaunches automatically after a reboot and is ready the moment you sit down.",
  },
];

export function LoginItemsApproval({
  steps = DEFAULT_STEPS,
}: {
  steps?: ApprovalStep[];
}) {
  return (
    <section
      id="login-items"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
            One-time setup
          </p>
          <h2 className="font-heading typography-heading mt-3 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Approve OpenLid in Login Items.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            macOS 13 and later ask you to approve any app that runs in the
            background. It takes about ten seconds — here's the whole flow.
          </p>
        </div>

        <ol className="mt-12 space-y-px overflow-hidden rounded-2xl border border-white/[0.07]">
          {steps.map((s, i) => (
            <li key={s.title} className="flex gap-5 bg-white/[0.015] p-6 sm:p-7">
              <div className="flex flex-col items-center">
                <span className="grid size-9 shrink-0 place-items-center rounded-full border border-ploy-accent-primary/40 bg-ploy-accent-primary/10 font-mono text-sm text-ploy-accent-primary">
                  {i + 1}
                </span>
                {i < steps.length - 1 && (
                  <span className="mt-2 w-px flex-1 bg-white/[0.08]" />
                )}
              </div>
              <div className="pb-1">
                <h3 className="text-lg font-semibold text-ploy-text-primary">
                  {s.title}
                </h3>
                <p className="mt-2 max-w-2xl text-sm leading-relaxed text-ploy-text-secondary">
                  {s.body}
                </p>
              </div>
            </li>
          ))}
        </ol>

        <p className="mt-6 max-w-2xl text-sm leading-relaxed text-ploy-text-secondary/80">
          No kernel extension, no admin password, nothing to install at the
          system level — OpenLid only asks for the standard background-item
          approval, and you can revoke it any time from the same screen.
        </p>
      </div>
    </section>
  );
}
