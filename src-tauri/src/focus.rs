/// Win32: find and focus terminal windows via process-tree tracing.
use crate::process::ProcessInfo;

/// Known terminal process names (lowercase).
const TERMINAL_PROCESSES: &[&str] = &[
    "windowsterminal.exe", "wt.exe",
    "cmd.exe", "powershell.exe", "pwsh.exe",
    "mintty.exe", "conhost.exe",
    "warp.exe", "alacritty.exe", "hyper.exe",
    "wezterm-gui.exe", "kitty.exe", "tabby.exe",
    "code.exe", // VS Code integrated terminal
];

/// Never match these.
const NON_TERMINAL_PROCESSES: &[&str] = &[
    "explorer.exe", "searchhost.exe", "shellexperiencehost.exe",
    "searchui.exe", "startmenuexperiencehost.exe",
];

pub fn find_and_focus_terminal_with_pid(cwd: &str, cached_processes: &[ProcessInfo], pid: Option<u32>) -> bool {
    #[cfg(windows)]
    {
        // Single Toolhelp32 snapshot for all process-tree walks
        let snapshot = ProcessSnapshot::capture();

        if !cwd.is_empty() {
            // Strategy 1: for each agent process, walk up to terminal, check title matches CWD
            if let Some(hwnd) = find_terminal_for_cwd(cwd, cached_processes, &snapshot) {
                return focus_hwnd(hwnd);
            }

            // Strategy 2: title matching (fallback, scans all windows)
            if let Some(hwnd) = find_terminal_by_title(cwd) {
                return focus_hwnd(hwnd);
            }
        }

        // Strategy 3: direct PID walk (if caller provides a known-good PID)
        if let Some(p) = pid {
            if let Some(hwnd) = walk_to_terminal(&snapshot, p) {
                return focus_hwnd(hwnd);
            }
        }
    }

    let _ = (cwd, cached_processes, pid);
    false
}

/// Cached Toolhelp32 process snapshot — avoids creating one per walk level.
#[cfg(windows)]
struct ProcessSnapshot {
    /// (pid, parent_pid, exe_name)
    entries: Vec<(u32, u32, String)>,
}

#[cfg(windows)]
impl ProcessSnapshot {
    fn capture() -> Self {
        use windows::Win32::System::Diagnostics::ToolHelp::*;
        use windows::Win32::Foundation::CloseHandle;

        let mut entries = Vec::new();
        unsafe {
            let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(h) => h,
                Err(_) => return Self { entries },
            };
            let mut entry = PROCESSENTRY32W::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())]
                    );
                    entries.push((entry.th32ProcessID, entry.th32ParentProcessID, name));
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        Self { entries }
    }

    /// Get parent PID and parent's exe name for a given PID.
    fn get_parent_info(&self, pid: u32) -> Option<(u32, &str)> {
        let parent_pid = self.entries.iter()
            .find(|(p, _, _)| *p == pid)
            .map(|(_, pp, _)| *pp)
            .filter(|&pp| pp != 0 && pp != pid)?;
        let parent_name = self.entries.iter()
            .find(|(p, _, _)| *p == parent_pid)
            .map(|(_, _, n)| n.as_str())
            .unwrap_or("");
        Some((parent_pid, parent_name))
    }
}

/// For each agent process, walk up to find its terminal window,
/// then check if the terminal's title contains the target CWD.
#[cfg(windows)]
fn find_terminal_for_cwd(cwd: &str, cached: &[ProcessInfo], snapshot: &ProcessSnapshot) -> Option<isize> {
    let cwd_lower = cwd.replace('/', "\\").to_lowercase();
    let cwd_fwd = cwd.replace('\\', "/").to_lowercase();
    let basename = cwd.rsplit(&['/', '\\']).next().unwrap_or("").to_lowercase();
    let variants = vec![cwd_lower.clone(), cwd_fwd, basename.clone()];

    for proc in cached {
        if let Some(hwnd) = walk_to_terminal(snapshot, proc.pid) {
            // Got the terminal window — check if its title contains the CWD
            let title = get_window_title(hwnd);
            let title_lower = title.to_lowercase();
            if variants.iter().any(|v| !v.is_empty() && title_lower.contains(v.as_str())) {
                tracing::debug!("find_terminal_for_cwd: PID {} → terminal '{}' matches cwd '{}'",
                    proc.pid, title, cwd);
                return Some(hwnd);
            }
        }
    }
    None
}

