# Open-Lid — Manual Smoke Checklist

CI covers unit + integration tests across `openlid-core` and `openlid-helper`.
The AppKit / IOKit / NSXPC / SMAppService surfaces aren't testable from CI, so
each release is smoke-tested by hand against a real Apple Silicon MacBook
before tagging. This file is that checklist.

## Prep

- [ ] Fresh shell. Previous Open-Lid uninstalled:
  - `pkill -f /Applications/OpenLid.app/Contents/MacOS/openlid || true`
  - `./scripts/dev-uninstall-helper.sh`
  - `rm -rf /Applications/OpenLid.app ~/Library/Application\ Support/io.openlid.app`
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

- [ ] `openlid status` reflects the menu bar state.
- [ ] `openlid off` deactivates; menu bar icon updates within ~500ms.
- [ ] `openlid on` reactivates using the default duration.
- [ ] `openlid for 1m` activates with timer; after ~1 minute, state returns
      to OFF *without* user action.
- [ ] `openlid until <future HH:MM>` activates with a wall-clock deadline.
- [ ] `openlid status --json` emits valid JSON matching the `Snapshot` shape.

## Lid + display behavior

- [ ] With Open-Lid on, no external display, lid closed → display turns off,
      system stays awake (helper log: `pmset disablesleep 1`).
- [ ] Open lid → display wakes; system was never asleep.
- [ ] With Open-Lid on, external display attached, lid closed → both displays
      stay awake.
- [ ] With Open-Lid on, lid open, idle 10 min → screen does **not** lock
      (IOPMAssertion active). Turning off "Keep display awake while preventing
      sleep" in Preferences restores normal idle-lock behavior.

## Schedule (recurring time-window)

- [ ] `openlid schedule clear` — leaves things in a known state.
- [ ] `openlid schedule set --from 09:00 --to 18:00` while toggle is OFF —
      output ends with "openlid is now ON"; `openlid status` shows
      `Schedule: 09:00-18:00 (daily)`; if local time is inside the window,
      `pmset -g | grep SleepDisabled` returns `1`, otherwise `0`.
- [ ] Outside the window (or simulate by setting a window in the past):
      `openlid status` still shows enabled=true but `preventing_sleep_now`
      false; `pmset` shows sleep is allowed.
- [ ] `openlid schedule show` prints a one-line summary; `--json` emits the
      raw modifier object or `null`.
- [ ] `openlid schedule set --from 09:00 --to 09:00` rejects with a
      "non-empty" error.
- [ ] `openlid schedule set --from 22:00 --to 02:00` (cross-midnight) — at
      23:30 sleep is prevented; at 03:00 it is not. (Easiest to verify
      with a tighter window for testing.)
- [ ] `openlid schedule set --from 09:00 --to 18:00 --days Mon,Wed,Fri` —
      `openlid status` shows `Mon, Wed, Fri` in the schedule line.
- [ ] `openlid schedule clear` — `openlid status` no longer prints the
      Schedule line; the toggle remains ON (asymmetry by design).
- [ ] Open Preferences. Tick "Active only during scheduled hours" — the
      From/To fields and day checkboxes go enabled; the runtime now has a
      schedule (verify with `openlid schedule show`).
- [ ] Edit From to `08:00`, tab out — `openlid schedule show` reflects 08:00.
- [ ] Uncheck Sat and Sun — `openlid status` shows `Mon-Fri`.
- [ ] Untick the master — `openlid status` no longer prints Schedule.
- [ ] Type a garbage value into From (e.g. `nope`), tab out — the runtime
      keeps the prior valid time (parse failure is silent at the UI layer).

## Cleanup

- [ ] Quit via menu → `pmset -g | grep SleepDisabled` returns `0`.
- [ ] Helper has exited (idle-exit after 15s of no XPC traffic).
- [ ] `./scripts/dev-uninstall-helper.sh` removes the LaunchDaemon plist.
