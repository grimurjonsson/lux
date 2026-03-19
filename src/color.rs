use anyhow::{bail, Result};
use owo_colors::{Style, XtermColors};

/// Parse a style specification string into an `owo_colors::Style`.
///
/// Supports combined specs with `+` separator:
/// - Named ANSI colors: red, green, blue, bright-red, etc.
/// - Extended named colors: orange, pink, grey/gray, purple, teal, lime, etc.
/// - Hex colors: #ff0000 or 0xff0000
/// - 256-color numeric: 196
/// - Background colors: bg-red, bg-#ff0000, bg-196
/// - Style tokens: bold, dim, italic, underline
/// - Combined: bold+red+bg-white
pub fn parse_style(spec: &str) -> Result<Style> {
    let spec = spec.trim();
    if spec.is_empty() {
        bail!("empty style specification");
    }

    let mut style = Style::new();

    for token in spec.split('+') {
        let token = token.trim();
        if token.is_empty() {
            bail!("empty token in style specification '{spec}'");
        }

        if let Some(color_spec) = token.strip_prefix("bg-") {
            style = apply_bg_color(style, color_spec)?;
        } else {
            style = apply_fg_or_effect(style, token)?;
        }
    }

    Ok(style)
}

/// Parse a hex color string (with # or 0x prefix) into RGB components.
fn parse_hex(s: &str) -> Result<(u8, u8, u8)> {
    let hex = if let Some(h) = s.strip_prefix('#') {
        h
    } else if let Some(h) = s.strip_prefix("0x") {
        h
    } else {
        bail!("invalid hex color format: '{s}'");
    };

    if hex.len() != 6 {
        bail!("hex color must be 6 digits: '{s}'");
    }

    let r = u8::from_str_radix(&hex[0..2], 16)
        .map_err(|_| anyhow::anyhow!("invalid hex color: '{s}'"))?;
    let g = u8::from_str_radix(&hex[2..4], 16)
        .map_err(|_| anyhow::anyhow!("invalid hex color: '{s}'"))?;
    let b = u8::from_str_radix(&hex[4..6], 16)
        .map_err(|_| anyhow::anyhow!("invalid hex color: '{s}'"))?;

    Ok((r, g, b))
}

/// Try to parse a color spec as a foreground color or text effect.
fn apply_fg_or_effect(style: Style, token: &str) -> Result<Style> {
    // Text effects
    match token {
        "bold" => return Ok(style.bold()),
        "dim" => return Ok(style.dimmed()),
        "italic" => return Ok(style.italic()),
        "underline" => return Ok(style.underline()),
        _ => {}
    }

    // Named ANSI 16 colors (foreground)
    match token {
        "black" => return Ok(style.black()),
        "red" => return Ok(style.red()),
        "green" => return Ok(style.green()),
        "yellow" => return Ok(style.yellow()),
        "blue" => return Ok(style.blue()),
        "magenta" => return Ok(style.magenta()),
        "cyan" => return Ok(style.cyan()),
        "white" => return Ok(style.white()),
        "bright-black" => return Ok(style.bright_black()),
        "bright-red" => return Ok(style.bright_red()),
        "bright-green" => return Ok(style.bright_green()),
        "bright-yellow" => return Ok(style.bright_yellow()),
        "bright-blue" => return Ok(style.bright_blue()),
        "bright-magenta" => return Ok(style.bright_magenta()),
        "bright-cyan" => return Ok(style.bright_cyan()),
        "bright-white" => return Ok(style.bright_white()),
        _ => {}
    }

    // Extended named colors -> XtermColors (256-color)
    if let Some(xterm) = extended_color_index(token) {
        return Ok(style.color(XtermColors::from(xterm)));
    }

    // Hex colors: #rrggbb or 0xrrggbb
    if token.starts_with('#') || token.starts_with("0x") {
        let (r, g, b) = parse_hex(token)?;
        return Ok(style.truecolor(r, g, b));
    }

    // 256-color numeric
    if let Ok(n) = token.parse::<u8>() {
        return Ok(style.color(XtermColors::from(n)));
    }

    bail!(
        "unknown color or style '{token}'. Valid options: \
         red, green, blue, yellow, cyan, magenta, white, black, \
         bright-red, bright-green, bright-blue, bright-yellow, \
         bright-cyan, bright-magenta, bright-white, bright-black, \
         orange, pink, grey, gray, purple, teal, lime, brown, olive, coral, lavender, \
         bold, dim, italic, underline, \
         #rrggbb, 0xrrggbb, 0-255"
    )
}

/// Apply a background color from a color spec (after stripping the bg- prefix).
fn apply_bg_color(style: Style, color_spec: &str) -> Result<Style> {
    // Named ANSI 16 colors (background)
    match color_spec {
        "black" => return Ok(style.on_black()),
        "red" => return Ok(style.on_red()),
        "green" => return Ok(style.on_green()),
        "yellow" => return Ok(style.on_yellow()),
        "blue" => return Ok(style.on_blue()),
        "magenta" => return Ok(style.on_magenta()),
        "cyan" => return Ok(style.on_cyan()),
        "white" => return Ok(style.on_white()),
        "bright-black" => return Ok(style.on_bright_black()),
        "bright-red" => return Ok(style.on_bright_red()),
        "bright-green" => return Ok(style.on_bright_green()),
        "bright-yellow" => return Ok(style.on_bright_yellow()),
        "bright-blue" => return Ok(style.on_bright_blue()),
        "bright-magenta" => return Ok(style.on_bright_magenta()),
        "bright-cyan" => return Ok(style.on_bright_cyan()),
        "bright-white" => return Ok(style.on_bright_white()),
        _ => {}
    }

    // Extended named colors (background)
    if let Some(xterm) = extended_color_index(color_spec) {
        return Ok(style.on_color(XtermColors::from(xterm)));
    }

    // Hex colors (background)
    if color_spec.starts_with('#') || color_spec.starts_with("0x") {
        let (r, g, b) = parse_hex(color_spec)?;
        return Ok(style.on_truecolor(r, g, b));
    }

    // 256-color numeric (background)
    if let Ok(n) = color_spec.parse::<u8>() {
        return Ok(style.on_color(XtermColors::from(n)));
    }

    bail!(
        "unknown background color '{color_spec}'. Valid options: \
         red, green, blue, yellow, cyan, magenta, white, black, \
         bright-* variants, orange, pink, grey, purple, teal, lime, \
         #rrggbb, 0xrrggbb, 0-255"
    )
}

