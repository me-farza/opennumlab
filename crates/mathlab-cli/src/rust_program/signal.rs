// SPDX-License-Identifier: GPL-3.0-or-later
//! Only the opt-in one-shot program route changes the process signal disposition.
use std::io;
#[cfg(unix)]
mod platform {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    static INTERRUPTED: AtomicBool = AtomicBool::new(false);
    extern "C" fn interrupt(_: libc::c_int) {
        INTERRUPTED.store(true, Ordering::Release);
    }
    pub(super) struct Guard {
        previous: libc::sigaction,
    }
    impl Guard {
        pub(super) fn install() -> io::Result<Self> {
            INTERRUPTED.store(false, Ordering::Release);
            // Both structures are C signal-action records; zero initialization supplies
            // valid scalar/pointer fields and sigemptyset initializes the signal mask.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = interrupt as *const () as usize;
            if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::sigaction(libc::SIGINT, &action, &mut previous) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { previous })
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            // This one-shot process exits after the guard drops; restoration never calls user code.
            unsafe {
                libc::sigaction(libc::SIGINT, &self.previous, std::ptr::null_mut());
            }
        }
    }
    pub(super) fn requested() -> bool {
        INTERRUPTED.load(Ordering::Acquire)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn disposition_is_restored() {
            let mut before: libc::sigaction = unsafe { std::mem::zeroed() };
            let mut after: libc::sigaction = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut before) },
                0
            );
            {
                let _guard = Guard::install().unwrap();
                assert!(!requested());
                interrupt(libc::SIGINT);
                assert!(requested());
            }
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut after) },
                0
            );
            assert_eq!(before.sa_sigaction, after.sa_sigaction);
            assert_eq!(before.sa_flags, after.sa_flags);
        }
    }
}
#[cfg(unix)]
pub(super) struct SignalGuard {
    _guard: platform::Guard,
}
#[cfg(unix)]
impl SignalGuard {
    pub(super) fn install() -> io::Result<Self> {
        Ok(Self {
            _guard: platform::Guard::install()?,
        })
    }
}
#[cfg(unix)]
pub(super) fn requested() -> bool {
    platform::requested()
}
#[cfg(not(unix))]
pub(super) struct SignalGuard;
#[cfg(not(unix))]
impl SignalGuard {
    pub(super) fn install() -> io::Result<Self> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
#[cfg(not(unix))]
pub(super) fn requested() -> bool {
    false
}
