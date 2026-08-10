use std::io::Write as _;
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lux() -> Command {
    Command::cargo_bin("lux").unwrap()
}

/// Helper: check if output contains ANSI escape sequences.
fn has_ansi_codes(s: &str) -> bool {
    s.contains("\x1b[")
}

#[test]
fn pipe_stdin_passthrough() {
    lux()
        .arg("--color")
        .arg("never")
        .write_stdin("hello world\n")
        .assert()
        .success()
        .stdout("hello world\n");
}

#[test]
fn default_error_coloring() {
    let output = lux()
        .arg("--color")
        .arg("always")
        .write_stdin("ERROR: something broke\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes in: {stdout:?}"
    );
}

#[test]
fn default_warn_coloring() {
    let output = lux()
        .arg("--color")
        .arg("always")
        .write_stdin("WARNING: disk full\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes in: {stdout:?}"
    );
}

#[test]
fn default_debug_coloring() {
    let output = lux()
        .arg("--color")
        .arg("always")
        .write_stdin("DEBUG: entering function\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes in: {stdout:?}"
    );
}

#[test]
fn unmatched_passthrough() {
    let output = lux()
        .arg("--color")
        .arg("always")
        .write_stdin("just a normal line\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !has_ansi_codes(&stdout),
        "Expected no ANSI codes in: {stdout:?}"
    );
    assert_eq!(stdout.trim(), "just a normal line");
}

#[test]
fn custom_rule() {
    let output = lux()
        .args(["-r", "TODO:green", "--color", "always"])
        .write_stdin("TODO: fix this\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes for custom rule in: {stdout:?}"
    );
}

#[test]
fn invalid_rule_exits() {
    lux()
        .args(["-r", "invalid"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("lux:"));
}

#[test]
fn invalid_regex_exits() {
    lux()
        .args(["-r", "(unclosed:red"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("lux:"));
}

#[test]
fn no_color_env() {
    let output = lux()
        .env("NO_COLOR", "1")
        .write_stdin("ERROR: test\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !has_ansi_codes(&stdout),
        "Expected no ANSI codes with NO_COLOR: {stdout:?}"
    );
}

#[test]
fn color_never_flag() {
    let output = lux()
        .args(["--color", "never"])
        .write_stdin("ERROR: test\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !has_ansi_codes(&stdout),
        "Expected no ANSI codes with --color never: {stdout:?}"
    );
}

#[test]
#[ignore]
fn sigpipe_clean_exit() {
    // Hard to test in-process. Manual verification:
    // echo -e "ERROR\nERROR\nERROR" | cargo run -- --color always | head -1
    // Should exit cleanly with exit code 0 and no error output.
}

#[test]
fn multiple_rules() {
    let output = lux()
        .args([
            "-r",
            "CUSTOM1:green",
            "-r",
            "CUSTOM2:blue",
            "--color",
            "always",
        ])
        .write_stdin("CUSTOM1: first\nCUSTOM2: second\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "Expected 2 lines, got: {lines:?}");
    assert!(
        has_ansi_codes(lines[0]),
        "Expected ANSI codes in first line: {:?}",
        lines[0]
    );
    assert!(
        has_ansi_codes(lines[1]),
        "Expected ANSI codes in second line: {:?}",
        lines[1]
    );
}

// === Config file integration tests ===

fn write_config(dir: &TempDir, content: &str) -> std::path::PathBuf {
    let config_path = dir.path().join("config.toml");
    let mut f = std::fs::File::create(&config_path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    config_path
}

#[test]
fn config_file_rules() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[[rules]]
pattern = "CUSTOM_PATTERN"
style = "green"
"#,
    );

    let output = lux()
        .args(["--config", config_path.to_str().unwrap(), "--color", "always"])
        .write_stdin("CUSTOM_PATTERN: test line\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes from config rule in: {stdout:?}"
    );
}

#[test]
fn profile_selection() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[profiles.myprofile]
[[profiles.myprofile.rules]]
pattern = "PROFILE_MATCH"
style = "blue"
"#,
    );

    let output = lux()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "--profile",
            "myprofile",
            "--color",
            "always",
        ])
        .write_stdin("PROFILE_MATCH: hello\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes from profile rule in: {stdout:?}"
    );
}

