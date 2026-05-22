//! NSMenu construction and the Obj-C `MenuHandler` target object.
//!
//! The handler stores a single `Arc<dyn MenuActions>` ivar; menu-item
//! selectors dispatch through that trait. This keeps the AppKit-facing
//! glue free of the `StateRuntime`'s many generic parameters.
//!
//! Menu structure (post-mode-removal):
//!
//! ```text
//! Status: Preventing sleep · Lid open · AC       [disabled item]
//! ─────────
//! Turn Off    (or "Turn On")                    [action: toggle]
//! ─────────
//! Preferences…    ⌘,                            [action: open_preferences]
//! ─────────
//! Quit OpenLid    ⌘Q                            [action: quit]
//! ```
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use openlid_core::ipc::control::Snapshot;
use openlid_core::mode::{LidState, PowerSource};
use std::cell::OnceCell;
use std::sync::Arc;

/// Operations the menu can invoke. Implemented over the (generic) StateRuntime
/// by the outer module so AppKit code only ever sees this trait.
pub trait MenuActions: Send + Sync {
    /// Single-click / "Turn On" / "Turn Off". Always indefinite — no timer.
    fn toggle(&self);
    /// Right-click or option-click on the status item button: show the menu.
    /// Implementation is expected to call `UIShared::show_menu`.
    fn show_menu(&self);
    /// "Preferences…" — opens the prefs window.
    fn open_preferences(&self);
    /// "Quit OpenLid".
    fn quit(&self);
}

#[derive(Default)]
pub struct MenuHandlerIvars {
    actions: OnceCell<Arc<dyn MenuActions>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `MenuHandler` does not implement `Drop`; ivars are dropped via the
    //   `define_class!` machinery.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuHandlerIvars]
    pub struct MenuHandler;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for MenuHandler {}

    impl MenuHandler {
        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(toggle:))]
        fn toggle(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.toggle();
            }
        }

        /// Button click on the status item. Inspect the current event to
        /// decide: left click → toggle; right-click or option-click → menu.
        ///
        /// SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(statusItemClicked:))]
        fn status_item_clicked(&self, _sender: Option<&AnyObject>) {
            let Some(actions) = self.ivars().actions.get() else { return };
            let Some(mtm) = MainThreadMarker::new() else { return };
            // SAFETY: `NSApplication::sharedApplication` returns an autoreleased
            // singleton; reading `currentEvent` is documented as main-thread-safe.
            let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
            let is_right_or_option = if let Some(event) = app.currentEvent() {
                let event_type = event.r#type();
                let is_right = event_type == objc2_app_kit::NSEventType::RightMouseUp;
                let modifiers = event.modifierFlags();
                let has_option = modifiers.contains(objc2_app_kit::NSEventModifierFlags::Option);
                is_right || has_option
            } else {
                false
            };
            if is_right_or_option {
                actions.show_menu();
            } else {
                actions.toggle();
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(openPreferences:))]
        fn open_preferences(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.open_preferences();
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.quit();
            }
        }
    }
);

impl MenuHandler {
    pub fn new(mtm: MainThreadMarker, actions: Arc<dyn MenuActions>) -> Retained<Self> {
        let ivars = MenuHandlerIvars::default();
        let _ = ivars.actions.set(actions);
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's `init` is safe to call.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct BuiltMenu {
    pub menu: Retained<NSMenu>,
    pub status_item: Retained<NSMenuItem>,
    pub toggle_item: Retained<NSMenuItem>,
}

pub fn build_menu(mtm: MainThreadMarker, handler: &MenuHandler) -> BuiltMenu {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
    menu.setAutoenablesItems(false);

    let handler_obj: &AnyObject = handler.as_ref();

    // 1. Status header (disabled).
    let status_item = make_item(mtm, "Status", None, handler_obj);
    status_item.setEnabled(false);
    menu.addItem(&status_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 2. Toggle.
    let toggle_item = make_item(mtm, "Turn On", Some(sel!(toggle:)), handler_obj);
    menu.addItem(&toggle_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 3. Preferences.
    let prefs = make_item(
        mtm,
        "Preferences…",
        Some(sel!(openPreferences:)),
        handler_obj,
    );
    prefs.setKeyEquivalent(ns_string!(","));
    menu.addItem(&prefs);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 4. Quit.
    let quit_item = make_item(mtm, "Quit OpenLid", Some(sel!(quit:)), handler_obj);
    quit_item.setKeyEquivalent(ns_string!("q"));
    menu.addItem(&quit_item);

    BuiltMenu {
        menu,
        status_item,
        toggle_item,
    }
}

fn make_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    // SAFETY: title is valid NSString; action selectors are declared above.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &title,
            action,
            ns_string!(""),
        )
    };
    if action.is_some() {
        // SAFETY: `target` is a MenuHandler NSObject.
        unsafe {
            item.setTarget(Some(target));
        }
    }
    item
}

/// Format the status row: "Active until 18:30 · Lid open · AC" or similar.
pub fn format_status_header(snap: &Snapshot) -> String {
    let prevention = if snap.preventing_sleep_now {
        if let Some(t) = snap.until {
            format!("Active until {}", t.format("%H:%M"))
        } else {
            "Active (indefinite)".to_string()
        }
    } else if snap.enabled {
        "Armed (idle)".to_string()
    } else {
        "Off".to_string()
    };
    let lid = match snap.lid {
        LidState::Open => "lid open",
        LidState::Closed => "lid closed",
    };
    let power = match snap.power {
        PowerSource::Ac => "AC".to_string(),
        PowerSource::Battery { percent } => format!("battery {percent}%"),
    };
    format!("{prevention} · {lid} · {power}")
}

pub fn refresh_menu(menu: &BuiltMenu, snap: &Snapshot) {
    let header = NSString::from_str(&format_status_header(snap));
    menu.status_item.setTitle(&header);

    let toggle_title = if snap.enabled { "Turn Off" } else { "Turn On" };
    menu.toggle_item.setTitle(&NSString::from_str(toggle_title));
}
