// SPDX-License-Identifier: GPL-3.0-or-later
// A standalone target executable, intentionally restricted to Rust's standard
// library. It emits one short console fence through its inherited stdout handle.

use std::io;
use std::process::ExitCode;

const PREFIX: &[u8] = b"\x1eMATHLAB:";
const SUFFIX: u8 = 0x1f;
// POSIX guarantees atomic writes of at least 512 bytes to a pipe. Never split a
// fence into several writes, including when an OS reports a short write.
const MAX_FENCE_BYTES: usize = 512;

fn valid_fence(fence: &str) -> bool {
    let bytes = fence.as_bytes();
    if bytes.len() > MAX_FENCE_BYTES
        || bytes.len() <= PREFIX.len() + 1
        || !bytes.starts_with(PREFIX)
        || bytes.last() != Some(&SUFFIX)
    {
        return false;
    }
    // The interior cannot contain another framing delimiter or a line break.
    // Keep validation on the UTF-8 string so non-ASCII text cannot be split.
    !fence[PREFIX.len()..fence.len() - 1]
        .chars()
        .any(char::is_control)
}

#[cfg(unix)]
fn publish(fence: &[u8]) -> io::Result<()> {
    use std::ffi::c_void;

    unsafe extern "C" {
        fn write(fd: i32, buffer: *const c_void, length: usize) -> isize;
    }

    loop {
        // SAFETY: the byte slice remains valid for the call, and fd 1 is the
        // inherited console pipe. A <=512-byte write is within POSIX PIPE_BUF.
        let written = unsafe { write(1, fence.as_ptr().cast(), fence.len()) };
        if written == fence.len() as isize {
            return Ok(());
        }
        if written >= 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "short console fence write",
            ));
        }
        let error = io::Error::last_os_error();
        // EINTR means no bytes were written. A positive short write is never
        // retried, because doing so would lose the single-write fence guarantee.
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(windows)]
fn publish(fence: &[u8]) -> io::Result<()> {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetStdHandle"]
        fn get_std_handle(identifier: u32) -> Handle;
        #[link_name = "WriteFile"]
        fn write_file(
            handle: Handle,
            buffer: *const c_void,
            length: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    // SAFETY: GetStdHandle accepts the documented STD_OUTPUT_HANDLE constant.
    let handle = unsafe { get_std_handle((-11_i32) as u32) };
    if handle.is_null() || handle == usize::MAX as Handle {
        return Err(io::Error::last_os_error());
    }
    let mut written = 0_u32;
    // SAFETY: the input slice and output count remain valid during the one
    // synchronous call; a null OVERLAPPED requests synchronous completion.
    let success = unsafe {
        write_file(
            handle,
            fence.as_ptr().cast(),
            fence.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    if written as usize != fence.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "short console fence write",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn publish(_fence: &[u8]) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "console fence output requires Unix or Windows",
    ))
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os();
    arguments.next();
    let Some(fence) = arguments.next().and_then(|value| value.into_string().ok()) else {
        return ExitCode::from(64);
    };
    if arguments.next().is_some() || !valid_fence(&fence) {
        return ExitCode::from(64);
    }
    if publish(fence.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        // Never print a diagnostic into the protocol stream, including on error.
        ExitCode::from(74)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_one_bounded_utf8_fence() {
        assert!(valid_fence("\x1eMATHLAB:nonce:1:B:1:7\x1f"));
        assert!(valid_fence("\x1eMATHLAB:λ:1:E:1:7\x1f"));
        assert!(!valid_fence("MATHLAB:nonce:1:B:1:7\x1f"));
        assert!(!valid_fence("\x1eMATHLAB:\x1f"));
        assert!(!valid_fence("\x1eMATHLAB:x\x1fy\x1f"));
        assert!(!valid_fence("\x1eMATHLAB:x\ny\x1f"));
        let limit = format!("\x1eMATHLAB:{}\x1f", "x".repeat(502));
        assert_eq!(limit.len(), MAX_FENCE_BYTES);
        assert!(valid_fence(&limit));
        assert!(!valid_fence(&format!(
            "\x1eMATHLAB:{}\x1f",
            "x".repeat(503)
        )));
    }
}
