//! WebView-backed macOS Preferences window.
//!
//! The visible preferences surface is bundled HTML/CSS/JS rendered in a
//! `WKWebView` so it can match the approved mockups exactly. Rust still owns
//! the persistence boundary: JavaScript sends typed JSON messages to
//! `PrefsHandler`, which translates them into calls on `PrefsActions`.

use chrono::{NaiveTime, Timelike};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use openlid_core::ipc::control::Snapshot;
use openlid_core::mode::{DaysOfWeek, Schedule};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

#[link(name = "WebKit", kind = "framework")]
extern "C" {}

const PREFERENCES_HTML: &str = include_str!("preferences_webview.html");

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

const DEFAULT_BATTERY_PCT: u8 = 20;
const DEFAULT_IN_TRANSIT_MINUTES: u32 = 2;
const DEFAULT_SCHEDULE_START: (u32, u32) = (9, 0);
const DEFAULT_SCHEDULE_END: (u32, u32) = (17, 0);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum WebPrefsMessage {
    Ready,
    StartAtLogin {
        enabled: bool,
    },
    ActivateAtLaunch {
        enabled: bool,
    },
    PreventDisplaySleep {
        enabled: bool,
    },
    BatteryThreshold {
        enabled: bool,
        value: Option<u16>,
    },
    InTransitTimeout {
        enabled: bool,
        value: Option<u32>,
    },
    Schedule {
        enabled: bool,
        start: Option<String>,
        end: Option<String>,
        days: Option<Vec<String>>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebPrefsSnapshot {
    start_at_login: bool,
    activate_at_launch: bool,
    prevent_display_sleep: bool,
    battery_threshold: WebNumberSetting,
    in_transit_timeout: WebNumberSetting,
    schedule: WebScheduleSetting,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebNumberSetting {
    enabled: bool,
    value: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebScheduleSetting {
    enabled: bool,
    start: String,
    end: String,
    days: Vec<&'static str>,
}

fn apply_web_message(actions: &dyn PrefsActions, json: &str) -> anyhow::Result<()> {
    let message: WebPrefsMessage = serde_json::from_str(json)?;
    match message {
        WebPrefsMessage::Ready => {}
        WebPrefsMessage::StartAtLogin { enabled } => actions.set_start_at_login(enabled),
        WebPrefsMessage::ActivateAtLaunch { enabled } => actions.set_activate_at_launch(enabled),
        WebPrefsMessage::PreventDisplaySleep { enabled } => {
            actions.set_prevent_display_sleep(enabled)
        }
        WebPrefsMessage::BatteryThreshold { enabled, value } => {
            let pct = enabled.then(|| match value {
                Some(v) if (1..=100).contains(&v) => v as u8,
                _ => DEFAULT_BATTERY_PCT,
            });
            actions.set_battery_threshold(pct);
        }
        WebPrefsMessage::InTransitTimeout { enabled, value } => {
            let minutes = enabled.then(|| match value {
                Some(v) if (1..=120).contains(&v) => v,
                _ => DEFAULT_IN_TRANSIT_MINUTES,
            });
            actions.set_in_transit_timeout(minutes);
        }
        WebPrefsMessage::Schedule {
            enabled,
            start,
            end,
            days,
        } => {
            if enabled {
                actions.set_schedule(Some(schedule_from_web(start, end, days)?));
            } else {
                actions.set_schedule(None);
            }
        }
    }
    Ok(())
}

fn schedule_from_web(
    start: Option<String>,
    end: Option<String>,
    days: Option<Vec<String>>,
) -> anyhow::Result<Schedule> {
    let start = parse_web_time(
        start.as_deref(),
        NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_START.0, DEFAULT_SCHEDULE_START.1, 0).unwrap(),
    )?;
    let end = parse_web_time(
        end.as_deref(),
        NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_END.0, DEFAULT_SCHEDULE_END.1, 0).unwrap(),
    )?;
    let end = if start == end {
        end.overflowing_add_signed(chrono::TimeDelta::minutes(1)).0
    } else {
        end
    };
    Ok(Schedule {
        days: days_from_web(days)?,
        start,
        end,
    })
}

fn parse_web_time(raw: Option<&str>, default: NaiveTime) -> anyhow::Result<NaiveTime> {
    match raw {
        Some(value) => Ok(NaiveTime::parse_from_str(value, "%H:%M")?),
        None => Ok(default),
    }
}

fn days_from_web(raw: Option<Vec<String>>) -> anyhow::Result<DaysOfWeek> {
    let Some(raw) = raw else {
        return Ok(DaysOfWeek::all());
    };
    let mut days = DaysOfWeek::empty();
    for day in raw {
        days |= match day.as_str() {
            "mon" => DaysOfWeek::MON,
            "tue" => DaysOfWeek::TUE,
            "wed" => DaysOfWeek::WED,
            "thu" => DaysOfWeek::THU,
            "fri" => DaysOfWeek::FRI,
            "sat" => DaysOfWeek::SAT,
            "sun" => DaysOfWeek::SUN,
            other => anyhow::bail!("unknown schedule day from WebView: {other}"),
        };
    }
    if days.is_empty() {
        Ok(DaysOfWeek::all())
    } else {
        Ok(days)
    }
}

fn snapshot_to_web_json(snap: &Snapshot) -> anyhow::Result<String> {
    let schedule = snap.modifiers.schedule.as_ref();
    let web = WebPrefsSnapshot {
        start_at_login: snap.start_at_login,
        activate_at_launch: snap.activate_at_launch,
        prevent_display_sleep: snap.prevent_display_sleep,
        battery_threshold: WebNumberSetting {
            enabled: snap.battery_threshold_pct.is_some(),
            value: snap.battery_threshold_pct.unwrap_or(DEFAULT_BATTERY_PCT) as u32,
        },
        in_transit_timeout: WebNumberSetting {
            enabled: snap.in_transit_timeout_minutes.is_some(),
            value: snap
                .in_transit_timeout_minutes
                .unwrap_or(DEFAULT_IN_TRANSIT_MINUTES),
        },
        schedule: WebScheduleSetting {
            enabled: schedule.is_some(),
            start: hhmm(schedule.map(|s| s.start).unwrap_or_else(|| {
                NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_START.0, DEFAULT_SCHEDULE_START.1, 0)
                    .unwrap()
            })),
            end: hhmm(schedule.map(|s| s.end).unwrap_or_else(|| {
                NaiveTime::from_hms_opt(DEFAULT_SCHEDULE_END.0, DEFAULT_SCHEDULE_END.1, 0).unwrap()
            })),
            days: days_to_web(schedule.map(|s| s.days).unwrap_or_else(DaysOfWeek::all)),
        },
    };
    Ok(serde_json::to_string(&web)?)
}

fn hhmm(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn days_to_web(days: DaysOfWeek) -> Vec<&'static str> {
    const DAY_FLAGS: [(DaysOfWeek, &str); 7] = [
        (DaysOfWeek::MON, "mon"),
        (DaysOfWeek::TUE, "tue"),
        (DaysOfWeek::WED, "wed"),
        (DaysOfWeek::THU, "thu"),
        (DaysOfWeek::FRI, "fri"),
        (DaysOfWeek::SAT, "sat"),
        (DaysOfWeek::SUN, "sun"),
    ];
    DAY_FLAGS
        .iter()
        .filter_map(|(flag, name)| days.contains(*flag).then_some(*name))
        .collect()
}

#[derive(Default)]
pub struct PrefsHandlerIvars {
    actions: OnceCell<Arc<dyn PrefsActions>>,
    web_view: OnceCell<Retained<AnyObject>>,
    latest_snapshot_json: OnceCell<Arc<Mutex<Option<String>>>>,
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
        // SAFETY: The signature matches WebKit's WKScriptMessageHandler entry point.
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive_script_message(
            &self,
            _controller: Option<&AnyObject>,
            message: Option<&AnyObject>,
        ) {
            let Some(message) = message else {
                return;
            };
            let Some(json) = script_message_body_string(message) else {
                tracing::warn!("preferences WebView posted a non-string message body");
                return;
            };

            let parsed: Result<WebPrefsMessage, _> = serde_json::from_str(&json);
            if matches!(parsed, Ok(WebPrefsMessage::Ready)) {
                self.push_latest_snapshot();
                return;
            }

            let Some(actions) = self.ivars().actions.get() else {
                tracing::warn!("preferences WebView message received before actions were installed");
                return;
            };
            if let Err(e) = apply_web_message(actions.as_ref(), &json) {
                tracing::warn!("preferences WebView message ignored: {e:#}");
            }
        }
    }
);

