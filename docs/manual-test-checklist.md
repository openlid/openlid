# Open-Lid MVP — Manual Test Checklist

Run on a real Apple Silicon MacBook (lid behavior cannot be simulated).
Before each run, uninstall any previous version via `scripts/dev-uninstall-helper.sh`.

## Prep
- [ ] `cargo build -p open-lid -p open-lid-helper`
- [ ] `./scripts/dev-install-helper.sh` — helper installed (one sudo prompt)
- [ ] `/Library/Logs/open-lid/helper.log` exists and contains "open-lid-helper starting"

## Menu bar app
- [ ] `./target/debug/open-lid` launches → icon appears in menu bar
- [ ] Icon is eye-slash (inactive) at first
- [ ] Click icon → menu appears with "Turn On", "Mode" submenu, "Quit"
- [ ] Click "Turn On" → icon switches to eye (active)
- [ ] In another terminal, `pmset -g | grep SleepDisabled` shows `1`

## CLI parity
- [ ] In a third terminal, `open-lid status` shows "Sleep prevention: ACTIVE"
- [ ] `open-lid off` → icon switches to eye-slash; `pmset -g` shows `SleepDisabled 0`
- [ ] `open-lid on` → re-enables
- [ ] `open-lid mode always-awake` → status shows mode = AlwaysAwake
- [ ] `open-lid for 2m` → status shows mode = Timed and `until` ≈ now+2min
- [ ] Wait 2 min → status shows mode unchanged but `preventing_sleep_now = false`
  (Note: timed auto-revert of `enabled` is in Plan 2; for MVP the timer just
   stops *preventing* sleep; the user is responsible for switching mode back)

## Lid behavior
- [ ] With mode = lid-closed and enabled, close the laptop lid with no
      external display attached → display turns off; system stays awake
- [ ] Tail `/Library/Logs/open-lid/helper.log` — see `pmset disablesleep 1` invocations
- [ ] Open lid → display wakes
- [ ] With mode = lid-closed and an external display attached → closing the
      lid does NOT force display off; system stays awake on the external display

## Cleanup
- [ ] Quit app via menu
- [ ] `pmset -g | grep SleepDisabled` shows `0` (sleep restored)
- [ ] `./scripts/dev-uninstall-helper.sh` — helper removed
- [ ] `ls /Library/LaunchDaemons/io.openlid.*` returns nothing
