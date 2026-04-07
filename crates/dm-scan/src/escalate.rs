use crate::walker::{self, ScanPhase, ScanProgress};
use dm_core::model::TreeFragment;
use std::process::Command;

/// Error during privilege escalation.
#[derive(Debug)]
pub struct EscalateError {
    pub message: String,
}

impl std::fmt::Display for EscalateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "escalation error: {}", self.message)
    }
}

impl std::error::Error for EscalateError {}

/// Run an escalated scan of denied paths using pkexec (Linux).
///
/// Spawns the current binary with `--scan-paths` flag via `pkexec`,
/// which shows the system password dialog. Permission is NOT persisted.
///
/// The escalated process:
/// 1. Validates each path (canonicalize, must be dir, refuse system-critical)
/// 2. Scans only stat/readdir metadata (never file contents)
/// 3. Outputs JSON to stdout
/// 4. Exits immediately
pub fn escalated_scan_pkexec(
    paths: &[String],
    on_progress: impl Fn(ScanProgress),
) -> Result<Vec<TreeFragment>, EscalateError> {
    let current_exe = std::env::current_exe().map_err(|e| EscalateError {
        message: format!("cannot determine current executable: {}", e),
    })?;

    let mut cmd = Command::new("pkexec");
    cmd.arg(&current_exe);
    cmd.arg("--scan-paths");
    for path in paths {
        cmd.arg(path);
    }
    cmd.arg("--output-json");

    on_progress(ScanProgress {
        files_scanned: 0,
        dirs_scanned: 0,
        bytes_scanned: 0,
        current_path: String::new(),
        phase: ScanPhase::EscalatedScan,
    });

    let output = cmd.output().map_err(|e| EscalateError {
        message: format!("failed to run pkexec: {}", e),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EscalateError {
            message: if stderr.contains("dismissed")
                || stderr.contains("Not authorized")
                || output.status.code() == Some(126)
            {
                "authentication was cancelled by the user".into()
            } else {
                format!("pkexec failed: {}", stderr)
            },
        });
    }

    let json = String::from_utf8(output.stdout).map_err(|e| EscalateError {
        message: format!("invalid UTF-8 in scan output: {}", e),
    })?;

    let fragments: Vec<TreeFragment> =
        serde_json::from_str(&json).map_err(|e| EscalateError {
            message: format!("invalid JSON in scan output: {}", e),
        })?;

    Ok(fragments)
}

/// Handle the `--scan-paths` CLI mode.
///
/// This is called when the binary is invoked via pkexec with elevated privileges.
/// It scans the specified paths and outputs JSON to stdout.
pub fn handle_scan_paths_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    let mut output_json = false;

    let mut iter = args.iter().skip(1); // Skip binary name
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--scan-paths" => {}
            "--output-json" => output_json = true,
            other => {
                // Validate each path
                let path = std::path::Path::new(other);
                let canonical = path.canonicalize()?;

                if !canonical.is_dir() {
                    eprintln!("warning: skipping non-directory: {}", other);
                    continue;
                }

                let path_str = canonical.to_string_lossy();
                if matches!(
                    path_str.as_ref(),
                    "/proc" | "/sys" | "/dev" | "/run" | "/snap"
                ) {
                    eprintln!("warning: refusing system-critical path: {}", other);
                    continue;
                }

                paths.push(canonical.to_string_lossy().into_owned());
            }
        }
    }

    if !output_json {
        return Err("--output-json flag is required".into());
    }

    let fragments = walker::scan_denied_paths(&paths, |_| {})?;
    serde_json::to_writer(std::io::stdout(), &fragments)?;

    Ok(())
}