impl PrefsHandler {
    fn new(
        mtm: MainThreadMarker,
        actions: Arc<dyn PrefsActions>,
        latest_snapshot_json: Arc<Mutex<Option<String>>>,
    ) -> Retained<Self> {
        let ivars = PrefsHandlerIvars::default();
        let _ = ivars.actions.set(actions);
        let _ = ivars.latest_snapshot_json.set(latest_snapshot_json);
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's `init` is safe to call.
        unsafe { msg_send![super(this), init] }
    }

    fn install_web_view(&self, web_view: Retained<AnyObject>) {
        let _ = self.ivars().web_view.set(web_view);
    }

    fn push_latest_snapshot(&self) {
        let Some(web_view) = self.ivars().web_view.get() else {
            return;
        };
        let Some(latest) = self.ivars().latest_snapshot_json.get() else {
            return;
        };
        let Ok(guard) = latest.lock() else {
            return;
        };
        if let Some(json) = guard.as_deref() {
            evaluate_snapshot_script(web_view, json);
        }
    }
}

/// The preferences window itself. Constructed once on first `show()`; kept
/// alive thereafter. All AppKit/WebKit refs are held by `Retained`.
pub struct PreferencesWindow {
    window: Retained<NSWindow>,
    web_view: Retained<AnyObject>,
    latest_snapshot_json: Arc<Mutex<Option<String>>>,
    // Anchor the handler: WKUserContentController stores a weak-ish script
    // handler reference; keep the object alive for the window lifetime.
    _handler: Retained<PrefsHandler>,
}

