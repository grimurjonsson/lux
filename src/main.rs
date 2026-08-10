use std::io::{self, BufRead, BufWriter, IsTerminal, Write};
use std::path::Path;

use anyhow::Context;
use clap::{CommandFactory, Parser};

use lux::cli::{Cli, Command, ConfigAction, ProfileAction, StripAnsi};
use clap_complete::generate;
use lux::config;
use lux::engine::Engine;
use lux::filter::LineFilter;
use lux::follow;
use lux::md_include::{render_root, IncludeCtx};
use lux::md_table::{FeedResult, FlushResult, TableAssembler};
use lux::output::detect_color_mode;
use lux::rules::build_rules_with_config;
use lux::slow::SlowLineAnnotator;
use lux::syntax::SyntaxHighlighter;
use lux::tail::{self, LineSpec};
use lux::trigger::{OutputDecision, TriggerFilter};
use lux::wizard;

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Subcommands
    if let Some(cmd) = &cli.command {
        match cmd {
            Command::Completions { shell } => {
                let mut cmd = Cli::command();
                generate(*shell, &mut cmd, "lux", &mut std::io::stdout());
                std::process::exit(0);
            }
            Command::Update => {
                lux::update::run_update_command();
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
                    ProfileAction::Show { name, config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        config::show_profile(cfg, name)?;
                    }
                    ProfileAction::SetDefault { name, config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        config::set_default_profile(cfg, Some(name))?;
                        eprintln!("Default profile set to: {name}");
                    }
                    ProfileAction::ClearDefault { config } => {
                        let cfg = config.as_deref().or(cli.config.as_deref()).map(Path::new);
                        config::set_default_profile(cfg, None)?;
                        eprintln!("Default profile cleared");
                    }
                }
                std::process::exit(0);
            }
            Command::Config { action } => {
                match action {
                    ConfigAction::DefaultFileMode { value } => {
                        let valid = ["less", "cat"];
                        if !valid.contains(&value.as_str()) {
                            eprintln!("lux: invalid mode '{}'. Valid options: less, cat", value);
                            std::process::exit(1);
                        }
                        config::set_config_field("default_file_mode", Some(value))?;
                        eprintln!("Default file mode set to: {value}");
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
        && !cli.list_themes
        && !cli.list_syntaxes
        && cli.rules.is_empty()
        && cli.profile.is_none()
        && cli.trigger.is_empty()
        && cli.include.is_empty()
        && cli.exclude.is_empty()
        && !cli.follow_descriptor
        && !cli.follow_name
        && !cli.less
        && !cli.cat
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
    if cli.list_themes {
        for name in SyntaxHighlighter::available_themes() {
            println!("{name}");
        }
        std::process::exit(0);
    }
    if cli.list_syntaxes {
        for (name, exts) in SyntaxHighlighter::available_syntaxes() {
            println!("{name}: {}", exts.join(", "));
        }
        std::process::exit(0);
    }

    let color_mode = detect_color_mode(&cli.color);

    // Load config file
    let config = config::load_config(cli.config.as_deref().map(Path::new))?;

    // Update check: read cache (instant), spawn refresh subprocess if interval elapsed
    let update_interval = config
        .as_ref()
        .map(|(c, _)| c.update_check_interval_days)
        .unwrap_or(7);
    let update_mode_str = config
        .as_ref()
        .and_then(|(c, _)| c.update_mode.clone());
    let cached_new_version = lux::update::check_cached(update_mode_str.as_deref());
    lux::update::spawn_cache_refresh(update_interval, update_mode_str.as_deref());

    // Build merged profiles: built-in first, user profiles override
    let mut merged_profiles = config::builtin_profiles();
    if let Some((ref cfg, _)) = config {
        for (k, v) in &cfg.profiles {
            merged_profiles.insert(k.clone(), v.clone());
        }
    }

    // Local profiles: repo root, then CWD — later inserts win
    let cwd = std::env::current_dir().unwrap_or_default();
    let local_profiles = config::discover_local_profiles(&cwd)?;
    for (profiles, _) in &local_profiles {
        for (k, v) in profiles {
            merged_profiles.insert(k.clone(), v.clone());
        }
    }

    // Determine active profile: --no-profile disables all auto-detection;
    // otherwise: explicit --profile > extension auto-select > content sniff > default_profile > None
    let mut stdin_buffer: Vec<String> = Vec::new();
    let active_profile_name: Option<String> = if cli.no_profile {
        None
    } else if cli.profile.is_some() {
        cli.profile.clone()
    } else if let Some(ref file_path) = cli.file {
        // Auto-select profile by file extension
        let path = std::path::Path::new(file_path);
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| config::find_profile_by_extension(ext, &merged_profiles))
    } else if stdin_is_pipe() {
        // Buffer first lines from stdin for content-based detection
        let stdin = io::stdin();
        let locked = stdin.lock();
        for line in locked.lines().take(10) {
            match line {
                Ok(l) => stdin_buffer.push(l),
                Err(_) => break,
            }
        }
        config::detect_profile_from_content(&stdin_buffer)
    } else {
        None
    }
    .or_else(|| {
        if cli.no_profile {
            None
        } else {
            config
                .as_ref()
                .and_then(|(cfg, _)| cfg.default_profile.clone())
        }
    });

    // If the resolved profile doesn't exist and --ignore-missing-profiles is set,
    // silently clear it instead of erroring later.
    let active_profile_name = if let Some(ref name) = active_profile_name {
        if !merged_profiles.contains_key(name.as_str()) && cli.ignore_missing_profiles {
            eprintln!("warning: profile '{name}' not found, continuing without it");
            None
        } else {
            active_profile_name
        }
    } else {
        active_profile_name
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

    // Create syntect highlighter:
    // - File mode: resolve by path (extension, filename, syntax_map).
    // - Stdin mode: sniff content (currently detects Markdown).
    // Theme priority: --theme CLI flag > config.toml theme > default.
    let syntax_highlighter = if cli.no_profile {
        None
    } else {
        let theme = cli
            .theme
            .as_deref()
            .or_else(|| config.as_ref().and_then(|(c, _)| c.theme.as_deref()));
        if let Some(ref file_path) = cli.file {
            let path = std::path::Path::new(file_path);
            let syntax_map = config
                .as_ref()
                .map(|(c, _)| &c.syntax_map)
                .filter(|m| !m.is_empty());
            SyntaxHighlighter::for_file(path, theme, syntax_map)
        } else if let Some(syntax_name) = config::detect_syntax_from_content(&stdin_buffer) {
            SyntaxHighlighter::for_syntax_name(syntax_name, theme)
        } else {
            None
        }
    };

    let is_markdown_syntax = syntax_highlighter.as_ref().is_some_and(|h| h.is_markdown());

    let mut engine = Engine::new(rules, color_mode.color_enabled(), syntax_highlighter);

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

    // Resolve --slow: CLI flag overrides profile setting
    let slow_threshold_str = cli.slow.as_deref().or_else(|| {
        profile.and_then(|p| p.slow.as_deref())
    });
    let slow_style_str = if cli.slow.is_some() {
        &cli.slow_style
    } else if let Some(ps) = profile.and_then(|p| p.slow_style.as_deref()) {
        ps
    } else {
        &cli.slow_style
    };
    let mut slow_annotator = slow_threshold_str
        .map(|s| {
            let threshold = lux::slow::parse_duration(s)
                .unwrap_or_else(|e| {
                    eprintln!("lux: invalid --slow duration '{s}': {e}");
                    std::process::exit(1);
                });
            SlowLineAnnotator::new(threshold, slow_style_str, color_mode.color_enabled())
        });

    // Markdown table rendering: active only for markdown content, with color,
    // and without trigger/slow modes (multi-line buffering doesn't compose
    // with context windows or per-line timing).
    let mut table_assembler = if color_mode.color_enabled()
        && !trigger_filter.is_active()
        && slow_annotator.is_none()
        && is_markdown_syntax
    {
        Some(TableAssembler::new())
    } else {
        None
    };

    // Include expansion: file view only, markdown only, incompatible with
    // trigger/slow (same composition rule as table rendering).
    let expand_refs_active = cli.expand_refs
        && cli.file.is_some()
        && is_markdown_syntax
        && !trigger_filter.is_active()
        && slow_annotator.is_none();
    if cli.expand_refs && cli.file.is_none() {
        eprintln!("lux: --expand-refs requires a file argument; ignoring");
    }
    if cli.expand_refs && cli.file.is_some() && !is_markdown_syntax {
        eprintln!("lux: --expand-refs only applies to markdown files; viewing normally");
    }

    let stdout_is_terminal = io::stdout().is_terminal();
    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);

    if let Some(ref file_path) = cli.file {
        // FILE MODE -- but which sub-mode?
        // Detect piped stdin: specifically check for FIFO/pipe, not just "not a terminal".
        // This avoids false positives when stdin is /dev/null (e.g., in daemon contexts).
        if stdin_is_pipe() {
            anyhow::bail!("cannot use file argument with piped input");
        }

        let has_follow_flag = cli.follow_descriptor || cli.follow_name;

        // Mode resolution: --less > --cat > -f/-F > config default > "cat"
        let use_pager = if cli.less {
            true
        } else if cli.cat || has_follow_flag {
            false
        } else {
            // Check config default_file_mode
            config.as_ref()
                .and_then(|(c, _)| c.default_file_mode.as_deref())
                .unwrap_or("cat") == "less"
        };

        let is_print_and_exit = !has_follow_flag && !use_pager;

        // line_spec is computed for follow/cat modes; pager mode ignores it and reads the full file.
        // Resolve line count: explicit CLI -n > profile lines > default
        // Print-and-exit shows the whole file by default; follow modes show last 20
        let default_lines = if is_print_and_exit { "+1" } else { "20" };
        let lines_str = cli.lines.as_deref()
            .or(profile_lines.as_deref())
            .unwrap_or(default_lines);
        let line_spec = tail::parse_line_spec(lines_str)?;
        let path = std::path::Path::new(file_path);

        if use_pager {
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let lines = tail::read_from_line(&mut file, 1)?;
            let rule_count = engine.rule_count();
            if expand_refs_active {
                let ctx = IncludeCtx {
                    color_enabled: color_mode.color_enabled(),
                    filter: &filter,
                };
                let rendered = render_root(&lines, path, &mut engine, &ctx);
                lux::pager::run_prerendered(path, active_profile_name.as_deref(), rule_count, &rendered)?;
            } else {
                lux::pager::run(
                    path,
                    &mut engine,
                    &filter,
                    &mut trigger_filter,
                    active_profile_name.as_deref(),
                    rule_count,
                    &lines,
                    table_assembler.as_mut(),
                )?;
            }
        } else if cli.follow_descriptor {
            // -f mode: file MUST exist
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let initial_lines = read_initial(&mut file, &line_spec)?;
            print_lines_filtered(&initial_lines, &mut engine, &mut writer, &mut trigger_filter, &filter, table_assembler.as_mut())?;
            let trigger_opt = if trigger_filter.is_active() {
                Some(trigger_filter)
            } else {
                None
            };
            let filter_opt = if filter.is_active() { Some(&filter) } else { None };
            follow::run(path, follow::FollowMode::Descriptor, file, &mut engine, &mut writer, trigger_opt, filter_opt, slow_annotator, table_assembler.as_mut())?;
        } else if is_print_and_exit {
            // Print-and-exit: file + explicit -n + no follow flag
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let initial_lines = read_initial(&mut file, &line_spec)?;
            if expand_refs_active {
                let ctx = IncludeCtx {
                    color_enabled: color_mode.color_enabled(),
                    filter: &filter,
                };
                for l in render_root(&initial_lines, path, &mut engine, &ctx) {
                    writeln!(writer, "{l}")?;
                }
                writer.flush()?;
            } else {
                print_lines_filtered(&initial_lines, &mut engine, &mut writer, &mut trigger_filter, &filter, table_assembler.as_mut())?;
            }
            // Done -- return without following
        } else {
            // -F: follow by name, file may not exist yet
            match std::fs::File::open(path) {
                Ok(mut file) => {
                    let initial_lines = read_initial(&mut file, &line_spec)?;
                    print_lines_filtered(&initial_lines, &mut engine, &mut writer, &mut trigger_filter, &filter, table_assembler.as_mut())?;
                    let trigger_opt = if trigger_filter.is_active() {
                        Some(trigger_filter)
                    } else {
                        None
                    };
                    let filter_opt = if filter.is_active() { Some(&filter) } else { None };
                    follow::run(path, follow::FollowMode::Name, file, &mut engine, &mut writer, trigger_opt, filter_opt, slow_annotator, table_assembler.as_mut())?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "lux: '{}' does not exist; waiting for it to be created...",
                        file_path
                    );
                    let trigger_opt = if trigger_filter.is_active() {
                        Some(trigger_filter)
                    } else {
                        None
                    };
                    let filter_opt = if filter.is_active() { Some(&filter) } else { None };
                    follow::run_waiting(path, &mut engine, &mut writer, trigger_opt, filter_opt, slow_annotator, table_assembler.as_mut())?;
                }
                Err(e) => return Err(e).with_context(|| format!("cannot open '{file_path}'")),
            }
        }
    } else {
        // Stdin pipe mode
        // -n is silently ignored per user decision
        // Chain any buffered lines (from content detection) with remaining stdin
        let stdin = io::stdin().lock();
        let buffered = stdin_buffer.into_iter().map(Ok);
        let all_lines = buffered.chain(stdin.lines());
        if trigger_filter.is_active() {
            for line in all_lines {
                let line = line?;
                if filter.is_active() && !filter.should_show(&line) {
                    continue;
                }
                let result = engine.apply(&line);
                match trigger_filter.process_line(&line, result.flatten()) {
                    OutputDecision::Pass(lines) => {
                        for l in lines {
                            if let Some(ref mut ann) = slow_annotator {
                                if let Some(prev) = ann.annotate(&l) {
                                    writeln!(writer, "{prev}")?;
                                }
                            } else {
                                writeln!(writer, "{l}")?;
                            }
                        }
                    }
                    OutputDecision::Flush(lines) => {
                        for l in lines {
                            if let Some(ref mut ann) = slow_annotator {
                                if let Some(prev) = ann.annotate(&l) {
                                    writeln!(writer, "{prev}")?;
                                }
                            } else {
                                writeln!(writer, "{l}")?;
                            }
                        }
                    }
                    OutputDecision::Suppress => {}
                }
                if stdout_is_terminal {
                    writer.flush()?;
                }
            }
        } else {
            for line in all_lines {
                let line = line?;
                if filter.is_active() && !filter.should_show(&line) {
                    continue;
                }
                match table_assembler.as_mut() {
                    None => {
                        let result = engine.apply(&line);
                        for l in &result.before {
                            writeln!(writer, "{l}")?;
                        }
                        if let Some(ref mut ann) = slow_annotator {
                            if let Some(prev) = ann.annotate(&result.line) {
                                writeln!(writer, "{prev}")?;
                            }
                        } else {
                            writeln!(writer, "{}", result.line)?;
                        }
                        for l in &result.after {
                            writeln!(writer, "{l}")?;
                        }
                    }
                    Some(t) => match t.feed(&line) {
                        FeedResult::Pass(raw_lines) => {
                            for raw in raw_lines {
                                let result = engine.apply(&raw);
                                for l in result.flatten() {
                                    writeln!(writer, "{l}")?;
                                }
                            }
                        }
                        FeedResult::Buffered => {}
                        FeedResult::Table { rendered, trailing } => {
                            for l in rendered {
                                writeln!(writer, "{l}")?;
                            }
                            if let Some(raw) = trailing {
                                let result = engine.apply(&raw);
                                for l in result.flatten() {
                                    writeln!(writer, "{l}")?;
                                }
                            }
                        }
                    },
                }
                if stdout_is_terminal {
                    writer.flush()?;
                }
            }
        }
        // Flush the last buffered line from the slow annotator
        if let Some(ref mut ann) = slow_annotator {
            if let Some(last) = ann.flush() {
                writeln!(writer, "{last}")?;
            }
        }
        // Flush any pending table state at EOF.
        if let Some(ref mut t) = table_assembler {
            match t.flush() {
                FlushResult::Nothing => {}
                FlushResult::Raw(raw) => {
                    let result = engine.apply(&raw);
                    for l in result.flatten() {
                        writeln!(writer, "{l}")?;
                    }
                }
                FlushResult::Table(rendered) => {
                    for l in rendered {
                        writeln!(writer, "{l}")?;
                    }
                }
            }
        }
    }

    // Notify about available update (from cache, instant)
    if let Some(ref new_version) = cached_new_version {
        let update_mode = config
            .as_ref()
            .and_then(|(c, _)| c.update_mode.as_deref());
        lux::update::handle_update_result(new_version, update_mode);
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
    engine: &mut Engine,
    writer: &mut BufWriter<impl Write>,
    trigger: &mut TriggerFilter,
    filter: &LineFilter,
    mut table: Option<&mut TableAssembler>,
) -> anyhow::Result<()> {
    if trigger.is_active() {
        for line in lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            let result = engine.apply(line);
            match trigger.process_line(line, result.flatten()) {
                OutputDecision::Pass(out_lines) => {
                    for l in out_lines {
                        writeln!(writer, "{l}")?;
                    }
                }
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
            match table.as_deref_mut() {
                None => {
                    let result = engine.apply(line);
                    for l in result.flatten() {
                        writeln!(writer, "{l}")?;
                    }
                }
                Some(t) => match t.feed(line) {
                    FeedResult::Pass(raw_lines) => {
                        for raw in raw_lines {
                            let result = engine.apply(&raw);
                            for l in result.flatten() {
                                writeln!(writer, "{l}")?;
                            }
                        }
                    }
                    FeedResult::Buffered => {}
                    FeedResult::Table { rendered, trailing } => {
                        for l in rendered {
                            writeln!(writer, "{l}")?;
                        }
                        if let Some(raw) = trailing {
                            let result = engine.apply(&raw);
                            for l in result.flatten() {
                                writeln!(writer, "{l}")?;
                            }
                        }
                    }
                },
            }
        }
        // EOF for this batch: flush any pending table state.
        if let Some(t) = table {
            match t.flush() {
                FlushResult::Nothing => {}
                FlushResult::Raw(raw) => {
                    let result = engine.apply(&raw);
                    for l in result.flatten() {
                        writeln!(writer, "{l}")?;
                    }
                }
                FlushResult::Table(rendered) => {
                    for l in rendered {
                        writeln!(writer, "{l}")?;
                    }
                }
            }
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
        eprintln!("lux: {e:#}");
        std::process::exit(1);
    }
}
