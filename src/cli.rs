use clap::{Parser, Subcommand};
use clap_complete::Shell;

/// Lit Up teXt — instantly readable colored output
#[derive(Parser)]
#[command(
    name = "lux",
    version,
    about = "Lit Up teXt — instantly readable colored output",
    max_term_width = 100,
    styles = clap::builder::Styles::styled()
        .header(clap::builder::styling::AnsiColor::Cyan.on_default().bold())
        .usage(clap::builder::styling::AnsiColor::Cyan.on_default().bold())
        .literal(clap::builder::styling::Style::new().bold())
        .placeholder(clap::builder::styling::Style::new().dimmed()),
    after_help = "\
\x1b[1;36mRule reference:\x1b[0m
  PATTERN:STYLE[:SCOPE]                 e.g. 'ERROR:bold+red:match'
  line (default)  whole line            match  matched text only
  capN            capture group N       nextN  next N lines after a match
  insert-before:TEXT / insert-after:TEXT insert a line before / after
  prepend:TEXT / append:TEXT            add text to the matching line

  Patterns support lookaround: 'error(?!-style):red:match'.
  -r replaces profile/global rules with the exact same pattern.
  Repeat -r, -i, -e, or -t for multiple patterns.

\x1b[1;36mExamples:\x1b[0m
  tail -f app.log | lux -r 'ERROR:red'   Color a stream
  lux app.log -t ERROR -b 5 -a 10        Show errors with context
  lux README.md --less                  Browse with the pager

Use 'lux <command> --help' for command details."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Color text with PATTERN:STYLE[:SCOPE] (reference below)
    #[arg(short = 'r', long = "rule", value_name = "RULE", help_heading = "Rules & color", action = clap::ArgAction::Append)]
    pub rules: Vec<String>,

    /// Color output: auto, always, never
    #[arg(
        long,
        default_value = "auto",
        value_name = "WHEN",
        hide_possible_values = true,
        help_heading = "Rules & color"
    )]
    pub color: ColorChoice,

    /// Syntax theme (overrides config.toml)
    #[arg(long, help_heading = "Rules & color")]
    pub theme: Option<String>,

    /// Select a named profile
    #[arg(short = 'p', long, help_heading = "Profiles & config")]
    pub profile: Option<String>,

    /// Skip auto profiles and syntax; keep -r rules
    #[arg(
        long,
        alias = "plain",
        conflicts_with = "profile",
        help_heading = "Profiles & config"
    )]
    pub no_profile: bool,

    /// Allow missing profiles (for shared configs)
    #[arg(long, help_heading = "Profiles & config")]
    pub ignore_missing_profiles: bool,

    /// Config file (instead of XDG discovery)
    #[arg(long, value_name = "PATH", help_heading = "Profiles & config")]
    pub config: Option<String>,

    /// Follow descriptor; do not reopen after rename/delete
    #[arg(short = 'f', help_heading = "File viewing")]
    pub follow_descriptor: bool,

    /// Follow name; reopen on rename/truncate/recreate
    #[arg(
        short = 'F',
        conflicts_with = "follow_descriptor",
        help_heading = "File viewing"
    )]
    pub follow_name: bool,

    /// Expand own-line @file.md references in Markdown
    #[arg(
        long = "expand-refs",
        help_heading = "File viewing",
        visible_alias = "expand-referenced-files",
        conflicts_with_all = ["follow_descriptor", "follow_name"]
    )]
    pub expand_refs: bool,

    /// Open in an interactive pager (like less)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "cat"], help_heading = "File viewing")]
    pub less: bool,

    /// Print and exit (default)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "less"], help_heading = "File viewing")]
    pub cat: bool,

    /// Last N lines, or +N to start at line N
    #[arg(short = 'n', value_name = "N", help_heading = "File viewing")]
    pub lines: Option<String>,

    /// Show matching lines and their context; hide the rest
    #[arg(short = 't', long = "trigger", value_name = "REGEX", help_heading = "Filtering & context", action = clap::ArgAction::Append)]
    pub trigger: Vec<String>,

    /// Before: line count or regex boundary
    #[arg(
        short = 'b',
        long,
        default_value = "20",
        value_name = "N|REGEX",
        help_heading = "Filtering & context"
    )]
    pub before: String,

    /// After: line count or regex boundary
    #[arg(
        short = 'a',
        long,
        default_value = "20",
        value_name = "N|REGEX",
        help_heading = "Filtering & context"
    )]
    pub after: String,

    /// Only show matching lines
    #[arg(short = 'i', long = "include", value_name = "REGEX", help_heading = "Filtering & context", action = clap::ArgAction::Append)]
    pub include: Vec<String>,

    /// Hide matching lines
    #[arg(short = 'e', long = "exclude", value_name = "REGEX", help_heading = "Filtering & context", action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,

    /// ANSI: auto/always strip; never keeps
    #[arg(
        long = "strip-ansi",
        default_value = "auto",
        value_name = "MODE",
        hide_possible_values = true,
        help_heading = "Filtering & context"
    )]
    pub strip_ansi: StripAnsi,

    /// Mark delays: 500ms, 5s, 1m30s (pipe/follow only)
    #[arg(long, value_name = "DURATION", help_heading = "Timing")]
    pub slow: Option<String>,

    /// Delayed-line style
    #[arg(
        long,
        default_value = "dim+yellow",
        value_name = "STYLE",
        help_heading = "Timing"
    )]
    pub slow_style: String,

    /// Available profiles
    #[arg(long, help_heading = "Discovery")]
    pub list_profiles: bool,

    /// Color names and styles
    #[arg(long, help_heading = "Discovery")]
    pub list_colors: bool,

    /// Syntax highlighting themes
    #[arg(long, help_heading = "Discovery")]
    pub list_themes: bool,

    /// Syntax names and file extensions
    #[arg(long, help_heading = "Discovery")]
    pub list_syntaxes: bool,

    /// Read a file, or pipe text to stdin
    pub file: Option<String>,
}

