#!/usr/bin/env node
// @ts-check
/**
 * Verify the project compiles and builds cleanly.
 *
 * Runs `astro check` followed by `astro build`. Designed to be called by
 * agents, CI, or humans — the output rules are the same either way:
 *
 *   - No ANSI escape codes in the final output. We set NO_COLOR=1 and
 *     FORCE_COLOR=0 so anything that honors those (astro CLI, vite, tsc,
 *     rollup) emits plain text. We *also* strip ANSI post-hoc from the
 *     captured buffer, because @astrojs/check's diagnostic formatter
 *     bypasses NO_COLOR and TTY detection and writes raw SGR codes
 *     regardless. Belt-and-suspenders on purpose.
 *
 *   - stdout and stderr are captured into one buffer, roughly interleaved
 *     as they arrive. Astro writes diagnostics to stderr and progress to
 *     stdout; a caller who only reads one stream gets half the story and
 *     has to retry with `2>&1`.
 *
 *   - Silent on success, verbose on failure. A successful verify exits 0
 *     and prints nothing (not even a trailing newline). A failed verify
 *     prints the full captured output of the step that failed, then
 *     exits with that step's exit code. Saves tokens in agent tool
 *     results and puts the useful output front-and-center.
 *
 * Run via `bun run verify` / `npm run verify` — those runners add
 * `./node_modules/.bin` to PATH so the bare `astro` spawn resolves.
 */

import { spawn } from "node:child_process";
import { exit } from "node:process";

const env = {
  ...process.env,
  // Disable ANSI at the source for anything that honors these. astro CLI,
  // vite, tsc, rollup all do. @astrojs/check's diagnostic formatter does
  // not — that's handled by stripAnsi() below.
  NO_COLOR: "1",
  FORCE_COLOR: "0",
  // CI=1 makes some tools pick their non-interactive, non-TTY code paths
  // (no spinners, no progress bars, no prompts).
  CI: "1",
};

// CSI + simple ESC sequences. Covers SGR colors (\x1b[...m), cursor moves,
// and OSC hyperlinks — enough to clean any terminal output that leaks
// through despite NO_COLOR.
const ANSI_PATTERN =
  // eslint-disable-next-line no-control-regex
  /\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(?:\x07|\x1B\\))/g;

/** @param {string} text */
function stripAnsi(text) {
  return text.replace(ANSI_PATTERN, "");
}

/**
 * Spawn a command, capture stdout+stderr together, resolve with exit code
 * and combined buffer. Preserves interleaving as best as Node's stream
 * events allow — good enough for human and LLM reading.
 *
 * @param {string} cmd
 * @param {string[]} args
 * @returns {Promise<{ code: number; output: string }>}
 */
function run(cmd, args) {
  return new Promise((resolve) => {
    const proc = spawn(cmd, args, {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let output = "";
    proc.stdout.on("data", (chunk) => {
      output += chunk.toString();
    });
    proc.stderr.on("data", (chunk) => {
      output += chunk.toString();
    });

    proc.on("error", (err) => {
      resolve({ code: 1, output: `${output}\n${err.message}` });
    });
    proc.on("close", (code) => {
      resolve({ code: code ?? 1, output });
    });
  });
}

/** @type {Array<[string, string[]]>} */
const steps = [
  ["eslint", ["."]],
  ["astro", ["check"]],
  ["astro", ["build"]],
];

for (const [cmd, args] of steps) {
  const { code, output } = await run(cmd, args);
  if (code !== 0) {
    // Failure: write the buffer (minus ANSI) to STDERR, then exit with
    // the step's code. stderr is the right channel for diagnostics, and
    // it's also the stream that `bun run` / sandbox bash executors tend
    // to surface on non-zero exit — so the caller actually sees it.
    process.stderr.write(stripAnsi(output));
    exit(code);
  }
}

// Success: exit silently. Nothing to print.
exit(0);
