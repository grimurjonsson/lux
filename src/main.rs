use std::io::{self, BufRead, BufWriter, Write};
use std::path::Path;

use anyhow::Context;
use clap::{CommandFactory, Parser};

use ctail::cli::{Cli, Command, ProfileAction, StripAnsi};
use clap_complete::generate;
use ctail::config;
use ctail::engine::Engine;
use ctail::filter::LineFilter;
use ctail::follow;
use ctail::output::detect_color_mode;
use ctail::rules::build_rules_with_config;
use ctail::tail::{self, LineSpec};
use ctail::trigger::{OutputDecision, TriggerFilter};
use ctail::wizard;

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Subcommands
    if let Some(cmd) = &cli.command {
        match cmd {
            Command::Completions { shell } => {
                let mut cmd = Cli::command();
                generate(*shell, &mut cmd, "ctail", &mut std::io::stdout());
                std::process::exit(0);
            }
            Command::Profile { action } => {
                match action {
                    ProfileAction::New { config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        wizard::run_new_profile(cfg)?;
                    }
                    ProfileAction::Edit { name, config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        wizard::run_edit_profile(cfg, name.as_deref())?;
                    }
                    ProfileAction::Delete { name, config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        wizard::run_delete_profile(cfg, name.as_deref())?;
                    }
                    ProfileAction::List { config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        config::print_profiles(cfg)?;
                    }
                }
                std::process::exit(0);
            }
        }
    }

    // Show help if no arguments and stdin is a terminal
    if cli.file.is_none()
        && !cli.list_colors
        && !cli.list_profiles
        && cli.rules.is_empty()
        && cli.profile.is_none()
        && cli.trigger.is_empty()
        && cli.include.is_empty()
        && cli.exclude.is_empty()
        && !cli.follow_descriptor
        && !cli.follow_name
        && !stdin_is_pipe()
    {
        Cli::command().print_help()?;
        println!();
        std::process::exit(0);
    }

    // Early-exit discovery commands
    if cli.list_colors {
        config::print_colors();
        std::process::exit(0);
    }
    if cli.list_profiles {
        config::print_profiles(cli.config.as_deref().map(Path::new))?;
        std::process::exit(0);
    }

    let color_mode = detect_color_mode(&cli.color);

    // Load config file
    let config = config::load_config(cli.config.as_deref().map(Path::new))?;

    // Build merged profiles: built-in first, user profiles override
    let mut merged_profiles = config::builtin_profiles();
    if let Some((ref cfg, _)) = config {
        for (k, v) in &cfg.profiles {
            merged_profiles.insert(k.clone(), v.clone());
        }
    }

    // Determine active profile: explicit --profile > extension auto-select > None
    let active_profile_name: Option<String> = if cli.profile.is_some() {
        cli.profile.clone()
    } else if let Some(ref file_path) = cli.file {
        // Auto-select profile by file extension
        std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| config::find_profile_by_extension(ext, &merged_profiles))
    } else {
        None
    };

    // Look up the active profile config
    let profile = active_profile_name
        .as_deref()
        .and_then(|name| merged_profiles.get(name));

    let rules = build_rules_with_config(
        &cli.rules,
        config.as_ref().map(|(c, _)| c),
        active_profile_name.as_deref(),
        Some(&merged_profiles),
    )?;
    let engine = Engine::new(rules, color_mode.color_enabled());

    // Merge trigger settings: CLI flags override profile settings
    let trigger_patterns: Vec<String> = if !cli.trigger.is_empty() {
        cli.trigger.clone()
    } else if let Some(p) = profile {
        p.trigger.clone()
    } else {
        vec![]
    };
    let before_spec = if cli.before != "20" {
        cli.before.clone()
    } else if let Some(p) = profile.and_then(|p| p.before.as_ref()) {
        p.clone()
    } else {
        cli.before.clone()
    };
    let after_spec = if cli.after != "20" {
        cli.after.clone()
    } else if let Some(p) = profile.and_then(|p| p.after.as_ref()) {
        p.clone()
    } else {
        cli.after.clone()
    };
    // Profile can set -n default (overridden by explicit CLI -n)
    let profile_lines = profile.and_then(|p| p.lines.clone());

    let mut trigger_filter = TriggerFilter::new(&trigger_patterns, &before_spec, &after_spec, color_mode.color_enabled())?;

    // Build line filter for --include/--exclude
    let strip = match cli.strip_ansi {
        StripAnsi::Auto => true,
        StripAnsi::Always => true,
        StripAnsi::Never => false,
    };
    let filter = LineFilter::new(&cli.include, &cli.exclude, strip)?;

    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);

    if let Some(ref file_path) = cli.file {
        // FILE MODE -- but which sub-mode?
        // Detect piped stdin: specifically check for FIFO/pipe, not just "not a terminal".
        // This avoids false positives when stdin is /dev/null (e.g., in daemon contexts).
        if stdin_is_pipe() {
            anyhow::bail!("cannot use file argument with piped input");
        }

        let has_explicit_n = cli.lines.is_some();
        let has_follow_flag = cli.follow_descriptor || cli.follow_name;

        // Three-mode detection:
        // 1. file + explicit -n + NO follow flag = PRINT AND EXIT
        // 2. file + follow flag (with or without -n) = FOLLOW
        // 3. bare file (no -n, no -f/-F) = implied -n 10 -F = FOLLOW
        // Profile lines count as explicit -n for print-and-exit detection
        let has_n = has_explicit_n || profile_lines.is_some();
        let is_print_and_exit = has_n && !has_follow_flag;

        // Resolve line count: explicit CLI -n > profile lines > default 10
        let lines_str = cli.lines.as_deref()
            .or(profile_lines.as_deref())
            .unwrap_or("10");
        let line_spec = tail::parse_line_spec(lines_str)?;
        let path = std::path::Path::new(file_path);

        if cli.follow_descriptor {
            // -f mode: file MUST exist
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let initial_lines = read_initial(&mut file, &line_spec)?;
            print_lines_filtered(&initial_lines, &engine, &mut writer, &mut trigger_filter, &filter)?;
            let trigger_opt = if trigger_filter.is_active() {
                Some(trigger_filter)
            } else {
                None
            };
            let filter_opt = if filter.is_active() { Some(&filter) } else { None };
            follow::run(path, follow::FollowMode::Descriptor, file, &engine, &mut writer, trigger_opt, filter_opt)?;
        } else if is_print_and_exit {
            // Print-and-exit: file + explicit -n + no follow flag
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let initial_lines = read_initial(&mut file, &line_spec)?;
            print_lines_filtered(&initial_lines, &engine, &mut writer, &mut trigger_filter, &filter)?;
            // Done -- return without following
        } else {
            // -F or bare file: follow by name, file may not exist yet
            match std::fs::File::open(path) {
                Ok(mut file) => {
                    let initial_lines = read_initial(&mut file, &line_spec)?;
                    print_lines_filtered(&initial_lines, &engine, &mut writer, &mut trigger_filter, &filter)?;
                    let trigger_opt = if trigger_filter.is_active() {
                        Some(trigger_filter)
                    } else {
                        None
                    };
                    let filter_opt = if filter.is_active() { Some(&filter) } else { None };
                    follow::run(path, follow::FollowMode::Name, file, &engine, &mut writer, trigger_opt, filter_opt)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "ctail: '{}' does not exist; waiting for it to be created...",
                        file_path
                    );
                    let trigger_opt = if trigger_filter.is_active() {
                        Some(trigger_filter)
                    } else {
                        None
                    };
                    let filter_opt = if filter.is_active() { Some(&filter) } else { None };
                    follow::run_waiting(path, &engine, &mut writer, trigger_opt, filter_opt)?;
                }
                Err(e) => return Err(e).with_context(|| format!("cannot open '{file_path}'")),
            }
        }
    } else {
        // Stdin pipe mode
        // -n is silently ignored per user decision
        let stdin = io::stdin().lock();
        if trigger_filter.is_active() {
            for line in stdin.lines() {
                let line = line?;
                if filter.is_active() && !filter.should_show(&line) {
                    continue;
                }
                let colored = engine.apply(&line);
                match trigger_filter.process_line(&line, colored) {
                    OutputDecision::Pass(s) => writeln!(writer, "{s}")?,
                    OutputDecision::Flush(lines) => {
                        for l in lines {
                            writeln!(writer, "{l}")?;
                        }
                    }
                    OutputDecision::Suppress => {}
                }
            }
        } else {
            for line in stdin.lines() {
                let line = line?;
                if filter.is_active() && !filter.should_show(&line) {
                    continue;
                }
                let output = engine.apply(&line);
                writeln!(writer, "{output}")?;
            }
        }
    }

    Ok(())
}