/// How to handle ANSI escape codes in input for pattern matching.
#[derive(Clone, Debug, clap::ValueEnum)]
pub enum StripAnsi {
    /// Auto-detect: strip ANSI codes (default, safest)
    Auto,
    /// Always strip ANSI codes
    Always,
    /// Never strip ANSI codes (match against raw input)
    Never,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },
    /// Manage config profiles (new, edit, delete, list)
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Check for updates and upgrade interactively
    Update,
    /// Manage lux configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// Create a new profile interactively
    New {
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// Edit an existing profile
    Edit {
        /// Profile name to edit
        name: Option<String>,
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// Delete a profile
    Delete {
        /// Profile name to delete
        name: Option<String>,
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// List all profiles
    List {
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// Show details for a specific profile with a preview
    Show {
        /// Profile name to show
        name: String,
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// Set the default profile (used when no --profile or extension match)
    SetDefault {
        /// Profile name to set as default
        name: String,
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
    /// Clear the default profile
    ClearDefault {
        /// Path to a custom config file (overrides XDG discovery)
        #[arg(long)]
        config: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set the default file mode (less = pager, cat = print-and-exit)
    DefaultFileMode {
        /// Mode: "less" or "cat"
        value: String,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_args() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(cli.rules.is_empty());
        assert!(matches!(cli.color, ColorChoice::Auto));
    }

    #[test]
    fn test_single_rule() {
        let cli = Cli::try_parse_from(["lux", "-r", "ERROR:red"]).unwrap();
        assert_eq!(cli.rules.len(), 1);
        assert_eq!(cli.rules[0], "ERROR:red");
    }

    #[test]
    fn test_multiple_rules() {
        let cli = Cli::try_parse_from(["lux", "-r", "ERROR:red", "-r", "WARN:yellow"]).unwrap();
        assert_eq!(cli.rules.len(), 2);
    }

    #[test]
    fn test_color_always() {
        let cli = Cli::try_parse_from(["lux", "--color", "always"]).unwrap();
        assert!(matches!(cli.color, ColorChoice::Always));
    }

    #[test]
    fn test_color_never() {
        let cli = Cli::try_parse_from(["lux", "--color", "never"]).unwrap();
        assert!(matches!(cli.color, ColorChoice::Never));
    }

    #[test]
    fn test_color_auto_default() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(matches!(cli.color, ColorChoice::Auto));
    }

    #[test]
    fn test_profile_flag() {
        let cli = Cli::try_parse_from(["lux", "--profile", "django"]).unwrap();
        assert_eq!(cli.profile.as_deref(), Some("django"));
    }

    #[test]
    fn test_config_flag() {
        let cli = Cli::try_parse_from(["lux", "--config", "/tmp/my.toml"]).unwrap();
        assert_eq!(cli.config.as_deref(), Some("/tmp/my.toml"));
    }

    #[test]
    fn test_list_profiles_flag() {
        let cli = Cli::try_parse_from(["lux", "--list-profiles"]).unwrap();
        assert!(cli.list_profiles);
    }

    #[test]
    fn test_list_colors_flag() {
        let cli = Cli::try_parse_from(["lux", "--list-colors"]).unwrap();
        assert!(cli.list_colors);
    }

    #[test]
    fn test_no_profile_by_default() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(cli.profile.is_none());
        assert!(cli.config.is_none());
        assert!(!cli.list_profiles);
        assert!(!cli.list_colors);
    }

    #[test]
    fn test_follow_descriptor() {
        let cli = Cli::try_parse_from(["lux", "-f", "app.log"]).unwrap();
        assert!(cli.follow_descriptor);
        assert_eq!(cli.file.as_deref(), Some("app.log"));
        assert!(cli.lines.is_none());
    }

    #[test]
    fn test_follow_name() {
        let cli = Cli::try_parse_from(["lux", "-F", "app.log"]).unwrap();
        assert!(cli.follow_name);
        assert_eq!(cli.file.as_deref(), Some("app.log"));
        assert!(!cli.follow_descriptor);
    }

    #[test]
    fn test_bare_file() {
        let cli = Cli::try_parse_from(["lux", "app.log"]).unwrap();
        assert_eq!(cli.file.as_deref(), Some("app.log"));
        assert!(!cli.follow_descriptor);
        assert!(!cli.follow_name);
        assert!(cli.lines.is_none());
    }

    #[test]
    fn test_n_with_file() {
        let cli = Cli::try_parse_from(["lux", "-n", "20", "app.log"]).unwrap();
        assert_eq!(cli.lines.as_deref(), Some("20"));
        assert_eq!(cli.file.as_deref(), Some("app.log"));
        assert!(!cli.follow_descriptor);
    }

    #[test]
    fn test_n_plus_syntax() {
        let cli = Cli::try_parse_from(["lux", "-n", "+5", "app.log"]).unwrap();
        assert_eq!(cli.lines.as_deref(), Some("+5"));
    }

    #[test]
    fn test_n_with_follow() {
        let cli = Cli::try_parse_from(["lux", "-n", "5", "-f", "app.log"]).unwrap();
        assert_eq!(cli.lines.as_deref(), Some("5"));
        assert!(cli.follow_descriptor);
    }

    #[test]
    fn test_f_and_f_upper_conflict() {
        let result = Cli::try_parse_from(["lux", "-f", "-F", "app.log"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_f_without_file_allowed() {
        // -f without file arg is allowed by clap (validated later in main)
        let cli = Cli::try_parse_from(["lux", "-f"]).unwrap();
        assert!(cli.follow_descriptor);
        assert!(cli.file.is_none());
    }

    #[test]
    fn test_lines_none_when_omitted() {
        // Critical: lines must be None when not specified, not Some("10")
        let cli = Cli::try_parse_from(["lux", "app.log"]).unwrap();
        assert!(
            cli.lines.is_none(),
            "lines must be None when -n is not passed"
        );
    }

    #[test]
    fn test_lines_some_when_explicit() {
        let cli = Cli::try_parse_from(["lux", "-n", "20", "app.log"]).unwrap();
        assert_eq!(cli.lines, Some("20".to_string()));
    }

    #[test]
    fn test_trigger_single() {
        let cli = Cli::try_parse_from(["lux", "--trigger", "ERROR"]).unwrap();
        assert_eq!(cli.trigger, vec!["ERROR"]);
    }

    #[test]
    fn test_trigger_multiple() {
        let cli = Cli::try_parse_from(["lux", "--trigger", "ERROR", "--trigger", "WARN"]).unwrap();
        assert_eq!(cli.trigger, vec!["ERROR", "WARN"]);
    }

    #[test]
    fn test_trigger_with_before_after_count() {
        let cli = Cli::try_parse_from([
            "lux",
            "--trigger",
            "ERROR",
            "--trigger",
            "WARN",
            "--before",
            "5",
            "--after",
            "10",
        ])
        .unwrap();
        assert_eq!(cli.trigger, vec!["ERROR", "WARN"]);
        assert_eq!(cli.before, "5");
        assert_eq!(cli.after, "10");
    }

    #[test]
    fn test_trigger_with_before_after_pattern() {
        let cli = Cli::try_parse_from([
            "lux",
            "--trigger",
            "ERROR",
            "--before",
            "^===",
            "--after",
            "^---",
        ])
        .unwrap();
        assert_eq!(cli.before, "^===");
        assert_eq!(cli.after, "^---");
    }

    #[test]
    fn test_trigger_defaults() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(cli.trigger.is_empty());
        assert_eq!(cli.before, "20");
        assert_eq!(cli.after, "20");
    }

    #[test]
    fn test_profile_new_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "new"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Profile {
                action: ProfileAction::New { .. }
            })
        ));
    }

    #[test]
    fn test_profile_new_with_config() {
        let cli =
            Cli::try_parse_from(["lux", "profile", "new", "--config", "/tmp/my.toml"]).unwrap();
        match cli.command {
            Some(Command::Profile {
                action: ProfileAction::New { config },
            }) => {
                assert_eq!(config.as_deref(), Some("/tmp/my.toml"));
            }
            _ => panic!("expected Profile New"),
        }
    }

    #[test]
    fn test_profile_edit_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "edit", "django"]).unwrap();
        match cli.command {
            Some(Command::Profile {
                action: ProfileAction::Edit { name, .. },
            }) => {
                assert_eq!(name.as_deref(), Some("django"));
            }
            _ => panic!("expected Profile Edit"),
        }
    }

    #[test]
    fn test_profile_delete_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "delete", "django"]).unwrap();
        match cli.command {
            Some(Command::Profile {
                action: ProfileAction::Delete { name, .. },
            }) => {
                assert_eq!(name.as_deref(), Some("django"));
            }
            _ => panic!("expected Profile Delete"),
        }
    }

    #[test]
    fn test_profile_show_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "show", "logs"]).unwrap();
        match cli.command {
            Some(Command::Profile {
                action: ProfileAction::Show { name, .. },
            }) => {
                assert_eq!(name, "logs");
            }
            _ => panic!("expected Profile Show"),
        }
    }

    #[test]
    fn test_profile_show_with_config() {
        let cli =
            Cli::try_parse_from(["lux", "profile", "show", "logs", "--config", "/tmp/my.toml"])
                .unwrap();
        match cli.command {
            Some(Command::Profile {
                action: ProfileAction::Show { name, config },
            }) => {
                assert_eq!(name, "logs");
                assert_eq!(config.as_deref(), Some("/tmp/my.toml"));
            }
            _ => panic!("expected Profile Show"),
        }
    }

    #[test]
    fn test_profile_list_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Profile {
                action: ProfileAction::List { .. }
            })
        ));
    }

    #[test]
    fn test_no_subcommand_by_default() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_update_subcommand() {
        let cli = Cli::try_parse_from(["lux", "update"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Update)));
    }

    #[test]
    fn test_less_flag() {
        let cli = Cli::try_parse_from(["lux", "--less", "app.log"]).unwrap();
        assert!(cli.less);
        assert_eq!(cli.file.as_deref(), Some("app.log"));
    }

    #[test]
    fn test_cat_flag() {
        let cli = Cli::try_parse_from(["lux", "--cat", "app.log"]).unwrap();
        assert!(cli.cat);
        assert_eq!(cli.file.as_deref(), Some("app.log"));
    }

    #[test]
    fn test_less_conflicts_with_follow_descriptor() {
        let result = Cli::try_parse_from(["lux", "--less", "-f", "app.log"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_less_conflicts_with_follow_name() {
        let result = Cli::try_parse_from(["lux", "--less", "-F", "app.log"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_less_conflicts_with_cat() {
        let result = Cli::try_parse_from(["lux", "--less", "--cat", "app.log"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cat_conflicts_with_follow_descriptor() {
        let result = Cli::try_parse_from(["lux", "--cat", "-f", "app.log"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_ignore_missing_profiles_flag() {
        let cli = Cli::try_parse_from([
            "lux",
            "--profile",
            "nonexistent",
            "--ignore-missing-profiles",
        ])
        .unwrap();
        assert!(cli.ignore_missing_profiles);
        assert_eq!(cli.profile.as_deref(), Some("nonexistent"));
    }

    #[test]
    fn test_ignore_missing_profiles_default() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(!cli.ignore_missing_profiles);
    }

    #[test]
    fn test_slow_flag() {
        let cli = Cli::try_parse_from(["lux", "--slow", "5s"]).unwrap();
        assert_eq!(cli.slow.as_deref(), Some("5s"));
    }

    #[test]
    fn test_slow_flag_not_set() {
        let cli = Cli::try_parse_from(["lux"]).unwrap();
        assert!(cli.slow.is_none());
    }

    #[test]
    fn test_slow_style_default() {
        let cli = Cli::try_parse_from(["lux", "--slow", "5s"]).unwrap();
        assert_eq!(cli.slow_style, "dim+yellow");
    }

    #[test]
    fn test_slow_style_custom() {
        let cli = Cli::try_parse_from(["lux", "--slow", "5s", "--slow-style", "bold+red"]).unwrap();
        assert_eq!(cli.slow_style, "bold+red");
    }

    #[test]
    fn test_slow_requires_value() {
        let result = Cli::try_parse_from(["lux", "--slow"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default_file_mode_subcommand() {
        let cli = Cli::try_parse_from(["lux", "config", "default-file-mode", "less"]).unwrap();
        match cli.command {
            Some(Command::Config {
                action: ConfigAction::DefaultFileMode { value },
            }) => {
                assert_eq!(value, "less");
            }
            _ => panic!("expected Config DefaultFileMode"),
        }
    }

    #[test]
    fn test_expand_refs_flag() {
        let cli = Cli::try_parse_from(["lux", "--expand-refs", "file.md"]).unwrap();
        assert!(cli.expand_refs);
    }

    #[test]
    fn test_expand_refs_alias() {
        let cli = Cli::try_parse_from(["lux", "--expand-referenced-files", "file.md"]).unwrap();
        assert!(cli.expand_refs);
    }

    #[test]
    fn test_expand_refs_default_off() {
        let cli = Cli::try_parse_from(["lux", "file.md"]).unwrap();
        assert!(!cli.expand_refs);
    }

    #[test]
    fn test_expand_refs_conflicts_with_follow() {
        assert!(Cli::try_parse_from(["lux", "-f", "--expand-refs", "file.md"]).is_err());
        assert!(Cli::try_parse_from(["lux", "-F", "--expand-refs", "file.md"]).is_err());
    }
}
