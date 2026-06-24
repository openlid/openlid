//! User-facing banner notifications for helper recovery.
//!
//! When the privileged helper isn't `Enabled`, macOS requires a *manual*
//! approval toggle in System Settings → Login Items that no code can flip
//! for the user. This module surfaces that need as a `UNUserNotification`
//! banner whose tap (or "Open System Settings" action) deep-links to the
//! Login Items pane and kicks off the bounded approval follow-up.
//!
//! This is FFI glue — manual-checklist only. The recovery decision logic
//! and rate-limiting that drive it live in `crate::recovery` and are
//! unit-tested there.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_foundation::{ns_string, NSArray, NSError, NSObject, NSObjectProtocol, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::cell::OnceCell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Category identifier tying our notifications to the "Open System
/// Settings" action. Without a registered category the custom action
/// button does not appear reliably.
const CATEGORY_ID: &str = "io.openlid.helper_recovery";
const ACTION_OPEN_SETTINGS: &str = "open_settings";
const REQUEST_ID_APPROVAL: &str = "io.openlid.helper-approval";
const REQUEST_ID_NOT_FOUND: &str = "io.openlid.helper-not-found";

/// Resolved notification-authorization state, set by the async
/// authorization callback. `0` = unknown (optimistic), `1` = granted,
/// `2` = denied. The recovery surface reads [`auth_denied`] to decide
/// whether to fall back to opening System Settings directly.
static NOTIF_AUTH: AtomicU8 = AtomicU8::new(0);

/// `true` once the authorization callback has reported the user denied
/// notifications. Used for the denied fallback so a user who turned
/// banners off still gets sent to System Settings.
pub fn auth_denied() -> bool {
    NOTIF_AUTH.load(Ordering::SeqCst) == 2
}

#[derive(Default)]
pub struct NotifyDelegateIvars {
    /// Invoked when the user taps the banner or its action. Provided by
    /// the menubar wiring: opens Login Items and starts the approval
    /// follow-up. `Send + Sync` because UN delivers responses on an
    /// internal queue, not the main thread.
    on_tap: OnceCell<Arc<dyn Fn() + Send + Sync>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject has no subclassing requirements.
    // - `NotifyDelegate` does not implement `Drop`; ivars are dropped by
    //   the `define_class!` machinery.
    // - Not `MainThreadOnly`: UNUserNotificationCenter delivers delegate
    //   callbacks on an internal queue, so this must be usable off-main.
    #[unsafe(super = NSObject)]
    #[ivars = NotifyDelegateIvars]
    pub struct NotifyDelegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for NotifyDelegate {}

    // SAFETY: signatures match `UNUserNotificationCenterDelegate`.
    unsafe impl UNUserNotificationCenterDelegate for NotifyDelegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive_response(
            &self,
            _center: &UNUserNotificationCenter,
            _response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            // Any response (default tap or our explicit action) means
            // "take me to the fix": run the supplied tap action.
            if let Some(cb) = self.ivars().on_tap.get() {
                cb();
            }
            completion_handler.call(());
        }

        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // Show the banner even though OpenLid is a foreground-capable
            // Accessory app; without this the OS may suppress it.
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl NotifyDelegate {
    fn new(on_tap: Arc<dyn Fn() + Send + Sync>) -> Retained<Self> {
        let ivars = NotifyDelegateIvars::default();
        let _ = ivars.on_tap.set(on_tap);
        let this = Self::alloc().set_ivars(ivars);
        // SAFETY: NSObject's `init` is safe to call.
        unsafe { msg_send![super(this), init] }
    }
}

/// Wire up notifications: install the delegate, register the category +
/// action, and request authorization. Returns the retained delegate,
/// which the caller MUST keep alive for the app's lifetime — the
/// notification center holds the delegate weakly.
pub fn install(on_tap: Arc<dyn Fn() + Send + Sync>) -> Retained<NotifyDelegate> {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate = NotifyDelegate::new(on_tap);

    // The center stores the delegate weakly, so the caller keeps
    // `delegate` alive for the app's lifetime.
    let proto = ProtocolObject::from_ref(&*delegate);
    center.setDelegate(Some(proto));

    register_category(&center);
    request_authorization(&center);

    delegate
}

fn register_category(center: &UNUserNotificationCenter) {
    let action = UNNotificationAction::actionWithIdentifier_title_options(
        ns_string!(ACTION_OPEN_SETTINGS),
        ns_string!("Open System Settings"),
        UNNotificationActionOptions::Foreground,
    );
    let actions = NSArray::from_slice(&[&*action]);
    let intents: Retained<NSArray<NSString>> = NSArray::from_slice(&[]);
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        ns_string!(CATEGORY_ID),
        &actions,
        &intents,
        UNNotificationCategoryOptions::empty(),
    );
    let set = NSSet::from_slice(&[&*category]);
    center.setNotificationCategories(&set);
}

fn request_authorization(center: &UNUserNotificationCenter) {
    let handler = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        NOTIF_AUTH.store(if granted.as_bool() { 1 } else { 2 }, Ordering::SeqCst);
        if !granted.as_bool() {
            tracing::warn!("notification authorization denied; will fall back to opening Settings");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
}

/// Post the "helper needs approval" banner (carries the Open System
/// Settings action via [`CATEGORY_ID`]).
pub fn post_approval() {
    post(
        REQUEST_ID_APPROVAL,
        "OpenLid isn't keeping your Mac awake",
        "Approve OpenLid in System Settings \u{2192} Login Items so it can prevent sleep.",
        true,
    );
}

/// Post the "OpenLid must be in /Applications" banner. No action button —
/// the Login Items toggle can't fix a mis-placed app.
pub fn post_not_found() {
    post(
        REQUEST_ID_NOT_FOUND,
        "OpenLid can't install its helper",
        "Move OpenLid into the Applications folder, then reopen it.",
        false,
    );
}

fn post(request_id: &str, title: &str, body: &str, with_action: bool) {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    if with_action {
        content.setCategoryIdentifier(ns_string!(CATEGORY_ID));
    }

    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(request_id),
        &content,
        None,
    );
    center.addNotificationRequest_withCompletionHandler(&request, None);
}
