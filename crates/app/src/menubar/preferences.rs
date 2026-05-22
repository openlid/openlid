//! Native macOS Preferences window.
//!
//! A single `NSWindow` with four controls — start-at-login, activate-at-launch,
//! a default-duration popup, and a battery-threshold checkbox + numeric field.
//! Each control routes through one selector on `PrefsHandler` which calls into
//! a `PrefsActions` trait object. The outer `RuntimeActions` impl translates
//! each call into a single-field `PrefsPatch` and dispatches it through
//! `StateRuntime::set_preferences`.
//!
//! Threading: all construction and all callbacks happen on the main thread —
//! the menu click that opens the window, the AppKit action invocations after
//! that. `PrefsActions` callbacks are therefore free to do main-thread work
//! (like `RuntimeActions::refresh`). The shared `PrefsActions` is `Send + Sync`
//! so the ivar can hold it without contortions.
//!
//! Window lifecycle: the window object is constructed lazily on first
//! `show()`, then kept alive for the life of the app. Closing the window
//! (red button or our "Close" button) just hides it; the next `show()` brings
//! it back. Subsequent shows refresh the controls from the latest snapshot.

use chrono::NaiveTime;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn, NSPopUpButton,
    NSTextField, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use openlid_core::ipc::control::Snapshot;
use openlid_core::mode::{DaysOfWeek, Schedule};
use std::cell::OnceCell;
use std::sync::Arc;

/// Operations the preferences UI can invoke. Implemented over `RuntimeActions`
/// by the outer module so this file stays free of `StateRuntime`'s generics.
pub trait PrefsActions: Send + Sync {
    fn set_start_at_login(&self, enabled: bool);
    fn set_activate_at_launch(&self, enabled: bool);
    fn set_default_duration(&self, minutes: Option<u32>);
    fn set_battery_threshold(&self, pct: Option<u8>);
    fn set_prevent_display_sleep(&self, enabled: bool);
    /// Apply a schedule update. `None` clears any existing schedule;
    /// `Some(s)` sets it. Implementations should also turn the toggle on
    /// when transitioning from no-schedule to schedule, so the new gate
    /// has an enabled state to constrain.
    fn set_schedule(&self, schedule: Option<Schedule>);
}

/// Default battery-threshold value shown in the disabled numeric field when
/// the threshold preference is `None`.
const DEFAULT_BATTERY_PCT: u8 = 20;

/// The "Default duration" popup entries. `(label, minutes)` — `minutes == 0`
/// means indefinite (no timer). Matches the activate-for submenu in `menu.rs`.
const DURATION_ENTRIES: &[(&str, isize)] = &[
    ("Indefinitely", 0),
    ("5 minutes", 5),
    ("10 minutes", 10),
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("1 hour", 60),
    ("2 hours", 120),
    ("5 hours", 300),
];

/// AppKit control handles shared between the Rust wrapper and the Obj-C
/// handler. The handler needs the text-field handle so it can toggle its
/// `enabled` state when the battery checkbox is clicked.
///
/// # Safety
///
/// `Send + Sync` are asserted manually. Every touch of these fields happens
/// on the main thread — either from `PreferencesWindow::show` (called from a
/// main-thread menu click) or from the AppKit-invoked selectors on
/// `PrefsHandler` (which is `MainThreadOnly`).
struct PrefsControls {
    start_at_login: Retained<NSButton>,
    activate_at_launch: Retained<NSButton>,
    prevent_display_sleep: Retained<NSButton>,
    duration_popup: Retained<NSPopUpButton>,
    battery_checkbox: Retained<NSButton>,
    battery_field: Retained<NSTextField>,
    schedule_master: Retained<NSButton>,
    schedule_from: Retained<NSTextField>,
    schedule_to: Retained<NSTextField>,
    /// Day-of-week checkboxes in Mon..Sun order.
    schedule_days: [Retained<NSButton>; 7],
}

// SAFETY: see PrefsControls doc.
unsafe impl Send for PrefsControls {}
// SAFETY: see PrefsControls doc.
unsafe impl Sync for PrefsControls {}

#[derive(Default)]
pub struct PrefsHandlerIvars {
    actions: OnceCell<Arc<dyn PrefsActions>>,
    controls: OnceCell<Arc<PrefsControls>>,
    window: OnceCell<Retained<NSWindow>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `PrefsHandler` does not implement `Drop`; ivars are dropped via the
    //   `define_class!` machinery.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PrefsHandlerIvars]
    pub struct PrefsHandler;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for PrefsHandler {}