#[test]
fn missing_profile_error() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[profiles.spring]
[[profiles.spring.rules]]
pattern = "spring"
style = "green"
"#,
    );

    lux()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "--profile",
            "nonexistent",
        ])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("nonexistent"))
        .stderr(predicate::str::contains("spring"));
}

#[test]
fn malformed_config_error() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(&tmp, "[[rules]\nbad toml\n");

    lux()
        .args(["--config", config_path.to_str().unwrap()])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to parse"));
}

#[test]
fn missing_config_file_error() {
    lux()
        .args(["--config", "/nonexistent/path/config.toml"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// === Discovery command integration tests ===

#[test]
fn list_profiles_no_config() {
    // Even without a config file, built-in profiles are shown
    let tmp = TempDir::new().unwrap();
    lux()
        .args(["--list-profiles"])
        .env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap())
        .env_remove("HOME")
        .assert()
        .success()
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("built-in"));
}

#[test]
fn list_profiles_with_profiles() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[profiles.spring-boot]
[[profiles.spring-boot.rules]]
pattern = "spring1"
style = "green"

[[profiles.spring-boot.rules]]
pattern = "spring2"
style = "blue"

[profiles.django]
[[profiles.django.rules]]
pattern = "django"
style = "yellow"
"#,
    );

    lux()
        .args(["--list-profiles", "--config", config_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("spring-boot"))
        .stdout(predicate::str::contains("spring1"))
        .stdout(predicate::str::contains("django"))
        .stdout(predicate::str::contains("style:"))
        .stdout(predicate::str::contains(config_path.to_str().unwrap()));
}

#[test]
fn list_profiles_no_user_profiles_shows_builtins() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[[rules]]
pattern = "ERROR"
style = "red"
"#,
    );

    lux()
        .args(["--list-profiles", "--config", config_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("built-in"));
}

#[test]
fn list_colors_output() {
    lux()
        .args(["--list-colors"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ANSI Colors"))
        .stdout(predicate::str::contains("Styles"))
        .stdout(predicate::str::contains("bold"))
        .stdout(predicate::str::contains("red"));
}

#[test]
fn list_colors_forces_color() {
    // assert_cmd runs without a tty, so color would normally be suppressed
    let output = lux()
        .args(["--list-colors"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI escape codes in --list-colors output (even without tty): {stdout:?}"
    );
}

#[test]
fn list_profiles_exits_without_stdin() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[[rules]]
pattern = "test"
style = "red"
"#,
    );

    // assert_cmd has a default timeout; if the command hangs waiting for stdin, this will fail
    lux()
        .args(["--list-profiles", "--config", config_path.to_str().unwrap()])
        .assert()
        .success();
}

// === Trigger filter integration tests ===

#[test]
fn trigger_basic() {
    // With --trigger ERROR, only lines around the trigger should appear
    let input = "line1\nline2\nline3\nERROR boom\nline4\nline5\nline6\nline7\n";
    let output = lux()
        .args(["--trigger", "ERROR", "--before", "1", "--after", "1", "--color", "never"])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // Should see: line3 (before), ERROR boom (trigger), line4 (after)
    assert!(
        lines.contains(&"line3"),
        "Expected before-context 'line3' in: {lines:?}"
    );
    assert!(
        lines.contains(&"ERROR boom"),
        "Expected trigger line in: {lines:?}"
    );
    assert!(
        lines.contains(&"line4"),
        "Expected after-context 'line4' in: {lines:?}"
    );
    // Should NOT see line1, line2 (outside before window)
    assert!(
        !lines.contains(&"line1"),
        "line1 should be suppressed: {lines:?}"
    );
    assert!(
        !lines.contains(&"line2"),
        "line2 should be suppressed: {lines:?}"
    );
}

#[test]
fn trigger_before_after() {
    let input = "a\nb\nc\nd\nERROR here\ne\nf\ng\nh\n";
    let output = lux()
        .args(["--trigger", "ERROR", "--before", "2", "--after", "2", "--color", "never"])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // Before window (2): c, d
    // Trigger: ERROR here
    // After window (2): e, f
    assert_eq!(
        lines,
        vec!["c", "d", "ERROR here", "e", "f"],
        "Expected exactly before(2) + trigger + after(2)"
    );
}

#[test]
fn trigger_passthrough() {
    // Without --trigger, output should be identical to normal mode
    let input = "hello\nworld\n";
    let output = lux()
        .args(["--color", "never"])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "hello\nworld\n");
}

