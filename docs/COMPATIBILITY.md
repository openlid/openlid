# Compatibility Promise

Open-Lid v2.x adheres to [Semantic Versioning](https://semver.org/). This
document defines exactly which surfaces are covered by that promise.
Anything not listed here may change in any release.

> **v1 → v2 was a rename, not a redesign.** The CLI binary, cask, and
> Cargo crates were renamed from `open-lid` to `openlid`. The
> configuration directory moved from `io.openlid.open-lid` to
> `io.openlid.app`. Subcommands, flags, exit codes, config field names,
> control-socket wire shapes, and helper XPC method signatures are all
> unchanged — every v2.x stable surface is preserved verbatim under its
> new name. See `CHANGELOG.md` for the full v2.0.0 migration notes.

## Stable surfaces

Within the v2.x line, these do not break. A change that would break them
requires a v3.0 release.

### CLI: `openlid`

Stable:

- Subcommand names (`on`, `off`, `status`, `for`, `until`, `config`).
- Flag names (e.g. `--json` on `status`).
- Exit codes (0 = success; non-zero = failure with a stderr diagnostic).
- The semantic behavior of each subcommand.
- The structure and field names of `status --json` output.

Examples of allowed additive changes:

- A new subcommand (e.g. `openlid pause`).
- A new flag with a default that preserves existing behavior.
- A new field in `status --json`. Consumers MUST ignore unknown fields.

Examples requiring a v3.0:

- Renaming a subcommand.
- Removing a flag.
- Changing the meaning or output of `--json`.
- Returning a different exit code for the same success path.

### `config.toml`

Stable: every field name currently in the schema (see
`crates/core/src/config.rs`). At time of writing:

- `version`
- `enabled`
- `start_at_login`
- `activate_at_launch`
- `prevent_display_sleep`
- `default_duration_minutes`
- `battery_threshold_pct`
- `[modifiers]` sub-table (`only_on_ac`, `min_battery`, `schedule`)

The schema starts at `version = 1`. Future schema bumps (v2, v3, ...)
will warn-and-continue on load by current binaries; the `version` field
is the migration hook.

Allowed within v2.x:

- Adding new fields with sensible defaults (serde `#[serde(default)]`).
- Adding new optional sub-tables.

Not allowed within v2.x:

- Removing or renaming any field.
- Changing the type of an existing field.
- Changing the semantic meaning of an existing field.

### Control socket (Unix domain socket)

Stable: the JSON shapes of `ControlRequest`, `ControlResponse`, and
`Snapshot` as defined in `crates/core/src/ipc/control.rs`. Clients MUST
ignore unknown fields.

Allowed within v2.x:

- Adding new fields to any of these shapes.
- Adding new request variants (new values of the `cmd` tag).
- Adding new response variants (new values of the `result` tag).

Not allowed within v2.x:

- Removing or renaming variants or fields.
- Changing the type of an existing field.

The transport itself (line-delimited JSON over a Unix domain socket at
`~/Library/Application Support/io.openlid.app/control.sock`) is NOT covered
by this promise — see "Not stable" below. The wire shapes are; the
framing and path may evolve.

### Helper XPC protocol

Stable: the method signatures on `OpenLidHelperProtocol`, declared in
`crates/helper-protocol/objc/OpenLidHelperProtocol.h`.

The helper always ships with its client inside the same `.app` bundle,
so version skew between menubar and helper is impossible in practice.
The promise is still useful for third-party tools that may connect to
the helper directly.

Allowed within v2.x:

- Adding new methods to the protocol.

Not allowed within v2.x:

- Renaming or removing methods.
- Changing existing method signatures.

## What `version = 1` means

`config.toml` carries a `version` field. The constant
`openlid_core::config::SCHEMA_VERSION` is the highest version a given
build understands.

- Configs with no `version` field are treated as `version = 1`.
- Configs with `version == SCHEMA_VERSION` load normally.
- Configs with `version > SCHEMA_VERSION` (a future schema seen by an
  older binary) emit a warning and continue with serde's forward-compat
  behavior: unknown fields are silently ignored. A user who tries a v2
  beta and downgrades is not locked out of their config.

v2.0 wires in the first migration hook: `Config::load_with_v1_fallback`
reads from the v1 directory (`io.openlid.open-lid`) on first launch when
the v2 directory doesn't exist yet, so users upgrading from v1 keep
their settings. A future v3 may introduce schema-level (in-file) migration
for `config.toml` itself; v2 only handles the directory rename.

## Not stable

The following are explicitly NOT covered by the semver promise and may
change in any release:

- Internal Rust types, module structure, and trait shapes in
  `openlid-core` and `openlid-helper-protocol`.
- The control-socket path
  (`~/Library/Application Support/io.openlid.app/control.sock`).
- The control-socket transport framing (currently line-delimited JSON
  with one message per connection). The message *shapes* are locked;
  a future transport may exist alongside the current one.
- Log file paths and log line format.
- The CLI's auto-launch-the-app behavior when the menubar isn't
  running.
- Helper installation mechanics (`dev-install-helper.sh`,
  `SMAppService` (macOS Login Items API) registration, plist paths).
- Anything in `openlid-core` not surfaced through `Config` or the IPC
  types listed above.

## Reporting a break

If a v2.x release breaks a surface listed under "Stable surfaces", that
is a bug. File an issue at
<https://github.com/openlid/openlid/issues> with the failing
command/config and the version of Open-Lid you saw it on.
