# Open-Lid — Manual Smoke Checklist

CI covers unit + integration tests across `open-lid-core` and `open-lid-helper`.
The AppKit / IOKit / NSXPC / SMAppService surfaces aren't testable from CI, so
each release is smoke-tested by hand against a real Apple Silicon MacBook
before tagging. This file is that checklist.

## Prep

- [ ] Fresh shell. Previous Open-Lid uninstalled:
  - `pkill -f /Applications/OpenLid.app/Contents/MacOS/open-lid || true`
  - `./scripts/dev-uninstall-helper.sh`
  - `rm -rf /Applications/OpenLid.app ~/Library/Application\ Support/io.openlid.open-lid`
- [ ] `./scripts/install.sh` builds and installs into `/Applications`.
- [ ] `./scripts/dev-install-helper.sh` installs the helper (one sudo prompt).
  Local ad-hoc-signed builds bypass SMAppService; this path is dev-only.

## Menu bar app

- [ ] `open -a OpenLid` — laptop icon appears in the menu bar.
- [ ] Left-click — toggles state; icon updates (open ↔ closed laptop).
- [ ] Right-click — menu shows current status line + "Activate for ▸" submenu
      with Indefinitely / 5 / 10 / 15 / 30 min / 1 / 2 / 5 h.
- [ ] `pmset -g | grep SleepDisabled` returns `1` when on, `0` when off.

## CLI parity

- [ ] `open-lid status` reflects the menu bar state.
- [ ] `open-lid off` deactivates; menu bar icon updates within ~500ms.
- [ ] `open-lid on` reactivates using the default duration.
- [ ] `open-lid for 1m` activates with timer; after ~1 minute, state returns
      to OFF *without* user action.
- [ ] `open-lid until <future HH:MM>` activates with a wall-clock deadline.
- [ ] `open-lid status --json` emits valid JSON matching the `Snapshot` shape.

## Lid + display behavior

- [ ] With Open-Lid on, no external display, lid closed → display turns off,
      system stays awake (helper log: `pmset disablesleep 1`).
- [ ] Open lid → display wakes; system was never asleep.
- [ ] With Open-Lid on, external display attached, lid closed → both displays
      stay awake.
- [ ] With Open-Lid on, lid open, idle 10 min → screen does **not** lock
      (IOPMAssertion active). Turning off "Keep display awake while preventing
      sleep" in Preferences restores normal idle-lock behavior.

## Cleanup

- [ ] Quit via menu → `pmset -g | grep SleepDisabled` returns `0`.
- [ ] Helper has exited (idle-exit after 15s of no XPC traffic).
- [ ] `./scripts/dev-uninstall-helper.sh` removes the LaunchDaemon plist.
