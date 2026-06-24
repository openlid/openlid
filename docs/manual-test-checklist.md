# Manual test checklist

Cases that can't be unit-tested because they depend on macOS UI, the
notarized helper, or live power/network state. Run these on a **signed,
notarized Developer-ID build installed in `/Applications`** — daemon
registration and `UNUserNotificationCenter` behave differently for
ad-hoc/unbundled builds.

## Helper auto-recovery + approval surfacing

Pre-req: get the helper into the `RequiresApproval` state (fresh install,
or toggle OpenLid off in System Settings → General → Login Items &
Extensions → "Allow in the Background").

- [ ] **Approval banner on launch.** Launch OpenLid with the helper not
      yet approved → a notification banner ("OpenLid isn't keeping your
      Mac awake") appears.
- [ ] **Tap deep-links to Settings.** Tap the banner (or its "Open System
      Settings" action) → System Settings opens to the Login Items pane.
- [ ] **Reconnect without relaunch.** With OpenLid still running, flip the
      "Allow in the Background" toggle ON → within the bounded follow-up
      (≤ ~2 min) `pmset -g | grep SleepDisabled` reads `1`. No relaunch
      needed.
- [ ] **Closed-lid survival.** With sleep prevention ON and the helper
      enabled, close the lid on battery → the Mac stays awake and an
      `ssh`/ping session to it survives.
- [ ] **No nag while turning OFF.** With the helper unavailable, toggle
      OpenLid OFF → no approval banner appears (surfacing is prevent-path
      only).
- [ ] **Rate limiting.** Repeated reconciles while unhealthy post at most
      one banner per episode; after a successful reconnect a later
      unhealthy episode can surface again.
- [ ] **Notifications denied fallback.** Deny notification permission for
      OpenLid, then trigger an unhealthy episode → no banner, but System
      Settings is opened directly (once per episode) and the follow-up
      still runs.

## In-transit activity guard

Pre-req: enable the in-transit detector (Preferences → in-transit
timeout, e.g. 2 min). Set up: lid closed, on battery, no external
display, no reachable network.

- [ ] **Idle still auto-disables.** With the machine idle in the above
      state → OpenLid auto-disables after the timeout (unchanged
      behavior); the Mac then sleeps normally.
- [ ] **Busy defers.** Run a sustained CPU workload (e.g. a local agent /
      `yes > /dev/null`) in the same state → OpenLid does **not**
      auto-disable; the log shows "in-transit auto-disable deferred …
      machine is busy".
- [ ] **Defer then fire when idle.** Keep the offline/closed/battery
      state, stop the workload → on the next busy recheck (~60 s) OpenLid
      auto-disables.
- [ ] **Reachable cancels.** While deferred, bring the network back →
      the recheck is cancelled (generation bump) and OpenLid stays on.

## Regression — pre-existing behavior preserved

- [ ] On AC, in-transit never fires (busy or idle).
- [ ] With an external display attached, in-transit never fires.
- [ ] Battery-threshold auto-disable still works independently.
- [ ] `openlid status` and the menu reflect the toggle as before.
