use std::sync::atomic::AtomicBool;
#[cfg(target_family = "windows")]
use windows_sys::Win32::Foundation::TRUE;

pub static TO_CLOSE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_family = "unix")]
extern "C" fn handle_signal(sig: std::ffi::c_int) {
    if sig == libc::SIGHUP || sig == libc::SIGINT || sig == libc::SIGTERM {
        TO_CLOSE_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(target_family = "unix")]
pub fn register_signal(sig: std::ffi::c_int) -> Result<(), String> {
    let mut sigaction_struct: std::mem::MaybeUninit<libc::sigaction> =
        std::mem::MaybeUninit::zeroed();

    let signal_fn_usize: usize = handle_signal as *mut std::ffi::c_void as usize;

    unsafe {
        libc::sigemptyset(&mut (*sigaction_struct.as_mut_ptr()).sa_mask as *mut libc::sigset_t);
        (*sigaction_struct.as_mut_ptr()).sa_sigaction = signal_fn_usize;
        if libc::sigaction(sig, sigaction_struct.as_ptr(), std::ptr::null_mut()) == -1 {
            return Err(format!("Failed to register signal {}", sig));
        }
    }

    Ok(())
}

#[cfg(target_family = "windows")]
unsafe extern "system" fn handler_routine(ctrltype: u32) -> windows_sys::core::BOOL {
    if ctrltype == windows_sys::Win32::System::Console::CTRL_BREAK_EVENT
        || ctrltype == windows_sys::Win32::System::Console::CTRL_C_EVENT
        || ctrltype == windows_sys::Win32::System::Console::CTRL_CLOSE_EVENT
        || ctrltype == windows_sys::Win32::System::Console::CTRL_SHUTDOWN_EVENT
    {
        TO_CLOSE_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    TRUE
}

#[cfg(target_family = "windows")]
pub fn register_ctrl_handler() -> Result<(), String> {
    let ret = unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(handler_routine), TRUE)
    };
    if ret == 0 {
        Err(String::from(
            "Failed to register Win32 SetConsoleCtrlHandler (close request handling)!",
        ))
    } else {
        Ok(())
    }
}