    impl PrefsHandler {
        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setStartAtLogin:))]
        fn set_start_at_login(&self, sender: Option<&AnyObject>) {
            let state = checkbox_state(sender);
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_start_at_login(state);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setActivateAtLaunch:))]
        fn set_activate_at_launch(&self, sender: Option<&AnyObject>) {
            let state = checkbox_state(sender);
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_activate_at_launch(state);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setPreventDisplaySleep:))]
        fn set_prevent_display_sleep(&self, sender: Option<&AnyObject>) {
            let state = checkbox_state(sender);
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_prevent_display_sleep(state);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setDefaultDuration:))]
        fn set_default_duration(&self, sender: Option<&AnyObject>) {
            // The sender is the NSPopUpButton itself; read the selected item's
            // tag (minutes; 0 = indefinite).
            let tag: isize = match sender {
                Some(s) => unsafe { msg_send![s, selectedTag] },
                None => 0,
            };
            let minutes = if tag <= 0 { None } else { Some(tag as u32) };
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_default_duration(minutes);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setBatteryThreshold:))]
        fn set_battery_threshold(&self, _sender: Option<&AnyObject>) {
            // Two trigger paths: the checkbox toggled (sender == checkbox), or
            // the text field committed a new value (sender == text field).
            // Either way we read both controls and derive the new threshold.
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let checkbox_on = controls.battery_checkbox.state() == NSControlStateValueOn;
            // Always keep the text-field's enabled state in sync with the
            // checkbox — even if this invocation came from the text field
            // (no-op in that case).
            controls.battery_field.setEnabled(checkbox_on);

            let pct = if checkbox_on {
                let v = controls.battery_field.integerValue();
                // Clamp to a sane range. We accept 1–100; 0 disables the
                // feature semantically but the checkbox is the on/off — out-
                // of-range values get clamped to the default.
                let clamped = if (1..=100).contains(&v) {
                    v as u8
                } else {
                    DEFAULT_BATTERY_PCT
                };
                Some(clamped)
            } else {
                None
            };

            if let Some(actions) = self.ivars().actions.get() {
                actions.set_battery_threshold(pct);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setScheduleMaster:))]
        fn set_schedule_master(&self, _sender: Option<&AnyObject>) {
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let master_on = controls.schedule_master.state() == NSControlStateValueOn;
            schedule_set_subcontrols_enabled(controls, master_on);

            let new = if master_on {
                Some(build_schedule_from_controls(controls))
            } else {
                None
            };
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_schedule(new);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setScheduleField:))]
        fn set_schedule_field(&self, _sender: Option<&AnyObject>) {
            // Any of from/to/day controls changed. If the master is off, we
            // don't send updates — those controls are visually disabled and
            // any stray AppKit event we get for them is meaningless.
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let master_on = controls.schedule_master.state() == NSControlStateValueOn;
            if !master_on {
                return;
            }
            let s = build_schedule_from_controls(controls);
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_schedule(Some(s));
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(closeWindow:))]
        fn close_window(&self, _sender: Option<&AnyObject>) {
            if let Some(window) = self.ivars().window.get() {
                window.close();
            }
        }
    }
);

/// Default window when the master is flipped on with no prior schedule.
const DEFAULT_SCHEDULE_START: (u32, u32) = (9, 0);
const DEFAULT_SCHEDULE_END: (u32, u32) = (17, 0);