#[cfg(windows)]
fn get_window_title(hwnd: isize) -> String {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::HWND;
    unsafe {
        let h = HWND(hwnd as *mut _);
        let len = GetWindowTextLengthW(h);
        if len <= 0 { return String::new(); }
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(h, &mut buf);
        String::from_utf16_lossy(&buf)
    }
}

#[cfg(windows)]
fn walk_to_terminal(snapshot: &ProcessSnapshot, pid: u32) -> Option<isize> {
    let mut current_pid = pid;
    tracing::debug!("walk_to_terminal: starting from PID {}", pid);

    for level in 0..6 {
        let (parent_pid, parent_name) = snapshot.get_parent_info(current_pid)?;
        let parent_lower = parent_name.to_lowercase();
        tracing::debug!("  level {}: PID {} → parent PID {} ({})", level, current_pid, parent_pid, parent_name);

        if TERMINAL_PROCESSES.contains(&parent_lower.as_str()) {
            if let Some(hwnd) = find_window_for_pid(parent_pid) {
                tracing::debug!("  → found terminal window hwnd={} for {} (PID {})", hwnd, parent_name, parent_pid);

                // Windows Terminal: switch to the correct tab
                // current_pid is the direct child of WT (the per-tab shell)
                if parent_lower == "windowsterminal.exe" || parent_lower == "wt.exe" {
                    switch_wt_tab(parent_pid, current_pid);
                }
                return Some(hwnd);
            }
            // Shell process inside WT — no visible window, keep walking
            tracing::debug!("  → {} (PID {}) is terminal but has no visible window", parent_name, parent_pid);
        }
        current_pid = parent_pid;
    }
    tracing::debug!("  → no terminal found after 6 levels");
    None
}

/// Switch Windows Terminal to the tab containing `target_shell_pid`.
///
/// Strategy: enumerate WT's direct child processes (the per-tab shells),
/// sort by creation time (approximates tab order), find the index of
/// `target_shell_pid`, and run `wt.exe -w 0 focus-tab -t <index>`.
#[cfg(windows)]
fn switch_wt_tab(wt_pid: u32, target_shell_pid: u32) {
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::Foundation::CloseHandle;

    // 1. Find all direct children of WT that are known shell processes
    let mut children: Vec<(u32, u64)> = Vec::new(); // (pid, create_time)
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return,
        };
        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ParentProcessID == wt_pid {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())]
                    ).to_lowercase();
                    // Only count shell processes (each WT tab has one)
                    let is_shell = matches!(name.as_str(),
                        "powershell.exe" | "pwsh.exe" | "cmd.exe" | "bash.exe"
                        | "wsl.exe" | "ubuntu.exe" | "git-bash.exe" | "nu.exe"
                        | "fish.exe" | "zsh.exe"
                    );
                    if is_shell {
                        let ctime = get_process_create_time(entry.th32ProcessID);
                        children.push((entry.th32ProcessID, ctime));
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    // 2. Sort by creation time (tab order)
    children.sort_by_key(|(_, t)| *t);

    tracing::debug!("switch_wt_tab: WT PID={}, target shell PID={}, children={:?}", wt_pid, target_shell_pid, children);

    // 3. Find the index of target_shell_pid
    let tab_index = children.iter().position(|(pid, _)| *pid == target_shell_pid);

    if let Some(idx) = tab_index {
        tracing::debug!("  → switching to tab index {} via wt.exe", idx);
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("wt.exe")
                .args(["-w", "0", "focus-tab", "-t", &idx.to_string()])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn();
        }
    } else {
        tracing::debug!("  → target shell PID {} not found in WT children", target_shell_pid);
    }
}

