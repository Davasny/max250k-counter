use std::{
    fs,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_max250k"))
        .current_dir(dir.path())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn counts_unicode_lines_and_estimated_tokens() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "Hello\né🦀").unwrap();
    fs::write(dir.path().join("empty"), "").unwrap();
    let result = run(&dir, &["count"]);
    assert!(result.status.success());
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        format!(
            "lines: 2\ncharacters: 8\ntokens: {}\n",
            tokenx_rs::estimate_token_count("Hello\né🦀")
        )
    );
}

#[test]
fn read_is_sorted_and_excludes_directories_and_files() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("skip")).unwrap();
    for (path, content) in [("b", "B"), ("a", "A"), ("skip/c", "C"), ("ignore", "X")] {
        fs::write(dir.path().join(path), content).unwrap();
    }
    let result = run(
        &dir,
        &["read", "--exclude", "./skip", "--exclude", "./ignore"],
    );
    assert!(result.status.success());
    assert_eq!(result.stdout, b"# file content: a\nA\n# file content: b\nB");
}

#[test]
fn headers_handle_empty_and_nested_files_and_can_be_hidden() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("a"), "A").unwrap();
    fs::write(dir.path().join("empty"), "").unwrap();
    fs::write(dir.path().join("nested/b"), "B\n").unwrap();
    let result = run(&dir, &["read"]);
    assert!(result.status.success());
    assert_eq!(
        result.stdout,
        b"# file content: a\nA\n# file content: empty\n# file content: nested/b\nB\n"
    );
    let hidden = run(&dir, &["read", "--hide-filenames"]);
    assert!(hidden.status.success());
    assert_eq!(hidden.stdout, b"AB\n");
    fs::write(
        dir.path().join("max250k.json"),
        r#"{"exclude":["max250k.json"],"hide_filenames":true}"#,
    )
    .unwrap();
    let config = run(&dir, &["read"]);
    assert!(config.status.success());
    assert_eq!(config.stdout, hidden.stdout);
    let override_config = run(&dir, &["read", "--hide-filenames=false"]);
    assert!(override_config.status.success());
    assert_eq!(override_config.stdout, result.stdout);
    let output = run(&dir, &["read", "--hide-filenames", "--output", "out.txt"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(dir.path().join("out.txt")).unwrap(), hidden.stdout);
}

#[test]
fn config_and_cli_override_and_output_self_exclusion() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a"), "A").unwrap();
    fs::write(dir.path().join("b"), "B").unwrap();
    fs::write(
        dir.path().join("config.json"),
        r#"{"exclude":["config.json","b"],"output":"out"}"#,
    )
    .unwrap();
    fs::write(dir.path().join("out"), "OLD").unwrap();
    let result = run(&dir, &["read", "--config", "config.json"]);
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("out")).unwrap(),
        "# file content: a\nA"
    );
    let result = run(
        &dir,
        &[
            "read",
            "--config",
            "config.json",
            "--exclude",
            "config.json",
            "--exclude",
            "a",
            "--exclude",
            "out",
            "--output",
            "other",
        ],
    );
    assert!(result.status.success());
    assert_eq!(
        fs::read_to_string(dir.path().join("other")).unwrap(),
        "# file content: b\nB"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("out")).unwrap(),
        "# file content: a\nA"
    );
}

#[test]
fn skips_binary_and_invalid_utf8() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("binary"), b"a\0b").unwrap();
    fs::write(dir.path().join("invalid"), [255]).unwrap();
    fs::write(dir.path().join("text"), "OK").unwrap();
    let result = run(&dir, &["read"]);
    assert!(result.status.success());
    assert_eq!(result.stdout, b"# file content: text\nOK");
    assert!(result.stderr.is_empty());
}

