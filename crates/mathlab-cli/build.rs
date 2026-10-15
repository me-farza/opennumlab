// SPDX-License-Identifier: GPL-3.0-or-later
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read source directory") {
            collect(&entry.expect("source entry").path(), files);
        }
    } else if path.extension().is_some_and(|e| e == "rs" || e == "toml") {
        files.push(path.to_owned());
    }
}

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut files = vec![
        root.join("Cargo.lock"),
        root.join("Cargo.toml"),
        root.join("octave/mathlab_worker.m"),
        root.join("octave/mathlab_value_page.m"),
        root.join("octave/mathlab_value_edit.m"),
        root.join("octave/mathlab_ready_json.m"),
    ];
    for name in [
        "mathlab-cli",
        "mathlab-engine",
        "mathlab-language",
        "mathlab-debugger",
    ] {
        let base = root.join("crates").join(name);
        files.push(base.join("Cargo.toml"));
        collect(&base.join("src"), &mut files);
        if base.join("build.rs").exists() {
            files.push(base.join("build.rs"));
        }
        if base.join("helpers").exists() {
            collect(&base.join("helpers"), &mut files);
        }
    }
    files.sort();
    files.dedup();
    let mut digest = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        digest.update(
            path.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .as_bytes(),
        );
        digest.update([0]);
        let bytes = fs::read(&path).expect("read build source");
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    println!(
        "cargo:rustc-env=MATHLAB_BUILD_SOURCE_SHA256={:x}",
        digest.finalize()
    );
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    println!(
        "cargo:rustc-env=MATHLAB_BUILD_REVISION={}",
        git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unavailable".into())
    );
    for reference in [Some("HEAD".into()), git(&["symbolic-ref", "HEAD"])]
        .into_iter()
        .flatten()
    {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    println!(
        "cargo:rustc-env=MATHLAB_BUILD_TARGET={}",
        env::var("TARGET").unwrap()
    );
}
