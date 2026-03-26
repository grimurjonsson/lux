use regex::Captures;

/// Interpolate capture group references in a template string.
///
/// - `$0` = full match, `$1` = first capture group, etc.
/// - `$$` = literal `$`
/// - `$N` where group N didn't participate = empty string
/// - Bare `$` at end or before non-digit = literal `$`
pub fn interpolate(template: &str, caps: &Captures) -> String {
    let mut result = String::with_capacity(template.len());
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 >= chars.len() {
                // Bare $ at end
                result.push('$');
                i += 1;
            } else if chars[i + 1] == '$' {
                // Escaped $$
                result.push('$');
                i += 2;
            } else if chars[i + 1].is_ascii_digit() {
                // Parse group index (may be multi-digit)
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let idx_str: String = chars[start..end].iter().collect();
                let idx: usize = idx_str.parse().unwrap();
                if let Some(m) = caps.get(idx) {
                    result.push_str(m.as_str());
                }
                // If group didn't participate, push nothing (empty string)
                i = end;
            } else {
                // Bare $ before non-digit
                result.push('$');
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn test_interpolate_full_match() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("[$0]", &caps), "[ERROR auth]");
    }

    #[test]
    fn test_interpolate_capture_group() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("module=$1", &caps), "module=auth");
    }

    #[test]
    fn test_interpolate_dollar_escape() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("cost: $$5", &caps), "cost: $5");
    }

    #[test]
    fn test_interpolate_optional_group_missing() {
        let re = Regex::new(r"(a)?(b)").unwrap();
        let text = "b";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("[$1][$2]", &caps), "[][b]");
    }

    #[test]
    fn test_interpolate_no_placeholders() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("plain text", &caps), "plain text");
    }

    #[test]
    fn test_interpolate_bare_dollar_at_end() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("end$", &caps), "end$");
    }
}
