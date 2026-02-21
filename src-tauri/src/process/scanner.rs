use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub agent_type: String,
    pub cwd: String,
    pub uptime: u64,
    pub create_time: f64,
}

pub struct ProcessScanner {
    /// UTF-16 encoded process names for direct comparison (no heap alloc for non-matches).
    #[cfg(windows)]
    process_names_u16: Vec<Vec<u16>>,
    #[cfg(windows)]
    exclude_names_u16: Vec<Vec<u16>>,
    #[allow(dead_code)]
    process_names: Vec<String>,
    agent_type: String,
}

impl ProcessScanner {
    pub fn new(agent_type: &str, process_names: &[&str], exclude_names: &[&str]) -> Self {
        Self {
            #[cfg(windows)]
            process_names_u16: process_names
                .iter()
                .map(|s| s.to_lowercase().encode_utf16().collect())
                .collect(),
            #[cfg(windows)]
            exclude_names_u16: exclude_names
                .iter()
                .map(|s| s.to_lowercase().encode_utf16().collect())
                .collect(),
            process_names: process_names.iter().map(|s| s.to_lowercase()).collect(),
            agent_type: agent_type.to_string(),
        }
    }

    /// Scan for processes using Win32 Toolhelp32 API.
    pub fn scan(&mut self) -> Vec<ProcessInfo> {
        #[cfg(windows)]
        {
            self.scan_windows()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    #[cfg(windows)]
    fn scan_windows(&mut self) -> Vec<ProcessInfo> {
        use windows::Win32::System::Diagnostics::ToolHelp::*;
        use windows::Win32::Foundation::*;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut results = Vec::new();

        unsafe {
            let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(h) => h,
                Err(_) => return results,
            };

            let mut entry = PROCESSENTRY32W::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;

                    // UTF-16 direct comparison — no String allocation for non-matching processes
                    let name_len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name_slice = &entry.szExeFile[..name_len];

                    if self.is_target_process(name_slice) {
                        let name = String::from_utf16_lossy(name_slice);
                        let (cwd, create_time) = Self::query_process(pid, now);
                        let uptime = (now - create_time) as u64;

                        results.push(ProcessInfo {
                            pid,
                            name,
                            agent_type: self.agent_type.clone(),
                            cwd,
                            uptime,
                            create_time,
                        });
                    }

                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
        }

        results
    }

    /// Check if a process name (UTF-16) matches our target list.
    /// Compares lowercased UTF-16 directly — no heap allocation.
    #[cfg(windows)]
    fn is_target_process(&self, name_u16: &[u16]) -> bool {
        // ASCII-range lowercase on stack (exe names are always < 260 chars)
        let mut lower_buf = [0u16; 260];
        let len = name_u16.len().min(260);
        for i in 0..len {
            lower_buf[i] = if name_u16[i] >= b'A' as u16 && name_u16[i] <= b'Z' as u16 {
                name_u16[i] + 32
            } else {
                name_u16[i]
            };
        }
        let lower = &lower_buf[..len];

        // Check excludes first
        for excl in &self.exclude_names_u16 {
            if lower == excl.as_slice() {
                return false;
            }
        }
        // Check includes
        for target in &self.process_names_u16 {
            if lower == target.as_slice() {
                return true;
            }
        }
        false
    }

    /// Open process once, query CWD + create time, close once.
    #[cfg(windows)]
    fn query_process(pid: u32, now: f64) -> (String, f64) {
        use windows::Win32::Foundation::*;
        use windows::Win32::System::Threading::*;

        unsafe {
            let handle = match OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            ) {
                Ok(h) => h,
                Err(_) => return (String::new(), now),
            };

            // CWD (exe directory as approximation)
            let cwd = {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    windows::core::PWSTR(buf.as_mut_ptr()),
                    &mut len,
                )
                .is_ok()
                    && len > 0
                {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    if let Some(pos) = path.rfind('\\') {
                        path[..pos].to_string()
                    } else {
                        path
                    }
                } else {
                    String::new()
                }
            };

            // Create time
            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();

            let create_time = if GetProcessTimes(
                handle,
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
            .is_ok()
            {
                let ft =
                    ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
                let unix_offset: u64 = 116_444_736_000_000_000;
                if ft > unix_offset {
                    (ft - unix_offset) as f64 / 10_000_000.0
                } else {
                    now
                }
            } else {
                now
            };

            let _ = CloseHandle(handle);
            (cwd, create_time)
        }
    }
}
