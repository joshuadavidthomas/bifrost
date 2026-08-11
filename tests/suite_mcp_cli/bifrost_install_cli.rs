#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn install_registers_all_native_mcp_hosts_without_using_real_home() {
        let temporary = TempDir::new().expect("temporary directory");
        let bin = temporary.path().join("bin");
        let logs = temporary.path().join("logs");
        let home = temporary.path().join("home");
        fs::create_dir_all(&bin).expect("fake bin directory");
        fs::create_dir_all(&logs).expect("log directory");
        fs::create_dir_all(&home).expect("home directory");

        for name in ["codex", "claude", "opencode2", "kimi", "hermes", "omp"] {
            write_fake_host(&bin, name);
        }

        let first = run_install(&bin, &logs, &home);
        assert!(
            first.status.success(),
            "install failed: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        let executable = env!("CARGO_BIN_EXE_bifrost");
        assert_log(
            &logs,
            "codex",
            &["mcp", "add", "brokk", "--", executable, "--mcp", "core|nlp"],
        );
        assert_log(
            &logs,
            "claude",
            &[
                "mcp",
                "add",
                "--transport",
                "stdio",
                "--scope",
                "user",
                "brokk",
                "--",
                executable,
                "--mcp",
                "core|nlp",
            ],
        );
        assert_log(
            &logs,
            "opencode2",
            &[
                "mcp", "add", "brokk", "--global", "--", executable, "--mcp", "core|nlp",
            ],
        );
        assert_log(
            &logs,
            "kimi",
            &["mcp", "add", "brokk", "--", executable, "--mcp", "core|nlp"],
        );
        assert_log(
            &logs,
            "hermes",
            &[
                "mcp",
                "add",
                "brokk",
                "--command",
                executable,
                "--args",
                "--mcp",
                "core|nlp",
            ],
        );

        let config_path = home.join(".omp/agent/mcp.json");
        let mut config: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).expect("Oh My Pi MCP config"))
                .expect("valid Oh My Pi MCP config");
        assert_eq!(
            config["mcpServers"]["brokk"],
            serde_json::json!({
                "type": "stdio",
                "command": executable,
                "args": ["--mcp", "core|nlp"]
            })
        );

        config["unrelated"] = Value::Bool(true);
        fs::write(
            &config_path,
            format!("{}\n", serde_json::to_string_pretty(&config).unwrap()),
        )
        .expect("seed unrelated config");
        let second = run_install(&bin, &logs, &home);
        assert!(
            second.status.success(),
            "second install must be idempotent: {}",
            String::from_utf8_lossy(&second.stderr)
        );
        let updated: Value = serde_json::from_str(
            &fs::read_to_string(config_path).expect("updated Oh My Pi MCP config"),
        )
        .expect("valid updated config");
        assert_eq!(updated["unrelated"], true);
    }

    #[test]
    fn install_fails_when_no_supported_host_is_installed() {
        let temporary = TempDir::new().expect("temporary directory");
        let bin = temporary.path().join("bin");
        let home = temporary.path().join("home");
        fs::create_dir_all(&bin).expect("empty bin directory");
        fs::create_dir_all(&home).expect("home directory");

        let output = Command::new(env!("CARGO_BIN_EXE_bifrost"))
            .arg("--install")
            .env("PATH", &bin)
            .env("HOME", &home)
            .output()
            .expect("run installer");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("No supported coding host was found")
        );
    }

    #[test]
    fn install_is_a_standalone_action() {
        let help = Command::new(env!("CARGO_BIN_EXE_bifrost"))
            .arg("--help")
            .output()
            .expect("run help");
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains("bifrost --install"));

        let output = Command::new(env!("CARGO_BIN_EXE_bifrost"))
            .args(["--install", "--mcp", "core"])
            .output()
            .expect("run installer");
        assert!(!output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "--install cannot be combined with other options\n"
        );
    }

    #[test]
    fn install_merges_stable_opencode_configuration() {
        let temporary = TempDir::new().expect("temporary directory");
        let bin = temporary.path().join("bin");
        let config = temporary.path().join("config");
        fs::create_dir_all(&bin).expect("fake bin directory");
        write_fake_host(&bin, "opencode");
        let config_path = config.join("opencode/opencode.json");
        fs::create_dir_all(config_path.parent().unwrap()).expect("config directory");
        fs::write(&config_path, "{\"unrelated\":true,\"mcp\":{}}\n").expect("seed OpenCode config");

        let output = Command::new(env!("CARGO_BIN_EXE_bifrost"))
            .arg("--install")
            .env("PATH", &bin)
            .env("XDG_CONFIG_HOME", &config)
            .output()
            .expect("run installer");
        assert!(
            output.status.success(),
            "install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let document: Value = serde_json::from_str(
            &fs::read_to_string(config_path).expect("updated OpenCode config"),
        )
        .expect("valid OpenCode config");
        assert_eq!(document["unrelated"], true);
        assert_eq!(
            document["mcp"]["brokk"],
            serde_json::json!({
                "type": "local",
                "command": [env!("CARGO_BIN_EXE_bifrost"), "--mcp", "core|nlp"],
                "enabled": true
            })
        );
    }

    fn run_install(bin: &Path, logs: &Path, home: &Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_bifrost"))
            .arg("--install")
            .env("PATH", bin)
            .env("HOME", home)
            .env("LOG_DIR", logs)
            .output()
            .expect("run installer")
    }

    fn write_fake_host(bin: &Path, name: &str) {
        let path = bin.join(name);
        fs::write(
            &path,
            if name == "omp" {
                "#!/bin/sh\nif [ \"$1\" = config ] && [ \"$2\" = path ]; then\n  printf '%s\\n' \"$HOME/.omp/agent\"\n  exit 0\nfi\nexit 1\n".to_string()
            } else if name == "claude" {
                format!("#!/bin/sh\nstate=missing\nif [ -e \"$LOG_DIR/claude-state\" ]; then\n  IFS= read -r state < \"$LOG_DIR/claude-state\"\nfi\nif [ \"$1\" = mcp ] && [ \"$2\" = remove ]; then\n  printf '%s\\n' removed > \"$LOG_DIR/claude-state\"\n  exit 0\nfi\nif [ \"$state\" = active ]; then\n  printf '%s\\n' 'MCP server brokk already exists in user config' >&2\n  exit 1\nfi\nprintf '%s\\n' active > \"$LOG_DIR/claude-state\"\nprintf '%s\\n' \"$@\" > \"$LOG_DIR/{name}\"\n")
            } else if name == "hermes" {
                format!("#!/bin/sh\nif [ \"$1\" = mcp ] && [ \"$2\" = list ]; then\n  printf '%s\\n' brokk\n  exit 0\nfi\nIFS= read -r answer\nprintf '%s\\n' \"$@\" > \"$LOG_DIR/{name}\"\n")
            } else {
                format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$LOG_DIR/{name}\"\n")
            },
        )
        .expect("write fake host");
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make fake host executable");
    }

    fn assert_log(logs: &Path, name: &str, expected: &[&str]) {
        let actual = fs::read_to_string(logs.join(name)).expect("host argument log");
        assert_eq!(actual.lines().collect::<Vec<_>>(), expected);
    }
}
