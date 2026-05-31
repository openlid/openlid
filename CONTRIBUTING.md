# Contributing to Open-Lid

Thank you for considering a contribution! Open-Lid is a small project
maintained in spare time, so I appreciate every issue, PR, and question.

## Ground rules

- **Be patient.** I respond to issues and PRs within a week or two, sometimes
  longer. If a thread goes quiet, a polite bump is fine after 14 days.
- **Be respectful.** This project follows the [Contributor Covenant Code of
  Conduct](CODE_OF_CONDUCT.md). Violations are taken seriously.
- **Match the project's scope.** Open-Lid is intentionally small. Big new
  features need a design discussion before implementation.

## Quick start for contributors

```bash
# Prerequisites: Rust 1.88+. To build/run the macOS backend you also need
# macOS 13+ on Apple Silicon and Xcode CLT. `openlid-core` (the pure-logic
# crate) compiles on Linux and Windows too — no platform-specific toolchain
# needed for working on the state machine, config, or IPC schemas.
git clone https://github.com/openlid/openlid.git
cd openlid
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

The full development loop (rebuild app, install into `/Applications`,
refresh caches):

```bash
./scripts/install.sh
./scripts/dev-install-helper.sh    # one-time, requires sudo
open -a OpenLid
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the layered design.

## What kinds of contributions are welcome

### Welcome and likely to be accepted

- **Bug fixes** with a clear reproduction. Open an issue first if the fix is
  non-trivial — it's easier to align on the approach before code review.
- **Documentation improvements** — typos, clarifications, new examples,
  troubleshooting entries.
- **Test coverage improvements** — especially the `openlid` (app) crate,
  which is the hardest to cover due to AppKit / IOKit FFI.
- **Performance improvements** with before/after measurements.
- **Accessibility improvements** — VoiceOver labels, keyboard navigation.
- **Localization** — once an i18n framework lands; ping me first if you want
  to drive that work.

### Discuss before opening a PR

- **New features.** Open an issue describing the use case and proposed
  shape. I'd rather discuss the design than receive a finished PR for
  something that doesn't fit the project's direction.
- **Refactors that touch >5 files.** Big architectural changes need
  alignment first.
- **Dependency additions.** Every dependency adds maintenance burden and
  attack surface. Justify each one.
- **Breaking changes to CLI or config format.** These need a migration
  plan.

### Likely to be declined

- **Linux/Windows backend implementations without prior design discussion.**
  Linux support is planned for v3.0.0, and Windows support depends on
  user demand. Please open an issue first describing which OS, which APIs
  (logind/D-Bus, `SetThreadExecutionState`, etc.), and what your testing
  setup looks like.
- **Adding the ability to commercially resell Open-Lid as a different product.**
  Apache 2.0 permits this legally, but you don't need to ask me; just do it
  with proper attribution per the license terms.
- **Telemetry, analytics, or "phone-home" features** beyond opt-in
  anonymized usage stats. Open-Lid runs with elevated privileges; trust
  matters more than data.

## Architecture quick tour

Three crates:

| Crate | Role |
|---|---|
| `openlid-core` | Pure logic: types, state machine, config schema, IPC types, platform traits. Zero macOS dependencies. Compiles on any target. |
| `openlid-helper` | Privileged launchd daemon. Owns the NSXPC listener and `pmset` calls. |
| `openlid` | Menu bar app + CLI dispatcher. Owns AppKit UI, IOKit sensors, NSXPC client to the helper, Unix-socket control server for the CLI. |

A few more in support:

| Crate | Role |
|---|---|
| `openlid-helper-protocol` | Clang-emitted NSXPC protocol metadata. Shared between app and helper. |
| `xtask` | Build automation (release pipeline). |

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design rationale.

## Coding standards

- **Edition 2021**, MSRV **1.81**, stable Rust only.
- **`cargo fmt`** enforced in CI.
- **`cargo clippy --all-targets -- -D warnings`** must pass.
- **Tests** for any new logic in `openlid-core` or `openlid-helper`.
  AppKit / FFI code doesn't need automated tests; rely on manual smoke
  testing instead.
- **Comments explain *why*, not *what*.** If your code needs a comment to
  explain *what* it does, consider clearer names instead.
- **Unsafe code requires a SAFETY comment** above every `unsafe` block,
  explaining the invariant being upheld. This is enforced by clippy.

## Commit messages

Use the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
format:

```
type(scope): short imperative summary

Optional longer paragraph explaining the why. Reference issues if relevant.
```

Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`.
Common scopes: `core`, `helper`, `app`, `app/macos`, `menubar`, `cli`,
`prefs`, `scripts`, `ci`.

Examples from this repo's history:

```
feat(menubar): single-instance enforcement via control-socket probe
fix(app): icon refreshes after CLI-driven state changes
refactor: consolidate state machine into a single decision function
```

## Pull requests

Before opening a PR:

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --check` is clean
- [ ] If you changed user-visible behavior, [CHANGELOG.md](CHANGELOG.md) is updated
- [ ] If you touched docs, links still resolve

In your PR description:

- Summarize the change in one sentence
- Link the issue this resolves (if any)
- Note any manual smoke-testing you did (since CI can't test AppKit)

## Reporting issues

Open a [new issue](https://github.com/openlid/openlid/issues/new/choose).
There are templates for bugs and feature requests. The bug template asks
for:

- macOS version (`sw_vers -productVersion`)
- Mac model / CPU architecture (`uname -m`)
- Open-Lid version (`openlid --version`)
- What you did, what you expected, what actually happened
- Relevant lines from `~/Library/Logs/openlid/app.log`

## Security issues

**Do not file security issues as public GitHub issues.** See
[SECURITY.md](SECURITY.md) for the disclosure process.

## License

By contributing, you agree that your contributions will be licensed under
the [Apache License, Version 2.0](LICENSE). You don't need to sign a CLA —
your `git commit` is the implicit grant.
