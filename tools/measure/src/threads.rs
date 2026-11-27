//! OS thread count of this process, to check that sequential styling spawns none.

/// Number of threads in this process, or `None` where the OS gives no cheap answer.
#[cfg(windows)]
pub fn count() -> Option<usize> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    // SAFETY: plain Win32 calls on a snapshot handle we own and close.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let pid = GetCurrentProcessId();
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut n = 0;
        let mut ok = Thread32First(snap, &mut entry);
        while ok != 0 {
            n += usize::from(entry.th32OwnerProcessID == pid);
            ok = Thread32Next(snap, &mut entry);
        }
        CloseHandle(snap);
        Some(n)
    }
}

#[cfg(target_os = "linux")]
pub fn count() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .and_then(|v| v.trim().parse().ok())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn count() -> Option<usize> {
    None
}
