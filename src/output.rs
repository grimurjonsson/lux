use crate::cli::ColorChoice;
use supports_color::Stream;

/// Represents the color capability level of the terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorMode {
    TrueColor,
    Color256,
    Color16,
    NoColor,
}

impl ColorMode {
    /// Returns true if any color output is enabled.
    pub fn color_enabled(&self) -> bool {
        !matches!(self, ColorMode::NoColor)
    }
}

/// Detect the appropriate color mode based on CLI choice, environment, and terminal capability.
pub fn detect_color_mode(cli_choice: &ColorChoice) -> ColorMode {
    match cli_choice {
        ColorChoice::Never => ColorMode::NoColor,
        ColorChoice::Always => ColorMode::TrueColor,
        ColorChoice::Auto => {
            // Check NO_COLOR env var (https://no-color.org/)
            if std::env::var_os("NO_COLOR").is_some() {
                return ColorMode::NoColor;
            }

            // FORCE_COLOR forces color even when not a terminal (e.g., piping)
            // Precedence: --color flag > NO_COLOR > FORCE_COLOR > auto-detect
            if std::env::var_os("FORCE_COLOR").is_some() {
                return ColorMode::TrueColor;
            }

            // Check if stdout is a terminal
            if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                return ColorMode::NoColor;
            }

            // Use supports-color to detect capability
            match supports_color::on(Stream::Stdout) {
                Some(level) => {
                    if level.has_16m {
                        ColorMode::TrueColor
                    } else if level.has_256 {
                        ColorMode::Color256
                    } else {
                        ColorMode::Color16
                    }
                }
                None => ColorMode::NoColor,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_enabled_true_for_truecolor() {
        assert!(ColorMode::TrueColor.color_enabled());
    }

    #[test]
    fn test_color_enabled_true_for_256() {
        assert!(ColorMode::Color256.color_enabled());
    }

    #[test]
    fn test_color_enabled_true_for_16() {
        assert!(ColorMode::Color16.color_enabled());
    }

    #[test]
    fn test_color_enabled_false_for_nocolor() {
        assert!(!ColorMode::NoColor.color_enabled());
    }

    #[test]
    fn test_never_returns_nocolor() {
        assert_eq!(detect_color_mode(&ColorChoice::Never), ColorMode::NoColor);
    }

    #[test]
    fn test_always_returns_truecolor() {
        assert_eq!(detect_color_mode(&ColorChoice::Always), ColorMode::TrueColor);
    }

    #[test]
    fn force_color_enables_color() {
        // SAFETY: No other test mutates FORCE_COLOR concurrently (serial execution)
        unsafe { std::env::set_var("FORCE_COLOR", "1") };
        // Remove NO_COLOR if set
        unsafe { std::env::remove_var("NO_COLOR") };
        let result = detect_color_mode(&ColorChoice::Auto);
        unsafe { std::env::remove_var("FORCE_COLOR") };
        assert_eq!(result, ColorMode::TrueColor);
    }

    #[test]
    fn no_color_overrides_force_color() {
        // SAFETY: No other test mutates these vars concurrently (serial execution)
        unsafe { std::env::set_var("FORCE_COLOR", "1") };
        unsafe { std::env::set_var("NO_COLOR", "1") };
        let result = detect_color_mode(&ColorChoice::Auto);
        unsafe { std::env::remove_var("FORCE_COLOR") };
        unsafe { std::env::remove_var("NO_COLOR") };
        assert_eq!(result, ColorMode::NoColor);
    }

    #[test]
    fn explicit_never_overrides_force_color() {
        // SAFETY: No other test mutates FORCE_COLOR concurrently (serial execution)
        unsafe { std::env::set_var("FORCE_COLOR", "1") };
        let result = detect_color_mode(&ColorChoice::Never);
        unsafe { std::env::remove_var("FORCE_COLOR") };
        assert_eq!(result, ColorMode::NoColor);
    }
}
