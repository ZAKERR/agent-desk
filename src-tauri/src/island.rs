//! Dynamic Island window shape management.
//!
//! Uses Win32 `CreateRoundRectRgn` + `SetWindowRgn` to clip the Tauri window
//! into a pill / rounded-rect shape. Tauri `transparent: false` avoids the
//! WebView2 hit-test bug on Windows while still giving us custom shapes.
//!
//! Transitions:
//! - **Pill width** (idle ↔ active): 6-frame spring with overshoot (~150ms)
//! - **Expand** (pill → panel): 10-frame spring ease-out (~200ms)
//! - **Collapse** (panel → pill): 8-frame ease-out (~160ms)

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use tauri::WebviewWindow;

// Fixed dimensions (not configurable)
const PILL_H: u32 = 36;
const PILL_RADIUS: i32 = 18;
const PANEL_RADIUS: i32 = 16;

// Default pill width — only used for AtomicU32 initialization
const DEFAULT_PILL_W: u32 = 300;

// Pill animation state
static PILL_TARGET_W: AtomicU32 = AtomicU32::new(DEFAULT_PILL_W);
static PILL_ANIMATING: AtomicBool = AtomicBool::new(false);
static ISLAND_EXPANDED: AtomicBool = AtomicBool::new(false);

// Morph animation guard + stored expanded dimensions (for collapse)
static MORPH_ANIMATING: AtomicBool = AtomicBool::new(false);
static EXPANDED_W: AtomicU32 = AtomicU32::new(480);
static EXPANDED_H: AtomicU32 = AtomicU32::new(320);

/// Pill width spring keyframes (normalized 0→1 with overshoot).
const SPRING_FRAMES: [f64; 6] = [0.30, 0.65, 1.00, 1.15, 1.05, 1.00];
const FRAME_MS: u64 = 25;

/// Expand: spring ease-out with subtle overshoot (~200ms, 10 frames × 20ms).
const EXPAND_CURVE: [f64; 10] = [
    0.12, 0.33, 0.54, 0.72, 0.86, 0.95, 1.01, 1.03, 1.01, 1.00,
];

/// Collapse: smooth ease-out (~160ms, 8 frames × 20ms).
const COLLAPSE_CURVE: [f64; 8] = [
    0.15, 0.38, 0.60, 0.78, 0.90, 0.97, 0.99, 1.00,
];

const MORPH_FRAME_MS: u64 = 20;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lerp_u32(a: u32, b: u32, t: f64) -> u32 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u32
}

fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b as f64 - a as f64) * t).round() as i32
}

/// Apply a rounded-rect region to an HWND.
/// All coordinates are in **physical pixels** (pre-scaled).
#[cfg(windows)]
fn apply_region(hwnd: windows::Win32::Foundation::HWND, w: i32, h: i32, radius: i32) {
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    unsafe {
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius);
        // SetWindowRgn takes ownership of the region; do not delete it.
        let _ = SetWindowRgn(hwnd, Some(rgn), true);
    }
}

/// Extract the HWND from a Tauri WebviewWindow.
#[cfg(windows)]
fn get_hwnd(window: &WebviewWindow) -> Option<windows::Win32::Foundation::HWND> {
    let raw = window.hwnd().ok()?;
    Some(windows::Win32::Foundation::HWND(raw.0))
}

/// Position the window at top-center of the primary monitor.
pub fn position_top_center(window: &WebviewWindow, w: u32, h: u32) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let screen_w = monitor.size().width as f64 / scale;
        let x = (screen_w - w as f64) / 2.0;
        let y = 8.0; // small gap from top edge
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(w as f64, h as f64)));
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
    }
}

