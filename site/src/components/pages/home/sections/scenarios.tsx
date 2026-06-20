import { Terminal } from "../components/terminal";
import { MenuBarCard } from "../components/menu-bar-card";

/**
 * @ployComponent
 * @ployComponentId OpenLidScenarios
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription "Why" section. Left-aligned section heading + two
 * large framed product panels (one terminal showing a running build that
 * survives a closed lid, one menu-bar card showing idle-lock prevention). Each
 * panel pairs a rendered artifact with a concise text block. Dark surface,
 * hairline borders — mirrors the lookbook overview rhythm. Static (no scroll-
 * gated opacity) so the panels are always visible.
 */
export function Scenarios() {
  return (
    <section className="border-t border-white/[0.06] bg-ploy-background-secondary">
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <h2 className="font-heading typography-heading text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            For the moments macOS gets in your way.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            Closing the lid sleeps the system. Stepping away locks the screen.
            OpenLid quietly handles both — without leaving sleep disabled forever.
          </p>
        </div>

        <div className="mt-12 grid gap-5 lg:grid-cols-2">
          <Panel
            title="Carry it without killing the run"
            body="A coding agent is mid-task. Your build is four minutes out. Close the lid, walk to the meeting room — the work keeps going instead of dying on sleep."
          >
            <Terminal
              title="cargo build — release"
              lines={[
                { kind: "prompt", text: "cargo build --release" },
                { kind: "out", text: "  Compiling openlid-core v2.3.2" },
                { kind: "comment", text: "  // lid closed — 09:41" },
                { kind: "out", text: "  Compiling openlid v2.3.2" },
                { kind: "ok", text: "Finished release [optimized] in 3m 12s" },
              ]}
            />
          </Panel>

          <Panel
            title="Stop the lock-screen tax"
            body="At your desk, step away for five minutes and come back to a locked screen — for the third time today. Keep the display awake while OpenLid is on, including closed-lid remote sessions."
          >
            <div className="flex justify-center py-2">
              <MenuBarCard />
            </div>
          </Panel>
        </div>
      </div>
    </section>
  );
}

function Panel({
  title,
  body,
  children,
}: {
  title: string;
  body: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col rounded-2xl border border-white/[0.07] bg-white/[0.015] p-6 sm:p-7">
      <div className="flex flex-1 items-center rounded-xl bg-[#0b0c0d]/60 p-5">
        <div className="w-full">{children}</div>
      </div>
      <h3 className="mt-6 text-lg font-semibold text-ploy-text-primary">
        {title}
      </h3>
      <p className="mt-2 text-sm leading-relaxed text-ploy-text-secondary">
        {body}
      </p>
    </div>
  );
}
