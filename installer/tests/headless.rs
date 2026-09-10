use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "devenv-headless-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("bin")).unwrap();
        Self(root)
    }
    fn script(&self, name: &str, contents: &str) {
        let path = self.0.join("bin").join(name);
        fs::write(&path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn run(&self, config: &str) -> std::process::Output {
        let path = self.0.join("config.toml");
        fs::write(&path, config).unwrap();
        Command::new(env!("CARGO_BIN_EXE_devenv"))
            .args(["--config", path.to_str().unwrap()])
            .env("HOME", &self.0)
            .env("PATH", self.0.join("bin"))
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn failed_tool_returns_failure_but_other_components_continue() {
    let fixture = Fixture::new();
    fixture.script(
        "mise",
        "#!/bin/sh\ncase \"$*\" in *node*) exit 1;; esac\nexit 0\n",
    );
    let output = fixture.run(
        "[[components]]\nid='node'\nenabled=true\n[[components]]\nid='config-bash'\nenabled=true\n",
    );
    assert!(!output.status.success());
    assert!(fixture.0.join(".bashrc").exists());
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("node: Failed") && report.contains("config-bash: Succeeded"));
    assert!(!report.contains("All done!"));
}

#[test]
fn kept_components_return_success_without_writes() {
    let fixture = Fixture::new();
    let output = fixture.run("");
    assert!(output.status.success());
    assert!(!fixture.0.join(".config").exists());
    assert!(!fixture.0.join(".bashrc").exists());
}

#[test]
fn failed_download_never_executes_partial_script() {
    let fixture = Fixture::new();
    for command in ["sh", "mktemp", "rm"] {
        std::os::unix::fs::symlink(
            format!("/usr/bin/{command}"),
            fixture.0.join("bin").join(command),
        )
        .unwrap();
    }
    fixture.script(
        "curl",
        "#!/bin/sh\nprintf ': > \"$HOME/executed\"\\n' > \"$4\"\nexit 22\n",
    );
    let output = fixture.run("[[components]]\nid='config-bash'\nenabled=true\n");
    assert!(!output.status.success());
    assert!(!fixture.0.join("executed").exists());
    assert!(!fixture.0.join(".bashrc").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("mise prerequisite"));
}

#[test]
fn fish_activation_selects_interactive_hooks_or_shims() {
    let fish = Command::new("sh")
        .args(["-c", "command -v fish"])
        .output()
        .unwrap();
    if !fish.status.success() {
        eprintln!("Fish is unavailable; shell execution test skipped");
        return;
    }
    let fish = String::from_utf8(fish.stdout).unwrap();
    let fixture = Fixture::new();
    fixture.script(
        "mise",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/activation.log\"\nprintf 'true\\n'\n",
    );
    assert!(
        fixture
            .run("[[components]]\nid='config-fish'\nenabled=true\n")
            .status
            .success()
    );
    for (interactive, expected) in [
        (false, "activate fish --shims\n"),
        (true, "activate fish\n"),
    ] {
        let mut command = Command::new(fish.trim());
        command.arg("--no-config");
        if interactive {
            command.arg("-i");
        }
        let output = command
            .args(["-c", "source \"$HOME/.config/fish/config.fish\""])
            .env("HOME", &fixture.0)
            .env("PATH", fixture.0.join("bin"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("activation.log")).unwrap(),
            expected
        );
        fs::remove_file(fixture.0.join("activation.log")).unwrap();
    }
}

#[test]
fn bash_activation_falls_back_to_home_with_spaces() {
    let fixture = Fixture::new();
    fixture.script("mise", "#!/bin/sh\nprintf 'true\\n'\n");
    assert!(
        fixture
            .run("[[components]]\nid='config-bash'\nenabled=true\n")
            .status
            .success()
    );
    let home = fixture.0.join("home with spaces");
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::copy(fixture.0.join(".bashrc"), home.join(".bashrc")).unwrap();
    let mise = home.join(".local/bin/mise");
    fs::write(
        &mise,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HOME/activation.log\"\nprintf 'true\\n'\n",
    )
    .unwrap();
    fs::set_permissions(mise, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-ic", "source \"$HOME/.bashrc\""])
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(home.join("activation.log")).unwrap(),
        "activate bash\n"
    );
}