/// Check if stdin is a pipe (FIFO), as opposed to a terminal or /dev/null.
fn stdin_is_pipe() -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata("/dev/stdin")
        .map(|m| m.file_type().is_fifo())
        .unwrap_or(false)
}

/// Read initial lines from a file based on the line spec.
fn read_initial(file: &mut std::fs::File, line_spec: &LineSpec) -> anyhow::Result<Vec<String>> {
    match line_spec {
        LineSpec::Last(n) => tail::read_last_n_lines(file, *n),
        LineSpec::FromLine(n) => tail::read_from_line(file, *n),
    }
}

/// Print lines through the engine and trigger filter to the writer.
fn print_lines_filtered(
    lines: &[String],
    engine: &Engine,
    writer: &mut BufWriter<impl Write>,
    trigger: &mut TriggerFilter,
    filter: &LineFilter,
) -> anyhow::Result<()> {
    if trigger.is_active() {
        for line in lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            let colored = engine.apply(line);
            match trigger.process_line(line, colored) {
                OutputDecision::Pass(s) => writeln!(writer, "{s}")?,
                OutputDecision::Flush(flushed) => {
                    for l in flushed {
                        writeln!(writer, "{l}")?;
                    }
                }
                OutputDecision::Suppress => {}
            }
        }
    } else {
        for line in lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            let output = engine.apply(line);
            writeln!(writer, "{output}")?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        // Check for BrokenPipe (happens when piped to head, etc.)
        if let Some(io_err) = e.downcast_ref::<io::Error>()
            && io_err.kind() == io::ErrorKind::BrokenPipe
        {
            std::process::exit(0);
        }
        eprintln!("ctail: {e:#}");
        std::process::exit(1);
    }
}
