# Architecture

This document explains why Open-Lid is structured the way it is. For
high-level usage, see the [README](../README.md).

## The fundamental constraint

macOS sleeps the system when the lid closes. This behavior is enforced
by the kernel, not user-space — `IOPMAssertion` APIs *cannot* override
it. The only way to prevent lid-driven sleep is to write to the system
power-management settings via `pmset -a disablesleep 1`, which requires
**root**. There is no IOKit assertion, no `caffeinate` flag, no
ServiceManagement API that achieves this without root.

Everything else in the architecture follows from this one constraint.

## Process model

Three runtime roles, two binaries:

```
OpenLid.app/Contents/MacOS/openlid               (signed, runs as user)
    ├─ menubar      — long-running UI process
    └─ cli          — short-lived; talks to menubar

OpenLid.app/Contents/MacOS/openlid-helper        (signed, root via launchd)
    └─ daemon       — only component that calls pmset disablesleep
```

The user-space role is a single binary that dispatches by argv. The
privileged helper is a separate binary because macOS code signing applies
entitlements *per binary* — keeping the privileged helper isolated lets
us declare a minimal entitlement surface for it.

### Why two IPC channels

```
   ┌──────────────┐    UDS    ┌───────────────┐    XPC    ┌────────────┐
   │   openlid   │  ──────►  │   openlid    │  ──────►  │ openlid-  │
   │  (cli role)  │           │  (menubar)    │           │  helper    │
   │   per-cmd    │           │  long-running │           │  (root)    │
   └──────────────┘           └───────────────┘           └────────────┘
```

- **CLI ↔ menubar (Unix domain socket).** Same-user, no privilege boundary.
  The menubar process owns *all state* — current mode, timer, prefs, helper
  connection. The CLI is a thin client. Single source of truth.

- **menubar ↔ helper (NSXPC).** Crosses a privilege boundary (user → root).
  NSXPC provides Mach-port-level isolation, code-signature-based client
  validation, and is the macOS-native way to talk to a launchd daemon.

A Unix socket would work for the user→root path too, but NSXPC adds:

1. **Code-signature validation.** The helper rejects connections from
   anything not signed with the matching Team ID + bundle identifier.
   With a raw socket, any local user could control the daemon.

2. **Lazy launch.** launchd starts the helper on first connection and
   restarts it after the helper's idle-exit timer fires. NSXPC handles
   the reconnect transparently.

3. **Typed protocol.** Our `OpenLidHelperProtocol` is a Clang-emitted
   Obj-C protocol shared by both ends; the runtime validates each call.

## Crate layout

```
crates/core            — openlid-core (lib)
                          pure logic: types, state machine, config schema,
                          IPC types, platform traits. No macOS deps.

crates/helper-protocol — openlid-helper-protocol (lib)
                          Clang-emitted NSXPC protocol metadata. Shared
                          by app and helper.

crates/helper          — openlid-helper (bin)
                          privileged daemon. Owns the NSXPC listener,
                          pmset wrapper, idle-exit timer, ownership marker,
                          code-requirement validator.

crates/app             — openlid (bin, two roles via argv)
                          menubar role:
                            - NSStatusItem + custom-drawn icon
                            - IOKit lid monitor (IOPMrootDomain interest)
                            - IOPS power-source monitor
                            - CGDisplay external-display detection
                            - NSXPC helper client
                            - UDS control server
                            - Native preferences NSWindow
                            - State runtime (orchestrator)
                          cli role:
                            - clap arg parser
                            - UDS control client
                            - auto-launches .app if menubar not running

xtask                  — release pipeline (planned: bundle, sign, notarize, dmg)
```

### Dependency direction

```
xtask
   │
   ▼
app   ──►  core  ◄──  helper
  │           ▲           │
  └─ helper-protocol ─────┘
```

`core` has zero macOS dependencies. The same crate compiles on Linux or
Windows. This is what makes future cross-platform support a `add a
platform/linux/` job, not a rewrite.

## State machine

The whole "should we be preventing sleep right now?" question reduces to
one pure function in `openlid-core::state`:

```rust
pub fn should_prevent_sleep(state: &AppState, now: DateTime<Local>) -> bool {
    if !state.enabled { return false; }
    if !modifiers_allow(&state.modifiers, now, &state.power) { return false; }
    if let Some(until) = state.until {
        return now < until;
    }
    true
}
```

This is the **single source of truth**. The menu bar icon, the menu's
status row, the CLI status output, and the helper's `setSleepPrevention`
call all read from this same function. They can't disagree.

The function is pure and exhaustively unit-tested in `core/src/state.rs`.

## Event flow

The menubar process subscribes to:

- **IOKit `IOPMrootDomain` clamshell-state-change** → updates `state.lid`,
  re-evaluates, and if the lid just closed (and no external display) calls
  `pmset displaysleepnow` to turn the laptop screen off.
- **IOPS power-source change** → updates `state.power`, re-evaluates, and
  if battery dropped below the configured threshold, auto-deactivates.
- **CLI requests** over UDS → mutate state (`set_enabled`, `set_preferences`)
  and trigger a reconcile.
- **Timer expiry** (one-shot scheduled thread) → re-evaluates; expired
  timer clears `enabled` and `until`.
- **Menu clicks** → invoke `MenuActions` trait methods, which call into
  `StateRuntime`.

