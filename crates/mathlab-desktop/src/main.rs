use clap::Parser;
use mathlab_desktop::{launch, DesktopOptions, RUN_SECTION_SHORTCUT};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mathlab-desktop",
    about = "OpenNumLab native Rust scientific desktop",
    after_help = format!("Project recovery defaults to .mathlab/session.json in the startup directory. Screenshot runs skip recovery unless --state is supplied.\n\nF5: run/continue · F12: toggle breakpoint · F10: step · F11: step in · Shift+F11: step out · Shift+F5: quit debugging\n{RUN_SECTION_SHORTCUT}: run section · F9: selection · Ctrl+S: save · Ctrl+O: open · Ctrl+C: interrupt · Ctrl+F: find · Ctrl+G: go to line")
)]
struct Arguments {
    #[arg(long = "cwd", value_name = "DIRECTORY")]
    directory: Option<PathBuf>,
    #[arg(long, value_name = "EXECUTABLE", conflicts_with = "startup_config")]
    octave: Option<PathBuf>,
    #[arg(
        long,
        value_name = "JSON",
        conflicts_with_all = ["octave", "debugger", "packaged_startup"],
        help = "Use an explicit pinned qualified startup configuration"
    )]
    startup_config: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with_all = ["startup_config", "octave", "debugger"],
        help = "Use qualified startup from the running installed app bundle"
    )]
    packaged_startup: bool,
    #[arg(long, value_name = "FILE.m", conflicts_with = "file", group = "source")]
    open: Option<PathBuf>,
    #[arg(value_name = "FILE.m", group = "source")]
    file: Option<PathBuf>,
    #[arg(long, help = "Run the startup script after connecting the engine")]
    run: bool,
    #[arg(
        long,
        value_name = "VARIABLE",
        requires = "run",
        help = "Open a variable created by the startup script in Variable Editor"
    )]
    inspect: Option<String>,
    #[arg(
        long,
        value_name = "FILE.png",
        help = "Capture the rendered desktop after startup, then exit"
    )]
    screenshot: Option<PathBuf>,
    #[arg(
        long,
        value_name = "SESSION.json",
        help = "Override the project-local session recovery file"
    )]
    state: Option<PathBuf>,
    #[arg(
        long,
        value_name = "BRIDGE.oct",
        help = "Enable the optional native Octave debugger adapter"
    )]
    debugger: Option<PathBuf>,
    #[arg(long = "break-at", value_name = "LINE", value_parser = positive_line, requires = "run", help = "Set a breakpoint in the startup editor buffer before running")]
    break_at: Option<usize>,
    #[arg(long = "error-target", value_name = "FRAME", value_parser = positive_line, requires = "run", help = "Navigate to a user source frame after the startup script fails (one based)")]
    error_target: Option<usize>,
}

fn positive_line(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|&line| line > 0)
        .ok_or_else(|| "Expected a positive integer.".into())
}

fn main() -> eframe::Result<()> {
    let args = Arguments::parse();
    launch(DesktopOptions {
        directory: args
            .directory
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        executable: args.octave,
        startup_config: args.startup_config,
        packaged_startup: args.packaged_startup,
        initial_file: args.open.or(args.file),
        run_on_start: args.run,
        screenshot: args.screenshot,
        state_file: args.state,
        initial_variable: args.inspect,
        debugger_bridge: args.debugger,
        initial_breakpoint: args.break_at,
        initial_error_target: args.error_target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_startup_requires_explicit_config_and_rejects_runtime_overrides() {
        let ordinary = Arguments::try_parse_from(["mathlab-desktop"]).unwrap();
        assert!(ordinary.startup_config.is_none());
        let selected =
            Arguments::try_parse_from(["mathlab-desktop", "--startup-config", "qualified.json"])
                .unwrap();
        assert_eq!(
            selected.startup_config,
            Some(PathBuf::from("qualified.json"))
        );
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--startup-config"]).is_err());
        for flag in ["--octave", "--debugger"] {
            assert!(Arguments::try_parse_from([
                "mathlab-desktop",
                "--startup-config",
                "qualified.json",
                flag,
                "override",
            ])
            .is_err());
        }
    }

    #[test]
    fn packaged_startup_is_boolean_and_rejects_competing_selectors_and_overrides() {
        let selected =
            Arguments::try_parse_from(["mathlab-desktop", "--packaged-startup"]).unwrap();
        assert!(selected.packaged_startup);
        assert!(selected.startup_config.is_none());
        assert!(
            !Arguments::try_parse_from(["mathlab-desktop"])
                .unwrap()
                .packaged_startup
        );
        for flag in ["--startup-config", "--octave", "--debugger"] {
            assert!(Arguments::try_parse_from([
                "mathlab-desktop",
                "--packaged-startup",
                flag,
                "override"
            ])
            .is_err());
        }
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--packaged-startup=true"]).is_err());
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--startup-config="]).is_err());
    }

    #[test]
    fn command_line_rejects_missing_values_and_ambiguous_files() {
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--state"]).is_err());
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--inspect", "A"]).is_err());
        assert!(
            Arguments::try_parse_from(["mathlab-desktop", "one.m", "--open", "two.m"]).is_err()
        );
        let args =
            Arguments::try_parse_from(["mathlab-desktop", "one.m", "--run", "--inspect", "A"])
                .unwrap();
        assert!(args.run);
        assert_eq!(args.inspect.as_deref(), Some("A"));
        assert!(Arguments::try_parse_from([
            "mathlab-desktop",
            "--open",
            "proof.m",
            "--run",
            "--break-at",
            "0"
        ])
        .is_err());
        assert!(Arguments::try_parse_from(["mathlab-desktop", "--break-at", "2"]).is_err());
        let args = Arguments::try_parse_from([
            "mathlab-desktop",
            "--open",
            "proof.m",
            "--run",
            "--break-at",
            "2",
        ])
        .unwrap();
        assert_eq!(args.break_at, Some(2));
        let recovered = Arguments::try_parse_from([
            "mathlab-desktop",
            "--state",
            "session.json",
            "--run",
            "--break-at",
            "4",
        ])
        .unwrap();
        assert_eq!(recovered.break_at, Some(4));
    }
}
