use crate::registry::Component;
use crate::sys::{get_distro, run_cmd_streaming};

pub fn install_system_packages<F>(components: &[&Component], mut log: F) -> Result<(), String>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if components.is_empty() {
        return Ok(());
    }

    let distro = get_distro();
    let (mut install_cmd, pre_cmd) = match distro.as_str() {
        "debian" => (
            vec!["sudo", "apt-get", "install", "-y"],
            Some(vec!["sudo", "apt-get", "update"]),
        ),
        "arch" => (
            vec!["sudo", "pacman", "-S", "--noconfirm"],
            Some(vec!["sudo", "pacman", "-Sy"]),
        ),
        "redhat" => (vec!["sudo", "dnf", "install", "-y"], None), // Assuming dnf checks on its own or dnf check-update
        _ => return Err(format!("Unsupported distribution family: {}", distro)),
    };

    if let Some(cmd) = pre_cmd {
        log(&format!("Updating package lists..."));
        let res = run_cmd_streaming(cmd[0], &cmd[1..], log.clone());
        if !res.success {
            log(&format!(
                "Warning: Package list update returned non-zero. Error: {}",
                res.stderr
            ));
        }
    }

    let mut pkgs = Vec::new();
    for c in components {
        match c.id.as_str() {
            "base-deps" => match distro.as_str() {
                "debian" => pkgs.extend_from_slice(&[
                    "build-essential",
                    "curl",
                    "wget",
                    "git",
                    "unzip",
                    "tar",
                ]),
                "arch" => {
                    pkgs.extend_from_slice(&["base-devel", "curl", "wget", "git", "unzip", "tar"])
                }
                "redhat" => pkgs.extend_from_slice(&[
                    "gcc", "gcc-c++", "make", "curl", "wget", "git", "unzip", "tar",
                ]),
                _ => {}
            },
            "tmux" => pkgs.push("tmux"),
            _ => log(&format!("Ignoring unknown system component: {}", c.id)),
        }
    }

    if pkgs.is_empty() {
        return Ok(());
    }

    install_cmd.extend(pkgs.clone());
    log(&format!("Installing system packages: {}", pkgs.join(" ")));

    let result = run_cmd_streaming(install_cmd[0], &install_cmd[1..], log);
    if result.success {
        Ok(())
    } else {
        Err(format!(
            "Failed to install system packages. See detailed log."
        ))
    }
}