After every state mutation, a listener chain fires (registered via
`StateRuntime::add_listener`). The menubar UI subscribes to refresh the
icon and menu items; the listener dispatches to the AppKit main thread
via `dispatch_async_f` so callbacks from worker threads (like the UDS
server) don't touch AppKit off-main.

### Two outputs from one reconciler

The reconciler computes two desired states from the same `AppState`:

1. **System-sleep prevention** (`desired = should_prevent_sleep(state)`) →
   dispatched to the helper over NSXPC, which runs `pmset -a disablesleep`
   under root.
2. **Display-idle prevention** (`want_assertion = desired && cfg
   .prevent_display_sleep && (lid_open || external_display)`) → managed
   in-process via `IOPMAssertion`, no helper involvement. Held only while
   there is actually a display worth keeping awake; released when the lid
   closes with no external monitor so that the `force_display_sleep`
   branch above lands uncontested.

Each output has its own `last_applied`-style cache (`last_applied` and
`last_assertion_held` respectively) so a reconcile that finds nothing
changed is a no-op for both layers.

## Reconcile pattern

The helper is **stateless about modes** — it only knows on/off. The
menubar process is responsible for computing "should we be preventing
sleep right now" and telling the helper. After every state change:

```rust
fn reconcile(&self) {
    let desired = should_prevent_sleep(&state, Local::now());
    // (1) System sleep — dispatched over NSXPC to the privileged helper.
    if last_applied != desired {
        match desired {
            true  => self.power.prevent_sleep(),
            false => self.power.allow_sleep(),
        }
        last_applied = desired;
    }
    // (2) Display-idle sleep — in-process IOPMAssertion. Held only when
    //     there's a display worth keeping awake.
    let want = desired
        && cfg.prevent_display_sleep
        && (state.lid == LidState::Open || display.has_external_display());
    if last_assertion_held != want {
        match want {
            true  => self.display.prevent_display_sleep(),
            false => self.display.allow_display_sleep(),
        }
        last_assertion_held = want;
    }
}
```

This is the diff-and-apply pattern from declarative-configuration
literature, scaled to two booleans. The benefits:

- **Idempotent.** A `set_enabled(true)` followed by another `set_enabled(true)`
  results in *one* XPC call and *one* assertion acquire, not two.
- **Crash-resistant.** On menubar restart, the state runtime loads the
  persisted `enabled` flag from config, calls reconcile once, and the
  helper picks up where it left off. Display assertions are
  process-scoped and are released by macOS automatically when the app
  exits — no extra recovery code needed.
- **Helper independence.** The helper has its own startup-time
  reconciliation: if its ownership marker exists but no client has
  connected, it assumes the previous app crashed and restores normal
  sleep before accepting new connections.

## Helper validation

The helper accepts NSXPC connections only from clients whose signing
identity matches a code-requirement string:

```
identifier "io.openlid.app"
  and anchor apple generic
  and certificate leaf[subject.OU] = "<TeamID>"
  and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */
  and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */
```

This translates to: "the binary must be signed by a Developer ID
Application certificate issued under Apple's Developer ID CA to my team."

Until the project's Apple Developer enrollment completes, the helper uses
a permissive dev requirement (`identifier "io.openlid.app"`) that
accepts ad-hoc-signed local builds. The production string lives in
`crates/helper/src/main.rs` as `PROD_REQUIREMENT` and is activated by
flipping the `validator = …` line.

## Idle-exit timer

The helper is launched by launchd on first connection and exits 15
seconds after the last client disconnects. This pattern:

- **Minimizes the privileged surface area.** The helper is only running
  when actively needed.
- **Avoids resource consumption.** No background daemon eating memory.
- **Survives crashes gracefully.** launchd relaunches on the next
  connection.

Implementation uses a generation counter on each `arm()` call to
invalidate stale sleep-and-fire threads when a new connection arrives.
Same pattern used in the menubar's timer-expiry scheduler.

## Why a custom-drawn icon and not bundled raster

The menu-bar icon uses `objc2-app-kit` to draw with `NSBezierPath` inside
an `NSImage` drawing handler. This:

- Produces a proper template image (alpha-only, system-tinted) at any
  backing scale factor.
- Adapts automatically to light/dark menu bar themes.
- Avoids shipping multiple PNG sizes.
- Is one source of truth for the icon's shape.

## Future cross-platform plans

The `openlid-core` traits already represent the cross-platform shape:

```rust
trait PowerController: Send + Sync {
    fn prevent_sleep(&self) -> Result<(), PlatformError>;
    fn allow_sleep(&self) -> Result<(), PlatformError>;
}

trait LidObserver: Send + Sync {
    fn current(&self) -> LidState;
    fn subscribe(&self, callback: LidStateCallback);
}

// ...PowerSourceMonitor, DisplayController
```

Future Windows implementation: `SetThreadExecutionState(ES_SYSTEM_REQUIRED
| ES_CONTINUOUS)` + WM_POWERBROADCAST.

Future Linux implementation: D-Bus to `org.freedesktop.login1`'s `Inhibit`
method, possibly via the `zbus` crate.

These platforms would add `crates/app/src/platform/{windows,linux}/`
directories without touching `core` or the menubar UI structure (modulo
trait-implementation glue).
