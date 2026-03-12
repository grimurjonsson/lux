use std::io::Write as _;
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn ctail() -> Command {
    Command::cargo_bin("ctail").unwrap()
}

/// Helper: check if output contains ANSI escape sequences.
fn has_ansi_codes(s: &str) -> bool {
    s.contains("\x1b[")
}

#[test]
fn pipe_stdin_passthrough() {
    ctail()
        .arg("--color")
        .arg("never")
        .write_stdin("hello world\n")
        .assert()
        .success()
        .stdout("hello world\n");
}

#[test]
fn default_error_coloring() {
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    ctail()
        .args(["-r", "invalid"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ctail:"));
}

#[test]
fn invalid_regex_exits() {
    ctail()
        .args(["-r", "(unclosed:red"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ctail:"));
}

#[test]
fn no_color_env() {
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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

    let output = ctail()
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

    let output = ctail()
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

    ctail()
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

    ctail()
        .args(["--config", config_path.to_str().unwrap()])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to parse"));
}

#[test]
fn missing_config_file_error() {
    ctail()
        .args(["--config", "/nonexistent/path/config.toml"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// === Discovery command integration tests ===

#[test]
fn list_profiles_no_config() {
    let tmp = TempDir::new().unwrap();
    ctail()
        .args(["--list-profiles"])
        .env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap())
        .env_remove("HOME")
        .assert()
        .success()
        .stdout(predicate::str::contains("No config file found"));
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

    ctail()
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
fn list_profiles_no_profiles_defined() {
    let tmp = TempDir::new().unwrap();
    let config_path = write_config(
        &tmp,
        r#"
[[rules]]
pattern = "ERROR"
style = "red"
"#,
    );

    ctail()
        .args(["--list-profiles", "--config", config_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No profiles defined"));
}

#[test]
fn list_colors_output() {
    ctail()
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
    let output = ctail()
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
    ctail()
        .args(["--list-profiles", "--config", config_path.to_str().unwrap()])
        .assert()
        .success();
}

// === Trigger filter integration tests ===

#[test]
fn trigger_basic() {
    // With --trigger ERROR, only lines around the trigger should appear
    let input = "line1\nline2\nline3\nERROR boom\nline4\nline5\nline6\nline7\n";
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    // Verify "--- ctail ---" separator between trigger groups
    let input = "a\nERROR one\nb\nc\nERROR two\nd\n";
    let output = ctail()
        .args(["--trigger", "ERROR", "--before", "0", "--after", "0", "--color", "never"])
        .write_stdin(input)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // First trigger group: ERROR one
    // Separator: --- ctail ---
    // Second trigger group: ERROR two
    assert_eq!(
        lines,
        vec!["ERROR one", "--- ctail ---", "ERROR two"],
        "Expected separator between groups: {lines:?}"
    );
}

// === File following integration tests ===

/// Get the path to the built ctail binary.
fn ctail_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("ctail")
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
    // ctail -n 5 <file> should print 5 lines and exit (NOT block)
    // Use StdCommand with null stdin to avoid piped-stdin conflict detection
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let output = StdCommand::new(ctail_bin())
        .args(["-n", "5", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run ctail");

    assert!(output.status.success(), "Expected exit 0, got: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("line 16"), "Expected line 16: {stdout:?}");
    assert!(stdout.contains("line 20"), "Expected line 20: {stdout:?}");

    let line_count = stdout.lines().count();
    assert_eq!(line_count, 5, "Expected 5 lines, got {line_count}: {stdout:?}");
}

#[test]
fn missing_file_f_errors() {
    // ctail -f nonexistent.log should exit with error
    // Use StdCommand with null stdin to avoid piped-stdin conflict detection
    let output = StdCommand::new(ctail_bin())
        .args(["-f", "nonexistent_file_12345.log"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run ctail");

    assert!(!output.status.success(), "Expected failure exit code");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot open"),
        "Expected 'cannot open' error: {stderr:?}"
    );
}

#[test]
fn file_stdin_conflict() {
    // echo test | ctail app.log should fail
    // assert_cmd pipes stdin by default, which is exactly what we want here
    let (_dir, path) = make_temp_log("test content\n");

    ctail()
        .arg(path.to_str().unwrap())
        .write_stdin("piped input\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot use file argument with piped input"));
}

#[test]
fn follow_reads_new_lines() {
    // Spawn ctail -f, append lines, verify they appear in output
    let lines: String = (1..=5).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let mut child = StdCommand::new(ctail_bin())
        .args(["-f", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctail");

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
fn bare_file_implies_follow() {
    // ctail <file> (no flags) should NOT exit immediately -- it follows
    let lines: String = (1..=5).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let mut child = StdCommand::new(ctail_bin())
        .arg("--color")
        .arg("never")
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctail");

    // Wait a bit -- if it exits immediately, it's wrong
    std::thread::sleep(Duration::from_millis(500));

    // Check it's still running
    let status = child.try_wait().unwrap();
    assert!(
        status.is_none(),
        "bare file arg should follow (not exit immediately)"
    );

    // Append a line and check it appears
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "BARE FILE FOLLOW TEST").unwrap();
        f.flush().unwrap();
    }

    std::thread::sleep(Duration::from_millis(1000));

    child.kill().ok();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BARE FILE FOLLOW TEST"),
        "Expected to see appended line in bare file follow: {stdout:?}"
    );
}

#[test]
fn n_with_follow_flag() {
    // ctail -n 5 -f file should show 5 lines and then follow (NOT print and exit)
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    let (_dir, path) = make_temp_log(&lines);

    let mut child = StdCommand::new(ctail_bin())
        .args(["-n", "5", "-f", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctail");

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
    // Create file, spawn ctail -F, rename file, create new file, write to it
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "original line 1").unwrap();
        writeln!(f, "original line 2").unwrap();
        f.flush().unwrap();
    }

    let mut child = StdCommand::new(ctail_bin())
        .args(["-F", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctail");

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
    // Create file, spawn ctail -F, truncate file, write new content
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app.log");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 1..=10 {
            writeln!(f, "original line {i}").unwrap();
        }
        f.flush().unwrap();
    }

    let mut child = StdCommand::new(ctail_bin())
        .args(["-F", "--color", "never"])
        .arg(path.to_str().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctail");

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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    ctail()
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
    ctail()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ctail"));
}

#[test]
fn completions_zsh() {
    ctail()
        .args(["completions", "zsh"])
        .assert()
        .success();
}

#[test]
fn completions_fish() {
    ctail()
        .args(["completions", "fish"])
        .assert()
        .success();
}

#[test]
fn version_output() {
    ctail()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ctail"))
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}

#[test]
fn version_short() {
    ctail()
        .arg("-V")
        .assert()
        .success();
}

#[test]
// === FORCE_COLOR integration tests ===

#[test]
fn force_color_in_pipe() {
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
    let output = ctail()
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
