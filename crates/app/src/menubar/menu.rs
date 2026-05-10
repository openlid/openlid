//! NSMenu construction and the Obj-C `MenuHandler` target object.
//!
//! The handler stores a single `Arc<dyn MenuActions>` ivar; menu-item
//! selectors dispatch through that trait. This keeps the AppKit-facing
//! glue free of the `StateRuntime`'s many generic parameters.
//!
//! The menu structure (MVP):
//!
//! ```text
//! Status: Preventing sleep · Mode: Lid-closed       [disabled item]
//! ─────────
//! Turn Off    (or "Turn On")           [action: toggle]
//! ─────────
//! Mode ▸
//!   ✓ Lid-closed                       [action: set mode lid-closed]
//!     Always awake                     [action: set mode always-awake]
//! ─────────
//! Quit Open-Lid    ⌘Q                  [action: quit]
//! ```
//!
//! Each refresh re-titles the toggle item and updates the checkmark on the
//! mode submenu items to reflect the current snapshot.
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::{ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use open_lid_core::ipc::control::Snapshot;
use open_lid_core::mode::{LidState, Mode, PowerSource};
use std::cell::OnceCell;
use std::sync::Arc;

/// Operations the menu can invoke. Implemented over the (generic) StateRuntime
/// by the outer module so AppKit code only ever sees this trait.
pub trait MenuActions: Send + Sync {
    fn toggle(&self);
    fn set_mode_lid_closed(&self);
    fn set_mode_always_awake(&self);
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

    /// Action selectors invoked by NSMenu when items are clicked. NSMenu calls
    /// each as `-[handler selector:sender]`, where `sender` is the NSMenuItem.
    /// We ignore the sender; the handler's `actions` ivar carries everything
    /// we need.
    impl MenuHandler {
        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(toggle:))]
        fn toggle(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.toggle();
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setModeLidClosed:))]
        fn set_mode_lid_closed(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_mode_lid_closed();
            }
        }

        // SAFETY: The signature `(self, sender) -> ()` matches what AppKit sends.
        #[unsafe(method(setModeAlwaysAwake:))]
        fn set_mode_always_awake(&self, _sender: Option<&AnyObject>) {
            if let Some(actions) = self.ivars().actions.get() {
                actions.set_mode_always_awake();
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
    /// Construct a new handler. The `actions` are installed once; cloning the
    /// `Retained<MenuHandler>` does not duplicate the ivars.
    pub fn new(mtm: MainThreadMarker, actions: Arc<dyn MenuActions>) -> Retained<Self> {
        let ivars = MenuHandlerIvars::default();
        let _ = ivars.actions.set(actions);
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's `init` is safe to call; this is the standard
        // post-`alloc` initializer.
        unsafe { msg_send![super(this), init] }
    }
}

// SAFETY: NSObjects are `Send`/`Sync` in the sense relevant here: the
// `Retained<MenuHandler>` may be moved between threads, but AppKit will only
// invoke selectors on the main thread (the menu and status item are both
// main-thread-only objects). The ivar `OnceCell<Arc<dyn MenuActions>>` only
// stores a `Send + Sync` value behind an `Arc`. We do not implement these
// here; default object behavior is sufficient because the menu handler only
// crosses thread boundaries via `Retained` clones, not by raw value moves.

// ---------------------------------------------------------------------------
// References to menu items we want to mutate on refresh.
//
// We hold direct `Retained<NSMenuItem>` for the toggle row and each mode row
// so refresh can update titles / checkmarks without re-walking the menu.

pub struct BuiltMenu {
    pub menu: Retained<NSMenu>,
    pub status_item: Retained<NSMenuItem>,
    pub toggle_item: Retained<NSMenuItem>,
    pub mode_lid_closed: Retained<NSMenuItem>,
    pub mode_always_awake: Retained<NSMenuItem>,
}

/// Build the MVP menu with all items targeted at `handler`. Returns the menu
/// plus the handles we need to mutate on snapshot refresh.
pub fn build_menu(mtm: MainThreadMarker, handler: &MenuHandler) -> BuiltMenu {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
    // Items are enabled/disabled explicitly; the "Status: …" header item must
    // stay greyed out even though it has no target.
    menu.setAutoenablesItems(false);

    let handler_obj: &AnyObject = handler.as_ref();

    // 1. Status header (disabled).
    let status_item = make_item(mtm, "Status", None, handler_obj);
    status_item.setEnabled(false);
    menu.addItem(&status_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 2. Toggle (Turn On / Turn Off).
    let toggle_item = make_item(mtm, "Turn On", Some(sel!(toggle:)), handler_obj);
    menu.addItem(&toggle_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 3. Mode submenu.
    let mode_item = make_item(mtm, "Mode", None, handler_obj);
    let mode_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Mode"));
    mode_menu.setAutoenablesItems(false);
    let mode_lid_closed = make_item(mtm, "Lid-closed", Some(sel!(setModeLidClosed:)), handler_obj);
    let mode_always_awake =
        make_item(mtm, "Always awake", Some(sel!(setModeAlwaysAwake:)), handler_obj);
    mode_menu.addItem(&mode_lid_closed);
    mode_menu.addItem(&mode_always_awake);
    mode_item.setSubmenu(Some(&mode_menu));
    menu.addItem(&mode_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    // 4. Quit.
    let quit_item = make_item(mtm, "Quit Open-Lid", Some(sel!(quit:)), handler_obj);
    quit_item.setKeyEquivalent(ns_string!("q"));
    menu.addItem(&quit_item);

    BuiltMenu {
        menu,
        status_item,
        toggle_item,
        mode_lid_closed,
        mode_always_awake,
    }
}

/// Construct an NSMenuItem with the given title, optional action selector,
/// and target. The empty key-equivalent string means "no shortcut".
fn make_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(title);
    // SAFETY: title is a valid NSString, `action` (if Some) is a real selector
    // declared on `MenuHandler`, and the empty key-equivalent is documented
    // as "no shortcut".
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &title,
            action,
            ns_string!(""),
        )
    };
    if action.is_some() {
        // SAFETY: `target` is an NSObject (the MenuHandler), which is the
        // correct type for `setTarget:`.
        unsafe {
            item.setTarget(Some(target));
        }
    }
    item
}

// ---------------------------------------------------------------------------
// Snapshot → UI text/state.

/// Format the "Status: … · Mode: …" line shown in the disabled header item.
pub fn format_status_header(snap: &Snapshot) -> String {
    let prevention = if snap.preventing_sleep_now {
        "Preventing sleep"
    } else if snap.enabled {
        "Armed (idle)"
    } else {
        "Off"
    };
    let mode = match &snap.mode {
        Mode::LidClosed => "Lid-closed".to_string(),
        Mode::AlwaysAwake => "Always awake".to_string(),
        Mode::Timed { until } => format!("Timed (until {})", until.format("%H:%M")),
    };
    let lid = match snap.lid {
        LidState::Open => "open",
        LidState::Closed => "closed",
    };
    let power = match snap.power {
        PowerSource::Ac => "AC".to_string(),
        PowerSource::Battery { percent } => format!("battery {percent}%"),
    };
    format!("Status: {prevention} · Mode: {mode} · Lid {lid} · {power}")
}

/// Refresh the menu's mutable items from the current snapshot. Caller has
/// already confirmed we're on the main thread (NSMenu requires it).
pub fn refresh_menu(menu: &BuiltMenu, snap: &Snapshot) {
    // Status header.
    let header = NSString::from_str(&format_status_header(snap));
    menu.status_item.setTitle(&header);

    // Toggle row title flips with the current enabled state.
    let toggle_title = if snap.enabled { "Turn Off" } else { "Turn On" };
    menu.toggle_item.setTitle(&NSString::from_str(toggle_title));

    // Mode submenu checkmarks.
    let (lc, aa) = match snap.mode {
        Mode::LidClosed => (NSControlStateValueOn, NSControlStateValueOff),
        Mode::AlwaysAwake => (NSControlStateValueOff, NSControlStateValueOn),
        // Timed is not exposed in the MVP submenu; show neither checked.
        Mode::Timed { .. } => (NSControlStateValueOff, NSControlStateValueOff),
    };
    menu.mode_lid_closed.setState(lc);
    menu.mode_always_awake.setState(aa);
}