#[test]
fn trigger_multiple() {
    // Multiple --trigger flags should OR together
    let input = "line1\nERROR bad\nline2\nline3\nWARN oops\nline4\n";
    let output = lux()
        .args([
            "--trigger", "ERROR", "--trigger", "WARN",
            "--before", "0", "--after", "0", "--color", "never",
        ])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.contains(&"ERROR bad"),
        "Expected ERROR line: {lines:?}"
    );
    assert!(
        lines.contains(&"WARN oops"),
        "Expected WARN line: {lines:?}"
    );
    // Non-trigger lines should be suppressed (before=0, after=0)
    assert!(
        !lines.contains(&"line1"),
        "line1 should be suppressed: {lines:?}"
    );
}

#[test]
fn trigger_separator() {
    // Verify "--- lux ---" separator between trigger groups
    let input = "a\nERROR one\nb\nc\nERROR two\nd\n";
    let output = lux()
        .args(["--trigger", "ERROR", "--before", "0", "--after", "0", "--color", "never"])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // First trigger group: ERROR one
    // Separator: --- lux ---
    // Second trigger group: ERROR two
    assert_eq!(lines.len(), 3, "Expected 3 lines: {lines:?}");
    assert_eq!(lines[0], "ERROR one");
    assert!(lines[1].contains("--- lux ---"), "separator should contain label: {:?}", lines[1]);
    assert_eq!(lines[2], "ERROR two");
}

// === File following integration tests ===

/// Get the path to the built lux binary.
fn lux_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("lux")
}

/// Create a temp file with the given content and return (dir, file_path).
fn make_temp_log(content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.log");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    (dir, path)
}

#[test]
fn print_and_exit_last_n() {
    // lux --cat -n 5 <file> should print 5 lines and exit (NOT block)
    // Use StdCommand with null stdin to avoid piped-stdin conflict detection
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let output = StdCommand::new(lux_bin())
        .args(["--cat", "-n", "5", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("line 16"), "Expected line 16: {stdout:?}");
    assert!(stdout.contains("line 20"), "Expected line 20: {stdout:?}");

    let line_count = stdout.lines().count();
    assert_eq!(line_count, 5, "Expected 5 lines, got {line_count}: {stdout:?}");
}

#[test]
fn missing_file_f_errors() {
    // lux -f nonexistent.log should exit with error
    // Use StdCommand with null stdin to avoid piped-stdin conflict detection
    let output = StdCommand::new(lux_bin())
        .args(["-f", "nonexistent_file_12345.log"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(!output.status.success(), "Expected failure exit code");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot open"),
        "Expected 'cannot open' error: {stderr:?}"
    );
}

#[test]
fn file_stdin_conflict() {
    // echo test | lux app.log should fail
    // assert_cmd pipes stdin by default, which is exactly what we want here
    let (_dir, path) = make_temp_log("test content\n");

    lux()
        .arg(path.to_str().unwrap())
        .write_stdin("piped input\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot use file argument with piped input"));
}

#[test]
fn follow_reads_new_lines() {
    // Spawn lux -f, append lines, verify they appear in output
    let lines: String = (1..=5).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let mut child = StdCommand::new(lux_bin())
        .args(["-f", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn lux");

    // Give it time to start and print initial lines
    std::thread::sleep(Duration::from_millis(500));

    // Append a new line
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "NEW LINE APPENDED").unwrap();
        f.flush().unwrap();
    }

    // Wait for the new line to be processed
    std::thread::sleep(Duration::from_millis(1000));

    // Kill the process and read output
    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("NEW LINE APPENDED"),
        "Expected to see appended line in output: {stdout:?}"
    );
}

