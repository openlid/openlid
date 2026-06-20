import { motion } from "motion/react";
import { AppleMark } from "@/components/ui/apple-mark";
import { Button } from "@/components/ui/button";
import { Terminal, type TermLine } from "@/components/ui/terminal";

/**
 * @ployComponent
 * @ployComponentId OpenLidInstallMethods
 * @ployComponentType section
 * @ployComponentPattern feature
 * @ployComponentStatus stable
 * @ployComponentDescription Install paths for OpenLid. Stacked two-column blocks
 * (text left, Terminal right) mirroring the home Cli section anatomy: eyebrow +
 * heading + copy + icy-blue bullet list, with a rendered Terminal panel of the
 * actual commands. Covers Homebrew (recommended), signed DMG download, and
 * build-from-source. The DMG block carries the white primary Download CTA. Items
 * are prop-overridable via the methods array.
 */
export interface InstallMethod {
  eyebrow: string;
  heading: string;
  body: string;
  points: string[];
  terminalTitle: string;
  lines: TermLine[];
  download?: { label: string; url: string };
}

const DEFAULT_METHODS: InstallMethod[] = [
  {
    eyebrow: "Recommended",
    heading: "Install with Homebrew",
    body: "One command installs the signed app and puts the openlid CLI on your PATH. Updates flow through brew like everything else.",
    points: [
      "Pulls the signed & notarized cask — no Gatekeeper detour",
      "Adds the openlid binary for scripts, schedules, and CI",
      "Upgrades cleanly with brew upgrade",
    ],
    terminalTitle: "openlid — install via Homebrew",
    lines: [
      { kind: "prompt", text: "brew install --cask openlid/tap/openlid" },
      { kind: "out", text: "==> Downloading openlid.dmg (signed, notarized)" },
      { kind: "ok", text: "Installed OpenLid → /Applications/OpenLid.app" },
      { kind: "comment", text: "" },
      { kind: "prompt", text: "openlid --version" },
      { kind: "out", text: "openlid 2.3.2" },
    ],
  },
  {
    eyebrow: "Direct download",
    heading: "Signed disk image (.dmg)",
    body: "Prefer not to use Homebrew? Grab the signed, notarized DMG straight from GitHub Releases, drag OpenLid to Applications, and launch.",
    points: [
      "Apple-signed & notarized — opens without right-click workarounds",
      "Drag-to-Applications, then open from Launchpad or Spotlight",
      "Verify the checksum against the release notes if you like",
    ],
    terminalTitle: "verify the download (optional)",
    lines: [
      { kind: "prompt", text: "shasum -a 256 ~/Downloads/OpenLid.dmg" },
      { kind: "out", text: "9f2c…a41  OpenLid.dmg" },
      { kind: "comment", text: "// compare against the SHA-256 in the release notes" },
      { kind: "prompt", text: "spctl --assess --type open --context context:primary-signature \\" },
      { kind: "prompt", text: "  /Applications/OpenLid.app" },
      { kind: "ok", text: "source=Notarized Developer ID — accepted" },
    ],
    download: {
      label: "Download the .dmg",
      url: "https://github.com/openlid/openlid/releases/latest",
    },
  },
  {
    eyebrow: "From source",
    heading: "Build it yourself",
    body: "It's Apache-2.0 Rust. Clone the repo, build a release binary, and you have the exact app — auditable line by line.",
    points: [
      "Requires the Rust toolchain (rustup) and Xcode command line tools",
      "cargo build --release produces the menu bar app + CLI",
      "Bundle locally or run the binary directly — your call",
    ],
    terminalTitle: "openlid — build from source",
    lines: [
      { kind: "prompt", text: "git clone https://github.com/openlid/openlid.git" },
      { kind: "prompt", text: "cd openlid" },
      { kind: "prompt", text: "cargo build --release" },
      { kind: "out", text: "   Compiling openlid-core v2.3.2" },
      { kind: "out", text: "   Compiling openlid v2.3.2" },
      { kind: "ok", text: "Finished release [optimized] target(s)" },
      { kind: "prompt", text: "./target/release/openlid on" },
      { kind: "ok", text: "Active — preventing sleep now" },
    ],
  },
];

export function InstallMethods({
  methods = DEFAULT_METHODS,
}: {
  methods?: InstallMethod[];
}) {
  return (
    <section
      id="methods"
      className="border-t border-white/[0.06] bg-ploy-background-primary"
    >
      <div className="mx-auto max-w-6xl px-5 py-24 sm:px-8">
        <div className="max-w-2xl">
          <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
            Three ways in
          </p>
          <h2 className="font-heading typography-heading mt-3 text-balance text-3xl tracking-[-0.02em] text-ploy-text-primary sm:text-4xl">
            Pick the install path that fits your setup.
          </h2>
          <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
            Every path ships the same signed, notarized app. Homebrew is the
            fastest; the DMG is the click-and-drag classic; source is for the
            people who like to read the code first.
          </p>
        </div>

        <div className="mt-14 space-y-px overflow-hidden rounded-2xl border border-white/[0.07]">
          {methods.map((m) => (
            <div
              key={m.heading}
              className="grid items-center gap-10 bg-white/[0.015] p-6 sm:p-10 lg:grid-cols-[0.9fr_1.1fr]"
            >
              <div>
                <p className="font-eyebrow text-xs uppercase tracking-[0.18em] text-ploy-accent-primary">
                  {m.eyebrow}
                </p>
                <h3 className="font-heading typography-heading mt-3 text-balance text-2xl tracking-[-0.02em] text-ploy-text-primary sm:text-3xl">
                  {m.heading}
                </h3>
                <p className="mt-4 text-base leading-relaxed text-ploy-text-secondary">
                  {m.body}
                </p>
                <ul className="mt-6 space-y-3 text-sm text-ploy-text-secondary">
                  {m.points.map((p) => (
                    <li key={p} className="flex items-start gap-3">
                      <span className="mt-2 size-1 shrink-0 rounded-full bg-ploy-accent-primary" />
                      {p}
                    </li>
                  ))}
                </ul>
                {m.download && (
                  <Button asChild size="lg" className="mt-7">
                    <a href={m.download.url} target="_blank" rel="noreferrer">
                      <AppleMark className="size-[1.15em]" />
                      {m.download.label}
                    </a>
                  </Button>
                )}
              </div>

              <motion.div
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true, margin: "-80px" }}
                transition={{ duration: 0.5 }}
              >
                <Terminal title={m.terminalTitle} lines={m.lines} />
              </motion.div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