/// Read the schedule sub-controls and synthesize a `Schedule`. Falls back to
/// 09:00-17:00 / all days when any control is unparseable or no day is
/// checked, so a freshly-ticked master checkbox always yields a sensible
/// schedule rather than something that gates to never-active.
fn build_schedule_from_controls(controls: &PrefsControls) -> Schedule {
    let start = parse_hhmm_field(&controls.schedule_from).unwrap_or_else(|| {
        NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_START.0, DEFAULT_SCHEDULE_START.1, 0).unwrap()
    });
    let end = parse_hhmm_field(&controls.schedule_to).unwrap_or_else(|| {
        NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_END.0, DEFAULT_SCHEDULE_END.1, 0).unwrap()
    });
    // Equal start/end is a zero-length window — guard the same way the CLI
    // does. Bump end by one minute so the schedule is at least nominally
    // active. The user can correct it after seeing the value in the field.
    let end = if start == end {
        end.overflowing_add_signed(chrono::TimeDelta::minutes(1)).0
    } else {
        end
    };
    const DAY_FLAGS: [DaysOfWeek; 7] = [
        DaysOfWeek::MON,
        DaysOfWeek::TUE,
        DaysOfWeek::WED,
        DaysOfWeek::THU,
        DaysOfWeek::FRI,
        DaysOfWeek::SAT,
        DaysOfWeek::SUN,
    ];
    let mut days = DaysOfWeek::empty();
    for (i, btn) in controls.schedule_days.iter().enumerate() {
        if btn.state() == NSControlStateValueOn {
            days |= DAY_FLAGS[i];
        }
    }
    if days.is_empty() {
        days = DaysOfWeek::all();
    }
    Schedule { days, start, end }
}

fn parse_hhmm_field(field: &NSTextField) -> Option<NaiveTime> {
    let nsstr = field.stringValue();
    NaiveTime::parse_from_str(nsstr.to_string().trim(), "%H:%M").ok()
}

fn schedule_set_subcontrols_enabled(controls: &PrefsControls, on: bool) {
    controls.schedule_from.setEnabled(on);
    controls.schedule_to.setEnabled(on);
    for btn in &controls.schedule_days {
        btn.setEnabled(on);
    }
}

/// Read an NSButton (checkbox)'s state as a plain bool. `sender` is the
/// checkbox itself; reading `state` is a normal Obj-C message.
fn checkbox_state(sender: Option<&AnyObject>) -> bool {
    match sender {
        Some(s) => {
            let v: isize = unsafe { msg_send![s, state] };
            v == NSControlStateValueOn
        }
        None => false,
    }
}

impl PrefsHandler {
    fn new(mtm: MainThreadMarker, actions: Arc<dyn PrefsActions>) -> Retained<Self> {
        let ivars = PrefsHandlerIvars::default();
        let _ = ivars.actions.set(actions);
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's `init` is safe to call.
        unsafe { msg_send![super(this), init] }
    }

    fn install(&self, controls: Arc<PrefsControls>, window: Retained<NSWindow>) {
        let _ = self.ivars().controls.set(controls);
        let _ = self.ivars().window.set(window);
    }
}

/// The preferences window itself. Constructed once on first `show()`; kept
/// alive thereafter. All AppKit refs are held by `Retained` (strong refs).
pub struct PreferencesWindow {
    window: Retained<NSWindow>,
    controls: Arc<PrefsControls>,
    // Anchor the handler: NSControl `target` is a weak reference, so dropping
    // the handler would invalidate every control's action. Stored (never
    // read directly) for lifetime alone.
    _handler: Retained<PrefsHandler>,
}

// SAFETY: The retained AppKit objects are only touched on the main thread.
// Callers obtain a `MainThreadMarker` before invoking `show`. The `Arc` and
// the inner `PrefsControls` are `Send + Sync` by manual unsafe impl on the
// latter (same reasoning).
unsafe impl Send for PreferencesWindow {}
// SAFETY: see Send impl above.
unsafe impl Sync for PreferencesWindow {}

impl PreferencesWindow {
    /// Build the window, lay out all controls, and wire them to `actions`.
    /// The window starts hidden; call `show()` to display it.
    pub fn new(mtm: MainThreadMarker, actions: Arc<dyn PrefsActions>) -> Self {
        // Frame is in screen coordinates at construction time; `center()`
        // is called on each show so the origin here is irrelevant.
        //
        // Height grew from 360 to 480 to fit the schedule section below the
        // battery row. AppKit y is bottom-up, so adding 120 pixels means the
        // existing controls move UP by 120 from their previous y-coordinates.
        let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(480.0, 480.0));
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;

        // SAFETY: the parameters are valid; we are on the main thread.
        let window: Retained<NSWindow> = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                content_rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(ns_string!("Open-Lid Preferences"));
        // SAFETY: by default a closed NSWindow is released by AppKit, which
        // would dangle our `Retained<NSWindow>`. We hold the strong ref and
        // expect to reuse the window on subsequent `show()` calls.
        unsafe { window.setReleasedWhenClosed(false) };