#[test]
fn bare_file_prints_and_exits() {
    // lux --cat <file> should print the whole file and exit (not follow, not pager)
    let lines: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let output = StdCommand::new(lux_bin())
        .arg("--cat")
        .arg("--color")
        .arg("never")
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "lux should exit successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let out_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(out_lines.len(), 30, "bare file should show all lines");
    assert!(out_lines[0].contains("line 1"), "should start at line 1");
    assert!(out_lines[29].contains("line 30"), "should end at line 30");
}

#[test]
fn n_with_follow_flag() {
    // lux -n 5 -f file should show 5 lines and then follow (NOT print and exit)
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let mut child = StdCommand::new(lux_bin())
        .args(["-n", "5", "-f", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn lux");

    // Wait and check it's still running (following, not exited)
    std::thread::sleep(Duration::from_millis(500));
    let status = child.try_wait().unwrap();
    assert!(
        status.is_none(),
        "-n 5 -f should follow (not print and exit)"
    );

    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have printed the last 5 lines
    assert!(
        stdout.contains("line 16"),
        "Expected last 5 lines in output: {stdout:?}"
    );
}

#[test]
fn follow_name_rotation() {
    // Create file, spawn lux -F, rename file, create new file, write to it
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "original line 1").unwrap();
        writeln!(f, "original line 2").unwrap();
        f.flush().unwrap();
    }

    let mut child = StdCommand::new(lux_bin())
        .args(["-F", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn lux");

    // Wait for it to start
    std::thread::sleep(Duration::from_millis(500));

    // Simulate rotation: rename old file, create new one
    let rotated_path = dir.path().join("app.log.1");
    std::fs::rename(&path, &rotated_path).unwrap();

    // Small delay to let watcher process the rename
    std::thread::sleep(Duration::from_millis(300));

    // Create new file at same path
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "ROTATED LINE").unwrap();
        f.flush().unwrap();
    }

    // Wait for rotation detection + reading
    std::thread::sleep(Duration::from_millis(1500));

    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("ROTATED LINE"),
        "Expected to see line from rotated file: {stdout:?}"
    );
}

#[test]
fn follow_copytruncate() {
    // Create file, spawn lux -F, truncate file, write new content
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 1..=10 {
            writeln!(f, "original line {i}").unwrap();
        }
        f.flush().unwrap();
    }

    let mut child = StdCommand::new(lux_bin())
        .args(["-F", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn lux");

    // Wait for it to start and read initial content
    std::thread::sleep(Duration::from_millis(500));

    // Simulate copytruncate: truncate and write shorter content
    {
        // Truncate by creating (overwriting) the file
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "TRUNCATED NEW LINE").unwrap();
        f.flush().unwrap();
    }

    // Wait for copytruncate detection
    std::thread::sleep(Duration::from_millis(1500));

    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("TRUNCATED NEW LINE"),
        "Expected to see line after copytruncate: {stdout:?}"
    );
}

// === Line filter integration tests ===

