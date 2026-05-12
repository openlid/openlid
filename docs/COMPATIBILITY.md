# Compatibility Promise

Open-Lid v1.x adheres to [Semantic Versioning](https://semver.org/). This
document defines exactly which surfaces are covered by that promise.
Anything not listed here may change in any release.

## Stable surfaces

Within the v1.x line, these do not break. A change that would break them
requires a v2.0 release.

### CLI: `open-lid`

Stable:

- Subcommand names (`on`, `off`, `status`, `for`, `until`, `config`).
- Flag names (e.g. `--json` on `status`).
- Exit codes (0 = success; non-zero = failure with a stderr diagnostic).
- The semantic behavior of each subcommand.
- The structure and field names of `status --json` output.

Examples of allowed additive changes:

- A new subcommand (e.g. `open-lid pause`).
- A new flag with a default that preserves existing behavior.
- A new field in `status --json`. Consumers MUST ignore unknown fields.

Examples requiring a v2.0:

- Renaming a subcommand.
- Removing a flag.
- Changing the meaning or output of `--json`.
- Returning a different exit code for the same success path.

### `config.toml`

Stable: every field name currently in the schema (see
`crates/core/src/config.rs`).

The schema starts at `version = 1`. Future schema bumps (v2, v3, ...)
will warn-and-continue on load by current binaries; the `version` field
is the migration hook.

Allowed within v1.x:

- Adding new fields with sensible defaults (serde `#[serde(default)]`).
- Adding new optional sub-tables.

Not allowed within v1.x:

- Removing or renaming any field.
- Changing the type of an existing field.
- Changing the semantic meaning of an existing field.

### Control socket (Unix domain socket)

Stable: the JSON shapes of `ControlRequest`, `ControlResponse`, and
`Snapshot` as defined in `crates/core/src/ipc/control.rs`. Clients MUST
ignore unknown fields.

Allowed within v1.x:

- Adding new fields to any of these shapes.
- Adding new request variants (new values of the `cmd` tag).
- Adding new response variants (new values of the `result` tag).

Not allowed within v1.x:

- Removing or renaming variants or fields.
- Changing the type of an existing field.

The transport itself (line-delimited JSON over a Unix domain socket at
`~/Library/Application Support/open-lid/control.sock`) is NOT covered
by this promise — see "Not stable" below. The wire shapes are; the
framing and path may evolve.

### Helper XPC protocol

Stable: the method signatures on `OpenLidHelperProtocol`, declared in
`crates/helper-protocol/objc/OpenLidHelperProtocol.h`.

The helper always ships with its client inside the same `.app` bundle,
so version skew between menubar and helper is impossible in practice.
The promise is still useful for third-party tools that may connect to
the helper directly.

Allowed within v1.x:

- Adding new methods to the protocol.

Not allowed within v1.x:

- Renaming or removing methods.
- Changing existing method signatures.

## What `version = 1` means

`config.toml` carries a `version` field. The constant
`open_lid_core::config::SCHEMA_VERSION` is the highest version a given
build understands.

- Configs with no `version` field are treated as `version = 1`.
- Configs with `version == SCHEMA_VERSION` load normally.
- Configs with `version > SCHEMA_VERSION` (a future schema seen by an
  older binary) emit a warning and continue with serde's forward-compat
  behavior: unknown fields are silently ignored. A user who tries a v2
  beta and downgrades is not locked out of their config.

A future v2 release may introduce migration logic; v1.0 only adds the
hook.

## Not stable

The following are explicitly NOT covered by the semver promise and may
change in any release:

- Internal Rust types, module structure, and trait shapes in
  `open-lid-core` and `open-lid-helper-protocol`.
- The control-socket path
  (`~/Library/Application Support/open-lid/control.sock`).
- The control-socket transport framing (currently line-delimited JSON
  with one message per connection). The message *shapes* are locked;
  a future transport may exist alongside the current one.
- Log file paths and log line format.
- The CLI's auto-launch-the-app behavior when the menubar isn't
  running.
- Helper installation mechanics (`dev-install-helper.sh`,
  `SMAppService` (macOS Login Items API) registration, plist paths).
- Anything in `open-lid-core` not surfaced through `Config` or the IPC
  types listed above.

## Reporting a break

If a v1.x release breaks a surface listed under "Stable surfaces", that
is a bug. File an issue at
<https://github.com/diyanbogdanov/open-lid/issues> with the failing
command/config and the version of Open-Lid you saw it on.
