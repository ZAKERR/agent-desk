/// Win32: inject Unicode text into the focused window via SendInput, then press Enter.

/// Send `text` as Unicode keystrokes to the currently focused window, followed by Enter.
///
/// - Newlines are replaced with spaces (Enter submits in Claude Code).
/// - Long messages are chunked (100 chars) with 10ms delays to avoid buffer overflow.
/// - Surrogate pairs are handled for characters above U+FFFF (emoji, etc.).
#[cfg(windows)]
pub fn send_text_to_focused_window(text: &str) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let clean: String = text
        .replace('\r', "")
        .replace('\n', " ")
        .trim()
        .to_owned();
    if clean.is_empty() {
        return Err("empty message".into());
    }

    // Send text in chunks to avoid input buffer overflow
    const CHUNK: usize = 100;
    let chars: Vec<char> = clean.chars().collect();
    let multi = chars.len() > CHUNK;

    for chunk in chars.chunks(CHUNK) {
        let inputs = build_unicode_inputs(chunk);
        unsafe {
            let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
            if sent == 0 {
                return Err("SendInput failed".into());
            }
        }
        if multi {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    // Small pause before Enter so the terminal can process the text
    std::thread::sleep(std::time::Duration::from_millis(50));
    send_enter_key();
    Ok(())
}

#[cfg(not(windows))]
pub fn send_text_to_focused_window(_text: &str) -> Result<(), String> {
    Err("SendInput is only supported on Windows".into())
}

/// Build INPUT array: each UTF-16 code unit gets a key-down + key-up pair with KEYEVENTF_UNICODE.
#[cfg(windows)]
fn build_unicode_inputs(chars: &[char]) -> Vec<windows::Win32::UI::Input::KeyboardAndMouse::INPUT> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let mut inputs = Vec::with_capacity(chars.len() * 4);

    for &ch in chars {
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);

        for &code_unit in encoded.iter() {
            // Key down
            let mut ki_down = KEYBDINPUT::default();
            ki_down.wScan = code_unit;
            ki_down.dwFlags = KEYEVENTF_UNICODE;
            let mut inp_down = INPUT::default();
            inp_down.r#type = INPUT_KEYBOARD;
            inp_down.Anonymous.ki = ki_down;
            inputs.push(inp_down);

            // Key up
            let mut ki_up = KEYBDINPUT::default();
            ki_up.wScan = code_unit;
            ki_up.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
            let mut inp_up = INPUT::default();
            inp_up.r#type = INPUT_KEYBOARD;
            inp_up.Anonymous.ki = ki_up;
            inputs.push(inp_up);
        }
    }

    inputs
}

/// Press Enter (VK_RETURN) — down + up.
#[cfg(windows)]
fn send_enter_key() {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let mut ki_down = KEYBDINPUT::default();
    ki_down.wVk = VK_RETURN;
    let mut inp_down = INPUT::default();
    inp_down.r#type = INPUT_KEYBOARD;
    inp_down.Anonymous.ki = ki_down;

    let mut ki_up = KEYBDINPUT::default();
    ki_up.wVk = VK_RETURN;
    ki_up.dwFlags = KEYEVENTF_KEYUP;
    let mut inp_up = INPUT::default();
    inp_up.r#type = INPUT_KEYBOARD;
    inp_up.Anonymous.ki = ki_up;

    unsafe {
        SendInput(&[inp_down, inp_up], size_of::<INPUT>() as i32);
    }
}