#[test]
fn filter_include_only_matching() {
    let output = lux()
        .args(["--include", "ERROR", "--color", "never"])
        .write_stdin("ERROR: bad\nDEBUG: verbose\nWARN: caution\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["ERROR: bad"], "Only ERROR line should appear: {lines:?}");
}

#[test]
fn filter_exclude_hides_matching() {
    let output = lux()
        .args(["--exclude", "DEBUG", "--color", "never"])
        .write_stdin("ERROR: bad\nDEBUG: verbose\nWARN: caution\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["ERROR: bad", "WARN: caution"], "DEBUG should be excluded: {lines:?}");
}

#[test]
fn filter_include_exclude_combo() {
    let output = lux()
        .args(["--include", "ERROR", "--exclude", "timeout", "--color", "never"])
        .write_stdin("ERROR timeout\nERROR disk\nDEBUG normal\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["ERROR disk"], "Include ERROR but exclude timeout: {lines:?}");
}

#[test]
fn filter_multiple_include() {
    let output = lux()
        .args(["--include", "ERROR", "--include", "WARN", "--color", "never"])
        .write_stdin("ERROR: bad\nDEBUG: verbose\nWARN: caution\nINFO: normal\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["ERROR: bad", "WARN: caution"], "Both ERROR and WARN should appear: {lines:?}");
}

#[test]
fn filter_with_trigger() {
    // Excluded lines should not appear in trigger context
    let output = lux()
        .args([
            "--trigger", "ERROR", "--exclude", "line1",
            "--before", "1", "--after", "0", "--color", "never",
        ])
        .write_stdin("line1\nline2\nERROR\nline4\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // line1 is excluded, so before-context should be line2 only
    assert!(
        !lines.contains(&"line1"),
        "Excluded line1 should not appear: {lines:?}"
    );
    assert!(
        lines.contains(&"ERROR"),
        "Trigger line should appear: {lines:?}"
    );
}

#[test]
fn filter_invalid_regex() {
    lux()
        .args(["--include", "(unclosed", "--color", "never"])
        .write_stdin("test\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid include pattern"));
}

#[test]
// === Shell completions and version integration tests ===

#[test]
fn completions_bash() {
    lux()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("lux"));
}

#[test]
fn completions_zsh() {
    lux()
        .args(["completions", "zsh"])
        .assert()
        .success();
}

#[test]
fn completions_fish() {
    lux()
        .args(["completions", "fish"])
        .assert()
        .success();
}

#[test]
fn version_output() {
    lux()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("lux"))
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}

#[test]
fn version_short() {
    lux()
        .arg("-V")
        .assert()
        .success();
}

#[test]
// === FORCE_COLOR integration tests ===

#[test]
fn force_color_in_pipe() {
    let output = lux()
        .env("FORCE_COLOR", "1")
        .env_remove("NO_COLOR")
        .write_stdin("ERROR: test\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes with FORCE_COLOR=1 in pipe: {stdout:?}"
    );
}

#[test]
fn force_color_overridden_by_no_color() {
    let output = lux()
        .env("FORCE_COLOR", "1")
        .env("NO_COLOR", "1")
        .write_stdin("ERROR: test\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !has_ansi_codes(&stdout),
        "Expected no ANSI codes when NO_COLOR overrides FORCE_COLOR: {stdout:?}"
    );
}

#[test]
fn force_color_overridden_by_color_never() {
    let output = lux()
        .args(["--color", "never"])
        .env("FORCE_COLOR", "1")
        .write_stdin("ERROR: test\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !has_ansi_codes(&stdout),
        "Expected no ANSI codes when --color never overrides FORCE_COLOR: {stdout:?}"
    );
}

#[test]
fn filter_strip_ansi_matching() {
    // Input with ANSI codes, --include should match through them with default strip-ansi
    let output = lux()
        .args(["--include", "ERROR", "--color", "never"])
        .write_stdin("\x1b[31mERROR\x1b[0m: bad\n\x1b[33mWARN\x1b[0m: ok\n")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "Only ERROR line should match through ANSI: {lines:?}");
    assert!(
        lines[0].contains("ERROR"),
        "Line should contain ERROR: {lines:?}"
    );
}

// === Syntect highlighting and profile auto-selection tests ===

#[test]
fn syntect_highlights_markdown_file() {
    // lux --cat with a .md file should get syntect highlighting (ANSI codes)
    let dir = TempDir::new().unwrap();
    let md_path = dir.path().join("test.md");
    std::fs::write(&md_path, "# Hello\n\nSome **bold** text\n").unwrap();

    let output = StdCommand::new(lux_bin())
        .args(["--cat", "--color", "always"])
        .arg(md_path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes from syntect for markdown: {stdout:?}"
    );
}

#[test]
fn explicit_profile_overrides_extension_auto_select() {
    // --profile nonexistent with a .md file should error about missing profile,
    // proving --profile takes priority over extension auto-selection
    let dir = TempDir::new().unwrap();
    let md_path = dir.path().join("test.md");
    std::fs::write(&md_path, "# Test\n").unwrap();

    let output = StdCommand::new(lux_bin())
        .args(["--profile", "nonexistent"])
        .arg(md_path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(!output.status.success(), "Expected failure exit code");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nonexistent"),
        "Expected error about missing profile: {stderr:?}"
    );
}

#[test]
fn list_profiles_shows_logs_builtin() {
    let tmp = TempDir::new().unwrap();
    lux()
        .args(["--list-profiles"])
        .env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap())
        .env_remove("HOME")
        .assert()
        .success()
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("built-in"));
}

