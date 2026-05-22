//! Menu bar icons.
//!
//! Drawn programmatically via `NSBezierPath` so they render as proper template
//! images (alpha-only, system-tinted) at any backing scale factor. Adapted
//! from Tabler's `device-laptop` and `device-laptop-off` SVG paths.
//!
//! Tabler Icons are MIT licensed:
//!   https://github.com/tabler/tabler-icons/blob/main/LICENSE

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_app_kit::{NSBezierPath, NSColor, NSImage, NSLineCapStyle, NSLineJoinStyle};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// Logical menu-bar icon size in points. macOS renders at the display's
/// backing scale (1x or 2x).
const ICON_SIZE: f64 = 22.0;

/// The Tabler source SVGs use a 24×24 viewBox.
const SVG_SIZE: f64 = 24.0;

/// Scale factor applied to every coordinate from the source SVG.
const SCALE: f64 = ICON_SIZE / SVG_SIZE;

/// Stroke width tuned for menu-bar legibility. The Tabler default is 2pt in a
/// 24pt canvas; scaling that proportionally to our 22pt canvas gives ~1.83pt.
const STROKE_WIDTH: f64 = 1.8;

/// Alpha used for the armed state. Template images use alpha as the tint mask,
/// so stroking at 0.45 yields an icon the system renders at ~45% of its normal
/// menu-bar tint — "ready but not currently engaged."
const ARMED_ALPHA: f64 = 0.45;

/// Which laptop glyph + tint to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// Toggle is off. `device-laptop-off` (slashed).
    Off,
    /// Toggle is on but a modifier (schedule window, battery threshold) is
    /// currently gating it. Same glyph as `Active`, drawn dimmed.
    Armed,
    /// Toggle is on and sleep is actively held. `device-laptop` at full tint.
    Active,
}

/// Build the laptop icon for the menu bar.
pub fn laptop_icon(state: IconState) -> Retained<NSImage> {
    let size = NSSize::new(ICON_SIZE, ICON_SIZE);

    // The drawing handler is invoked every time AppKit needs to rasterize the
    // image (initial render, theme change, scale change). It must be Fn (not
    // FnOnce) because of that.
    let block = RcBlock::new(move |_dst_rect: NSRect| -> Bool {
        draw(state);
        Bool::YES
    });

    // `flipped: true` means the drawing handler sees a coordinate system with
    // y=0 at the *top*, matching the SVG. NSBezierPath then needs no manual
    // y-flipping for each point.
    NSImage::imageWithSize_flipped_drawingHandler(size, true, &block)
}

fn draw(state: IconState) {
    // Template images discard color and use alpha as a mask. The system tints
    // the result for the active menu-bar appearance. Setting a sub-1 alpha on
    // the stroke color produces a partially-transparent mask, which the system
    // then tints proportionally — i.e. a dimmer icon.
    let color = match state {
        IconState::Armed => NSColor::blackColor().colorWithAlphaComponent(ARMED_ALPHA),
        IconState::Off | IconState::Active => NSColor::blackColor(),
    };
    color.set();

    let path = NSBezierPath::bezierPath();
    path.setLineWidth(STROKE_WIDTH);
    path.setLineCapStyle(NSLineCapStyle::Round);
    path.setLineJoinStyle(NSLineJoinStyle::Round);

    match state {
        IconState::Off => draw_laptop_off(&path),
        IconState::Active | IconState::Armed => draw_laptop(&path),
    }

    path.stroke();
}

/// `device-laptop` — Tabler SVG paths verbatim, scaled:
///   M3 19 l18 0                           ← keyboard front edge
///   M5 7 a1 1 0 0 1 1-1 h12 a1 1 0 0 1 1 1
///       v8 a1 1 0 0 1-1 1 h-12 a1 1 0 0 1-1-1 l0-8
///                                         ← rounded rect (5,6)–(19,16) r=1
fn draw_laptop(path: &NSBezierPath) {
    // Keyboard front edge.
    path.moveToPoint(p(3.0, 19.0));
    path.lineToPoint(p(21.0, 19.0));

    // Laptop body: rounded rect from (5,6) to (19,16) with corner radius 1.
    let body = NSRect::new(
        NSPoint::new(5.0 * SCALE, 6.0 * SCALE),
        NSSize::new(14.0 * SCALE, 10.0 * SCALE),
    );
    path.appendBezierPathWithRoundedRect_xRadius_yRadius(body, 1.0 * SCALE, 1.0 * SCALE);
}

/// `device-laptop-off` — same laptop but with two strategic gaps where a
/// diagonal slash crosses through it, plus the slash itself. Reproduces
/// Tabler's three-`<path>` structure faithfully so the visual breakage at the
/// slash crossings matches.
fn draw_laptop_off(path: &NSBezierPath) {
    // Shorter keyboard front edge (the slash chops a piece off the right).
    path.moveToPoint(p(3.0, 19.0));
    path.lineToPoint(p(19.0, 19.0));

    // Top-right partial of the laptop body:
    //   M10 6 h8 a1 1 0 0 1 1 1 v8
    path.moveToPoint(p(10.0, 6.0));
    path.lineToPoint(p(18.0, 6.0));
    arc_corner(path, p(19.0, 6.0), p(19.0, 7.0), 1.0 * SCALE);
    path.lineToPoint(p(19.0, 15.0));

    // Bottom-left partial of the laptop body:
    //   M16 16 h-10 a1 1 0 0 1-1-1 v-8 a1 1 0 0 1 1-1
    path.moveToPoint(p(16.0, 16.0));
    path.lineToPoint(p(6.0, 16.0));
    arc_corner(path, p(5.0, 16.0), p(5.0, 15.0), 1.0 * SCALE);
    path.lineToPoint(p(5.0, 7.0));
    arc_corner(path, p(5.0, 6.0), p(6.0, 6.0), 1.0 * SCALE);

    // The diagonal slash.
    path.moveToPoint(p(3.0, 3.0));
    path.lineToPoint(p(21.0, 21.0));
}

/// Convert SVG coordinates (0..24, y-down — the flipped image context lets us
/// use them as-is) to scaled NSPoints.
fn p(x: f64, y: f64) -> NSPoint {
    NSPoint::new(x * SCALE, y * SCALE)
}

/// Draw an SVG-style rounded corner: arc tangent to the lines from the
/// current point through `via` and on to `to`. `radius` is the corner radius.
fn arc_corner(path: &NSBezierPath, via: NSPoint, to: NSPoint, radius: f64) {
    path.appendBezierPathWithArcFromPoint_toPoint_radius(via, to, radius);
}
