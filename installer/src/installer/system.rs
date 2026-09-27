use crate::registry::{Component, Prerequisite};
use crate::sys::{DistroFamily, check_command_exists, get_distro, is_root, run_cmd_streaming};
use anyhow::{Result, anyhow};
use std::path::Path;

/// CA bundle locations used by curl on the supported distros.
const CA_BUNDLES: [&str; 3] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/cert.pem",
];

/// Directories where distros install libatomic.so.1 (needed by Node.js builds).
const LIB_DIRS: [&str; 5] = [
    "/usr/lib64",
    "/usr/lib",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/lib64",
];

/// Whether a system prerequisite is already available on this machine.
pub fn is_present(prerequisite: Prerequisite) -> bool {
    match prerequisite {
        Prerequisite::Curl => check_command_exists("curl"),
        Prerequisite::CaCertificates => CA_BUNDLES.iter().any(|path| Path::new(path).is_file()),
        Prerequisite::Git => check_command_exists("git"),
        Prerequisite::Compiler => check_command_exists("cc"),
        Prerequisite::Libatomic => LIB_DIRS
            .iter()
            .any(|dir| Path::new(dir).join("libatomic.so.1").exists()),
    }
}

/// Whether system packages can be installed: as root, or through sudo.
pub fn can_install_packages() -> bool {
    is_root() || check_command_exists("sudo")
}

fn prerequisite_packages(
    distro: DistroFamily,
    prerequisite: Prerequisite,
) -> &'static [&'static str] {
    match (prerequisite, distro) {
        (Prerequisite::Curl, _) => &["curl"],
        (Prerequisite::CaCertificates, _) => &["ca-certificates"],
        (Prerequisite::Git, _) => &["git"],
        (Prerequisite::Compiler, DistroFamily::Debian) => &["build-essential"],
        (Prerequisite::Compiler, DistroFamily::Arch) => &["base-devel"],
        (Prerequisite::Compiler, _) => &["gcc", "gcc-c++", "make"],
        (Prerequisite::Libatomic, DistroFamily::Debian) => &["libatomic1"],
        (Prerequisite::Libatomic, DistroFamily::Arch) => &["gcc-libs"],
        (Prerequisite::Libatomic, _) => &["libatomic"],
    }
}

fn build_tools_packages(distro: DistroFamily) -> &'static [&'static str] {
    match distro {
        DistroFamily::Debian => &[
            "build-essential",
            "ca-certificates",
            "curl",
            "wget",
            "git",
            "unzip",
            "tar",
            "xz-utils",
        ],
        DistroFamily::Arch => &[
            "base-devel",
            "ca-certificates",
            "curl",
            "wget",
            "git",
            "unzip",
            "tar",
            "xz",
        ],
        DistroFamily::RedHat => &[
            "gcc",
            "gcc-c++",
            "make",
            "ca-certificates",
            "curl",
            "wget",
            "git",
            "unzip",
            "tar",
            "xz",
        ],
        DistroFamily::Unknown => &[],
    }
}

/// The deduplicated package list for the selected system components plus
/// the missing prerequisites.
fn packages_for(
    distro: DistroFamily,
    components: &[&Component],
    prerequisites: &[Prerequisite],
) -> Vec<&'static str> {
    let mut packages: Vec<&'static str> = Vec::new();
    let mut add = |list: &[&'static str]| {
        for package in list {
            if !packages.contains(package) {
                packages.push(package);
            }
        }
    };
    for component in components {
        if component.id == "base-deps" {
            add(build_tools_packages(distro));
        }
    }
    for prerequisite in prerequisites {
        add(prerequisite_packages(distro, *prerequisite));
    }
    packages
}

/// Install the selected system components and missing prerequisites with the
/// distro package manager, through sudo unless running as root.
pub fn install_system_packages<F>(
    components: &[&Component],
    prerequisites: &[Prerequisite],
    mut log: F,
) -> Result<()>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if components.is_empty() && prerequisites.is_empty() {
        return Ok(());
    }

    let distro = get_distro();
    let (mut install_cmd, mut pre_cmd) = match distro {
        DistroFamily::Debian => (
            vec!["sudo", "apt-get", "install", "-y"],
            Some(vec!["sudo", "apt-get", "update"]),
        ),
        DistroFamily::Arch => (
            vec!["sudo", "pacman", "-S", "--needed", "--noconfirm"],
            None,
        ),
        DistroFamily::RedHat => (vec!["sudo", "dnf", "install", "-y"], None),
        DistroFamily::Unknown => {
            return Err(anyhow!("Unsupported distribution family: Unknown"));
        }
    };

    if is_root() {
        install_cmd.remove(0);
        if let Some(cmd) = &mut pre_cmd {
            cmd.remove(0);
        }
    } else if !check_command_exists("sudo") {
        return Err(anyhow!(
            "sudo is not installed; run devenv as root or install sudo first"
        ));
    }

    let packages = packages_for(distro, components, prerequisites);
    if packages.is_empty() {
        return Ok(());
    }

    if let Some(cmd) = pre_cmd {
        log("Updating package lists...");
        let res = run_cmd_streaming(cmd[0], &cmd[1..], log.clone())?;
        if !res.success {
            log(&format!(
                "Warning: Package list update returned non-zero. Error: {}",
                res.stderr
            ));
        }
    }

    install_cmd.extend(packages.iter().copied());
    log(&format!(
        "Installing system packages: {}",
        packages.join(" ")
    ));

    let result = run_cmd_streaming(install_cmd[0], &install_cmd[1..], log)?;
    if result.success {
        Ok(())
    } else {
        Err(anyhow!(
            "Failed to install system packages: {}",
            result.stderr.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Category, Group};

    fn build_tools() -> Component {
        Component::new(
            "base-deps",
            "Build Tools",
            "compilers",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        )
    }

    #[test]
    fn prerequisites_should_map_to_distro_packages() {
        assert_eq!(
            packages_for(
                DistroFamily::Debian,
                &[],
                &[Prerequisite::Curl, Prerequisite::Compiler]
            ),
            vec!["curl", "build-essential"]
        );
        assert_eq!(
            packages_for(DistroFamily::RedHat, &[], &[Prerequisite::Compiler]),
            vec!["gcc", "gcc-c++", "make"]
        );
        assert_eq!(
            packages_for(DistroFamily::Arch, &[], &[Prerequisite::Compiler]),
            vec!["base-devel"]
        );
        assert_eq!(
            packages_for(DistroFamily::RedHat, &[], &[Prerequisite::Libatomic]),
            vec!["libatomic"]
        );
    }

    #[test]
    fn build_tools_and_prerequisites_should_not_duplicate_packages() {
        let packages = packages_for(
            DistroFamily::Debian,
            &[&build_tools()],
            &[Prerequisite::Git, Prerequisite::CaCertificates],
        );

        assert_eq!(
            packages,
            build_tools_packages(DistroFamily::Debian).to_vec()
        );
    }
}