// SAFETY: The retained AppKit/WebKit objects are only touched on the main
// thread. Callers obtain a `MainThreadMarker` before invoking `show`.
unsafe impl Send for PreferencesWindow {}
// SAFETY: see Send impl above.
unsafe impl Sync for PreferencesWindow {}

impl PreferencesWindow {
    /// Build the window and load the bundled mock-matching WebView UI.
    /// The window starts hidden; call `show()` to display it.
    pub fn new(mtm: MainThreadMarker, actions: Arc<dyn PrefsActions>) -> Self {
        let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(720.0, 590.0));
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
        // would dangle our retained handle. We reuse the same window.
        unsafe { window.setReleasedWhenClosed(false) };

        let latest_snapshot_json = Arc::new(Mutex::new(None));
        let handler = PrefsHandler::new(mtm, actions, latest_snapshot_json.clone());
        let web_view = make_web_view(content_rect, handler.as_ref());
        handler.install_web_view(web_view.clone());

        let content_view = window
            .contentView()
            .expect("NSWindow always has a content view");
        // SAFETY: WKWebView is an NSView subclass, and addSubview: accepts it.
        unsafe {
            let _: () = msg_send![&*content_view, addSubview: &*web_view];
        }
        load_preferences_html(&web_view);

        Self {
            window,
            web_view,
            latest_snapshot_json,
            _handler: handler,
        }
    }

    /// Open the window (or bring it to the front if already visible). Reads
    /// the current snapshot to refresh the WebView first.
    pub fn show(&self, snapshot: &Snapshot, mtm: MainThreadMarker) {
        match snapshot_to_web_json(snapshot) {
            Ok(json) => {
                if let Ok(mut latest) = self.latest_snapshot_json.lock() {
                    *latest = Some(json.clone());
                }
                evaluate_snapshot_script(&self.web_view, &json);
            }
            Err(e) => tracing::warn!("failed to serialize preferences snapshot for WebView: {e:#}"),
        }

        if !self.window.isVisible() {
            self.window.center();
        }

        // OpenLid is an accessory (LSUIElement) app, so it is *not* the active
        // application when the user opens Preferences from the menu bar. A bare
        // makeKeyAndOrderFront then leaves the window behind the frontmost app.
        // activateIgnoringOtherApps: forces us above other apps; we order the
        // window front afterward.
        //
        // We deliberately avoid -[NSApplication activate]: it is macOS 14+ (an
        // unrecognized selector on our 13.0 deployment minimum) and is by design
        // "cooperative", so it will not reliably raise an accessory app over the
        // currently-active one — which is exactly the bug we are fixing.
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        self.window.makeKeyAndOrderFront(None);
    }
}

fn make_web_view(frame: NSRect, handler: &PrefsHandler) -> Retained<AnyObject> {
    let config_cls = AnyClass::get(c"WKWebViewConfiguration")
        .expect("WKWebViewConfiguration class is available when WebKit is linked");
    let controller_cls = AnyClass::get(c"WKUserContentController")
        .expect("WKUserContentController class is available when WebKit is linked");
    let web_view_cls =
        AnyClass::get(c"WKWebView").expect("WKWebView class is available when WebKit is linked");

    let config = unsafe {
        let raw: *mut AnyObject = msg_send![config_cls, new];
        Retained::from_raw(raw).expect("WKWebViewConfiguration.new returned nil")
    };
    let controller = unsafe {
        let raw: *mut AnyObject = msg_send![controller_cls, new];
        Retained::from_raw(raw).expect("WKUserContentController.new returned nil")
    };

    let message_name = NSString::from_str("openlid");
    // SAFETY: -addScriptMessageHandler:name: stores our NSObject subclass as
    // the handler for window.webkit.messageHandlers.openlid.postMessage(...).
    unsafe {
        let _: () = msg_send![
            &*controller,
            addScriptMessageHandler: handler,
            name: &*message_name
        ];
        let _: () = msg_send![&*config, setUserContentController: &*controller];
    }

    let web_view = unsafe {
        let allocated: *mut AnyObject = msg_send![web_view_cls, alloc];
        let raw: *mut AnyObject = msg_send![
            allocated,
            initWithFrame: frame,
            configuration: &*config
        ];
        Retained::from_raw(raw).expect("WKWebView initWithFrame:configuration: returned nil")
    };
    // NSViewWidthSizable | NSViewHeightSizable keeps the WebView flush with
    // the native content view if AppKit adjusts the content area.
    unsafe {
        let _: () = msg_send![&*web_view, setAutoresizingMask: 18usize];
    }
    web_view
}

