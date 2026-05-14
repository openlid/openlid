//! The NSStatusItem wrapper. Owns the menu bar slot, its button, its menu,
//! and the `MenuHandler` target object. Exposes a single mutating method,
//! [`UIShared::refresh`], invoked whenever the runtime's snapshot changes
//! (either by a click coming back through the menu or by a future event tap).
//!
//! The AppKit objects (NSStatusItem, NSMenu, NSMenuItem) are documented as
//! main-thread-only. We assert `Send + Sync` on the shared bundle anyway
//! because every concrete touch happens from a main-thread callback (the
//! menu click handlers in `MenuHandler`, and `mod::run` itself). If a future
//! caller wires refresh from a background thread, it MUST hop to main first.
use super::icons::laptop_icon;
use super::menu::{build_menu, refresh_menu, BuiltMenu, MenuActions, MenuHandler};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2_app_kit::{NSEventMask, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::{MainThreadMarker, NSString};
use open_lid_core::ipc::control::Snapshot;
use std::sync::Arc;

/// All AppKit refs we need to mutate after the initial build. Shared between
/// `StatusItemUI` (the owner) and the menu actions (which trigger refresh
/// from menu click handlers).
///
/// # Safety
///
/// `Send + Sync` are asserted manually. The members are AppKit objects which
/// AppKit only safely tolerates main-thread access for. All callers — the
/// menu handler selectors invoked by NSMenu and the initial setup in
/// `mod::run` — are on the main thread. Refresh callers obtain a
/// `MainThreadMarker` before touching the inner state.
pub struct UIShared {
    pub status_item: Retained<NSStatusItem>,
    pub menu: BuiltMenu,
}

// SAFETY: see UIShared doc.
unsafe impl Send for UIShared {}
// SAFETY: see UIShared doc.
unsafe impl Sync for UIShared {}

impl UIShared {
    /// Programmatically pop the menu (in response to a right-click or
    /// option-click). The pattern is the documented AppKit recipe for
    /// status-item menus: temporarily install the menu on the status item,
    /// fire a button click to make AppKit show it, then immediately
    /// un-install so subsequent left-clicks route back through the
    /// button action.
    pub fn show_menu(&self, mtm: MainThreadMarker) {
        if let Some(button) = self.status_item.button(mtm) {
            self.status_item.setMenu(Some(&self.menu.menu));
            unsafe { button.performClick(None) };
            self.status_item.setMenu(None);
        }
    }

    /// Recompute the status item's button image and the menu's mutable items
    /// from the snapshot. Caller MUST be on the main thread.
    pub fn refresh(&self, snap: &Snapshot, mtm: MainThreadMarker) {
        // The icon reflects the user's *intent* (the `enabled` toggle), not
        // the moment-to-moment "actually preventing sleep right now" state.
        // In mode `lid-closed` with the lid open, sleep isn't being prevented
        // yet — but the toggle is armed and the icon should show that. The
        // menu's status row separately shows the precise prevention state.
        //
        // Icons are programmatically drawn from Tabler's MIT-licensed SVGs:
        //   - `device-laptop`     for ON
        //   - `device-laptop-off` for OFF
        let image = laptop_icon(snap.enabled);
        image.setTemplate(true);
        let accessibility = NSString::from_str(if snap.enabled {
            "Open-Lid: armed"
        } else {
            "Open-Lid: off"
        });
        image.setAccessibilityDescription(Some(&accessibility));

        if let Some(button) = self.status_item.button(mtm) {
            button.setImage(Some(&image));
            // Clear any fallback title so we show only the icon.
            button.setTitle(&NSString::from_str(""));
        }

        refresh_menu(&self.menu, snap);
    }
}

/// Owns the menu bar slot and the menu handler. Keeps the AppKit objects
/// alive for the lifetime of the app via `Retained`. Hand out clones of the
/// `Arc<UIShared>` for code paths that need to refresh after state changes.
pub struct StatusItemUI {
    shared: Arc<UIShared>,
    // We must keep the handler alive: it is the menu's target. NSMenuItem's
    // target is a weak reference, so dropping `handler` would invalidate all
    // menu click handlers. Stored here (never read directly) to anchor it.
    _handler: Retained<MenuHandler>,
}

impl StatusItemUI {
    /// Create the status item slot, install the menu, and wire menu items to
    /// `actions`. The icon and menu titles are incorrect until the first
    /// [`refresh`](Self::refresh) call.
    pub fn new(mtm: MainThreadMarker, actions: Arc<dyn MenuActions>) -> anyhow::Result<Self> {
        let bar = NSStatusBar::systemStatusBar();
        let status_item = bar.statusItemWithLength(NSVariableStatusItemLength);

        let handler = MenuHandler::new(mtm, actions);
        let menu = build_menu(mtm, &handler);

        // Standard NSStatusItem pattern: the button's action handles BOTH
        // left and right clicks. `setMenu(None)` keeps the menu un-installed
        // so left-click reaches our action; on right/option click the action
        // calls UIShared::show_menu which temporarily attaches it.
        if let Some(button) = status_item.button(mtm) {
            let handler_obj: &AnyObject = handler.as_ref();
            unsafe {
                button.setTarget(Some(handler_obj));
                button.setAction(Some(sel!(statusItemClicked:)));
                let mask = NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp;
                button.sendActionOn(mask);
            }
        }

        let shared = Arc::new(UIShared { status_item, menu });

        Ok(Self {
            shared,
            _handler: handler,
        })
    }

    /// Clone the shared handle. Each clone holds Strong references to the
    /// underlying AppKit objects (they are reference-counted), so the menu
    /// will remain functional even if this `StatusItemUI` is dropped — but
    /// don't do that; the `_handler` ivar is also load-bearing.
    pub fn shared(&self) -> Arc<UIShared> {
        Arc::clone(&self.shared)
    }

    /// Convenience wrapper for the initial refresh in `run()`.
    pub fn refresh(&self, snap: &Snapshot, mtm: MainThreadMarker) {
        self.shared.refresh(snap, mtm);
    }
}
