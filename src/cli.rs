use clap::{Parser, Subcommand};
use clap_complete::Shell;

/// Lit Up teXt — instantly readable colored output
#[derive(Parser)]
#[command(name = "lux", version, about = "Lit Up teXt — instantly readable colored output")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Add a coloring rule: PATTERN:STYLE[:SCOPE]
    #[arg(short = 'r', long = "rule", action = clap::ArgAction::Append)]
    pub rules: Vec<String>,

    /// Control color output
    #[arg(long, default_value = "auto")]
    pub color: ColorChoice,

    /// Select a named profile from the config file
    #[arg(short = 'p', long)]
    pub profile: Option<String>,

    /// Disable automatic profile and syntax highlighting (only explicit -r rules apply)
    #[arg(long, alias = "plain", conflicts_with = "profile")]
    pub no_profile: bool,

    /// Path to a custom config file (overrides XDG discovery)
    #[arg(long)]
    pub config: Option<String>,

    /// List available profiles from the config file
    #[arg(long)]
    pub list_profiles: bool,

    /// List available color names and styles
    #[arg(long)]
    pub list_colors: bool,

    /// List available syntax highlighting themes
    #[arg(long)]
    pub list_themes: bool,

    /// List available syntax definitions and their file extensions
    #[arg(long)]
    pub list_syntaxes: bool,

    /// Syntax highlighting theme (overrides config.toml)
    #[arg(long)]
    pub theme: Option<String>,

    /// Follow file by descriptor (reopen not attempted after rename/delete)
    #[arg(short = 'f')]
    pub follow_descriptor: bool,

    /// Follow file by name (reopen on rename/truncate/recreate)
    #[arg(short = 'F', conflicts_with = "follow_descriptor")]
    pub follow_name: bool,

    /// Open file in interactive pager mode (like less)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "cat"])]
    pub less: bool,

    /// Print file and exit (non-interactive, this is the default)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "less"])]
    pub cat: bool,

    /// Number of lines to show (e.g. "10", "+5" for from-line)
    #[arg(short = 'n')]
    pub lines: Option<String>,

    /// Trigger pattern(s) — suppress output until a match, then show context window
    #[arg(short = 't', long = "trigger", action = clap::ArgAction::Append)]
    pub trigger: Vec<String>,

    /// Context before trigger: line count (e.g. "20") or regex boundary (e.g. "^===")
    #[arg(short = 'b', long, default_value = "20")]
    pub before: String,

    /// Context after trigger: line count (e.g. "20") or regex boundary (e.g. "^---")
    #[arg(short = 'a', long, default_value = "20")]
    pub after: String,

    /// Only show lines matching PATTERN (can be repeated)
    #[arg(short = 'i', long = "include", action = clap::ArgAction::Append)]
    pub include: Vec<String>,

    /// Hide lines matching PATTERN (can be repeated)
    #[arg(short = 'e', long = "exclude", action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Strip ANSI escape codes from input before pattern matching
    #[arg(long = "strip-ansi", default_value = "auto")]
    pub strip_ansi: StripAnsi,

    /// File to read (positional argument)
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
        assert!(cli.lines.is_none(), "lines must be None when -n is not passed");
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
        let cli =
            Cli::try_parse_from(["lux", "--trigger", "ERROR", "--trigger", "WARN"]).unwrap();
        assert_eq!(cli.trigger, vec!["ERROR", "WARN"]);
    }

    #[test]
    fn test_trigger_with_before_after_count() {
        let cli = Cli::try_parse_from([
            "lux", "--trigger", "ERROR", "--trigger", "WARN", "--before", "5", "--after", "10",
        ])
        .unwrap();
        assert_eq!(cli.trigger, vec!["ERROR", "WARN"]);
        assert_eq!(cli.before, "5");
        assert_eq!(cli.after, "10");
    }

    #[test]
    fn test_trigger_with_before_after_pattern() {
        let cli = Cli::try_parse_from([
            "lux", "--trigger", "ERROR", "--before", "^===", "--after", "^---",
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
            Some(Command::Profile { action: ProfileAction::New { .. } })
        ));
    }

    #[test]
    fn test_profile_new_with_config() {
        let cli = Cli::try_parse_from(["lux", "profile", "new", "--config", "/tmp/my.toml"]).unwrap();
        match cli.command {
            Some(Command::Profile { action: ProfileAction::New { config } }) => {
                assert_eq!(config.as_deref(), Some("/tmp/my.toml"));
            }
            _ => panic!("expected Profile New"),
        }
    }

    #[test]
    fn test_profile_edit_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "edit", "django"]).unwrap();
        match cli.command {
            Some(Command::Profile { action: ProfileAction::Edit { name, .. } }) => {
                assert_eq!(name.as_deref(), Some("django"));
            }
            _ => panic!("expected Profile Edit"),
        }
    }

    #[test]
    fn test_profile_delete_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "delete", "django"]).unwrap();
        match cli.command {
            Some(Command::Profile { action: ProfileAction::Delete { name, .. } }) => {
                assert_eq!(name.as_deref(), Some("django"));
            }
            _ => panic!("expected Profile Delete"),
        }
    }

    #[test]
    fn test_profile_list_subcommand() {
        let cli = Cli::try_parse_from(["lux", "profile", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Profile { action: ProfileAction::List { .. } })
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
    fn test_config_default_file_mode_subcommand() {
        let cli = Cli::try_parse_from(["lux", "config", "default-file-mode", "less"]).unwrap();
        match cli.command {
            Some(Command::Config { action: ConfigAction::DefaultFileMode { value } }) => {
                assert_eq!(value, "less");
            }
            _ => panic!("expected Config DefaultFileMode"),
        }
    }
}