        let handler = PrefsHandler::new(mtm, actions);
        let handler_obj: &AnyObject = handler.as_ref();

        // Build controls. Coordinates are in the content view's coordinate
        // system (origin = bottom-left, y grows upward).
        //
        // Layout sketch (480 wide, 480 tall content):
        //   y=390: header text (multi-line, 70 tall)
        //   y=360: "Start Open-Lid at login"
        //   y=330: "Activate Open-Lid at launch"
        //   y=300: "Keep display awake while preventing sleep"
        //   y=250: "Default duration:"  [popup]
        //   y=210: "Turn off when battery is below" [field] %
        //   y=160: "Active only during scheduled hours" (master checkbox)
        //   y=125: "From" [HH:MM] "To" [HH:MM]
        //   y= 90: [Mo][Tu][We][Th][Fr][Sa][Su]
        //   y= 16: [Close] (right-aligned)
        let content_view = window
            .contentView()
            .expect("NSWindow always has a content view");

        // Header label — two-paragraph wrapping text.
        let header_text = ns_string!(
            "Open-Lid is now running. You can find its icon in your menu bar.\n\nRight-click (or \u{2325}-click) the menu bar icon to show the Open-Lid menu."
        );
        let header = NSTextField::wrappingLabelWithString(header_text, mtm);
        header.setFrame(NSRect::new(
            NSPoint::new(20.0, 390.0),
            NSSize::new(440.0, 70.0),
        ));
        content_view.addSubview(&header);

