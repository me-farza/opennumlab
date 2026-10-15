use clap::Parser;
use mathlab_desktop::{launch, DesktopOptions};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mathlab-desktop",
    about = "OpenNumLab native Rust scientific desktop",
    after_help = "Project recovery defaults to .mathlab/session.json in the startup directory. Screenshot runs skip recovery unless --state is supplied.\n\nF5: run/continue · F12: toggle breakpoint · F10: step · F11: step in · Shift+F11: step out · Shift+F5: quit debugging\nCtrl+Enter: run section · F9: selection · Ctrl+S: save · Ctrl+O: open · Ctrl+C: interrupt · Ctrl+F: find · Ctrl+G: go to line"
)]
struct Arguments {
    #[arg(long = "cwd", value_name = "DIRECTORY")]
    directory: Option<PathBuf>,
    #[arg(long, value_name = "EXECUTABLE")]
    octave: Option<PathBuf>,
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
