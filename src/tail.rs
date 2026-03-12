use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

/// Specifies how to select lines from a file.
#[derive(Debug, PartialEq)]
pub enum LineSpec {
    /// Last N lines from the end of file.
    Last(usize),
    /// Starting from line N (1-based).
    FromLine(usize),
}

/// Parse a line specification string.
///
/// - "N" => LineSpec::Last(N)
/// - "+N" => LineSpec::FromLine(N)
pub fn parse_line_spec(s: &str) -> anyhow::Result<LineSpec> {
    if let Some(rest) = s.strip_prefix('+') {
        let n: usize = rest
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid line spec: '{s}'"))?;
        Ok(LineSpec::FromLine(n))
    } else {
        let n: usize = s
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid line spec: '{s}'"))?;
        Ok(LineSpec::Last(n))
    }
}

/// Read the last `n` lines from a file using backward-seeking with 8KB chunks.
///
/// Returns lines in order (first to last). The file position is left at EOF after this call.
pub fn read_last_n_lines(file: &mut std::fs::File, n: usize) -> anyhow::Result<Vec<String>> {
    if n == 0 {
        // Seek to end so file position is at EOF
        file.seek(SeekFrom::End(0))?;
        return Ok(Vec::new());
    }

    let file_len = file.seek(SeekFrom::End(0))?;
    if file_len == 0 {
        return Ok(Vec::new());
    }

    const CHUNK_SIZE: u64 = 8192;
    let mut remaining = file_len;
    let mut buf = Vec::new();
    // Read chunks backwards from EOF
    while remaining > 0 {
        let chunk_len = std::cmp::min(CHUNK_SIZE, remaining);
        remaining -= chunk_len;
        file.seek(SeekFrom::Start(remaining))?;

        let mut chunk = vec![0u8; chunk_len as usize];
        file.read_exact(&mut chunk)?;

        // Prepend chunk to accumulated buffer
        chunk.append(&mut buf);
        buf = chunk;

        // Count newlines in the buffer so far
        let newline_count = buf.iter().filter(|&&b| b == b'\n').count();

        // We need n+1 newlines (or n newlines + start-of-file) to capture n lines
        // If buf ends with \n, the last newline doesn't start a new line
        let lines_available = if buf.ends_with(b"\n") {
            newline_count
        } else {
            newline_count + 1
        };

        if lines_available > n && remaining > 0 {
            // We have enough -- no need to read more
            break;
        }
    }

    // Parse lines from accumulated buffer
    let text = String::from_utf8_lossy(&buf);
    let all_lines: Vec<&str> = text.lines().collect();

    // Take the last n lines
    let start = if all_lines.len() > n {
        all_lines.len() - n
    } else {
        0
    };
    let result: Vec<String> = all_lines[start..].iter().map(|s| s.to_string()).collect();

    // Leave file position at EOF
    file.seek(SeekFrom::End(0))?;

    Ok(result)
}

/// Read all lines from line number `line_num` (1-based) onwards.
///
/// The file position is left at EOF after this call.
pub fn read_from_line(file: &mut std::fs::File, line_num: usize) -> anyhow::Result<Vec<String>> {
    file.seek(SeekFrom::Start(0))?;
    let reader = BufReader::new(&*file);
    let mut result = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        // line_num is 1-based, enumerate is 0-based
        if i + 1 >= line_num {
            result.push(line);
        }
    }

    // Leave file position at EOF
    file.seek(SeekFrom::End(0))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- parse_line_spec tests ---

    #[test]
    fn parse_line_spec_last_10() {
        assert_eq!(parse_line_spec("10").unwrap(), LineSpec::Last(10));
    }

    #[test]
    fn parse_line_spec_from_line_5() {
        assert_eq!(parse_line_spec("+5").unwrap(), LineSpec::FromLine(5));
    }

    #[test]
    fn parse_line_spec_last_0() {
        assert_eq!(parse_line_spec("0").unwrap(), LineSpec::Last(0));
    }

    #[test]
    fn parse_line_spec_from_line_1() {
        assert_eq!(parse_line_spec("+1").unwrap(), LineSpec::FromLine(1));
    }

    #[test]
    fn parse_line_spec_error() {
        assert!(parse_line_spec("abc").is_err());
    }

    // --- read_last_n_lines tests ---

    fn make_temp_file(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn read_last_5_of_20() {
        let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        let tmp = make_temp_file(&lines);
        let mut file = std::fs::File::open(tmp.path()).unwrap();
        let result = read_last_n_lines(&mut file, 5).unwrap();
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], "line 16");
        assert_eq!(result[4], "line 20");
    }

    #[test]
    fn read_last_10_of_3() {
        let lines: String = (1..=3).map(|i| format!("line {i}\n")).collect();
        let tmp = make_temp_file(&lines);
        let mut file = std::fs::File::open(tmp.path()).unwrap();
        let result = read_last_n_lines(&mut file, 10).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 1");
        assert_eq!(result[2], "line 3");
    }

    #[test]
    fn read_last_n_empty_file() {
        let tmp = make_temp_file("");
        let mut file = std::fs::File::open(tmp.path()).unwrap();
        let result = read_last_n_lines(&mut file, 5).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn read_last_0_lines() {
        let tmp = make_temp_file("line 1\nline 2\n");
        let mut file = std::fs::File::open(tmp.path()).unwrap();
        let result = read_last_n_lines(&mut file, 0).unwrap();
        assert!(result.is_empty());
    }

    // --- read_from_line tests ---

    #[test]
    fn read_from_line_3() {
        let lines: String = (1..=5).map(|i| format!("line {i}\n")).collect();
        let tmp = make_temp_file(&lines);
        let mut file = std::fs::File::open(tmp.path()).unwrap();
        let result = read_from_line(&mut file, 3).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 3");
        assert_eq!(result[2], "line 5");
    }
}