fn load_preferences_html(web_view: &AnyObject) {
    let html = NSString::from_str(PREFERENCES_HTML);
    let nil_url: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: -loadHTMLString:baseURL: accepts an NSString and nullable NSURL.
    unsafe {
        let _: *mut AnyObject = msg_send![
            web_view,
            loadHTMLString: &*html,
            baseURL: nil_url
        ];
    }
}

fn evaluate_snapshot_script(web_view: &AnyObject, json: &str) {
    let script = NSString::from_str(&format!(
        "window.OpenLidPreferences && window.OpenLidPreferences.applySnapshot({json});"
    ));
    let nil_handler: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: -evaluateJavaScript:completionHandler: accepts a JS string and a
    // nullable completion block. Nil is fine because we do not need callbacks.
    unsafe {
        let _: () = msg_send![
            web_view,
            evaluateJavaScript: &*script,
            completionHandler: nil_handler
        ];
    }
}

fn script_message_body_string(message: &AnyObject) -> Option<String> {
    let body = unsafe {
        let raw: *mut AnyObject = msg_send![message, body];
        Retained::retain(raw)?
    };
    let string_cls = AnyClass::get(c"NSString")?;
    let is_string: Bool = unsafe { msg_send![&*body, isKindOfClass: string_cls] };
    if !is_string.as_bool() {
        return None;
    }
    let string: &NSString = unsafe { &*(&*body as *const AnyObject as *const NSString) };
    Some(string.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingActions {
        start_at_login: Mutex<Option<bool>>,
        battery_threshold: Mutex<Option<Option<u8>>>,
        schedule: Mutex<Option<Option<Schedule>>>,
    }

    impl PrefsActions for RecordingActions {
        fn set_start_at_login(&self, enabled: bool) {
            *self.start_at_login.lock().unwrap() = Some(enabled);
        }

        fn set_activate_at_launch(&self, _enabled: bool) {}

        fn set_battery_threshold(&self, pct: Option<u8>) {
            *self.battery_threshold.lock().unwrap() = Some(pct);
        }

        fn set_prevent_display_sleep(&self, _enabled: bool) {}

        fn set_schedule(&self, schedule: Option<Schedule>) {
            *self.schedule.lock().unwrap() = Some(schedule);
        }

        fn set_in_transit_timeout(&self, _minutes: Option<u32>) {}
    }

    #[test]
    fn apply_web_message_routes_boolean_preference_to_actions() {
        let actions = RecordingActions::default();

        apply_web_message(&actions, r#"{"type":"start-at-login","enabled":true}"#).unwrap();

        assert_eq!(*actions.start_at_login.lock().unwrap(), Some(true));
    }

    #[test]
    fn apply_web_message_clamps_enabled_battery_threshold() {
        let actions = RecordingActions::default();

        apply_web_message(
            &actions,
            r#"{"type":"battery-threshold","enabled":true,"value":125}"#,
        )
        .unwrap();

        assert_eq!(
            *actions.battery_threshold.lock().unwrap(),
            Some(Some(DEFAULT_BATTERY_PCT))
        );
    }

    #[test]
    fn apply_web_message_disabled_schedule_clears_schedule() {
        let actions = RecordingActions::default();

        apply_web_message(&actions, r#"{"type":"schedule","enabled":false}"#).unwrap();

        assert_eq!(*actions.schedule.lock().unwrap(), Some(None));
    }

    #[test]
    fn apply_web_message_enabled_schedule_parses_days_and_times() {
        let actions = RecordingActions::default();

        apply_web_message(
            &actions,
            r#"{"type":"schedule","enabled":true,"start":"09:30","end":"17:45","days":["mon","wed","fri"]}"#,
        )
        .unwrap();

        let schedule = actions.schedule.lock().unwrap().clone().unwrap().unwrap();
        assert_eq!(schedule.start, NaiveTime::from_hms_opt(9, 30, 0).unwrap());
        assert_eq!(schedule.end, NaiveTime::from_hms_opt(17, 45, 0).unwrap());
        assert!(schedule.days.contains(DaysOfWeek::MON));
        assert!(schedule.days.contains(DaysOfWeek::WED));
        assert!(schedule.days.contains(DaysOfWeek::FRI));
        assert!(!schedule.days.contains(DaysOfWeek::TUE));
    }
}
