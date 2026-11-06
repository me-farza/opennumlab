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
            // Establish a nontrivial disposition through libc, then query its canonical
            // representation. Linux/glibc may add SA_RESTORER on an install; the
            // inherited default disposition need not already contain that ABI flag.
            struct RestoreOriginal(libc::sigaction);
            impl Drop for RestoreOriginal {
                fn drop(&mut self) {
                    unsafe {
                        libc::sigaction(libc::SIGINT, &self.0, std::ptr::null_mut());
                    }
                }
            }
            extern "C" fn original_handler(_: libc::c_int) {}
            let mut original: libc::sigaction = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut original) },
                0
            );
            let _restore = RestoreOriginal(original);
            let mut baseline: libc::sigaction = unsafe { std::mem::zeroed() };
            baseline.sa_sigaction = original_handler as *const () as usize;
            baseline.sa_flags = libc::SA_RESTART;
            assert_eq!(unsafe { libc::sigemptyset(&mut baseline.sa_mask) }, 0);
            assert_eq!(unsafe { libc::sigaddset(&mut baseline.sa_mask, libc::SIGUSR1) }, 0);
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGINT, &baseline, std::ptr::null_mut()) },
                0
            );
            let mut before: libc::sigaction = unsafe { std::mem::zeroed() };
            let mut after: libc::sigaction = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut before) },
                0
            );
            assert_eq!(before.sa_sigaction, baseline.sa_sigaction);
            assert_ne!(before.sa_flags & libc::SA_RESTART, 0);
            assert_eq!(unsafe { libc::sigismember(&before.sa_mask, libc::SIGUSR1) }, 1);
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
            // Compare signal membership, not padding in libc's signal-set record.
            for signal in 1..=(std::mem::size_of::<libc::sigset_t>() * 8) {
                assert_eq!(
                    unsafe { libc::sigismember(&before.sa_mask, signal as libc::c_int) },
                    unsafe { libc::sigismember(&after.sa_mask, signal as libc::c_int) },
                    "restored signal-mask membership {signal}"
                );
            }
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
