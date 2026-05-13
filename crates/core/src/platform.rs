//! Platform-abstraction traits. macOS impls live in `crates/app/src/platform/macos/`.

use crate::mode::{LidState, PowerSource};
use std::sync::Arc;

pub type LidStateCallback = Arc<dyn Fn(LidState) + Send + Sync + 'static>;
pub type PowerSourceCallback = Arc<dyn Fn(PowerSource) + Send + Sync + 'static>;

pub trait PowerController: Send + Sync {
    fn prevent_sleep(&self) -> Result<(), PlatformError>;
    fn allow_sleep(&self) -> Result<(), PlatformError>;
}

pub trait LidObserver: Send + Sync {
    fn current(&self) -> LidState;
    fn subscribe(&self, callback: LidStateCallback);
}

pub trait PowerSourceMonitor: Send + Sync {
    fn current(&self) -> PowerSource;
    fn subscribe(&self, callback: PowerSourceCallback);
}

pub trait DisplayController: Send + Sync {
    fn has_external_display(&self) -> bool;
    fn force_display_sleep(&self) -> Result<(), PlatformError>;
    /// Acquire a power-management assertion that prevents the display from
    /// going to sleep due to user idle. While held, macOS treats the user as
    /// active — the display stays on, no screen-saver kicks in, no screen
    /// lock fires. Idempotent: calling twice while already held is a no-op.
    /// Has no effect on explicit `force_display_sleep` calls or lid-close.
    fn prevent_display_sleep(&self) -> Result<(), PlatformError>;
    /// Release the assertion acquired by `prevent_display_sleep`. Idempotent.
    fn allow_display_sleep(&self) -> Result<(), PlatformError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("platform call failed: {0}")]
    Native(String),
    #[error("helper unavailable")]
    HelperUnavailable,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
