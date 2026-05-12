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
//! Activate for ▸
//!   Indefinitely                                [tag=0]
//!   5 minutes                                   [tag=5]
//!   10 minutes
//!   15 minutes
//!   30 minutes
//!   1 hour                                      [tag=60]
//!   2 hours                                     [tag=120]
//!   5 hours                                     [tag=300]
//! ─────────
//! Preferences…    ⌘,                            [action: open_preferences]
//! ─────────
//! Quit Open-Lid    ⌘Q                           [action: quit]
//! ```
//!
//! All "Activate for" entries share a single selector that reads the menu
//! item's `tag` (in minutes; 0 = indefinite) and dispatches through
//! `MenuActions::activate_for_minutes`.
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use open_lid_core::ipc::control::Snapshot;
use open_lid_core::mode::{LidState, PowerSource};
use std::cell::OnceCell;
use std::sync::Arc;

/// Operations the menu can invoke. Implemented over the (generic) StateRuntime
/// by the outer module so AppKit code only ever sees this trait.
pub trait MenuActions: Send + Sync {
    /// Single-click / "Turn On" / "Turn Off". Uses default duration from prefs.
    fn toggle(&self);
    /// Explicit "Activate for N minutes" from the submenu. `None` = indefinite.
    fn activate_for_minutes(&self, minutes: Option<u32>);
    /// "Preferences…" — opens the prefs window.
    fn open_preferences(&self);
    /// "Quit Open-Lid".
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

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(activateFor:))]
        fn activate_for(&self, sender: Option<&AnyObject>) {
            // Read the NSMenuItem's `tag` to find which duration was selected.
            // tag = 0 → indefinite, otherwise tag = minutes.
            let tag: isize = match sender {
                Some(s) => unsafe { msg_send![s, tag] },
                None => 0,
            };
            let minutes = if tag <= 0 { None } else { Some(tag as u32) };
            if let Some(actions) = self.ivars().actions.get() {
                actions.activate_for_minutes(minutes);
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

/// The "Activate for" submenu entries. (label, minutes; minutes=0 → indefinite)
const ACTIVATE_FOR_ENTRIES: &[(&str, isize)] = &[
    ("Indefinitely", 0),
    ("5 minutes", 5),
    ("10 minutes", 10),
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("1 hour", 60),
    ("2 hours", 120),
    ("5 hours", 300),
];

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

    // 3. "Activate for" submenu.
    let activate_for_item = make_item(mtm, "Activate for", None, handler_obj);
    let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Activate for"));
    submenu.setAutoenablesItems(false);
    for (label, minutes) in ACTIVATE_FOR_ENTRIES {
        let item = make_item(mtm, label, Some(sel!(activateFor:)), handler_obj);
        item.setTag(*minutes);
        submenu.addItem(&item);
    }
    activate_for_item.setSubmenu(Some(&submenu));
    menu.addItem(&activate_for_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 4. Preferences.
    let prefs = make_item(
        mtm,
        "Preferences…",
        Some(sel!(openPreferences:)),
        handler_obj,
    );
    prefs.setKeyEquivalent(ns_string!(","));
    menu.addItem(&prefs);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 5. Quit.
    let quit_item = make_item(mtm, "Quit Open-Lid", Some(sel!(quit:)), handler_obj);
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
