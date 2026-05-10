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
use super::menu::{build_menu, refresh_menu, BuiltMenu, MenuActions, MenuHandler};
use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
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
    /// Recompute the status item's button image and the menu's mutable items
    /// from the snapshot. Caller MUST be on the main thread.
    pub fn refresh(&self, snap: &Snapshot, mtm: MainThreadMarker) {
        // Button image — SF Symbol. `eye.fill` means "Open-Lid is preventing
        // sleep now"; `eye.slash` means we are armed-but-idle or fully off.
        // Use `setTemplate(true)` so the menu bar renders correctly in both
        // light and dark menu-bar appearances.
        let symbol_name = if snap.preventing_sleep_now {
            "eye.fill"
        } else {
            "eye.slash"
        };
        let symbol_ns = NSString::from_str(symbol_name);
        let accessibility = NSString::from_str(if snap.preventing_sleep_now {
            "Open-Lid: preventing sleep"
        } else {
            "Open-Lid: idle"
        });
        // SF Symbols are available on macOS 11+; the call returns `None` on
        // older systems or if the symbol name is wrong. We fall back to a
        // text title in that case to avoid a blank menu bar entry.
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &symbol_ns,
            Some(&accessibility),
        );

        if let Some(button) = self.status_item.button(mtm) {
            if let Some(img) = image {
                img.setTemplate(true);
                button.setImage(Some(&img));
                // Clear any fallback title so we show only the icon.
                button.setTitle(&NSString::from_str(""));
            } else {
                // Fallback: short text. Better than a blank slot.
                button.setImage(None);
                let txt = if snap.preventing_sleep_now { "ON" } else { "off" };
                button.setTitle(&NSString::from_str(txt));
            }
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
        status_item.setMenu(Some(&menu.menu));

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