        // Checkbox: Start at login.
        // SAFETY: the convenience constructor calls Cocoa internals that are
        // documented to require main-thread execution; mtm proves we are.
        let start_at_login = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Start Open-Lid at login"),
                Some(handler_obj),
                Some(sel!(setStartAtLogin:)),
                mtm,
            )
        };
        start_at_login.setFrame(NSRect::new(
            NSPoint::new(20.0, 360.0),
            NSSize::new(440.0, 20.0),
        ));
        content_view.addSubview(&start_at_login);

        // Checkbox: Activate at launch.
        let activate_at_launch = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Activate Open-Lid at launch"),
                Some(handler_obj),
                Some(sel!(setActivateAtLaunch:)),
                mtm,
            )
        };
        activate_at_launch.setFrame(NSRect::new(
            NSPoint::new(20.0, 330.0),
            NSSize::new(440.0, 20.0),
        ));
        content_view.addSubview(&activate_at_launch);

        // Checkbox: Keep display awake while preventing sleep.
        // Default is on; users who actually want their screen to lock on
        // idle (e.g., for shoulder-surfing reasons) can turn it off.
        let prevent_display_sleep = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Keep display awake while preventing sleep"),
                Some(handler_obj),
                Some(sel!(setPreventDisplaySleep:)),
                mtm,
            )
        };
        prevent_display_sleep.setFrame(NSRect::new(
            NSPoint::new(20.0, 300.0),
            NSSize::new(440.0, 20.0),
        ));
        content_view.addSubview(&prevent_display_sleep);

        // Label + popup: Default duration.
        let duration_label = NSTextField::labelWithString(ns_string!("Default duration:"), mtm);
        duration_label.setFrame(NSRect::new(
            NSPoint::new(20.0, 254.0),
            NSSize::new(130.0, 20.0),
        ));
        content_view.addSubview(&duration_label);

        let popup_frame = NSRect::new(NSPoint::new(150.0, 250.0), NSSize::new(180.0, 26.0));
        let duration_popup =
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), popup_frame, false);
        for (label, minutes) in DURATION_ENTRIES {
            let ns_label = NSString::from_str(label);
            duration_popup.addItemWithTitle(&ns_label);
            if let Some(item) = duration_popup.itemAtIndex(duration_popup.numberOfItems() - 1) {
                item.setTag(*minutes);
            }
        }
        unsafe {
            duration_popup.setTarget(Some(handler_obj));
            duration_popup.setAction(Some(sel!(setDefaultDuration:)));
        }
        content_view.addSubview(&duration_popup);

        // Checkbox + field + label: Battery threshold.
        let battery_checkbox = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Turn off when battery is below"),
                Some(handler_obj),
                Some(sel!(setBatteryThreshold:)),
                mtm,
            )
        };
        battery_checkbox.setFrame(NSRect::new(
            NSPoint::new(20.0, 210.0),
            NSSize::new(260.0, 20.0),
        ));
        content_view.addSubview(&battery_checkbox);

        let battery_field_frame = NSRect::new(NSPoint::new(290.0, 206.0), NSSize::new(50.0, 24.0));
        let battery_field =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), battery_field_frame);
        battery_field.setBezeled(true);
        battery_field.setEditable(true);
        battery_field.setSelectable(true);
        battery_field.setIntegerValue(DEFAULT_BATTERY_PCT as isize);
        unsafe {
            battery_field.setTarget(Some(handler_obj));
            battery_field.setAction(Some(sel!(setBatteryThreshold:)));
        }
        content_view.addSubview(&battery_field);

        let percent_label = NSTextField::labelWithString(ns_string!("%"), mtm);
        percent_label.setFrame(NSRect::new(
            NSPoint::new(346.0, 210.0),
            NSSize::new(20.0, 20.0),
        ));
        content_view.addSubview(&percent_label);

        // Schedule section. The master checkbox toggles whether a schedule
        // is active; the sub-controls below are visually disabled when off.
        let schedule_master = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Active only during scheduled hours"),
                Some(handler_obj),
                Some(sel!(setScheduleMaster:)),
                mtm,
            )
        };
        schedule_master.setFrame(NSRect::new(
            NSPoint::new(20.0, 160.0),
            NSSize::new(440.0, 20.0),
        ));
        content_view.addSubview(&schedule_master);

        // "From" label + HH:MM text field.
        let from_label = NSTextField::labelWithString(ns_string!("From:"), mtm);
        from_label.setFrame(NSRect::new(
            NSPoint::new(40.0, 129.0),
            NSSize::new(40.0, 20.0),
        ));
        content_view.addSubview(&from_label);

        let schedule_from_frame = NSRect::new(NSPoint::new(80.0, 125.0), NSSize::new(70.0, 24.0));
        let schedule_from =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), schedule_from_frame);
        schedule_from.setBezeled(true);
        schedule_from.setEditable(true);
        schedule_from.setSelectable(true);
        schedule_from.setStringValue(ns_string!("09:00"));
        unsafe {
            schedule_from.setTarget(Some(handler_obj));
            schedule_from.setAction(Some(sel!(setScheduleField:)));
        }
        content_view.addSubview(&schedule_from);

        let to_label = NSTextField::labelWithString(ns_string!("To:"), mtm);
        to_label.setFrame(NSRect::new(
            NSPoint::new(170.0, 129.0),
            NSSize::new(30.0, 20.0),
        ));
        content_view.addSubview(&to_label);

        let schedule_to_frame = NSRect::new(NSPoint::new(200.0, 125.0), NSSize::new(70.0, 24.0));
        let schedule_to = NSTextField::initWithFrame(NSTextField::alloc(mtm), schedule_to_frame);
        schedule_to.setBezeled(true);
        schedule_to.setEditable(true);
        schedule_to.setSelectable(true);
        schedule_to.setStringValue(ns_string!("17:00"));
        unsafe {
            schedule_to.setTarget(Some(handler_obj));
            schedule_to.setAction(Some(sel!(setScheduleField:)));
        }
        content_view.addSubview(&schedule_to);

        // Seven day-of-week checkboxes laid out across the row.
        let day_titles = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        // 7 buttons, 60px each starting at x=20 -> end at x=440. Plenty of margin.
        let day_buttons = day_titles.map(|title| {
            let btn = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &NSString::from_str(title),
                    Some(handler_obj),
                    Some(sel!(setScheduleField:)),
                    mtm,
                )
            };
            // Default to checked so a freshly-ticked master maps to "every day".
            btn.setState(NSControlStateValueOn);
            btn
        });
        for (i, btn) in day_buttons.iter().enumerate() {
            btn.setFrame(NSRect::new(
                NSPoint::new(20.0 + (i as f64) * 60.0, 90.0),
                NSSize::new(58.0, 20.0),
            ));
            content_view.addSubview(btn);
        }

        // Close button (bottom-right).
        let close_button = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Close"),
                Some(handler_obj),
                Some(sel!(closeWindow:)),
                mtm,
            )
        };
        close_button.setFrame(NSRect::new(
            NSPoint::new(380.0, 16.0),
            NSSize::new(84.0, 32.0),
        ));
        content_view.addSubview(&close_button);

        // Stash controls for the handler to reach (it needs the battery field
        // to toggle its enabled state when the checkbox changes).
        let controls = Arc::new(PrefsControls {
            start_at_login,
            activate_at_launch,
            prevent_display_sleep,
            duration_popup,
            battery_checkbox,
            battery_field,
            schedule_master,
            schedule_from,
            schedule_to,
            schedule_days: day_buttons,
        });
        handler.install(controls.clone(), window.clone());

        Self {
            window,
            controls,
            _handler: handler,
        }
    }

    /// Open the window (or bring it to the front if already visible). Reads
    /// the current snapshot to refresh all control values first.
    pub fn show(&self, snapshot: &Snapshot, mtm: MainThreadMarker) {
        // Refresh values before display so the first paint shows the latest.
        apply_snapshot(&self.controls, snapshot);
        if !self.window.isVisible() {
            self.window.center();
        }
        self.window.makeKeyAndOrderFront(None);

        // Make sure we come to the foreground; in accessory mode the menubar
        // app would otherwise stay backgrounded.
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.activate();
    }
}

