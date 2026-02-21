//! Dynamic Island window shape management.
//!
//! Uses Win32 `CreateRoundRectRgn` + `SetWindowRgn` to clip the Tauri window
//! into a pill / rounded-rect shape. Tauri `transparent: false` avoids the
//! WebView2 hit-test bug on Windows while still giving us custom shapes.

use tauri::WebviewWindow;

// Collapsed pill dimensions
pub const PILL_W: u32 = 300;
pub const PILL_H: u32 = 36;
pub const PILL_RADIUS: i32 = 18;

// Expanded panel dimensions
pub const PANEL_W: u32 = 480;
pub const PANEL_H: u32 = 320;
pub const PANEL_RADIUS: i32 = 16;

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

/// Collapse to small pill.
pub fn collapse(window: &WebviewWindow) {
    position_top_center(window, PILL_W, PILL_H);
    // Small delay so the resize completes before we apply the region
    std::thread::sleep(std::time::Duration::from_millis(30));
    apply_shape(window, PILL_W, PILL_H, PILL_RADIUS);
}

/// Expand to full panel.
pub fn expand(window: &WebviewWindow) {
    position_top_center(window, PANEL_W, PANEL_H);
    std::thread::sleep(std::time::Duration::from_millis(30));
    apply_shape(window, PANEL_W, PANEL_H, PANEL_RADIUS);
}

/// Initial setup: collapse to pill + position.
pub fn setup(window: &WebviewWindow) {
    collapse(window);
}
