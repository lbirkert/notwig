use std::process::Command;

#[test]
fn help_lists_commands_and_versions() {
    let output = Command::new(env!("CARGO_BIN_EXE_notwig"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for command in ["ui", "new", "demo", "render", "info", "loopback"] {
        assert!(text.contains(command), "help is missing {command}");
    }
    assert!(
        !text.contains("scan-plugins"),
        "internal command should be hidden"
    );

    let version = Command::new(env!("CARGO_BIN_EXE_notwig"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_command_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_notwig"))
        .arg("not-a-command")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("unrecognized subcommand"));
}

#[test]
fn demo_creates_bundle_and_renders_audio() {
    let dir = std::env::temp_dir().join(format!("notwig-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_notwig"))
        .arg("demo")
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(dir.join("demo.notwig/project.json").exists());
    assert!(dir.join("demo.wav").exists());
    let output = Command::new(env!("CARGO_BIN_EXE_notwig"))
        .arg("info")
        .arg(dir.join("demo.notwig"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("tracks: 2"));
    assert!(text.contains("clips: 1"));
    std::fs::remove_dir_all(dir).unwrap();
}