/// Push snapshot values into the live controls. Called on every `show()`.
fn apply_snapshot(controls: &PrefsControls, snap: &Snapshot) {
    fn flag(b: bool) -> isize {
        if b {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        }
    }

    controls.start_at_login.setState(flag(snap.start_at_login));
    controls
        .activate_at_launch
        .setState(flag(snap.activate_at_launch));
    controls
        .prevent_display_sleep
        .setState(flag(snap.prevent_display_sleep));

    // Default duration: select the matching tag, falling back to 0 = "Indefinitely".
    let target_tag: isize = snap
        .default_duration_minutes
        .map(|m| m as isize)
        .unwrap_or(0);
    let selected = controls.duration_popup.selectItemWithTag(target_tag);
    if !selected {
        // No exact match — fall back to "Indefinitely" (tag 0).
        controls.duration_popup.selectItemWithTag(0);
    }

    // Battery threshold:
    //   None     → checkbox off, field shows default (e.g. 20), field disabled.
    //   Some(p)  → checkbox on,  field shows p,                 field enabled.
    match snap.battery_threshold_pct {
        Some(p) => {
            controls.battery_checkbox.setState(NSControlStateValueOn);
            controls.battery_field.setIntegerValue(p as isize);
            controls.battery_field.setEnabled(true);
        }
        None => {
            controls.battery_checkbox.setState(NSControlStateValueOff);
            controls
                .battery_field
                .setIntegerValue(DEFAULT_BATTERY_PCT as isize);
            controls.battery_field.setEnabled(false);
        }
    }

    // Schedule:
    //   None     → master off, sub-controls disabled, fields show 09:00-17:00
    //              + all-days defaults so flipping the master on yields a
    //              sensible starter schedule.
    //   Some(s)  → master on, fields show s.start/s.end, day buttons reflect
    //              s.days, sub-controls enabled.
    match snap.modifiers.schedule.as_ref() {
        Some(s) => {
            controls.schedule_master.setState(NSControlStateValueOn);
            controls
                .schedule_from
                .setStringValue(&NSString::from_str(&s.start.format("%H:%M").to_string()));
            controls
                .schedule_to
                .setStringValue(&NSString::from_str(&s.end.format("%H:%M").to_string()));
            const DAY_FLAGS: [DaysOfWeek; 7] = [
                DaysOfWeek::MON,
                DaysOfWeek::TUE,
                DaysOfWeek::WED,
                DaysOfWeek::THU,
                DaysOfWeek::FRI,
                DaysOfWeek::SAT,
                DaysOfWeek::SUN,
            ];
            for (i, btn) in controls.schedule_days.iter().enumerate() {
                btn.setState(flag(s.days.contains(DAY_FLAGS[i])));
            }
            schedule_set_subcontrols_enabled(controls, true);
        }
        None => {
            controls.schedule_master.setState(NSControlStateValueOff);
            controls.schedule_from.setStringValue(ns_string!("09:00"));
            controls.schedule_to.setStringValue(ns_string!("17:00"));
            for btn in &controls.schedule_days {
                btn.setState(NSControlStateValueOn);
            }
            schedule_set_subcontrols_enabled(controls, false);
        }
    }
}