/// Map extended color names to 256-color (XtermColor) indices.
fn extended_color_index(name: &str) -> Option<u8> {
    match name {
        "orange" => Some(208),
        "pink" => Some(213),
        "grey" | "gray" => Some(245),
        "purple" => Some(129),
        "teal" => Some(30),
        "lime" => Some(118),
        "brown" => Some(130),
        "olive" => Some(142),
        "coral" => Some(209),
        "lavender" => Some(183),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: verify a style applies formatting (output differs from plain text)
    fn assert_styled(spec: &str) {
        let style = parse_style(spec).unwrap_or_else(|e| panic!("parse_style({spec:?}) failed: {e}"));
        let styled = style.style("test").to_string();
        assert_ne!(
            styled, "test",
            "parse_style({spec:?}) should produce styled output, got plain text"
        );
    }

    // Helper: verify a style spec returns an error
    fn assert_error(spec: &str) {
        let result = parse_style(spec);
        assert!(
            result.is_err(),
            "parse_style({spec:?}) should return error but got Ok"
        );
    }

    // --- Named ANSI foreground colors ---

    #[test]
    fn test_red() {
        assert_styled("red");
    }

    #[test]
    fn test_green() {
        assert_styled("green");
    }

    #[test]
    fn test_blue() {
        assert_styled("blue");
    }

    #[test]
    fn test_yellow() {
        assert_styled("yellow");
    }

    #[test]
    fn test_cyan() {
        assert_styled("cyan");
    }

    #[test]
    fn test_magenta() {
        assert_styled("magenta");
    }

    #[test]
    fn test_white() {
        assert_styled("white");
    }

    #[test]
    fn test_black() {
        assert_styled("black");
    }

    // --- Bright variants ---

    #[test]
    fn test_bright_red() {
        assert_styled("bright-red");
    }

    #[test]
    fn test_bright_green() {
        assert_styled("bright-green");
    }

    #[test]
    fn test_bright_blue() {
        assert_styled("bright-blue");
    }

    #[test]
    fn test_bright_yellow() {
        assert_styled("bright-yellow");
    }

    #[test]
    fn test_bright_cyan() {
        assert_styled("bright-cyan");
    }

    #[test]
    fn test_bright_magenta() {
        assert_styled("bright-magenta");
    }

    #[test]
    fn test_bright_white() {
        assert_styled("bright-white");
    }

    #[test]
    fn test_bright_black() {
        assert_styled("bright-black");
    }

    // --- Style tokens ---

    #[test]
    fn test_bold() {
        assert_styled("bold");
    }

    #[test]
    fn test_dim() {
        assert_styled("dim");
    }

    #[test]
    fn test_italic() {
        assert_styled("italic");
    }

    #[test]
    fn test_underline() {
        assert_styled("underline");
    }

    // --- Combined styles ---

    #[test]
    fn test_bold_red() {
        assert_styled("bold+red");
    }

    #[test]
    fn test_bold_red_underline() {
        assert_styled("bold+red+underline");
    }

    // --- Hex colors ---

    #[test]
    fn test_hex_hash() {
        assert_styled("#ff0000");
    }

    #[test]
    fn test_hex_0x() {
        assert_styled("0xff0000");
    }

    // --- 256-color numeric ---

    #[test]
    fn test_256_color() {
        assert_styled("196");
    }

    // --- Background colors ---

    #[test]
    fn test_bg_red() {
        assert_styled("bg-red");
    }

    #[test]
    fn test_bg_hex() {
        assert_styled("bg-#ff0000");
    }

    #[test]
    fn test_bg_256() {
        assert_styled("bg-196");
    }

    // --- Combined with background ---

    #[test]
    fn test_red_bg_white() {
        assert_styled("red+bg-white");
    }

    #[test]
    fn test_bold_red_bg_white() {
        assert_styled("bold+red+bg-white");
    }

    // --- Extended named colors ---

    #[test]
    fn test_orange() {
        assert_styled("orange");
    }

    #[test]
    fn test_pink() {
        assert_styled("pink");
    }

    #[test]
    fn test_grey() {
        assert_styled("grey");
    }

    #[test]
    fn test_gray_alias() {
        assert_styled("gray");
    }

    #[test]
    fn test_purple() {
        assert_styled("purple");
    }

    #[test]
    fn test_teal() {
        assert_styled("teal");
    }

    #[test]
    fn test_lime() {
        assert_styled("lime");
    }

    // --- Error cases ---

    #[test]
    fn test_empty_error() {
        assert_error("");
    }

    #[test]
    fn test_invalid_error() {
        let result = parse_style("invalid_garbage");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid_garbage"),
            "Error message should mention the invalid token, got: {msg}"
        );
    }

    #[test]
    fn test_bg_invalid_error() {
        assert_error("bg-invalid");
    }
}