#[test]
fn syntect_highlights_sh_file() {
    // lux --cat with a .sh file should get syntect highlighting (ANSI codes)
    let dir = TempDir::new().unwrap();
    let sh_path = dir.path().join("test.sh");
    std::fs::write(&sh_path, "#!/bin/bash\n# comment\necho $HOME\n").unwrap();

    let output = StdCommand::new(lux_bin())
        .args(["--cat", "--color", "always"])
        .arg(sh_path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes from syntect for shell: {stdout:?}"
    );
}

#[test]
fn syntect_highlights_rust_file() {
    // lux --cat with a .rs file should get syntect highlighting
    let dir = TempDir::new().unwrap();
    let rs_path = dir.path().join("test.rs");
    std::fs::write(&rs_path, "fn main() {\n    let x = 42;\n    println!(\"hello\");\n}\n").unwrap();

    let output = StdCommand::new(lux_bin())
        .args(["--cat", "--color", "always"])
        .arg(rs_path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        has_ansi_codes(&stdout),
        "Expected ANSI codes from syntect for Rust: {stdout:?}"
    );
}

// === Pager mode flag tests ===

#[test]
fn cat_flag_prints_file_and_exits() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.log");
    std::fs::write(&file, "line 1\nline 2\nline 3\n").unwrap();

    let output = StdCommand::new(lux_bin())
        .args(["--cat", "--color", "never"])
        .arg(file.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "line 1\nline 2\nline 3\n");
}