/// Apply the rounded region, accounting for DPI scale factor.
pub fn apply_shape(window: &WebviewWindow, w: u32, h: u32, radius: i32) {
    #[cfg(windows)]
    {
        if let Some(hwnd) = get_hwnd(window) {
            let scale = window.scale_factor().unwrap_or(1.0);
            let pw = (w as f64 * scale) as i32;
            let ph = (h as f64 * scale) as i32;
            let pr = (radius as f64 * scale) as i32;
            apply_region(hwnd, pw, ph, pr);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (w, h, radius);
    }
}

// ---------------------------------------------------------------------------
// Pill width animation (idle ↔ active)
// ---------------------------------------------------------------------------

/// Animate pill width with spring overshoot (idle ↔ active).
///
/// Called from a blocking context. Updates `PILL_TARGET_W`, then plays a
/// 6-frame spring animation (~150 ms) that overshoots the target by ~15%
/// before settling. Skips if panel is expanded or another animation is
/// already running.
pub fn set_pill_active(window: &WebviewWindow, active: bool, pill_w: u32, pill_w_active: u32) {
    let target = if active { pill_w_active } else { pill_w };
    let prev = PILL_TARGET_W.swap(target, Ordering::SeqCst);
    if prev == target {
        return;
    }

    // Don't animate if panel is showing — just store the target for next collapse
    if ISLAND_EXPANDED.load(Ordering::SeqCst) {
        return;
    }

    // Skip if already animating (target is stored, next cycle will catch up)
    if PILL_ANIMATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let diff = target as f64 - prev as f64;
    for &f in &SPRING_FRAMES {
        let w = (prev as f64 + diff * f).round() as u32;
        position_top_center(window, w, PILL_H);
        apply_shape(window, w, PILL_H, PILL_RADIUS);
        std::thread::sleep(Duration::from_millis(FRAME_MS));
    }

    PILL_ANIMATING.store(false, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Expand / Collapse with morph animation
// ---------------------------------------------------------------------------

/// Expand from pill to full panel with spring animation.
///
/// Animates width, height, and corner radius from pill → panel over ~200ms.
/// If another morph animation is running, jumps directly to the final state.
pub fn expand(window: &WebviewWindow, panel_w: u32, panel_h: u32) {
    ISLAND_EXPANDED.store(true, Ordering::SeqCst);
    EXPANDED_W.store(panel_w, Ordering::SeqCst);
    EXPANDED_H.store(panel_h, Ordering::SeqCst);

    // If another morph is running, skip animation — just jump to final state
    if MORPH_ANIMATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        position_top_center(window, panel_w, panel_h);
        apply_shape(window, panel_w, panel_h, PANEL_RADIUS);
        return;
    }

    let start_w = PILL_TARGET_W.load(Ordering::SeqCst);
    let start_h = PILL_H;
    let start_r = PILL_RADIUS;

    for &t in &EXPAND_CURVE {
        let w = lerp_u32(start_w, panel_w, t);
        let h = lerp_u32(start_h, panel_h, t);
        let r = lerp_i32(start_r, PANEL_RADIUS, t);
        position_top_center(window, w, h);
        apply_shape(window, w, h, r);
        std::thread::sleep(Duration::from_millis(MORPH_FRAME_MS));
    }

    MORPH_ANIMATING.store(false, Ordering::SeqCst);
}

/// Collapse from panel to pill with ease-out animation.
///
/// Animates from stored expanded dimensions back to pill. If not currently
/// expanded (e.g. initial setup), sets pill shape directly without animation.
pub fn collapse(window: &WebviewWindow) {
    let was_expanded = ISLAND_EXPANDED.swap(false, Ordering::SeqCst);
    let target_w = PILL_TARGET_W.load(Ordering::SeqCst);

    if !was_expanded {
        // Not expanded — just set pill shape directly (e.g. initial setup)
        position_top_center(window, target_w, PILL_H);
        apply_shape(window, target_w, PILL_H, PILL_RADIUS);
        return;
    }

    // If another morph is running, skip animation — just jump to final state
    if MORPH_ANIMATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        position_top_center(window, target_w, PILL_H);
        apply_shape(window, target_w, PILL_H, PILL_RADIUS);
        return;
    }

    let start_w = EXPANDED_W.load(Ordering::SeqCst);
    let start_h = EXPANDED_H.load(Ordering::SeqCst);
    let start_r = PANEL_RADIUS;

    for &t in &COLLAPSE_CURVE {
        let w = lerp_u32(start_w, target_w, t);
        let h = lerp_u32(start_h, PILL_H, t);
        let r = lerp_i32(start_r, PILL_RADIUS, t);
        position_top_center(window, w, h);
        apply_shape(window, w, h, r);
        std::thread::sleep(Duration::from_millis(MORPH_FRAME_MS));
    }

    MORPH_ANIMATING.store(false, Ordering::SeqCst);
}

/// Toggle island visibility (used by global hotkey and API).
pub fn toggle_visibility(window: &WebviewWindow) {
    let visible = window.is_visible().unwrap_or(true);
    if visible {
        let _ = window.eval("if(typeof hideIsland==='function')hideIsland()");
        let _ = window.hide();
    } else {
        let _ = window.show();
        collapse(window);
    }
}

/// Initial setup: store configured pill width + set pill shape (no animation).
pub fn setup(window: &WebviewWindow, pill_w: u32) {
    PILL_TARGET_W.store(pill_w, Ordering::SeqCst);
    // Direct shape set — collapse() would skip animation anyway since !was_expanded
    collapse(window);
}