#[test]
fn debug_logs_read_and_skipped_paths_without_changing_stdout() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("text"), "OK").unwrap();
    fs::write(dir.path().join("binary"), b"a\0b").unwrap();
    fs::write(dir.path().join("invalid"), [255]).unwrap();
    fs::create_dir(dir.path().join("excluded")).unwrap();
    fs::write(dir.path().join("excluded/child"), "NO").unwrap();
    for command in ["count", "read"] {
        let info = run(&dir, &[command, "--exclude", "excluded"]);
        let debug = run(
            &dir,
            &[command, "--exclude", "excluded", "--log-level", "debug"],
        );
        assert!(info.status.success());
        assert!(debug.status.success());
        assert_eq!(info.stdout, debug.stdout);
        assert!(info.stderr.is_empty());
        let logs = String::from_utf8(debug.stderr).unwrap();
        assert!(logs.contains("Read: "));
        assert!(logs.contains("/text"));
        assert!(logs.contains(&format!(
            "/text | lines: 1 | characters: 2 | tokens: {}",
            tokenx_rs::estimate_token_count("OK")
        )));
        assert!(logs.contains("Skipped (excluded): "));
        assert!(logs.contains("/excluded"));
        assert!(logs.contains("Skipped (binary/non-UTF-8): "));
        assert!(logs.contains("/binary"));
        assert!(logs.contains("/invalid"));
        assert!(!logs.contains("/child"));
    }
}

#[test]
fn cli_log_level_overrides_config() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("text"), "OK").unwrap();
    fs::write(
        dir.path().join("config.json"),
        r#"{"exclude":["config.json"],"log_level":"debug"}"#,
    )
    .unwrap();
    let debug = run(&dir, &["read", "--config", "config.json"]);
    assert!(debug.status.success());
    assert_eq!(debug.stdout, b"# file content: text\nOK");
    assert!(!debug.stderr.is_empty());
    let info = run(
        &dir,
        &["read", "--config", "config.json", "--log-level", "info"],
    );
    assert!(info.status.success());
    assert_eq!(info.stdout, debug.stdout);
    assert!(info.stderr.is_empty());
    assert!(
        !run(&dir, &["count", "--log-level", "invalid"])
            .status
            .success()
    );
    fs::write(dir.path().join("config.json"), r#"{"log_level":"invalid"}"#).unwrap();
    assert!(
        !run(&dir, &["count", "--config", "config.json"])
            .status
            .success()
    );
}

#[test]
fn loads_default_config_and_explicit_config_replaces_it() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("text"), "OK").unwrap();
    fs::write(
        dir.path().join("max250k.json"),
        r#"{"exclude":["max250k.json"],"log_level":"debug"}"#,
    )
    .unwrap();
    let default = run(&dir, &["read"]);
    assert!(default.status.success());
    assert_eq!(default.stdout, b"# file content: text\nOK");
    assert!(!default.stderr.is_empty());
    let overridden = run(&dir, &["read", "--log-level", "info"]);
    assert!(overridden.status.success());
    assert_eq!(overridden.stdout, b"# file content: text\nOK");
    assert!(overridden.stderr.is_empty());
    fs::write(
        dir.path().join("other.json"),
        r#"{"exclude":["max250k.json","other.json"]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("max250k.json"), "invalid").unwrap();
    assert!(!run(&dir, &["count"]).status.success());
    let explicit = run(&dir, &["read", "--config", "other.json"]);
    assert!(explicit.status.success());
    assert_eq!(explicit.stdout, b"# file content: text\nOK");
    assert!(explicit.stderr.is_empty());
}

#[test]
fn invalid_or_missing_configs_fail() {
    let dir = TempDir::new().unwrap();
    for json in [r#"{"exclude":"bad"}"#, r#"{"unknown":true}"#, "{"] {
        fs::write(dir.path().join("config.json"), json).unwrap();
        assert!(
            !run(&dir, &["count", "--config", "config.json"])
                .status
                .success()
        );
    }
    assert!(
        !run(&dir, &["read", "--config", "missing.json"])
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn does_not_follow_symlinks() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a"), "A").unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("alias")).unwrap();
    let result = run(&dir, &["read"]);
    assert!(result.status.success());
    assert_eq!(result.stdout, b"# file content: a\nA");
}
