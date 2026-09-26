use anyhow::{Context, Result, anyhow};
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path_var)
        .chain(std::iter::once(mise_shims_dir()))
        .any(|dir| is_executable(&dir.join(cmd)))
}

/// Resolve the mise shims directory the same way mise does:
/// `$MISE_DATA_DIR`, then `$XDG_DATA_HOME/mise`, then `~/.local/share/mise`.
pub fn mise_shims_dir() -> PathBuf {
    mise_data_dir(
        std::env::var_os("MISE_DATA_DIR"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
    .join("shims")
}

fn mise_data_dir(
    mise_data_dir: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    let non_empty = |value: Option<OsString>| value.filter(|value| !value.is_empty());
    if let Some(dir) = non_empty(mise_data_dir) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = non_empty(xdg_data_home) {
        return PathBuf::from(dir).join("mise");
    }
    PathBuf::from(home.unwrap_or_default()).join(".local/share/mise")
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

pub fn get_distro() -> DistroFamily {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    parse_distro(&os_release)
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
    let mut id = "";
    let mut like = "";
    for line in os_release.lines() {
        if let Some((key, value)) = line.trim().split_once('=') {
            let value = value.trim().trim_matches(['"', '\'']);
            match key {
                "ID" => id = value,
                "ID_LIKE" => like = value,
                _ => {}
            }
        }
    }
    std::iter::once(id)
        .chain(like.split_whitespace())
        .find_map(|id| match id {
            "debian" | "ubuntu" => Some(DistroFamily::Debian),
            "arch" => Some(DistroFamily::Arch),
            "fedora" | "rhel" | "centos" => Some(DistroFamily::RedHat),
            _ => None,
        })
        .unwrap_or(DistroFamily::Unknown)
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
    use super::{DistroFamily, mise_data_dir, parse_command_version_output, parse_distro};
    use std::path::PathBuf;

    #[test]
    fn mise_data_dir_should_follow_mise_precedence() {
        let some = |value: &str| Some(value.into());
        assert_eq!(
            mise_data_dir(some("/data/mise"), some("/xdg"), some("/home/u")),
            PathBuf::from("/data/mise")
        );
        assert_eq!(
            mise_data_dir(some(""), some("/xdg"), some("/home/u")),
            PathBuf::from("/xdg/mise")
        );
        assert_eq!(
            mise_data_dir(None, None, some("/home/u")),
            PathBuf::from("/home/u/.local/share/mise")
        );
    }

    #[test]
    fn distro_fields_should_handle_quotes_tokens_and_id_precedence() {
        for (input, expected) in [
            ("ID=\"fedora\"\n", DistroFamily::RedHat),
            ("ID=rhel\nID_LIKE=\"fedora\"\n", DistroFamily::RedHat),
            (
                "ID=rocky\nID_LIKE=\"rhel centos fedora\"\n",
                DistroFamily::RedHat,
            ),
            ("ID=custom\nID_LIKE='ubuntu debian'\n", DistroFamily::Debian),
            ("ID=arch\nID_LIKE=debian\n", DistroFamily::Arch),
            ("ID=archipelago\n# ID=ubuntu\n", DistroFamily::Unknown),
        ] {
            assert_eq!(parse_distro(input), expected, "{input}");
        }
    }

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
