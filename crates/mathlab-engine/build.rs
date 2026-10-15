// SPDX-License-Identifier: GPL-3.0-or-later
// Compile the small, dependency-free target executable that publishes console
// fences with a single OS write. The engine embeds it in its own executable.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=helpers/console_fence.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=RUSTC_LINKER");

    let compiler = env::var_os("RUSTC").expect("Cargo must provide RUSTC");
    let target = env::var("TARGET").expect("Cargo must provide TARGET");
    let source = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    )
    .join("helpers/console_fence.rs");
    let windows = env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR")).join(
        if windows {
            "mathlab-console-fence.exe"
        } else {
            "mathlab-console-fence"
        },
    );

    let mut command = Command::new(compiler);
    command
        .arg(&source)
        .args([
            "--edition=2024",
            "--crate-name=mathlab_console_fence",
            "--crate-type=bin",
            "--target",
            &target,
            "-C",
            "opt-level=2",
            "-C",
            "panic=abort",
            "-C",
            "strip=debuginfo",
            "-o",
        ])
        .arg(&output);
    // Cargo supplies this when a target-specific linker is configured. Building
    // the helper for TARGET, rather than HOST, also supports cross compilation.
    if let Some(linker) = env::var_os("RUSTC_LINKER") {
        let mut option = std::ffi::OsString::from("linker=");
        option.push(linker);
        command.arg("-C").arg(option);
    }
    let status = command
        .status()
        .expect("could not start rustc for the console fence helper");
    assert!(status.success(), "console fence helper compilation failed");
    let output = output
        .canonicalize()
        .expect("compiled console fence helper is missing");
    println!(
        "cargo:rustc-env=MATHLAB_CONSOLE_FENCE_EXECUTABLE={}",
        output
            .to_str()
            .expect("console fence helper output path must be UTF-8")
    );
}
