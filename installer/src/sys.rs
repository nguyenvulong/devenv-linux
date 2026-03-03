use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

pub struct CommandResult {
    pub success: bool,
    pub _stdout: String,
    pub stderr: String,
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> CommandResult {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute process: {} {:?}", cmd, e));

    CommandResult {
        success: output.status.success(),
        _stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

pub fn run_cmd_streaming<F>(cmd: &str, args: &[&str], mut log: F) -> CommandResult
where
    F: FnMut(&str) + Send + 'static,
{
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to execute process: {} {:?}", cmd, e));

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Use channels or just log synchronously from one standard output while the other is in a thread
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_err = tx.clone();
    
    // Read stdout
    let stdout_thread = thread::spawn(move || {
        let mut stdout_str = String::new();
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                stdout_str.push_str(&line);
                stdout_str.push('\n');
                let _ = tx.send(line);
            }
        }
        stdout_str
    });

    // Read stderr
    let stderr_thread = thread::spawn(move || {
        let mut stderr_str = String::new();
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                stderr_str.push_str(&line);
                stderr_str.push('\n');
                let _ = tx_err.send(line);
            }
        }
        stderr_str
    });

    // Stream lines out
    while let Ok(line) = rx.recv() {
        log(&line);
    }

    let status = child.wait().unwrap();
    
    CommandResult {
        success: status.success(),
        _stdout: stdout_thread.join().unwrap(),
        stderr: stderr_thread.join().unwrap(),
    }
}

/// Returns true if `cmd` exists as an executable file in any directory on PATH.
/// Does not call `which` — works on any distro, including minimal containers.
pub fn check_command_exists(cmd: &str) -> bool {
    // Also check the mise shims dir explicitly, since it may not be on PATH yet.
    let home = std::env::var("HOME").unwrap_or_default();
    let mise_shims = std::path::PathBuf::from(&home)
        .join(".local/share/mise/shims");

    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path_var).collect();
    dirs.push(mise_shims);

    dirs.iter().any(|dir| {
        let candidate = dir.join(cmd);
        candidate.is_file()
    })
}

pub fn get_distro() -> String {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    if os_release.contains("ID=debian") || os_release.contains("ID=ubuntu") || os_release.contains("ID_LIKE=debian") {
        "debian".to_string()
    } else if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
        "arch".to_string()
    } else if os_release.contains("ID=fedora") || os_release.contains("ID=centos") || os_release.contains("ID_LIKE=fedora") || os_release.contains("ID_LIKE=centos") {
        "redhat".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Returns the currently active version of a tool managed by mise by running `mise ls <tool>`.
/// Examples of output we parse:
/// "rust   1.81.0    ~/.config/mise/config.toml" -> "1.81.0"
/// "node   22.0.0    (missing)" -> "22.0.0" (Even if missing locally, it's what's configured)
pub fn get_mise_tool_version(tool: &str) -> Option<String> {
    if !check_command_exists("mise") {
        return None;
    }

    // Call `mise ls <tool>` and parse its stdout.
    // E.g., `mise ls rust` 
    let out = Command::new("mise")
        .args(["ls", tool])
        .output()
        .ok()?;

    if !out.status.success() {
        return None; // Tool might not be known to mise yet.
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    
    // The format is typically: <tool> <version> <source>
    // We want the first line that starts with the tool name, and extract the second column.
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // parts[0] is the tool name (or alias), parts[1] is the version
        if parts.len() >= 2 && parts[0] == tool {
            let version = parts[1];
            // Ignore placeholders like "(missing)" if they end up here
            if !version.starts_with('(') {
                return Some(version.to_string());
            }
        }
    }
    
    None
}

/// Runs a generic command with args (e.g. `["--version"]`) and tries to
/// extract a version-like string from the first line of output.
pub fn get_command_version(cmd: &str, args: &[&str]) -> Option<String> {
    if !check_command_exists(cmd) {
        return None;
    }

    let out = Command::new(cmd)
        .args(args)
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    // Look at both stdout and stderr (some tools print version to stderr, e.g., java)
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Grab the first non-empty line
    let first_line = combined.lines().find(|l| !l.trim().is_empty())?;

    // Very naive version extraction: extract the first contiguous block that starts with a digit
    // e.g. "rustc 1.70.0 (90c541806 2023-05-31)" -> "1.70.0"
    // e.g. "tmux 3.3a" -> "3.3a"
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    for part in parts {
        // Find a string that looks like a version (starts with a digit or 'v' + digit)
        let clean_part = part.trim_start_matches('v'); // handle "v1.2.3"
        if clean_part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            return Some(clean_part.to_string());
        }
    }

    // Fallback if we couldn't parse a number out of it, just return the first line up to 20 chars
    let fallback = first_line.trim();
    if fallback.len() > 20 {
        Some(format!("{}...", &fallback[..17]))
    } else {
        Some(fallback.to_string())
    }
}