#[test]
fn less_and_cat_conflict() {
    lux()
        .arg("--less")
        .arg("--cat")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn less_and_follow_conflict() {
    lux()
        .arg("--less")
        .arg("-f")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn cat_and_follow_conflict() {
    lux()
        .arg("--cat")
        .arg("-f")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn config_subcommand_help() {
    lux()
        .arg("config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("default-file-mode"));
}

#[test]
fn list_profiles_shows_local_lux_profiles() {
    let tmp = TempDir::new().unwrap();
    // Create a fake git repo with .lux/profiles.toml
    std::fs::create_dir(tmp.path().join(".git")).unwrap();
    let lux_dir = tmp.path().join(".lux");
    std::fs::create_dir(&lux_dir).unwrap();
    std::fs::write(
        lux_dir.join("profiles.toml"),
        "[profiles.integration-test]\n[[profiles.integration-test.rules]]\npattern = \"HELLO\"\nstyle = \"green\"\n",
    ).unwrap();

    lux()
        .arg("--list-profiles")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("integration-test"))
        .stdout(predicate::str::contains("(local:"));
}

// === Slow-line annotation integration tests ===

#[test]
fn slow_annotation_pipe_mode() {
    let child = StdCommand::new("sh")
        .args(["-c", r#"echo "line one"; sleep 0.5; echo "line two""#])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let output = StdCommand::new(env!("CARGO_BIN_EXE_lux"))
        .args(["--slow", "100ms", "--color", "never", "--no-profile"])
        .stdin(child.stdout.unwrap())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The annotation should appear on "line one" (the line BEFORE the slow gap)
    assert!(stdout.contains("line one [took:"), "first line should have timing annotation: {stdout}");
    assert!(stdout.contains("line two\n"), "second line should have no annotation: {stdout}");
}

#[test]
fn slow_no_annotation_when_fast() {
    lux()
        .args(["--slow", "10s", "--color", "never", "--no-profile"])
        .write_stdin("line one\nline two\nline three\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("[took:").not());
}

#[test]
fn slow_annotation_follow_mode() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.log");
    std::fs::write(&file_path, "initial line\n").unwrap();

    let mut child = StdCommand::new(env!("CARGO_BIN_EXE_lux"))
        .args([
            "-f",
            file_path.to_str().unwrap(),
            "--slow", "50ms",
            "--color", "never",
            "--no-profile",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // Wait, then append lines with a gap between them
    std::thread::sleep(Duration::from_millis(200));
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new().append(true).open(&file_path).unwrap();
    writeln!(f, "first appended").unwrap();
    drop(f);

    std::thread::sleep(Duration::from_millis(200));
    let mut f = std::fs::OpenOptions::new().append(true).open(&file_path).unwrap();
    writeln!(f, "second appended").unwrap();
    drop(f);

    std::thread::sleep(Duration::from_millis(500));

    // Kill the follow process
    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("first appended"), "should see first appended line: {stdout}");
    assert!(stdout.contains("second appended"), "should see second appended line: {stdout}");
}

#[test]
fn slow_with_custom_style() {
    let child = StdCommand::new("sh")
        .args(["-c", r#"echo "line one"; sleep 0.5; echo "line two""#])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let output = StdCommand::new(env!("CARGO_BIN_EXE_lux"))
        .args(["--slow", "100ms", "--slow-style", "bold+red", "--color", "always", "--no-profile"])
        .stdin(child.stdout.unwrap())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[took:"), "should have timing annotation: {stdout}");
    assert!(has_ansi_codes(&stdout), "should have colored annotation: {stdout}");
}

#[test]
fn slow_plain_mode_no_ansi() {
    let child = StdCommand::new("sh")
        .args(["-c", r#"echo "line one"; sleep 0.5; echo "line two""#])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let output = StdCommand::new(env!("CARGO_BIN_EXE_lux"))
        .args(["--slow", "100ms", "--color", "never", "--no-profile"])
        .stdin(child.stdout.unwrap())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[took:"), "should have timing annotation: {stdout}");
    assert!(!has_ansi_codes(&stdout), "should not have ANSI codes in plain mode: {stdout}");
}

// ── Text insertion rule tests ──────────────────────────────────────────

#[test]
fn insert_before_rule() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg("ERROR::insert-before:--- alert ---")
        .write_stdin("INFO: ok\nERROR: fail\nINFO: ok\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "INFO: ok");
    assert_eq!(lines[1], "--- alert ---");
    assert_eq!(lines[2], "ERROR: fail");
    assert_eq!(lines[3], "INFO: ok");
}

#[test]
fn insert_after_rule() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg("ERROR::insert-after:^^^ check above ^^^")
        .write_stdin("ERROR: fail\nINFO: ok\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "ERROR: fail");
    assert_eq!(lines[1], "^^^ check above ^^^");
    assert_eq!(lines[2], "INFO: ok");
}

#[test]
fn prepend_rule() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg("WARN::prepend:!! ")
        .write_stdin("WARN: disk full\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "!! WARN: disk full");
}

#[test]
fn append_rule() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg("DEBUG::append: (debug)")
        .write_stdin("DEBUG: trace\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "DEBUG: trace (debug)");
}

#[test]
fn insert_with_capture_interpolation() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg(r"ERROR (\w+)::insert-before:--- error in $1 ---")
        .write_stdin("ERROR auth: failed\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "--- error in auth ---");
    assert_eq!(lines[1], "ERROR auth: failed");
}

#[test]
fn insert_stacks_with_style_rules() {
    let output = lux()
        .arg("--color").arg("always")
        .arg("-r").arg("ERROR:red")
        .arg("-r").arg("ERROR::insert-before:--- alert ---")
        .write_stdin("ERROR: fail\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "--- alert ---");
    assert!(has_ansi_codes(lines[1]), "ERROR line should be styled");
}

#[test]
fn insert_template_with_colons() {
    let output = lux()
        .arg("--color").arg("never")
        .arg("-r").arg("ERROR::insert-before:--- 12:34:56 ---")
        .write_stdin("ERROR: fail\n")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "--- 12:34:56 ---");
}

// === Markdown table rendering integration tests ===

#[test]
fn md_table_box_drawn_with_color() {
    let dir = TempDir::new().unwrap();
    let md = dir.path().join("t.md");
    std::fs::write(
        &md,
        "# Title\n\n| Name | Value |\n|------|-------|\n| foo | 12 |\n\nafter\n",
    )
    .unwrap();
    let output = StdCommand::new(lux_bin())
        .arg("--color")
        .arg("always")
        .arg(md.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "lux should exit successfully: {:?}", output.status);
    assert!(stdout.contains('┌'), "expected box drawing in: {stdout}");
    assert!(stdout.contains("│"), "expected box borders in: {stdout}");
    assert!(!stdout.contains("|---"), "raw delimiter should be gone: {stdout}");
    assert!(stdout.contains("after"), "trailing line preserved: {stdout}");
}

#[test]
fn md_table_raw_when_color_never() {
    let dir = TempDir::new().unwrap();
    let md = dir.path().join("t.md");
    std::fs::write(&md, "| a | b |\n|---|---|\n| 1 | 2 |\n").unwrap();
    let output = StdCommand::new(lux_bin())
        .arg("--color")
        .arg("never")
        .arg(md.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "lux should exit successfully: {:?}", output.status);
    assert!(stdout.contains("|---|---|"), "source must be untouched: {stdout}");
    assert!(!stdout.contains('┌'), "no box drawing without color: {stdout}");
}

#[test]
fn md_table_eof_mid_table_flushes() {
    let dir = TempDir::new().unwrap();
    let md = dir.path().join("t.md");
    // File ends while still inside the table
    std::fs::write(&md, "| a | b |\n|---|---|\n| 1 | 2 |\n").unwrap();
    let output = StdCommand::new(lux_bin())
        .arg("--color")
        .arg("always")
        .arg(md.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "lux should exit successfully: {:?}", output.status);
    assert!(stdout.contains('└'), "table should render at EOF: {stdout}");
}

#[test]
fn md_table_via_stdin_sniff() {
    // Fenced code block makes the sniffer detect markdown.
    let input = "```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    let output = lux()
        .arg("--color")
        .arg("always")
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('┌'), "stdin markdown should box-draw: {stdout}");
}

#[test]
fn non_md_file_tables_untouched() {
    let dir = TempDir::new().unwrap();
    let f = dir.path().join("t.log");
    std::fs::write(&f, "| a | b |\n|---|---|\n| 1 | 2 |\n").unwrap();
    let output = StdCommand::new(lux_bin())
        .arg("--color")
        .arg("always")
        .arg(f.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run lux");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "lux should exit successfully: {:?}", output.status);
    assert!(!stdout.contains('┌'), "non-markdown must not box-draw: {stdout}");
}
