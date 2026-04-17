use anyhow::{Context, Result, anyhow};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    Debian,
    Arch,
    RedHat,
    Unknown,
}

pub struct CommandResult {
    pub success: bool,
    pub _stdout: String,
    pub stderr: String,
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<CommandResult> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute process: {cmd} {args:?}"))?;

    Ok(CommandResult {
        success: output.status.success(),
        _stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn run_cmd_streaming<F>(cmd: &str, args: &[&str], mut log: F) -> Result<CommandResult>
where
    F: FnMut(&str) + Send + 'static,
{
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute process: {cmd} {args:?}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture stdout for {cmd}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to capture stderr for {cmd}"))?;

    let (tx, rx) = std::sync::mpsc::channel();
    let tx_err = tx.clone();

    let stdout_thread = thread::spawn(move || {
        let mut stdout_str = String::new();
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            stdout_str.push_str(&line);
            stdout_str.push('\n');
            let _ = tx.send(line);
        }
        stdout_str
    });

    let stderr_thread = thread::spawn(move || {
        let mut stderr_str = String::new();
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            stderr_str.push_str(&line);
            stderr_str.push('\n');
            let _ = tx_err.send(line);
        }
        stderr_str
    });

    while let Ok(line) = rx.recv() {
        log(&line);
    }

    let status = child
        .wait()
        .with_context(|| format!("failed waiting for process: {cmd} {args:?}"))?;

    Ok(CommandResult {
        success: status.success(),
        _stdout: stdout_thread
            .join()
            .map_err(|_| anyhow!("stdout thread panicked for {cmd}"))?,
        stderr: stderr_thread
            .join()
            .map_err(|_| anyhow!("stderr thread panicked for {cmd}"))?,
    })
}

pub fn check_command_exists(cmd: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let mise_shims = std::path::PathBuf::from(&home).join(".local/share/mise/shims");

    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path_var).collect();
    dirs.push(mise_shims);

    dirs.iter().any(|dir| dir.join(cmd).is_file())
}

pub fn get_distro() -> DistroFamily {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    parse_distro(&os_release)
}

pub fn get_mise_tool_version(tool: &str) -> Option<String> {
    if !check_command_exists("mise") {
        return None;
    }

    let out = Command::new("mise").args(["ls", tool]).output().ok()?;
    if !out.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_mise_tool_version(tool, &stdout)
}

pub fn get_command_version(cmd: &str, args: &[&str]) -> Option<String> {
    if !check_command_exists(cmd) {
        return None;
    }

    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    parse_command_version_output(&stdout, &stderr)
}

fn parse_distro(os_release: &str) -> DistroFamily {
    if os_release.contains("ID=debian")
        || os_release.contains("ID=ubuntu")
        || os_release.contains("ID_LIKE=debian")
    {
        DistroFamily::Debian
    } else if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
        DistroFamily::Arch
    } else if os_release.contains("ID=fedora")
        || os_release.contains("ID=centos")
        || os_release.contains("ID_LIKE=fedora")
        || os_release.contains("ID_LIKE=centos")
    {
        DistroFamily::RedHat
    } else {
        DistroFamily::Unknown
    }
}

fn parse_mise_tool_version(tool: &str, stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let tool_name = parts.next()?;
        let version = parts.next()?;
        if tool_name == tool && !version.starts_with('(') {
            return Some(version.to_string());
        }
    }

    None
}

fn parse_command_version_output(stdout: &str, stderr: &str) -> Option<String> {
    let combined = format!("{stdout}\n{stderr}");
    let first_line = combined.lines().find(|line| !line.trim().is_empty())?;

    for part in first_line.split_whitespace() {
        let clean_part = part.trim_start_matches('v');
        if clean_part
            .chars()
            .next()
            .is_some_and(|char| char.is_ascii_digit())
        {
            return Some(clean_part.to_string());
        }
    }

    let fallback = first_line.trim();
    if fallback.len() > 20 {
        Some(format!("{}...", &fallback[..17]))
    } else {
        Some(fallback.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DistroFamily, parse_command_version_output, parse_distro, parse_mise_tool_version,
    };

    #[test]
    fn parse_distro_should_detect_debian_like_distributions() {
        let os_release = "ID=ubuntu\nID_LIKE=debian\n";

        assert_eq!(parse_distro(os_release), DistroFamily::Debian);
    }

    #[test]
    fn parse_distro_should_detect_redhat_like_distributions() {
        let os_release = "ID=fedora\nID_LIKE=fedora\n";

        assert_eq!(parse_distro(os_release), DistroFamily::RedHat);
    }

    #[test]
    fn parse_mise_tool_version_should_return_matching_tool_version() {
        let output = "rust 1.85.0 ~/.config/mise/config.toml\nnode 22.0.0 (missing)\n";

        assert_eq!(
            parse_mise_tool_version("rust", output),
            Some("1.85.0".to_string())
        );
    }

    #[test]
    fn parse_mise_tool_version_should_ignore_missing_placeholders() {
        let output = "rust (missing) ~/.config/mise/config.toml\n";

        assert_eq!(parse_mise_tool_version("rust", output), None);
    }

    #[test]
    fn parse_command_version_output_should_parse_stdout_versions() {
        let version = parse_command_version_output("zellij 0.41.2\n", "");

        assert_eq!(version, Some("0.41.2".to_string()));
    }

    #[test]
    fn parse_command_version_output_should_fall_back_to_stderr() {
        let version = parse_command_version_output("", "java version 21.0.2\n");

        assert_eq!(version, Some("21.0.2".to_string()));
    }
}