/// Get process creation time (FILETIME as u64) for sorting.
#[cfg(windows)]
fn get_process_create_time(pid: u32) -> u64 {
    use windows::Win32::System::Threading::*;
    use windows::Win32::Foundation::*;

    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return 0,
        };
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        let _ = CloseHandle(handle);
        if ok.is_ok() {
            ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64)
        } else {
            0
        }
    }
}

#[cfg(windows)]
fn find_window_for_pid(target_pid: u32) -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let mut hwnd = match GetTopWindow(None) {
            Ok(h) => h,
            Err(_) => return None,
        };

        loop {
            if IsWindowVisible(hwnd).as_bool() {
                let mut wnd_pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut wnd_pid));
                if wnd_pid == target_pid {
                    let text_len = GetWindowTextLengthW(hwnd);
                    if text_len > 0 {
                        return Some(hwnd.0 as isize);
                    }
                }
            }
            hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                Ok(h) => h,
                Err(_) => break,
            };
        }
    }

    None
}

#[cfg(windows)]
fn find_terminal_by_title(cwd: &str) -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::*;

    let cwd_lower = cwd.replace('/', "\\").to_lowercase();
    let cwd_fwd = cwd.replace('\\', "/").to_lowercase();
    let basename = cwd.rsplit(&['/', '\\']).next().unwrap_or("").to_lowercase();

    let variants = vec![cwd_lower, cwd_fwd, basename];

    let mut best: Option<(i32, isize)> = None; // (priority, hwnd)

    unsafe {
        let mut hwnd = match GetTopWindow(None) {
            Ok(h) => h,
            Err(_) => return None,
        };

        loop {
            if IsWindowVisible(hwnd).as_bool() {
                let text_len = GetWindowTextLengthW(hwnd);
                if text_len > 0 {
                    let mut buf = vec![0u16; text_len as usize + 1];
                    GetWindowTextW(hwnd, &mut buf);
                    let title = String::from_utf16_lossy(&buf).to_lowercase();

                    if variants.iter().any(|v| !v.is_empty() && title.contains(v.as_str())) {
                        let proc_name = get_window_process_name(hwnd);
                        let proc_lower = proc_name.to_lowercase();

                        if !NON_TERMINAL_PROCESSES.contains(&proc_lower.as_str()) {
                            let priority = if TERMINAL_PROCESSES.contains(&proc_lower.as_str()) { 0 } else { 1 };
                            match &best {
                                Some((p, _)) if *p <= priority => {}
                                _ => { best = Some((priority, hwnd.0 as isize)); }
                            }
                        }
                    }
                }
            }
            hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                Ok(h) => h,
                Err(_) => break,
            };
        }
    }

    best.map(|(_, hwnd)| hwnd)
}

#[cfg(windows)]
fn get_window_process_name(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::System::ProcessStatus::*;
    use windows::Win32::Foundation::CloseHandle;

    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 { return String::new(); }

        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };

        let mut buf = [0u16; 260];
        let len = GetModuleBaseNameW(handle, None, &mut buf);
        let _ = CloseHandle(handle);

        if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        }
    }
}

#[cfg(windows)]
fn focus_hwnd(hwnd: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::Foundation::HWND;

    unsafe {
        let h = HWND(hwnd as *mut _);
        // Alt-key trick to allow SetForegroundWindow
        keybd_event(0x12, 0, KEYBD_EVENT_FLAGS(0), 0);       // VK_MENU down
        let _ = ShowWindow(h, SW_RESTORE);
        let _ = SetForegroundWindow(h);
        keybd_event(0x12, 0, KEYBD_EVENT_FLAGS(2), 0);       // VK_MENU up
        true
    }
}
