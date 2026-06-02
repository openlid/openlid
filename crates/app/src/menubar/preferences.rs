//! Native macOS Preferences window.
//!
//! A single `NSWindow` with sidebar panels for general settings, safeguards,
//! and recurring schedules. Each control routes through one selector on
//! `PrefsHandler` which calls into a `PrefsActions` trait object. The outer
//! `RuntimeActions` impl translates each call into a single-field `PrefsPatch`
//! and dispatches it through `StateRuntime::set_preferences`.
//!
//! Threading: all construction and all callbacks happen on the main thread —
//! the menu click that opens the window, the AppKit action invocations after
//! that. `PrefsActions` callbacks are therefore free to do main-thread work
//! (like `RuntimeActions::refresh`). The shared `PrefsActions` is `Send + Sync`
//! so the ivar can hold it without contortions.
//!
//! Window lifecycle: the window object is constructed lazily on first
//! `show()`, then kept alive for the life of the app. Closing the window
//! (red button) just hides it; the next `show()` brings it back. Subsequent
//! shows refresh the controls from the latest snapshot.

use chrono::NaiveTime;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn, NSMenu,
    NSPopUpButton, NSSegmentSwitchTracking, NSSegmentedControl, NSStepper, NSTextField, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
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
    fn set_battery_threshold(&self, pct: Option<u8>);
    fn set_prevent_display_sleep(&self, enabled: bool);
    /// Apply a schedule update. `None` clears any existing schedule;
    /// `Some(s)` sets it. Implementations should also turn the toggle on
    /// when transitioning from no-schedule to schedule, so the new gate
    /// has an enabled state to constrain.
    fn set_schedule(&self, schedule: Option<Schedule>);
    /// Apply an in-transit auto-disable change. `None` disables the
    /// feature; `Some(n)` sets the timeout in minutes.
    fn set_in_transit_timeout(&self, minutes: Option<u32>);
}

/// Default battery-threshold value shown in the disabled numeric field when
/// the threshold preference is `None`.
const DEFAULT_BATTERY_PCT: u8 = 20;

