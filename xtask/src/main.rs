use std::{
    path::Path,
    process::{Command, ExitCode},
};
fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let Some(command) = args.first().and_then(|value| value.to_str()) else {
        eprintln!(
            "Usage: cargo xtask check | package [--profile dev|release] | release --check DIR"
        );
        return ExitCode::FAILURE;
    };
    let mut child = match command {
        "check" => {
            let mut child = Command::new("powershell");
            child.args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "tools/check-yueshot.ps1",
            ]);
            child
        }
        "package" | "release" => {
            let mut child = Command::new("python");
            child.args(["tools/yueshot-delivery.py", command]);
            child
        }
        _ => {
            eprintln!("unknown task");
            return ExitCode::FAILURE;
        }
    };
    match child.current_dir(root).args(&args[1..]).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("task failed: {error}");
            ExitCode::FAILURE
        }
    }
}
