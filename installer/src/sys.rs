use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> CommandResult {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute process: {} {:?}", cmd, e));

    CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
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
        stdout: stdout_thread.join().unwrap(),
        stderr: stderr_thread.join().unwrap(),
    }
}

pub fn check_command_exists(cmd: &str) -> bool {
    let output = Command::new("which").arg(cmd).output();
    if let Ok(output) = output {
        output.status.success()
    } else {
        false
    }
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