/// Default in-transit timeout shown in the disabled numeric field when
/// the feature is off, and chosen automatically when the user first
/// ticks the checkbox. 2 minutes is the value suggested during
/// brainstorming -- short enough to catch a real backpack scenario,
/// long enough to ride out elevator-Wi-Fi blips.
const DEFAULT_IN_TRANSIT_MINUTES: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeFormat {
    TwentyFour,
    AmPm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimeParts {
    /// Selected hour index in the active popup:
    /// * 24-hour mode: 0..=23 maps directly to the hour.
    /// * AM/PM mode: 0..=11 maps to labels 1..=12.
    hour_index: isize,
    minute_index: isize,
    /// `None` in 24-hour mode; `Some(0)` for AM and `Some(1)` for PM.
    period_index: Option<isize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrefsPanel {
    General,
    Safeguards,
    Schedule,
}

fn time_parts_for(time: NaiveTime, format: TimeFormat) -> TimeParts {
    use chrono::Timelike;

    let hour = time.hour() as isize;
    let minute = time.minute() as isize;
    match format {
        TimeFormat::TwentyFour => TimeParts {
            hour_index: hour,
            minute_index: minute,
            period_index: None,
        },
        TimeFormat::AmPm => {
            let hour12 = hour % 12;
            TimeParts {
                // Popup labels are 1..=12, so label 12 lives at index 11.
                hour_index: if hour12 == 0 { 11 } else { hour12 - 1 },
                minute_index: minute,
                period_index: Some(if hour >= 12 { 1 } else { 0 }),
            }
        }
    }
}

fn time_from_parts(
    format: TimeFormat,
    hour_index: isize,
    minute_index: isize,
    period_index: Option<isize>,
) -> Option<NaiveTime> {
    if !(0..=59).contains(&minute_index) {
        return None;
    }
    let hour = match format {
        TimeFormat::TwentyFour => {
            if !(0..=23).contains(&hour_index) {
                return None;
            }
            hour_index
        }
        TimeFormat::AmPm => {
            if !(0..=11).contains(&hour_index) {
                return None;
            }
            let period = period_index?;
            if !(0..=1).contains(&period) {
                return None;
            }
            let hour12 = hour_index + 1;
            let base = if hour12 == 12 { 0 } else { hour12 };
            if period == 1 {
                base + 12
            } else {
                base
            }
        }
    };
    NaiveTime::from_hms_opt(hour as u32, minute_index as u32, 0)
}

/// AppKit control handles shared between the Rust wrapper and the Obj-C
/// handler. The handler uses these references to keep related controls in
/// sync when a checkbox, stepper, popup, or sidebar tab fires.
///
/// # Safety
///
/// `Send + Sync` are asserted manually. Every touch of these fields happens
/// on the main thread — either from `PreferencesWindow::show` (called from a
/// main-thread menu click) or from the AppKit-invoked selectors on
/// `PrefsHandler` (which is `MainThreadOnly`).
struct PrefsControls {
    general_tab: Retained<NSButton>,
    safeguards_tab: Retained<NSButton>,
    schedule_tab: Retained<NSButton>,
    general_panel: Retained<NSView>,
    safeguards_panel: Retained<NSView>,
    schedule_panel: Retained<NSView>,
    start_at_login: Retained<NSButton>,
    activate_at_launch: Retained<NSButton>,
    prevent_display_sleep: Retained<NSButton>,
    battery_checkbox: Retained<NSButton>,
    battery_field: Retained<NSTextField>,
    battery_stepper: Retained<NSStepper>,
    schedule_master: Retained<NSButton>,
    schedule_time_format: Retained<NSSegmentedControl>,
    schedule_from_hour: Retained<NSPopUpButton>,
    schedule_from_minute: Retained<NSPopUpButton>,
    schedule_from_period: Retained<NSPopUpButton>,
    schedule_to_hour: Retained<NSPopUpButton>,
    schedule_to_minute: Retained<NSPopUpButton>,
    schedule_to_period: Retained<NSPopUpButton>,
    /// Day-of-week checkboxes in Mon..Sun order.
    schedule_days: [Retained<NSButton>; 7],
    in_transit_checkbox: Retained<NSButton>,
    in_transit_field: Retained<NSTextField>,
    in_transit_stepper: Retained<NSStepper>,
}

// SAFETY: see PrefsControls doc.
unsafe impl Send for PrefsControls {}
// SAFETY: see PrefsControls doc.
unsafe impl Sync for PrefsControls {}

#[derive(Default)]
pub struct PrefsHandlerIvars {
    actions: OnceCell<Arc<dyn PrefsActions>>,
    controls: OnceCell<Arc<PrefsControls>>,
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
        #[unsafe(method(showGeneralPanel:))]
        fn show_general_panel(&self, _sender: Option<&AnyObject>) {
            if let Some(controls) = self.ivars().controls.get() {
                show_panel(controls, PrefsPanel::General);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(showSafeguardsPanel:))]
        fn show_safeguards_panel(&self, _sender: Option<&AnyObject>) {
            if let Some(controls) = self.ivars().controls.get() {
                show_panel(controls, PrefsPanel::Safeguards);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(showSchedulePanel:))]
        fn show_schedule_panel(&self, _sender: Option<&AnyObject>) {
            if let Some(controls) = self.ivars().controls.get() {
                show_panel(controls, PrefsPanel::Schedule);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setBatteryThreshold:))]
        fn set_battery_threshold(&self, sender: Option<&AnyObject>) {
            // Two trigger paths: the checkbox toggled, or the stepper changed.
            // Either way we read the controls and derive the new threshold.
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let checkbox_on = controls.battery_checkbox.state() == NSControlStateValueOn;
            // Always keep the read-only value field's enabled state in sync
            // with the checkbox.
            controls.battery_field.setEnabled(checkbox_on);
            controls.battery_stepper.setEnabled(checkbox_on);

            let from_stepper = sender.is_some_and(|s| {
                let stepper: &AnyObject = controls.battery_stepper.as_ref();
                std::ptr::eq(s, stepper)
            });
            let raw = if from_stepper {
                let v = controls.battery_stepper.intValue() as isize;
                controls.battery_field.setIntegerValue(v);
                v
            } else {
                let v = controls.battery_field.integerValue();
                controls.battery_stepper.setIntegerValue(v);
                v
            };

            let pct = if checkbox_on {
                // Clamp to a sane range. We accept 1–100; 0 disables the
                // feature semantically but the checkbox is the on/off — out-
                // of-range values get clamped to the default.
                let clamped = if (1..=100).contains(&raw) {
                    raw as u8
                } else {
                    DEFAULT_BATTERY_PCT
                };
                controls.battery_field.setIntegerValue(clamped as isize);
                controls.battery_stepper.setIntegerValue(clamped as isize);
                Some(clamped)
            } else {
                None
            };

            if let Some(actions) = self.ivars().actions.get() {
                actions.set_battery_threshold(pct);
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setInTransitTimeout:))]
        fn set_in_transit_timeout(&self, sender: Option<&AnyObject>) {
            // Same dual-trigger pattern as the battery threshold:
            // the checkbox or the stepper fires this. We always
            // read the controls and derive the result.
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let checkbox_on = controls.in_transit_checkbox.state() == NSControlStateValueOn;
            controls.in_transit_field.setEnabled(checkbox_on);
            controls.in_transit_stepper.setEnabled(checkbox_on);
            let from_stepper = sender.is_some_and(|s| {
                let stepper: &AnyObject = controls.in_transit_stepper.as_ref();
                std::ptr::eq(s, stepper)
            });
            let raw = if from_stepper {
                let v = controls.in_transit_stepper.intValue() as isize;
                controls.in_transit_field.setIntegerValue(v);
                v
            } else {
                let v = controls.in_transit_field.integerValue();
                controls.in_transit_stepper.setIntegerValue(v);
                v
            };
            let minutes = if checkbox_on {
                // 1..=120 minutes. Out-of-range values fall back to the
                // sensible default so a user typo can't silently disable
                // or stretch the safeguard. 120 minutes is a generous
                // upper bound: longer than that and the feature is
                // effectively never-fires.
                let clamped = if (1..=120).contains(&raw) {
                    raw as u32
                } else {
                    DEFAULT_IN_TRANSIT_MINUTES
                };
                controls.in_transit_field.setIntegerValue(clamped as isize);
                controls
                    .in_transit_stepper
                    .setIntegerValue(clamped as isize);
                Some(clamped)
            } else {
                None
            };
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_in_transit_timeout(minutes);
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
        #[unsafe(method(setScheduleTimeFormat:))]
        fn set_schedule_time_format(&self, _sender: Option<&AnyObject>) {
            let Some(controls) = self.ivars().controls.get() else {
                return;
            };
            let current = build_schedule_from_controls_with_format(controls, popup_time_format(controls));
            configure_time_popups(controls, selected_time_format(controls), current.start, current.end);
            if controls.schedule_master.state() != NSControlStateValueOn {
                return;
            }
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_schedule(Some(current));
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
    build_schedule_from_controls_with_format(controls, popup_time_format(controls))
}

fn build_schedule_from_controls_with_format(
    controls: &PrefsControls,
    popup_format: TimeFormat,
) -> Schedule {
    let start = time_from_popups(
        popup_format,
        &controls.schedule_from_hour,
        &controls.schedule_from_minute,
        &controls.schedule_from_period,
    )
    .unwrap_or_else(|| {
        NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_START.0, DEFAULT_SCHEDULE_START.1, 0).unwrap()
    });
    let end = time_from_popups(
        popup_format,
        &controls.schedule_to_hour,
        &controls.schedule_to_minute,
        &controls.schedule_to_period,
    )
    .unwrap_or_else(|| {
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

fn selected_time_format(controls: &PrefsControls) -> TimeFormat {
    match controls.schedule_time_format.selectedSegment() {
        1 => TimeFormat::AmPm,
        _ => TimeFormat::TwentyFour,
    }
}

fn popup_time_format(controls: &PrefsControls) -> TimeFormat {
    if controls.schedule_from_hour.numberOfItems() == 12 {
        TimeFormat::AmPm
    } else {
        TimeFormat::TwentyFour
    }
}

fn time_from_popups(
    format: TimeFormat,
    hour: &NSPopUpButton,
    minute: &NSPopUpButton,
    period: &NSPopUpButton,
) -> Option<NaiveTime> {
    time_from_parts(
        format,
        hour.indexOfSelectedItem(),
        minute.indexOfSelectedItem(),
        match format {
            TimeFormat::TwentyFour => None,
            TimeFormat::AmPm => Some(period.indexOfSelectedItem()),
        },
    )
}

fn configure_time_popups(
    controls: &PrefsControls,
    format: TimeFormat,
    start: NaiveTime,
    end: NaiveTime,
) {
    controls
        .schedule_time_format
        .setSelectedSegment(match format {
            TimeFormat::TwentyFour => 0,
            TimeFormat::AmPm => 1,
        });

    populate_hour_popup(&controls.schedule_from_hour, format);
    populate_hour_popup(&controls.schedule_to_hour, format);
    populate_minute_popup(&controls.schedule_from_minute);
    populate_minute_popup(&controls.schedule_to_minute);
    populate_period_popup(&controls.schedule_from_period);
    populate_period_popup(&controls.schedule_to_period);

    select_time_popups(
        &controls.schedule_from_hour,
        &controls.schedule_from_minute,
        &controls.schedule_from_period,
        start,
        format,
    );
    select_time_popups(
        &controls.schedule_to_hour,
        &controls.schedule_to_minute,
        &controls.schedule_to_period,
        end,
        format,
    );

    let show_period = format == TimeFormat::AmPm;
    controls.schedule_from_period.setHidden(!show_period);
    controls.schedule_to_period.setHidden(!show_period);
}

fn populate_hour_popup(popup: &NSPopUpButton, format: TimeFormat) {
    popup.removeAllItems();
    match format {
        TimeFormat::TwentyFour => {
            for h in 0..24 {
                popup.addItemWithTitle(&NSString::from_str(&format!("{h:02}")));
            }
        }
        TimeFormat::AmPm => {
            for h in 1..=12 {
                popup.addItemWithTitle(&NSString::from_str(&h.to_string()));
            }
        }
    }
}

fn populate_minute_popup(popup: &NSPopUpButton) {
    popup.removeAllItems();
    for m in 0..60 {
        popup.addItemWithTitle(&NSString::from_str(&format!("{m:02}")));
    }
}

fn populate_period_popup(popup: &NSPopUpButton) {
    popup.removeAllItems();
    popup.addItemWithTitle(ns_string!("AM"));
    popup.addItemWithTitle(ns_string!("PM"));
}

fn select_time_popups(
    hour: &NSPopUpButton,
    minute: &NSPopUpButton,
    period: &NSPopUpButton,
    time: NaiveTime,
    format: TimeFormat,
) {
    let parts = time_parts_for(time, format);
    hour.selectItemAtIndex(parts.hour_index);
    minute.selectItemAtIndex(parts.minute_index);
    if let Some(period_index) = parts.period_index {
        period.selectItemAtIndex(period_index);
    }
}

fn schedule_set_subcontrols_enabled(controls: &PrefsControls, on: bool) {
    controls.schedule_time_format.setEnabled(on);
    controls.schedule_from_hour.setEnabled(on);
    controls.schedule_from_minute.setEnabled(on);
    controls.schedule_from_period.setEnabled(on);
    controls.schedule_to_hour.setEnabled(on);
    controls.schedule_to_minute.setEnabled(on);
    controls.schedule_to_period.setEnabled(on);
    for btn in &controls.schedule_days {
        btn.setEnabled(on);
    }
}

fn show_panel(controls: &PrefsControls, panel: PrefsPanel) {
    controls
        .general_panel
        .setHidden(panel != PrefsPanel::General);
    controls
        .safeguards_panel
        .setHidden(panel != PrefsPanel::Safeguards);
    controls
        .schedule_panel
        .setHidden(panel != PrefsPanel::Schedule);

    controls
        .general_tab
        .setState(if panel == PrefsPanel::General {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    controls
        .safeguards_tab
        .setState(if panel == PrefsPanel::Safeguards {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    controls
        .schedule_tab
        .setState(if panel == PrefsPanel::Schedule {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
}

fn label(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    field
}

fn wrapping_label(text: &str, frame: NSRect, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    field
}

fn panel(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSView> {
    NSView::initWithFrame(NSView::alloc(mtm), frame)
}

fn popup(
    frame: NSRect,
    handler_obj: &AnyObject,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
    let button = unsafe {
        NSPopUpButton::popUpButtonWithMenu_target_action(&menu, Some(handler_obj), Some(action))
    };
    button.setFrame(frame);
    button
}

fn stepper(
    frame: NSRect,
    min: f64,
    max: f64,
    value: isize,
    handler_obj: &AnyObject,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSStepper> {
    let stepper = NSStepper::initWithFrame(NSStepper::alloc(mtm), frame);
    stepper.setMinValue(min);
    stepper.setMaxValue(max);
    stepper.setIncrement(1.0);
    stepper.setIntegerValue(value);
    stepper.setContinuous(false);
    unsafe {
        stepper.setTarget(Some(handler_obj));
        stepper.setAction(Some(action));
    }
    stepper
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

    fn install(&self, controls: Arc<PrefsControls>) {
        let _ = self.ivars().controls.set(controls);
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
        // Slightly wider than the previous single-column window so the
        // sidebar can carry navigation without crowding each settings panel.
        let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(680.0, 560.0));
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
        window.setTitle(ns_string!("OpenLid Preferences"));
        // SAFETY: by default a closed NSWindow is released by AppKit, which
        // would dangle our `Retained<NSWindow>`. We hold the strong ref and
        // expect to reuse the window on subsequent `show()` calls.
        unsafe { window.setReleasedWhenClosed(false) };

        let handler = PrefsHandler::new(mtm, actions);
        let handler_obj: &AnyObject = handler.as_ref();

        let content_view = window
            .contentView()
            .expect("NSWindow always has a content view");

        // Sidebar navigation.
        let sidebar = panel(
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(142.0, 560.0)),
            mtm,
        );
        content_view.addSubview(&sidebar);

        let general_tab = unsafe {
            NSButton::radioButtonWithTitle_target_action(
                ns_string!("General"),
                Some(handler_obj),
                Some(sel!(showGeneralPanel:)),
                mtm,
            )
        };
        general_tab.setFrame(NSRect::new(
            NSPoint::new(16.0, 496.0),
            NSSize::new(118.0, 24.0),
        ));
        general_tab.setState(NSControlStateValueOn);
        sidebar.addSubview(&general_tab);

        let safeguards_tab = unsafe {
            NSButton::radioButtonWithTitle_target_action(
                ns_string!("Safeguards"),
                Some(handler_obj),
                Some(sel!(showSafeguardsPanel:)),
                mtm,
            )
        };
        safeguards_tab.setFrame(NSRect::new(
            NSPoint::new(16.0, 464.0),
            NSSize::new(118.0, 24.0),
        ));
        sidebar.addSubview(&safeguards_tab);

        let schedule_tab = unsafe {
            NSButton::radioButtonWithTitle_target_action(
                ns_string!("Schedule"),
                Some(handler_obj),
                Some(sel!(showSchedulePanel:)),
                mtm,
            )
        };
        schedule_tab.setFrame(NSRect::new(
            NSPoint::new(16.0, 432.0),
            NSSize::new(118.0, 24.0),
        ));
        sidebar.addSubview(&schedule_tab);

        let panel_frame = NSRect::new(NSPoint::new(142.0, 0.0), NSSize::new(538.0, 560.0));
        let general_panel = panel(panel_frame, mtm);
        let safeguards_panel = panel(panel_frame, mtm);
        let schedule_panel = panel(panel_frame, mtm);
        safeguards_panel.setHidden(true);
        schedule_panel.setHidden(true);
        content_view.addSubview(&general_panel);
        content_view.addSubview(&safeguards_panel);
        content_view.addSubview(&schedule_panel);

        let general_title = label(
            "General",
            NSRect::new(NSPoint::new(28.0, 504.0), NSSize::new(480.0, 24.0)),
            mtm,
        );
        general_panel.addSubview(&general_title);
        let general_copy = wrapping_label(
            "OpenLid is running. Use these settings to control startup behavior and display sleep.",
            NSRect::new(NSPoint::new(28.0, 466.0), NSSize::new(480.0, 34.0)),
            mtm,
        );
        general_panel.addSubview(&general_copy);

        // Checkbox: Start at login.
        // SAFETY: the convenience constructor calls Cocoa internals that are
        // documented to require main-thread execution; mtm proves we are.
        let start_at_login = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Start OpenLid at login"),
                Some(handler_obj),
                Some(sel!(setStartAtLogin:)),
                mtm,
            )
        };
        start_at_login.setFrame(NSRect::new(
            NSPoint::new(28.0, 410.0),
            NSSize::new(460.0, 24.0),
        ));
        general_panel.addSubview(&start_at_login);
        let start_help = wrapping_label(
            "Open automatically when you sign in.",
            NSRect::new(NSPoint::new(50.0, 386.0), NSSize::new(430.0, 22.0)),
            mtm,
        );
        general_panel.addSubview(&start_help);

        // Checkbox: Activate at launch.
        let activate_at_launch = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Activate OpenLid at launch"),
                Some(handler_obj),
                Some(sel!(setActivateAtLaunch:)),
                mtm,
            )
        };
        activate_at_launch.setFrame(NSRect::new(
            NSPoint::new(28.0, 338.0),
            NSSize::new(460.0, 24.0),
        ));
        general_panel.addSubview(&activate_at_launch);
        let activate_help = wrapping_label(
            "Turn sleep prevention on whenever OpenLid starts. Turn this off to restore the last on/off state.",
            NSRect::new(NSPoint::new(50.0, 304.0), NSSize::new(430.0, 34.0)),
            mtm,
        );
        general_panel.addSubview(&activate_help);

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
            NSPoint::new(28.0, 246.0),
            NSSize::new(460.0, 24.0),
        ));
        general_panel.addSubview(&prevent_display_sleep);
        let display_help = wrapping_label(
            "Avoid idle dimming and screen lock while OpenLid is actively preventing sleep.",
            NSRect::new(NSPoint::new(50.0, 214.0), NSSize::new(430.0, 34.0)),
            mtm,
        );
        general_panel.addSubview(&display_help);

        let safeguards_title = label(
            "Safeguards",
            NSRect::new(NSPoint::new(28.0, 504.0), NSSize::new(480.0, 24.0)),
            mtm,
        );
        safeguards_panel.addSubview(&safeguards_title);
        let safeguards_copy = wrapping_label(
            "Automatic turn-off rules prevent accidental drain or heat when OpenLid should stand down.",
            NSRect::new(NSPoint::new(28.0, 466.0), NSSize::new(480.0, 34.0)),
            mtm,
        );
        safeguards_panel.addSubview(&safeguards_copy);

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
            NSPoint::new(28.0, 402.0),
            NSSize::new(300.0, 24.0),
        ));
        safeguards_panel.addSubview(&battery_checkbox);
        let battery_help = wrapping_label(
            "Disarm automatically before battery drain becomes risky.",
            NSRect::new(NSPoint::new(50.0, 376.0), NSSize::new(300.0, 24.0)),
            mtm,
        );
        safeguards_panel.addSubview(&battery_help);

        let battery_field_frame = NSRect::new(NSPoint::new(350.0, 399.0), NSSize::new(48.0, 24.0));
        let battery_field =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), battery_field_frame);
        battery_field.setBezeled(true);
        battery_field.setEditable(false);
        battery_field.setSelectable(false);
        battery_field.setIntegerValue(DEFAULT_BATTERY_PCT as isize);
        unsafe {
            battery_field.setTarget(Some(handler_obj));
            battery_field.setAction(Some(sel!(setBatteryThreshold:)));
        }
        safeguards_panel.addSubview(&battery_field);

        let battery_stepper = stepper(
            NSRect::new(NSPoint::new(402.0, 397.0), NSSize::new(20.0, 28.0)),
            1.0,
            100.0,
            DEFAULT_BATTERY_PCT as isize,
            handler_obj,
            sel!(setBatteryThreshold:),
            mtm,
        );
        safeguards_panel.addSubview(&battery_stepper);

        let percent_label = NSTextField::labelWithString(ns_string!("%"), mtm);
        percent_label.setFrame(NSRect::new(
            NSPoint::new(430.0, 402.0),
            NSSize::new(20.0, 20.0),
        ));
        safeguards_panel.addSubview(&percent_label);

        // Checkbox + field + label: in-transit auto-disable.
        // Mirrors the battery row's visual pattern (checkbox-label, numeric
        // field, unit label). 1..=120 minute range, defaults to 2.
        let in_transit_checkbox = unsafe {
            NSButton::checkboxWithTitle_target_action(
                ns_string!("Auto-disable in transit"),
                Some(handler_obj),
                Some(sel!(setInTransitTimeout:)),
                mtm,
            )
        };
        in_transit_checkbox.setFrame(NSRect::new(
            NSPoint::new(28.0, 286.0),
            NSSize::new(300.0, 24.0),
        ));
        safeguards_panel.addSubview(&in_transit_checkbox);
        let transit_help = wrapping_label(
            "Disarm if the laptop is likely packed away: lid closed, on battery, no display, and no network.",
            NSRect::new(NSPoint::new(50.0, 250.0), NSSize::new(300.0, 36.0)),
            mtm,
        );
        safeguards_panel.addSubview(&transit_help);

        let in_transit_field_frame =
            NSRect::new(NSPoint::new(350.0, 283.0), NSSize::new(48.0, 24.0));
        let in_transit_field =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), in_transit_field_frame);
        in_transit_field.setBezeled(true);
        in_transit_field.setEditable(false);
        in_transit_field.setSelectable(false);
        in_transit_field.setIntegerValue(DEFAULT_IN_TRANSIT_MINUTES as isize);
        unsafe {
            in_transit_field.setTarget(Some(handler_obj));
            in_transit_field.setAction(Some(sel!(setInTransitTimeout:)));
        }
        safeguards_panel.addSubview(&in_transit_field);

        let in_transit_stepper = stepper(
            NSRect::new(NSPoint::new(402.0, 281.0), NSSize::new(20.0, 28.0)),
            1.0,
            120.0,
            DEFAULT_IN_TRANSIT_MINUTES as isize,
            handler_obj,
            sel!(setInTransitTimeout:),
            mtm,
        );
        safeguards_panel.addSubview(&in_transit_stepper);

        let in_transit_min_label = NSTextField::labelWithString(ns_string!("min"), mtm);
        in_transit_min_label.setFrame(NSRect::new(
            NSPoint::new(430.0, 286.0),
            NSSize::new(28.0, 20.0),
        ));
        safeguards_panel.addSubview(&in_transit_min_label);

        let schedule_title = label(
            "Schedule",
            NSRect::new(NSPoint::new(28.0, 504.0), NSSize::new(480.0, 24.0)),
            mtm,
        );
        schedule_panel.addSubview(&schedule_title);
        let schedule_copy = wrapping_label(
            "Constrain OpenLid to recurring active hours. Changes save automatically as you edit.",
            NSRect::new(NSPoint::new(28.0, 466.0), NSSize::new(480.0, 34.0)),
            mtm,
        );
        schedule_panel.addSubview(&schedule_copy);

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
            NSPoint::new(28.0, 414.0),
            NSSize::new(460.0, 24.0),
        ));
        schedule_panel.addSubview(&schedule_master);

        let time_format_label = label(
            "Time format",
            NSRect::new(NSPoint::new(28.0, 370.0), NSSize::new(120.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&time_format_label);
        let labels = NSArray::from_slice(&[ns_string!("24-hour"), ns_string!("AM/PM")]);
        let schedule_time_format = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(handler_obj),
                Some(sel!(setScheduleTimeFormat:)),
                mtm,
            )
        };
        schedule_time_format.setFrame(NSRect::new(
            NSPoint::new(160.0, 364.0),
            NSSize::new(190.0, 28.0),
        ));
        schedule_time_format.setSelectedSegment(0);
        schedule_panel.addSubview(&schedule_time_format);

        let from_label = label(
            "From",
            NSRect::new(NSPoint::new(28.0, 320.0), NSSize::new(60.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&from_label);
        let schedule_from_hour = popup(
            NSRect::new(NSPoint::new(160.0, 314.0), NSSize::new(72.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_from_hour);
        let from_colon = label(
            ":",
            NSRect::new(NSPoint::new(238.0, 318.0), NSSize::new(10.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&from_colon);
        let schedule_from_minute = popup(
            NSRect::new(NSPoint::new(252.0, 314.0), NSSize::new(72.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_from_minute);
        let schedule_from_period = popup(
            NSRect::new(NSPoint::new(334.0, 314.0), NSSize::new(78.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_from_period);

        let to_label = label(
            "To",
            NSRect::new(NSPoint::new(28.0, 270.0), NSSize::new(60.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&to_label);
        let schedule_to_hour = popup(
            NSRect::new(NSPoint::new(160.0, 264.0), NSSize::new(72.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_to_hour);
        let to_colon = label(
            ":",
            NSRect::new(NSPoint::new(238.0, 268.0), NSSize::new(10.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&to_colon);
        let schedule_to_minute = popup(
            NSRect::new(NSPoint::new(252.0, 264.0), NSSize::new(72.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_to_minute);
        let schedule_to_period = popup(
            NSRect::new(NSPoint::new(334.0, 264.0), NSSize::new(78.0, 28.0)),
            handler_obj,
            sel!(setScheduleField:),
            mtm,
        );
        schedule_panel.addSubview(&schedule_to_period);

        let days_label = label(
            "Days",
            NSRect::new(NSPoint::new(28.0, 214.0), NSSize::new(80.0, 20.0)),
            mtm,
        );
        schedule_panel.addSubview(&days_label);

        // Seven day-of-week checkboxes laid out across the row.
        let day_titles = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
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
                NSPoint::new(160.0 + (i as f64) * 58.0, 210.0),
                NSSize::new(56.0, 24.0),
            ));
            schedule_panel.addSubview(btn);
        }

        // Stash controls for the handler to keep related values in sync when
        // sidebar tabs, checkboxes, steppers, and popups change.
        let controls = Arc::new(PrefsControls {
            general_tab,
            safeguards_tab,
            schedule_tab,
            general_panel,
            safeguards_panel,
            schedule_panel,
            start_at_login,
            activate_at_launch,
            prevent_display_sleep,
            battery_checkbox,
            battery_field,
            battery_stepper,
            schedule_master,
            schedule_time_format,
            schedule_from_hour,
            schedule_from_minute,
            schedule_from_period,
            schedule_to_hour,
            schedule_to_minute,
            schedule_to_period,
            schedule_days: day_buttons,
            in_transit_checkbox,
            in_transit_field,
            in_transit_stepper,
        });
        handler.install(controls.clone());

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

    // Battery threshold:
    //   None     → checkbox off, field shows default (e.g. 20), field disabled.
    //   Some(p)  → checkbox on,  field shows p,                 field enabled.
    match snap.battery_threshold_pct {
        Some(p) => {
            controls.battery_checkbox.setState(NSControlStateValueOn);
            controls.battery_field.setIntegerValue(p as isize);
            controls.battery_stepper.setIntegerValue(p as isize);
            controls.battery_field.setEnabled(true);
            controls.battery_stepper.setEnabled(true);
        }
        None => {
            controls.battery_checkbox.setState(NSControlStateValueOff);
            controls
                .battery_field
                .setIntegerValue(DEFAULT_BATTERY_PCT as isize);
            controls
                .battery_stepper
                .setIntegerValue(DEFAULT_BATTERY_PCT as isize);
            controls.battery_field.setEnabled(false);
            controls.battery_stepper.setEnabled(false);
        }
    }

    // In-transit auto-disable: same shape as the battery row.
    match snap.in_transit_timeout_minutes {
        Some(min) => {
            controls.in_transit_checkbox.setState(NSControlStateValueOn);
            controls.in_transit_field.setIntegerValue(min as isize);
            controls.in_transit_stepper.setIntegerValue(min as isize);
            controls.in_transit_field.setEnabled(true);
            controls.in_transit_stepper.setEnabled(true);
        }
        None => {
            controls
                .in_transit_checkbox
                .setState(NSControlStateValueOff);
            controls
                .in_transit_field
                .setIntegerValue(DEFAULT_IN_TRANSIT_MINUTES as isize);
            controls
                .in_transit_stepper
                .setIntegerValue(DEFAULT_IN_TRANSIT_MINUTES as isize);
            controls.in_transit_field.setEnabled(false);
            controls.in_transit_stepper.setEnabled(false);
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
            configure_time_popups(controls, selected_time_format(controls), s.start, s.end);
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
            configure_time_popups(
                controls,
                selected_time_format(controls),
                NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_START.0, DEFAULT_SCHEDULE_START.1, 0)
                    .unwrap(),
                NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_END.0, DEFAULT_SCHEDULE_END.1, 0).unwrap(),
            );
            for btn in &controls.schedule_days {
                btn.setState(NSControlStateValueOn);
            }
            schedule_set_subcontrols_enabled(controls, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_parts_for_twenty_four_hour_dropdowns_uses_clock_hour() {
        let t = NaiveTime::from_hms_opt(17, 45, 0).unwrap();

        let parts = time_parts_for(t, TimeFormat::TwentyFour);

        assert_eq!(parts.hour_index, 17);
        assert_eq!(parts.minute_index, 45);
        assert_eq!(parts.period_index, None);
    }

    #[test]
    fn time_parts_for_ampm_dropdowns_maps_evening_to_pm() {
        let t = NaiveTime::from_hms_opt(17, 45, 0).unwrap();

        let parts = time_parts_for(t, TimeFormat::AmPm);

        assert_eq!(parts.hour_index, 4);
        assert_eq!(parts.minute_index, 45);
        assert_eq!(parts.period_index, Some(1));
    }

    #[test]
    fn time_from_parts_handles_ampm_midnight_and_noon_edges() {
        let midnight = time_from_parts(TimeFormat::AmPm, 11, 0, Some(0)).unwrap();
        let noon = time_from_parts(TimeFormat::AmPm, 11, 0, Some(1)).unwrap();

        assert_eq!(midnight, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert_eq!(noon, NaiveTime::from_hms_opt(12, 0, 0).unwrap());
    }

    #[test]
    fn time_from_parts_rejects_out_of_range_dropdown_indexes() {
        assert!(time_from_parts(TimeFormat::TwentyFour, 24, 0, None).is_none());
        assert!(time_from_parts(TimeFormat::TwentyFour, 0, 60, None).is_none());
        assert!(time_from_parts(TimeFormat::AmPm, 12, 0, Some(0)).is_none());
        assert!(time_from_parts(TimeFormat::AmPm, 0, 0, Some(2)).is_none());
    }
}
